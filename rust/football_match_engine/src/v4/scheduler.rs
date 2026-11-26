use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::clock::Period;
use crate::v4::spatial::TeamSideV4;

/// Maximum consecutive zero-delta-time events popped before tripping the infinite-loop guard.
pub const MAX_ZERO_DT_STREAK: u32 = 64;
/// Maximum total valid events popped per match before tripping the runaway-simulation guard.
pub const MAX_TOTAL_POPPED_EVENTS: u64 = 250_000;

/// Deterministic same-millisecond tie-breaking priority levels.
/// Lower numeric value executes first when two events share the exact same `event_time_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EventPriority {
    /// Interceptions, blocks, touchline/goal-line crossings.
    PhysicsAndBoundary = 10,
    /// Goals, fouls, offsides, cards, injuries.
    RuleAndOfficiating = 20,
    /// First touch completion, control transitions, loose-ball contests.
    BallStateTransition = 30,
    /// Opening/closing `DeadBallState`, flushing `pending_substitutions`.
    DeadBallAndSubstitution = 40,
    /// Tactical commands becoming effective, AI manager checks.
    TacticalAndCommand = 50,
    /// Perception/decision readiness, passes, dribbles, shots.
    ActionDecision = 60,
    /// Added-time announcement, period-end whistle checks.
    PeriodBoundary = 70,
}

impl EventPriority {
    #[inline]
    pub const fn rank(self) -> u8 {
        self as u8
    }
}

/// Generation-based cancellation scope for scheduled DES events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancellationScope {
    /// Invalidated whenever live play is interrupted (`invalidate_live_play` or `invalidate_period`).
    LivePlayOnly,
    /// Survives live-play interruptions within the same period, invalidated by `invalidate_period`.
    PeriodScoped,
    /// Never invalidated by generation bumps (e.g. match-wide or cross-period commands).
    Persistent,
}

/// Structured DES event payloads covering ball physics, decisions, dead-ball phases,
/// tactical updates, and period boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesEventPayload {
    BallInFlightArrival {
        passer_id: Option<String>,
        receiver_id: Option<String>,
        target_x_mm: i32,
        target_y_mm: i32,
    },
    FirstTouchComplete {
        player_id: String,
        side: TeamSideV4,
    },
    DecisionReady {
        player_id: String,
        side: TeamSideV4,
    },
    ActionExecute {
        player_id: String,
        side: TeamSideV4,
    },
    DribbleStep {
        carrier_id: String,
        side: TeamSideV4,
    },
    ShotArrival {
        shooter_id: String,
        side: TeamSideV4,
        target_y_mm: i32,
        target_z_mm: i32,
    },
    DeadBallPhaseComplete {
        phase_tag: String,
    },
    TacticalCommandEffective {
        command_index: u32,
        side: TeamSideV4,
    },
    AiManagerCheck {
        side: TeamSideV4,
    },
    AnnounceAddedTime {
        period: Period,
    },
    CheckPeriodEnd {
        period: Period,
    },
    EarlyInteractionResolved {
        interaction_tag: String,
        actor_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledEvent {
    pub event_time_ms: i64,
    pub event_priority: EventPriority,
    pub insertion_sequence: u64,
    pub state_generation: u64,
    pub scope: CancellationScope,
    pub payload: DesEventPayload,
}

/// Internal min-heap wrapper ordering `ScheduledEvent` by ascending
/// `(event_time_ms, event_priority, insertion_sequence)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MinHeapEntry(ScheduledEvent);

impl PartialEq for MinHeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.event_time_ms == other.0.event_time_ms
            && self.0.event_priority == other.0.event_priority
            && self.0.insertion_sequence == other.0.insertion_sequence
    }
}

impl Eq for MinHeapEntry {}

impl Ord for MinHeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed comparison so `BinaryHeap` acts as a deterministic min-heap.
        other
            .0
            .event_time_ms
            .cmp(&self.0.event_time_ms)
            .then_with(|| other.0.event_priority.cmp(&self.0.event_priority))
            .then_with(|| other.0.insertion_sequence.cmp(&self.0.insertion_sequence))
    }
}

impl PartialOrd for MinHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Deterministic Discrete-Event Simulation priority queue with lazy generation-based
/// cancellation and runaway-loop safety guards.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventScheduler {
    queue: BinaryHeap<MinHeapEntry>,
    current_time_ms: i64,
    next_sequence: u64,
    live_play_generation: u64,
    period_generation: u64,
    zero_dt_streak: u32,
    total_popped: u64,
}

impl Default for EventScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventScheduler {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            current_time_ms: 0,
            next_sequence: 1,
            live_play_generation: 1,
            period_generation: 1,
            zero_dt_streak: 0,
            total_popped: 0,
        }
    }

    #[inline]
    pub fn current_time_ms(&self) -> i64 {
        self.current_time_ms
    }

    #[inline]
    pub fn live_play_generation(&self) -> u64 {
        self.live_play_generation
    }

    #[inline]
    pub fn period_generation(&self) -> u64 {
        self.period_generation
    }

    #[inline]
    pub fn zero_dt_streak(&self) -> u32 {
        self.zero_dt_streak
    }

    #[inline]
    pub fn total_popped(&self) -> u64 {
        self.total_popped
    }

    #[inline]
    pub fn len_raw(&self) -> usize {
        self.queue.len()
    }

    #[inline]
    pub fn is_empty_raw(&self) -> bool {
        self.queue.is_empty()
    }

    /// Schedules a new event at `event_time_ms >= current_time_ms`.
    /// Returns the assigned monotonic `insertion_sequence` on success.
    pub fn schedule(
        &mut self,
        event_time_ms: i64,
        event_priority: EventPriority,
        scope: CancellationScope,
        payload: DesEventPayload,
    ) -> Result<u64, &'static str> {
        if event_time_ms < self.current_time_ms {
            return Err("cannot schedule DES event in the past");
        }

        let insertion_sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1)
            .ok_or("DES insertion sequence exhausted")?;

        let state_generation = match scope {
            CancellationScope::LivePlayOnly => self.live_play_generation,
            CancellationScope::PeriodScoped => self.period_generation,
            CancellationScope::Persistent => 0,
        };

        self.queue.push(MinHeapEntry(ScheduledEvent {
            event_time_ms,
            event_priority,
            insertion_sequence,
            state_generation,
            scope,
            payload,
        }));

        Ok(insertion_sequence)
    }

    #[inline]
    fn is_event_valid(&self, event: &ScheduledEvent) -> bool {
        match event.scope {
            CancellationScope::LivePlayOnly => event.state_generation == self.live_play_generation,
            CancellationScope::PeriodScoped => event.state_generation == self.period_generation,
            CancellationScope::Persistent => true,
        }
    }

    /// Pops the next non-cancelled event in deterministic `(time, priority, sequence)` order.
    /// Enforces `zero_dt_streak <= 64` and `total_popped <= 250_000`.
    pub fn pop_next_valid(&mut self) -> Result<Option<ScheduledEvent>, &'static str> {
        while let Some(MinHeapEntry(event)) = self.queue.pop() {
            if !self.is_event_valid(&event) {
                continue;
            }
            if event.event_time_ms < self.current_time_ms {
                return Err("DES scheduler encountered event behind current_time_ms");
            }

            if event.event_time_ms == self.current_time_ms {
                self.zero_dt_streak = self.zero_dt_streak.saturating_add(1);
                if self.zero_dt_streak > MAX_ZERO_DT_STREAK {
                    return Err("DES zero_dt_streak exceeded maximum limit (64)");
                }
            } else {
                self.current_time_ms = event.event_time_ms;
                self.zero_dt_streak = 0;
            }

            self.total_popped = self.total_popped.saturating_add(1);
            if self.total_popped > MAX_TOTAL_POPPED_EVENTS {
                return Err("DES total_popped exceeded maximum limit (250000)");
            }

            return Ok(Some(event));
        }

        Ok(None)
    }

    /// Validate restored scheduler identity and timing, without purging stale entries
    /// or changing heap order. Cancelled entries may legitimately precede the cursor.
    pub(crate) fn validate_snapshot(&self)->Result<(), &'static str>{
        if self.current_time_ms<0 || self.next_sequence==0 || self.live_play_generation==0 || self.period_generation==0
            || self.zero_dt_streak>MAX_ZERO_DT_STREAK || self.total_popped>MAX_TOTAL_POPPED_EVENTS {
            return Err("invalid scheduler cursor, generation or watchdog counters");
        }
        let mut seen=std::collections::HashSet::new();
        for entry in &self.queue {
            let event=&entry.0;
            if event.insertion_sequence==0 || event.insertion_sequence>=self.next_sequence || !seen.insert(event.insertion_sequence){
                return Err("invalid or duplicate scheduler insertion sequence");
            }
            if self.is_event_valid(event) && event.event_time_ms<self.current_time_ms {
                return Err("valid scheduler event precedes cursor");
            }
        }
        Ok(())
    }

    /// Returns the timestamp of the next valid (non-cancelled) event without popping it,
    /// lazily purging cancelled events from the top of the heap.
    pub fn peek_next_valid_time_ms(&mut self) -> Option<i64> {
        while let Some(top) = self.queue.peek() {
            if self.is_event_valid(&top.0) {
                return Some(top.0.event_time_ms);
            }
            self.queue.pop();
        }
        None
    }

    /// Invalidates all pending `LivePlayOnly` events by advancing `live_play_generation`.
    pub fn invalidate_live_play(&mut self) -> u64 {
        self.live_play_generation = self.live_play_generation.saturating_add(1);
        self.live_play_generation
    }

    /// Invalidates all pending `LivePlayOnly` and `PeriodScoped` events by advancing
    /// both `live_play_generation` and `period_generation`.
    pub fn invalidate_period(&mut self) -> u64 {
        self.live_play_generation = self.live_play_generation.saturating_add(1);
        self.period_generation = self.period_generation.saturating_add(1);
        self.period_generation
    }

    /// Advances the scheduler's monotonic cursor to `time_ms`.
    /// Rejects moving backwards and resets `zero_dt_streak` when time strictly advances.
    pub fn advance_cursor_to(&mut self, time_ms: i64) -> Result<(), &'static str> {
        if time_ms < self.current_time_ms {
            return Err("cannot move DES scheduler cursor backwards");
        }
        if time_ms > self.current_time_ms {
            self.current_time_ms = time_ms;
            self.zero_dt_streak = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b11_probe_payload(index: u32) -> DesEventPayload {
        DesEventPayload::TacticalCommandEffective {
            command_index: index, side: TeamSideV4::Home,
        }
    }

    #[test]
    fn b11_scheduler_all_priorities_and_fifo_survive_restore() {
        use EventPriority::*;
        let priorities = [PhysicsAndBoundary, RuleAndOfficiating, BallStateTransition,
            DeadBallAndSubstitution, TacticalAndCommand, ActionDecision, PeriodBoundary];
        let mut sched = EventScheduler::new();
        for &priority in priorities.iter().rev() {
            sched.schedule(1201, priority, CancellationScope::Persistent, b11_probe_payload(1)).unwrap();
            sched.schedule(1201, priority, CancellationScope::Persistent, b11_probe_payload(2)).unwrap();
        }
        let earlier = sched.schedule(1200, PeriodBoundary, CancellationScope::Persistent, b11_probe_payload(0)).unwrap();
        let json = serde_json::to_string(&sched).unwrap();
        let mut restored: EventScheduler = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pop_next_valid().unwrap().unwrap().insertion_sequence, earlier);
        for priority in priorities {
            for index in [1, 2] {
                let ev = restored.pop_next_valid().unwrap().unwrap();
                assert_eq!(ev.event_time_ms, 1201);
                assert_eq!(ev.event_priority, priority);
                assert_eq!(ev.payload, b11_probe_payload(index));
            }
        }
        assert!(restored.pop_next_valid().unwrap().is_none());
    }

    #[test]
    fn b11_scheduler_cancelled_events_do_not_advance_time_or_watchdog() {
        let mut sched = EventScheduler::new();
        for i in 0..100 {
            sched.schedule(0, EventPriority::PhysicsAndBoundary, CancellationScope::LivePlayOnly, b11_probe_payload(i)).unwrap();
        }
        sched.invalidate_live_play();
        let id = sched.schedule(250, EventPriority::ActionDecision, CancellationScope::Persistent, b11_probe_payload(101)).unwrap();
        assert_eq!(sched.peek_next_valid_time_ms(), Some(250));
        assert_eq!(sched.current_time_ms(), 0);
        assert_eq!(sched.total_popped(), 0);
        assert_eq!(sched.zero_dt_streak(), 0);
        assert_eq!(sched.pop_next_valid().unwrap().unwrap().insertion_sequence, id);
        assert_eq!(sched.total_popped(), 1);
        assert_eq!(sched.current_time_ms(), 250);
        assert_eq!(sched.zero_dt_streak(), 0);
    }

    #[test]
    fn b11_scheduler_sequence_exhaustion_does_not_insert() {
        let mut sched = EventScheduler::new();
        sched.next_sequence = u64::MAX;
        let before = serde_json::to_string(&sched).unwrap();
        assert!(sched.schedule(0, EventPriority::ActionDecision, CancellationScope::Persistent, b11_probe_payload(1)).is_err());
        assert_eq!(serde_json::to_string(&sched).unwrap(), before);
    }

    #[test]
    fn deterministic_ordering_by_time_priority_and_sequence() {
        let mut sched = EventScheduler::new();

        let seq_dec = sched
            .schedule(
                1_000,
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DecisionReady {
                    player_id: "H09".into(),
                    side: TeamSideV4::Home,
                },
            )
            .unwrap();
        let seq_phys_1 = sched
            .schedule(
                1_000,
                EventPriority::PhysicsAndBoundary,
                CancellationScope::LivePlayOnly,
                DesEventPayload::EarlyInteractionResolved {
                    interaction_tag: "INTERCEPT_1".into(),
                    actor_id: Some("A04".into()),
                },
            )
            .unwrap();
        let seq_phys_2 = sched
            .schedule(
                1_000,
                EventPriority::PhysicsAndBoundary,
                CancellationScope::LivePlayOnly,
                DesEventPayload::EarlyInteractionResolved {
                    interaction_tag: "INTERCEPT_2".into(),
                    actor_id: Some("A05".into()),
                },
            )
            .unwrap();
        let seq_early = sched
            .schedule(
                500,
                EventPriority::PeriodBoundary,
                CancellationScope::PeriodScoped,
                DesEventPayload::CheckPeriodEnd {
                    period: Period::FirstHalf,
                },
            )
            .unwrap();

        assert_eq!(sched.peek_next_valid_time_ms(), Some(500));
        assert_eq!(sched.pop_next_valid().unwrap().unwrap().insertion_sequence, seq_early);
        assert_eq!(sched.pop_next_valid().unwrap().unwrap().insertion_sequence, seq_phys_1);
        assert_eq!(sched.pop_next_valid().unwrap().unwrap().insertion_sequence, seq_phys_2);
        assert_eq!(sched.pop_next_valid().unwrap().unwrap().insertion_sequence, seq_dec);
        assert!(sched.pop_next_valid().unwrap().is_none());
    }

    #[test]
    fn cancellation_scopes_and_past_rejection() {
        let mut sched = EventScheduler::new();
        sched.advance_cursor_to(2_000).unwrap();
        assert!(sched
            .schedule(
                1_999,
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::AiManagerCheck {
                    side: TeamSideV4::Home,
                },
            )
            .is_err());

        sched
            .schedule(
                3_000,
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DribbleStep {
                    carrier_id: "H10".into(),
                    side: TeamSideV4::Home,
                },
            )
            .unwrap();
        sched
            .schedule(
                4_000,
                EventPriority::PeriodBoundary,
                CancellationScope::PeriodScoped,
                DesEventPayload::AnnounceAddedTime {
                    period: Period::FirstHalf,
                },
            )
            .unwrap();
        sched
            .schedule(
                5_000,
                EventPriority::TacticalAndCommand,
                CancellationScope::Persistent,
                DesEventPayload::TacticalCommandEffective {
                    command_index: 1,
                    side: TeamSideV4::Away,
                },
            )
            .unwrap();

        sched.invalidate_live_play();
        assert_eq!(sched.peek_next_valid_time_ms(), Some(4_000));

        sched.invalidate_period();
        assert_eq!(sched.peek_next_valid_time_ms(), Some(5_000));

        let ev = sched.pop_next_valid().unwrap().unwrap();
        assert_eq!(ev.event_time_ms, 5_000);
        assert_eq!(ev.scope, CancellationScope::Persistent);
    }

    #[test]
    fn zero_dt_streak_guard_trips_after_64() {
        let mut sched = EventScheduler::new();
        for i in 0..=MAX_ZERO_DT_STREAK {
            sched
                .schedule(
                    0,
                    EventPriority::ActionDecision,
                    CancellationScope::Persistent,
                    DesEventPayload::TacticalCommandEffective {
                        command_index: i,
                        side: TeamSideV4::Home,
                    },
                )
                .unwrap();
        }
        for _ in 0..MAX_ZERO_DT_STREAK {
            assert!(sched.pop_next_valid().unwrap().is_some());
        }
        assert!(sched.pop_next_valid().is_err());
    }

    #[test]
    fn serde_roundtrip_preserves_queue_order() {
        let mut sched = EventScheduler::new();
        sched
            .schedule(
                1_500,
                EventPriority::BallStateTransition,
                CancellationScope::LivePlayOnly,
                DesEventPayload::FirstTouchComplete {
                    player_id: "H07".into(),
                    side: TeamSideV4::Home,
                },
            )
            .unwrap();
        sched
            .schedule(
                1_200,
                EventPriority::PhysicsAndBoundary,
                CancellationScope::LivePlayOnly,
                DesEventPayload::BallInFlightArrival {
                    passer_id: Some("H08".into()),
                    receiver_id: Some("H07".into()),
                    target_x_mm: 72_500,
                    target_y_mm: 34_000,
                },
            )
            .unwrap();

        let json = serde_json::to_string(&sched).unwrap();
        let mut restored: EventScheduler = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pop_next_valid().unwrap().unwrap().event_time_ms, 1_200);
        assert_eq!(restored.pop_next_valid().unwrap().unwrap().event_time_ms, 1_500);
    }
}
