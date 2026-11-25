use std::collections::BTreeMap;

use crate::clock::Period;
use crate::fingerprints;
use crate::rng::DeterministicRng;
use crate::v4::coach_adapter::evaluate_ai_managers;
use crate::v4::substitution::apply_manual_substitution_group;
use crate::v4::attributes::normalize_attr;
use crate::v4::ball_play::{resolve_carry_contact, execute_pass_and_first_touch_des, is_receiver_offside, pass_reach_candidate, EmittedPlayEvent};
use crate::v4::checkpoint::{
    PendingFlightSnapshotV4, FlightActionV4, FlightCounterDeltaV4, PendingDecisionV4, PendingCarryV4,
    compute_checkpoint_checksum, verify_checkpoint, MatchCheckpointV4, MatchSessionSnapshotV4,
    RngSnapshotV4, SessionStatsSnapshotV4, CHECKPOINT_SCHEMA_VERSION_V4,
};
use crate::v4::diagnostics::{DeadBallDiagKind, MatchDiagnosticsCollectorV4};
use crate::v4::error::V4Error;
use crate::v4::events::{
    fingerprint_event_parts, EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4,
    MatchResultV4, MatchSummaryV4, PendingCommandSummaryV4, PlayerMatchSummaryV4, SessionStageV4,
    SessionStepResultV4, TeamMatchSummaryV4, TeamRosterStateV4,
};
use crate::v4::input::{MatchInputV4, TeamInstructionsV4};
use crate::v4::interaction::{
    InteractionKind, PassContactMotion, BallTrajectory,
    contact_control_probability, blocked_ball_velocity, find_moving_ball_recovery,
    LooseBallResolution, BoundaryKind, EarlyInteractionHit, find_carry_contact,
};
use crate::v4::movement::{advance_off_ball_movement_with_report, evaluate_local_context, evaluate_passing_lane, lookup_attrs};
use crate::player_intelligence::{perceive_evaluate_and_decide_des, DecisionPlanV4, CarryIntentV4, carry_target, PassKindV4, PlannedActionV4, PI_POLICY_ID};
use crate::v4::psychology::{
    check_muscle_injury, compute_post_match_morale_delta,
    sample_prematch_form,
    shift_momentum, update_in_match_psychology,
};
use crate::v4::rules::{
    estimate_ball_flight_ms, estimate_dead_ball_phase_ms,
    initial_dead_ball_phase, next_dead_ball_phase, BallFlightKind, CommandLifecycleStatus,
    CommandOrigin, DeadBallKind, DeadBallPhase, DeadBallState, HalfEndPolicy, HalfStageV4,
    PendingSubstitutionItem, PendingTacticalCommand, RulesetConfig,
};
use crate::v4::scheduler::{CancellationScope, DesEventPayload, EventPriority, EventScheduler};
use crate::v4::set_pieces::{
    execute_corner_v4, execute_free_kick_v4, execute_penalty_kick_v4_with_rules,
};
use crate::v4::shooting::{resolve_shot_v4_des, ShotKindV4};
use crate::v4::spatial::{
    attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box, TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};
use crate::v4::tactics::{
    attach_tactical_flags, compute_tactical_delay_for_context_ms, compute_tactical_flags,
    update_tactical_phases,
};
use crate::v4::{
    ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, EVENT_SCHEMA_VERSION_V4,
};

pub use crate::v4::session_commands::{AppliedCommandV4, MatchCommandV4, SubstitutionChangeV4};
pub use crate::v4::session_replay::{replay_v4_with_timeline, simulate_batch_fast_v4, simulate_v4};

/// Discrete-Event Simulation (DES v1.4) Match Engine V4 session supporting:
/// - **Interactive time-targeted & step-by-step control** (`advance_until_ms`, `advance_steps`,
///   `start_next_period`, `submit_command`) with explicit `HalfTimeBreak` / `ExtraTimeBreak` pauses
/// - **Self-verifying checkpoint export/import** (`export_checkpoint`, `from_checkpoint`)
/// - **FastBatch / Seed-Replay** (`run_to_full_time`) with zero-allocation event fingerprinting
#[derive(Clone)]
pub struct MatchSessionV4 {
    input: MatchInputV4,
    ruleset: RulesetConfig,
    pub(super) state: MatchStateV4,
    scheduler: EventScheduler,
    rng: DeterministicRng,
    retain_events: bool,
    events: Vec<MatchEventV4>,
    diagnostics: MatchDiagnosticsCollectorV4,
    last_delivered_event_index: usize,
    event_count: u32,
    timeline_fingerprint: u64,
    went_to_extra_time: bool,
    went_to_shootout: bool,
    pub(super) finished: bool,
    last_passer_idx: Option<usize>,
    pub(super) stage: HalfStageV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    first_half_elapsed_ms: i64,
    second_half_elapsed_ms: i64,
    et1_elapsed_ms: i64,
    et2_elapsed_ms: i64,
    home_possession_ms: i64,
    away_possession_ms: i64,
    last_dt_ms: i64,
    pub(super) completed_beats: u64,
    next_command_id: u64,
    pending_substitutions: Vec<PendingSubstitutionItem>,
    pending_tactical_commands: Vec<PendingTacticalCommand>,
    command_timeline: Vec<AppliedCommandV4>,
    pending_flight: Option<PendingFlightSnapshotV4>,
    pending_decision: Option<PendingDecisionV4>,
    pending_carry: Option<PendingCarryV4>,
    tackle_ready_at: BTreeMap<String,i64>,
}

impl MatchSessionV4 {
    pub fn new(input: MatchInputV4) -> Result<Self, V4Error> {
        input.validate()?;
        let retain_events = input.simulation_mode.retain_events();
        let ruleset = input.effective_ruleset();
        let mut state = MatchStateV4::from_input(&input);
        let mut rng = DeterministicRng::new(input.seed);

        let importance = input.match_environment.match_importance;
        for idx in 0..state.players.len() {
            let attrs = *lookup_attrs(&input, &state.players[idx]);
            let (match_form,form_mult)=sample_prematch_form(&attrs,importance,&mut rng);
            state.players[idx].psychology.match_form_multiplier=match_form;
            state.players[idx].psychology.confidence =
                (state.players[idx].psychology.confidence * form_mult).clamp(0.15, 1.0);
        }

        Ok(Self {
            input,
            ruleset,
            state,
            scheduler: EventScheduler::new(),
            rng,
            retain_events,
            events: if retain_events {
                Vec::with_capacity(1024)
            } else {
                Vec::new()
            },
            diagnostics: MatchDiagnosticsCollectorV4::default(),
            last_delivered_event_index: 0,
            event_count: 0,
            timeline_fingerprint: fingerprints::start(),
            went_to_extra_time: false,
            went_to_shootout: false,
            finished: false,
            last_passer_idx: None,
            stage: HalfStageV4::PreMatch,
            possession_side: TeamSideV4::Home,
            carrier_idx: 10,
            first_half_elapsed_ms: 0,
            second_half_elapsed_ms: 0,
            et1_elapsed_ms: 0,
            et2_elapsed_ms: 0,
            home_possession_ms: 0,
            away_possession_ms: 0,
            last_dt_ms: 0,
            completed_beats: 0,
            next_command_id: 1,
            pending_substitutions: Vec::new(),
            pending_tactical_commands: Vec::new(),
            command_timeline: Vec::new(),
            pending_flight: None,
            pending_decision: None,
            pending_carry: None,
            tackle_ready_at: BTreeMap::new(),
        })
    }

    pub fn is_full_time(&self) -> bool {
        self.finished || self.stage.is_full_time()
    }

    pub fn stage(&self) -> HalfStageV4 {
        self.stage
    }

    pub fn sim_time_ms(&self) -> i64 {
        self.state.des_clock.sim_time_ms
    }

    /// Exports a self-verifying DES v1.4 checkpoint containing full engine state, scheduler,
    /// RNG state, pending commands, and deterministic payload checksum.
    pub fn export_checkpoint(&self) -> Result<MatchCheckpointV4, V4Error> {
        let mut diag_clone = self.diagnostics.clone();
        let diagnostics_summary = diag_clone.finish();
        let diagnostics_collector = self.diagnostics.clone();

        let payload = MatchSessionSnapshotV4 {
            input: self.input.clone(),
            state: self.state.clone(),
            scheduler: self.scheduler.clone(),
            rng: RngSnapshotV4 {
                root_seed: self.rng.root_seed(),
                state: self.rng.state(),
            },
            half_stage: self.stage,
            pending_substitutions: self.pending_substitutions.clone(),
            pending_tactical_commands: self.pending_tactical_commands.clone(),
            applied_commands: self.command_timeline.clone(),
            next_event_sequence: self.event_count,
            stats: SessionStatsSnapshotV4 {
                timeline_fingerprint: self.timeline_fingerprint,
                event_count: self.event_count,
                last_delivered_event_index: self.last_delivered_event_index,
                completed_beats: self.completed_beats,
                next_command_id: self.next_command_id,
                went_to_extra_time: self.went_to_extra_time,
                went_to_shootout: self.went_to_shootout,
                finished: self.finished,
                possession_side: self.possession_side,
                carrier_idx: self.carrier_idx,
                last_passer_idx: self.last_passer_idx,
                first_half_elapsed_ms: self.first_half_elapsed_ms,
                second_half_elapsed_ms: self.second_half_elapsed_ms,
                et1_elapsed_ms: self.et1_elapsed_ms,
                et2_elapsed_ms: self.et2_elapsed_ms,
                home_possession_ms: self.home_possession_ms,
                away_possession_ms: self.away_possession_ms,
                retained_events: if self.retain_events {
                    self.events.clone()
                } else {
                    Vec::new()
                },
            },
            diagnostics_collector,
            diagnostics_summary,
            pending_flight: self.pending_flight.clone(),
            pi_policy_id: PI_POLICY_ID.into(),
            pending_decision: self.pending_decision.clone(),
            pending_carry: self.pending_carry.clone(),
            tackle_ready_at: self.tackle_ready_at.clone(),
        };

        let payload_json = serde_json::to_string(&payload)?;
        let checksum_hex = compute_checkpoint_checksum(&payload_json);

        Ok(MatchCheckpointV4 {
            checkpoint_schema_version: CHECKPOINT_SCHEMA_VERSION_V4,
            engine_version: ENGINE_VERSION_V4.to_string(),
            protocol_version: ENGINE_PROTOCOL_VERSION_V4,
            event_schema_version: EVENT_SCHEMA_VERSION_V4,
            ruleset_identity: self.ruleset.identity.clone(),
            match_id: self.input.match_id.clone(),
            seed: self.input.seed,
            input_fingerprint: format!("{:016x}", self.input.input_fingerprint()),
            sim_time_ms: self.state.des_clock.sim_time_ms,
            checksum_hex,
            payload,
        })
    }

    /// Verifies and restores a `MatchSessionV4` from a saved `MatchCheckpointV4`.
    pub fn from_checkpoint(ckpt: MatchCheckpointV4) -> Result<Self, V4Error> {
        verify_checkpoint(&ckpt)?;
        let payload = ckpt.payload;
        payload.input.validate()?;
        let ruleset = payload.input.effective_ruleset();
        let retain_events = payload.input.simulation_mode.retain_events();
        let rng = DeterministicRng::from_seed_and_state(payload.rng.root_seed, payload.rng.state);
        let diagnostics = payload.diagnostics_collector;

        Ok(Self {
            input: payload.input,
            ruleset,
            state: payload.state,
            scheduler: payload.scheduler,
            rng,
            retain_events,
            events: payload.stats.retained_events,
            diagnostics,
            last_delivered_event_index: payload.stats.last_delivered_event_index,
            event_count: payload.stats.event_count,
            timeline_fingerprint: payload.stats.timeline_fingerprint,
            went_to_extra_time: payload.stats.went_to_extra_time,
            went_to_shootout: payload.stats.went_to_shootout,
            finished: payload.stats.finished,
            last_passer_idx: payload.stats.last_passer_idx,
            stage: payload.half_stage,
            possession_side: payload.stats.possession_side,
            carrier_idx: payload.stats.carrier_idx,
            first_half_elapsed_ms: payload.stats.first_half_elapsed_ms,
            second_half_elapsed_ms: payload.stats.second_half_elapsed_ms,
            et1_elapsed_ms: payload.stats.et1_elapsed_ms,
            et2_elapsed_ms: payload.stats.et2_elapsed_ms,
            home_possession_ms: payload.stats.home_possession_ms,
            away_possession_ms: payload.stats.away_possession_ms,
            last_dt_ms: 0,
            completed_beats: payload.stats.completed_beats,
            next_command_id: payload.stats.next_command_id,
            pending_substitutions: payload.pending_substitutions,
            pending_tactical_commands: payload.pending_tactical_commands,
            command_timeline: payload.applied_commands,
            pending_flight: payload.pending_flight,
            pending_decision: payload.pending_decision,
            pending_carry: payload.pending_carry,
            tackle_ready_at: payload.tackle_ready_at,
        })
    }

    /// Submits a live tactical update, manual substitution, or period-start command.
    ///
    /// - `UpdateTactics`: never applies instantaneously in live play; computes natural
    ///   propagation delay via `compute_tactical_delay_for_context_ms` and schedules
    ///   `TacticalCommandEffective` (immediate only during `HalfTimeBreak`/`ExtraTimeBreak`).
    /// - `Substitute` / `SubstituteBatch`: immediate without window cost during interval breaks;
    ///   queued in `pending_substitutions` during live play and executed on the next
    ///   `DeadBallPhase::ProcessingSubstitutions`.
    /// - `StartNextPeriod`: transitions out of `HalfTimeBreak` / `ExtraTimeBreak` / `PreMatch`.
    pub fn submit_command(&mut self, cmd: MatchCommandV4) -> Result<(), V4Error> {
        self.transact(|candidate| candidate.submit_command_inner(cmd))
    }

    /// Execute a public operation against a private candidate. The live session,
    /// including queue, RNG, ledger, diagnostics and event-delivery cursor,
    /// changes only after the whole operation succeeds. Batch size is bounded
    /// by the existing public API; do not clone the event history per DES event.
    fn transact<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T,V4Error>) -> Result<T,V4Error> {
        let mut candidate=self.clone();
        let result=operation(&mut candidate)?;
        crate::v4::transition::verify_commit_progress(
            self.state.des_clock.sim_time_ms,candidate.state.des_clock.sim_time_ms,
            self.event_count,candidate.event_count,
            self.completed_beats,candidate.completed_beats,
        )?;
        crate::v4::transition::verify_commit_structure(&candidate.state)?;
        *self=candidate;
        Ok(result)
    }

    fn submit_command_inner(&mut self, cmd: MatchCommandV4) -> Result<(), V4Error> {
        if self.is_full_time() {
            return Err(V4Error::SessionFinished);
        }
        let now_ms = self.state.des_clock.sim_time_ms;
        let command_id = self.alloc_command_id();
        let to_record = cmd.clone();

        match cmd {
            MatchCommandV4::StartNextPeriod => {
                self.begin_next_period_internal()?;
                self.command_timeline.push(AppliedCommandV4 {
                    command_id,
                    accepted_sim_time_ms: now_ms,
                    scheduled_effective_sim_time_ms: now_ms,
                    actual_effective_sim_time_ms: Some(now_ms),
                    applied_step_index: self.completed_beats,
                    status: CommandLifecycleStatus::Applied,
                    completed_beats: self.completed_beats,
                    game_second: (now_ms / 1000) as i32,
                    command: to_record,
                });
                Ok(())
            }
            MatchCommandV4::UpdateTactics { side, instructions } => {
                let is_break = self.stage.is_break() || matches!(self.stage, HalfStageV4::PreMatch);
                let is_dead_ball = self.state.dead_ball.is_some();
                let (avg_tw, avg_dec) = self.team_tactical_readiness(side);
                let old_instr = self.state.active_instructions(side).clone();
                let (start_delay_ms, _ramp_ms) = compute_tactical_delay_for_context_ms(
                    &old_instr,
                    &instructions,
                    is_dead_ball,
                    is_break,
                    avg_tw,
                    avg_dec,
                    &mut self.rng,
                );

                // Supersede any previously queued tactical command for the same side.
                for pending in &mut self.pending_tactical_commands {
                    if pending.side == side && pending.status == CommandLifecycleStatus::Scheduled {
                        pending.status = CommandLifecycleStatus::Superseded;
                        if let Some(entry) = self
                            .command_timeline
                            .iter_mut()
                            .rev()
                            .find(|c| c.command_id == pending.command_id)
                        {
                            entry.status = CommandLifecycleStatus::Superseded;
                        }
                    }
                }

                if start_delay_ms <= 0 {
                    self.apply_tactical_instructions_now(side, instructions, "INTERVAL_TACTICS_UPDATE");
                    self.command_timeline.push(AppliedCommandV4 {
                        command_id,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: now_ms,
                        actual_effective_sim_time_ms: Some(now_ms),
                        applied_step_index: self.completed_beats,
                        status: CommandLifecycleStatus::Applied,
                        completed_beats: self.completed_beats,
                        game_second: (now_ms / 1000) as i32,
                        command: to_record,
                    });
                } else {
                    let effective_ms = now_ms.saturating_add(start_delay_ms);
                    let cmd_idx = self.pending_tactical_commands.len() as u32;
                    self.pending_tactical_commands.push(PendingTacticalCommand {
                        command_id,
                        side,
                        origin: CommandOrigin::User,
                        instructions,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: effective_ms,
                        actual_effective_sim_time_ms: None,
                        status: CommandLifecycleStatus::Scheduled,
                    });
                    self.scheduler
                        .schedule(
                            effective_ms,
                            EventPriority::TacticalAndCommand,
                            CancellationScope::Persistent,
                            DesEventPayload::TacticalCommandEffective {
                                command_index: cmd_idx,
                                side,
                            },
                        )
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                    self.diagnostics.record_tactical_command_delayed();
                    self.command_timeline.push(AppliedCommandV4 {
                        command_id,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: effective_ms,
                        actual_effective_sim_time_ms: None,
                        applied_step_index: self.completed_beats,
                        status: CommandLifecycleStatus::Scheduled,
                        completed_beats: self.completed_beats,
                        game_second: (now_ms / 1000) as i32,
                        command: to_record,
                    });
                }
                Ok(())
            }
            MatchCommandV4::Substitute {
                side,
                player_out_id,
                player_in_id,
            } => self.handle_substitution_command(
                command_id,
                side,
                &[(player_out_id, player_in_id)],
                to_record,
            ),
            MatchCommandV4::SubstituteBatch { side, changes } => {
                let pairs = changes
                    .iter()
                    .map(|c| (c.player_out_id.clone(), c.player_in_id.clone()))
                    .collect::<Vec<_>>();
                self.handle_substitution_command(command_id, side, &pairs, to_record)
            }
        }
    }

    fn handle_substitution_command(
        &mut self,
        command_id: u64,
        side: TeamSideV4,
        changes: &[(String, String)],
        recorded_command: MatchCommandV4,
    ) -> Result<(), V4Error> {
        if changes.is_empty() {
            return Err(V4Error::InvalidInput("substitution group cannot be empty".into()));
        }
        let now_ms = self.state.des_clock.sim_time_ms;

        // Validate basic eligibility and quota upfront so invalid requests fail immediately.
        let period = self.state.des_clock.period;
        let max_subs = self.ruleset.max_substitutions_for_period(period);
        let max_windows = self.ruleset.max_in_play_sub_windows_for_period(period);
        let pending_for_side = self
            .pending_substitutions
            .iter()
            .filter(|s| s.side == side && s.status == CommandLifecycleStatus::QueuedForDeadBall)
            .count();
        let used_subs = self.state.team_state(side).substitutions_used as usize;
        if used_subs + pending_for_side + changes.len() > max_subs as usize {
            return Err(V4Error::InvalidInput("maximum substitutions reached".into()));
        }

        let at_interval = self.is_interval_substitution();
        if !at_interval
            && self.state.team_state(side).substitution_windows_used >= max_windows
            && pending_for_side == 0
        {
            return Err(V4Error::InvalidInput("maximum substitution windows reached".into()));
        }

        if at_interval {
            // Interval substitutions execute immediately without consuming an in-play window.
            let events = apply_manual_substitution_group(
                &mut self.state,
                &self.input,
                side,
                changes,
                true,
            )
            .map_err(|e| V4Error::InvalidInput(e.into()))?;
            let poss = self.possession_side;
            self.flush_play_events(events, poss);
            self.ensure_valid_carrier();
            self.command_timeline.push(AppliedCommandV4 {
                command_id,
                accepted_sim_time_ms: now_ms,
                scheduled_effective_sim_time_ms: now_ms,
                actual_effective_sim_time_ms: Some(now_ms),
                applied_step_index: self.completed_beats,
                status: CommandLifecycleStatus::Applied,
                completed_beats: self.completed_beats,
                game_second: (now_ms / 1000) as i32,
                command: recorded_command,
            });
            return Ok(());
        }

        // Validate players exist on pitch / bench and have no duplicates before queuing for next dead ball.
        let team_snap = match side {
            TeamSideV4::Home => &self.input.home_team,
            TeamSideV4::Away => &self.input.away_team,
        };
        let mut seen_out: Vec<&str> = Vec::with_capacity(changes.len());
        let mut seen_in: Vec<&str> = Vec::with_capacity(changes.len());
        for (out_id, in_id) in changes {
            if seen_out.contains(&out_id.as_str()) {
                return Err(V4Error::InvalidInput(format!(
                    "duplicate outgoing player '{out_id}' in substitution batch"
                )));
            }
            if seen_in.contains(&in_id.as_str()) {
                return Err(V4Error::InvalidInput(format!(
                    "duplicate incoming player '{in_id}' in substitution batch"
                )));
            }
            if self.pending_substitutions.iter().any(|s| {
                s.side == side
                    && s.status == CommandLifecycleStatus::QueuedForDeadBall
                    && s.player_out_id == *out_id
            }) {
                return Err(V4Error::InvalidInput(format!(
                    "outgoing player '{out_id}' is already queued for substitution"
                )));
            }
            if self.pending_substitutions.iter().any(|s| {
                s.side == side
                    && s.status == CommandLifecycleStatus::QueuedForDeadBall
                    && s.player_in_id == *in_id
            }) {
                return Err(V4Error::InvalidInput(format!(
                    "incoming player '{in_id}' is already queued for substitution"
                )));
            }
            seen_out.push(out_id.as_str());
            seen_in.push(in_id.as_str());

            let on_pitch = self
                .state
                .players
                .iter()
                .any(|p| p.side == side && p.player_id == *out_id && p.on_pitch && !p.sent_off);
            if !on_pitch {
                return Err(V4Error::InvalidInput(format!(
                    "outgoing player '{out_id}' is not active on pitch"
                )));
            }
            let on_bench = team_snap
                .bench
                .iter()
                .any(|p| p.player_id == *in_id && p.eligible && !p.injured)
                && !self.state.players.iter().any(|p| p.player_id == *in_id);
            if !on_bench {
                return Err(V4Error::InvalidInput(format!(
                    "incoming player '{in_id}' is not available on bench"
                )));
            }
        }

        for (out_id, in_id) in changes {
            self.pending_substitutions.push(PendingSubstitutionItem {
                command_id,
                side,
                origin: CommandOrigin::User,
                player_out_id: out_id.clone(),
                player_in_id: in_id.clone(),
                accepted_sim_time_ms: now_ms,
                status: CommandLifecycleStatus::QueuedForDeadBall,
            });
        }

        self.command_timeline.push(AppliedCommandV4 {
            command_id,
            accepted_sim_time_ms: now_ms,
            scheduled_effective_sim_time_ms: now_ms,
            actual_effective_sim_time_ms: None,
            applied_step_index: self.completed_beats,
            status: CommandLifecycleStatus::QueuedForDeadBall,
            completed_beats: self.completed_beats,
            game_second: (now_ms / 1000) as i32,
            command: recorded_command,
        });
        Ok(())
    }

    fn is_interval_substitution(&self) -> bool {
        if self.stage.is_break() {
            return self.ruleset.interval_subs_exempt_from_windows;
        }
        matches!(
            self.stage,
            HalfStageV4::InHalf {
                period: Period::SecondHalf | Period::ExtraTimeFirst | Period::ExtraTimeSecond,
                elapsed_in_half_ms: 0,
                ..
            }
        )
    }

    fn roster_snapshot(&self, side: TeamSideV4) -> TeamRosterStateV4 {
        let team = match side {
            TeamSideV4::Home => &self.input.home_team,
            TeamSideV4::Away => &self.input.away_team,
        };
        let on_pitch = self
            .state
            .players
            .iter()
            .filter(|p| p.side == side && p.on_pitch && !p.sent_off)
            .map(|p| p.player_id.clone())
            .collect::<Vec<_>>();
        let available_bench = team
            .bench
            .iter()
            .filter(|p| {
                p.eligible
                    && !p.injured
                    && !self
                        .state
                        .players
                        .iter()
                        .any(|runtime| runtime.player_id == p.player_id)
            })
            .map(|p| p.player_id.clone())
            .collect::<Vec<_>>();
        let counters = self.state.team_state(side);
        TeamRosterStateV4 {
            on_pitch_player_ids: on_pitch,
            available_bench_player_ids: available_bench,
            substitutions_used: counters.substitutions_used,
            substitution_windows_used: counters.substitution_windows_used,
            interval_substitution_allowed: self.is_interval_substitution(),
        }
    }

    fn session_stage_summary(&self) -> SessionStageV4 {
        match self.stage {
            HalfStageV4::PreMatch => SessionStageV4::PreMatch,
            HalfStageV4::InHalf { .. } => SessionStageV4::InHalf,
            HalfStageV4::HalfTimeBreak => SessionStageV4::HalfTimeBreak,
            HalfStageV4::ExtraTimeBreak { .. } => SessionStageV4::ExtraTimeBreak,
            HalfStageV4::PenaltyShootout => SessionStageV4::PenaltyShootout,
            HalfStageV4::FullTime => SessionStageV4::FullTime,
        }
    }

    fn dead_ball_state_label(&self) -> String {
        match &self.state.dead_ball {
            None => "LIVE_PLAY".to_string(),
            Some(db) => format!("{:?}::{:?}", db.kind, db.phase),
        }
    }

    fn pending_command_summaries(&self) -> Vec<PendingCommandSummaryV4> {
        let mut out = Vec::new();
        for tac in &self.pending_tactical_commands {
            if tac.status == CommandLifecycleStatus::Scheduled {
                out.push(PendingCommandSummaryV4 {
                    command_id: tac.command_id,
                    command_type: "UPDATE_TACTICS".to_string(),
                    side: tac.side,
                    accepted_sim_time_ms: tac.accepted_sim_time_ms,
                    scheduled_effective_sim_time_ms: Some(tac.scheduled_effective_sim_time_ms),
                    status: tac.status,
                });
            }
        }
        for sub in &self.pending_substitutions {
            if sub.status == CommandLifecycleStatus::QueuedForDeadBall {
                out.push(PendingCommandSummaryV4 {
                    command_id: sub.command_id,
                    command_type: "SUBSTITUTE".to_string(),
                    side: sub.side,
                    accepted_sim_time_ms: sub.accepted_sim_time_ms,
                    scheduled_effective_sim_time_ms: None,
                    status: sub.status,
                });
            }
        }
        out
    }

    fn build_step_result(&mut self, steps_executed: u32, stop_reason: &str) -> SessionStepResultV4 {
        let prev_len = self.last_delivered_event_index;
        let new_events = if self.retain_events && self.events.len() >= prev_len {
            self.events[prev_len..].to_vec()
        } else {
            Vec::new()
        };
        self.last_delivered_event_index = self.events.len();

        let dc = &self.state.des_clock;
        SessionStepResultV4 {
            ok: true,
            period: dc.period as i32,
            game_second: dc.game_second(),
            period_elapsed_second: dc.period_elapsed_second(),
            sim_time_ms: dc.sim_time_ms,
            period_elapsed_ms: dc.period_elapsed_ms,
            stage: self.session_stage_summary(),
            dead_ball_state: self.dead_ball_state_label(),
            stop_reason: stop_reason.to_string(),
            ball_in_play_ms: dc.ball_in_play_ms,
            ball_out_of_play_ms: dc.ball_out_of_play_ms,
            compensable_lost_ms: dc.compensable_lost_ms,
            announced_added_ms: dc.announced_added_ms,
            additional_added_ms: dc.additional_added_ms,
            effective_added_ms: dc.effective_added_ms(),
            steps_executed,
            state_generation: self.scheduler.live_play_generation(),
            completed_beats: self.completed_beats,
            home_goals: self.state.home.goals,
            away_goals: self.state.away.goals,
            is_full_time: self.is_full_time(),
            total_events: self.event_count,
            new_events,
            pending_commands: self.pending_command_summaries(),
            home_roster: self.roster_snapshot(TeamSideV4::Home),
            away_roster: self.roster_snapshot(TeamSideV4::Away),
        }
    }

    /// Starts the next match period from `PreMatch`, `HalfTimeBreak`, or `ExtraTimeBreak`.
    pub fn start_next_period(&mut self) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.start_next_period_inner())
    }

    fn start_next_period_inner(&mut self) -> Result<SessionStepResultV4, V4Error> {
        if self.is_full_time() {
            return Err(V4Error::SessionFinished);
        }
        self.begin_next_period_internal()?;
        Ok(self.build_step_result(1, "PERIOD_STARTED"))
    }

    /// Advances an interactive match by up to `max_steps` DES events, stopping early on
    /// `HalfTimeBreak`, `ExtraTimeBreak`, or `FullTime`.
    pub fn advance_steps(&mut self, max_steps: usize) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.advance_steps_inner(max_steps))
    }

    fn advance_steps_inner(&mut self, max_steps: usize) -> Result<SessionStepResultV4, V4Error> {
        let steps_limit = max_steps.clamp(1, 500);
        let mut executed = 0u32;
        let mut stop_reason = "MAX_STEPS_REACHED";

        for _ in 0..steps_limit {
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            self.step_one_des_event(false)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
            executed = executed.saturating_add(1);
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
        }

        Ok(self.build_step_result(executed, stop_reason))
    }

    /// Processes scheduled DES events up to the requested time, stopping
    /// at a period break, full time, or the step guard. This is an
    /// event-aligned API: the returned sim_time_ms can be less than the
    /// requested horizon if the next event lies beyond it.
    /// Polling must not invent a movement interval or consume policy RNG.
    /// TARGET_TIME_REACHED denotes an exhausted horizon, not necessarily
    /// equality of the actual event clock and the requested horizon.
    /// A stale (past) target is a no-op during an active period.
    pub fn advance_until_ms(
        &mut self,
        target_sim_time_ms: i64,
        max_steps_guard: usize,
    ) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.advance_until_ms_inner(target_sim_time_ms,max_steps_guard))
    }

    fn advance_until_ms_inner(
        &mut self,
        target_sim_time_ms: i64,
        max_steps_guard: usize,
    ) -> Result<SessionStepResultV4, V4Error> {
        if matches!(self.stage,HalfStageV4::InHalf{..})
            && target_sim_time_ms<self.state.des_clock.sim_time_ms {
            return Ok(self.build_step_result(0,"TARGET_TIME_REACHED"));
        }
        let guard = max_steps_guard.clamp(1, 10_000);
        let mut executed = 0u32;
        let mut stop_reason = "TARGET_TIME_REACHED";

        for _ in 0..guard {
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            if matches!(self.stage, HalfStageV4::PreMatch) {
                self.step_one_des_event(false)?;
                self.completed_beats = self.completed_beats.saturating_add(1);
                executed = executed.saturating_add(1);
                continue;
            }
            if let Some(next_ms) = self.scheduler.peek_next_valid_time_ms() {
                if next_ms > target_sim_time_ms {
                    stop_reason = "TARGET_TIME_REACHED";
                    break;
                }
            }
            self.step_one_des_event(false)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
            executed = executed.saturating_add(1);

            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            if self.state.des_clock.sim_time_ms >= target_sim_time_ms {
                stop_reason = "TARGET_TIME_REACHED";
                break;
            }
        }

        if executed as usize >= guard && stop_reason == "TARGET_TIME_REACHED" && self.state.des_clock.sim_time_ms < target_sim_time_ms {
            stop_reason = "MAX_STEPS_GUARD_REACHED";
        }

        Ok(self.build_step_result(executed, stop_reason))
    }

    /// Test-only explicit integration boundary. The dispatcher ignores
    /// a tactical command with no matching entry. This exercises the same
    /// clock and movement path as a real mid-interval tactical event,
    /// without changing instructions or injecting an event into production.
    #[cfg(test)]
    pub(super) fn schedule_test_clock_probe(&mut self,at:i64)->Result<(),V4Error> {
        self.scheduler.schedule(at,EventPriority::PeriodBoundary,CancellationScope::PeriodScoped,
            DesEventPayload::TacticalCommandEffective{command_index:u32::MAX,side:TeamSideV4::Home})
            .map(|_|()).map_err(|e|V4Error::InvalidClock(e.into()))
    }

    /// Runs the match to Full Time (used by `simulate_v4`, `FastBatch`, and interactive `finish`).
    /// Automatically advances through `HalfTimeBreak` / `ExtraTimeBreak` with half-time recovery.
    pub fn run_to_full_time(&mut self) -> Result<MatchResultV4, V4Error> {
        self.transact(|candidate| candidate.run_to_full_time_inner())
    }

    fn run_to_full_time_inner(&mut self) -> Result<MatchResultV4, V4Error> {
        if self.finished {
            return Err(V4Error::SessionFinished);
        }
        while !self.is_full_time() {
            if self.stage.is_break() {
                self.begin_next_period_internal()?;
                continue;
            }
            self.step_one_des_event(true)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
        }
        self.finished = true;
        Ok(self.build_result())
    }

    pub(super) fn begin_next_period_internal(&mut self) -> Result<(), V4Error> {
        match self.stage {
            HalfStageV4::PreMatch => {
                self.emit(
                    EventTypeV4::MatchStarted,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.begin_half(
                    Period::FirstHalf,
                    self.ruleset.half_duration_ms,
                    TeamSideV4::Home,
                )?;
                Ok(())
            }
            HalfStageV4::HalfTimeBreak => {
                self.apply_half_time_recovery();
                self.begin_half(
                    Period::SecondHalf,
                    self.ruleset.half_duration_ms,
                    TeamSideV4::Away,
                )?;
                Ok(())
            }
            HalfStageV4::ExtraTimeBreak { next_period } => {
                if next_period == Period::ExtraTimeFirst {
                    self.went_to_extra_time = true;
                    self.emit(
                        EventTypeV4::ExtraTimeStarted,
                        HighlightTier::Extended,
                        None,
                        None,
                        None,
                        None,
                        EventMetadataV4::default(),
                    );
                    self.begin_half(
                        Period::ExtraTimeFirst,
                        self.ruleset.extra_time_half_duration_ms,
                        TeamSideV4::Home,
                    )?;
                } else {
                    self.begin_half(
                        Period::ExtraTimeSecond,
                        self.ruleset.extra_time_half_duration_ms,
                        TeamSideV4::Away,
                    )?;
                }
                Ok(())
            }
            HalfStageV4::InHalf { .. } | HalfStageV4::PenaltyShootout => Ok(()),
            HalfStageV4::FullTime => Err(V4Error::SessionFinished),
        }
    }

    fn begin_half(
        &mut self,
        period: Period,
        regulation_ms: i64,
        kickoff_side: TeamSideV4,
    ) -> Result<(), V4Error> {
        self.scheduler.invalidate_period();
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.state.dead_ball = None;
        self.state.des_clock.begin_period(period, regulation_ms);
        self.state.sync_legacy_clock();
        self.reset_kickoff_positions(kickoff_side);

        self.carrier_idx = self.kickoff_carrier_index(kickoff_side);
        self.possession_side = kickoff_side;
        self.last_passer_idx = None;
        self.last_dt_ms = 0;

        let kicker_id = self.state.players[self.carrier_idx].player_id.clone();
        self.state.ball.pos = Vec3::center_spot();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(kickoff_side);
        self.state.ball.owner_player_id = Some(kicker_id.clone());
        self.state.ball.last_touch_side = None;
        self.state.ball.last_touch_player_id = None;

        self.emit(
            EventTypeV4::Kickoff,
            HighlightTier::None,
            None,
            Some(kickoff_side),
            Some(kicker_id.clone()),
            None,
            EventMetadataV4::default(),
        );

        self.stage = HalfStageV4::InHalf {
            period,
            regulation_duration_ms: regulation_ms,
            elapsed_in_half_ms: 0,
            added_time_announced: false,
            announced_added_ms: 0,
            additional_added_ms: 0,
        };

        let now_ms = self.state.des_clock.sim_time_ms;
        self.scheduler
            .advance_cursor_to(now_ms)
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        // Schedule period regulation boundary for fourth-official added-time announcement.
        let announce_ms = now_ms.saturating_add(regulation_ms);
        self.scheduler
            .schedule(
                announce_ms,
                EventPriority::PeriodBoundary,
                CancellationScope::PeriodScoped,
                DesEventPayload::AnnounceAddedTime { period },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        // Schedule periodic AI manager checks every 5 simulated minutes (300_000 ms).
        let ai_check_ms = now_ms.saturating_add(300_000);
        if ai_check_ms < announce_ms {
            self.scheduler
                .schedule(
                    ai_check_ms,
                    EventPriority::TacticalAndCommand,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::AiManagerCheck {
                        side: TeamSideV4::Home,
                    },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }

        // Schedule first live-play decision from kickoff.
        let first_dt = 1; // observation, not an extra decision delay
        self.scheduler
            .schedule(
                now_ms.saturating_add(first_dt),
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DecisionReady {
                    player_id: kicker_id,
                    side: kickoff_side,
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        Ok(())
    }

    pub(super) fn step_one_des_event(&mut self, auto_advance_breaks: bool) -> Result<(), V4Error> {
        match self.stage {
            HalfStageV4::PreMatch => {
                self.begin_next_period_internal()?;
                Ok(())
            }
            HalfStageV4::HalfTimeBreak | HalfStageV4::ExtraTimeBreak { .. } => {
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
                Ok(())
            }
            HalfStageV4::FullTime => Ok(()),
            HalfStageV4::PenaltyShootout => {
                self.run_penalty_shootout()?;
                self.finish_full_time();
                Ok(())
            }
            HalfStageV4::InHalf { period, .. } => {
                let next_ev = self
                    .scheduler
                    .pop_next_valid()
                    .map_err(|e| V4Error::InvalidClock(e.into()))?;

                let Some(ev) = next_ev else {
                    // Queue unexpectedly empty during half: schedule immediate period-end check.
                    self.complete_current_half(period, auto_advance_breaks)?;
                    return Ok(());
                };

                self.diagnostics.record_des_step();
                let prev_ms = self.state.des_clock.sim_time_ms;
                let dt_ms = ev.event_time_ms.saturating_sub(prev_ms).max(0);
                self.last_dt_ms = dt_ms;
                let is_ball_in_play = self.state.dead_ball.is_none();

                if dt_ms > 0 {
                    self.state
                        .des_clock
                        .advance_ms(dt_ms, is_ball_in_play)
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                    self.state.sync_legacy_clock();
                    self.sync_stage_from_clock();

                    // CRITICAL-08: Centralized world integration BEFORE dispatching any event.
                    let interrupted_by_injury = self.advance_world_for_dt(dt_ms, is_ball_in_play)?;
                    // Injury invalidates live-play actions, but must never discard a
                    // period boundary already popped from the scheduler. Losing the
                    // announcement/end check can leave a half running until watchdog.
                    if interrupted_by_injury
                        && !matches!(
                            &ev.payload,
                            DesEventPayload::AnnounceAddedTime { .. }
                                | DesEventPayload::CheckPeriodEnd { .. }
                        )
                    {
                        return Ok(());
                    }
                }

                match ev.payload {
                    DesEventPayload::AnnounceAddedTime { period: p } => {
                        self.handle_announce_added_time(p, auto_advance_breaks)?;
                    }
                    DesEventPayload::CheckPeriodEnd { period: p } => {
                        self.handle_check_period_end(p, auto_advance_breaks)?;
                    }
                    DesEventPayload::TacticalCommandEffective {
                        command_index,
                        side,
                    } => {
                        self.handle_tactical_command_effective(command_index as usize, side);
                    }
                    DesEventPayload::AiManagerCheck { .. } => {
                        self.handle_ai_manager_check(period)?;
                    }
                    DesEventPayload::DeadBallPhaseComplete { .. } => {
                        self.handle_dead_ball_phase_complete(dt_ms)?;
                    }
                    DesEventPayload::EarlyInteractionResolved { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_early_interaction_resolved()?;
                    }
                    DesEventPayload::BallInFlightArrival { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_ball_in_flight_arrival()?;
                    }
                    DesEventPayload::FirstTouchComplete { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_first_touch_complete()?;
                    }
                    DesEventPayload::ShotArrival { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_shot_arrival()?;
                    }
                    DesEventPayload::DecisionReady { .. } => {
                        if self.state.dead_ball.is_none() {
                            self.begin_player_decision(false)?;
                        }
                    }
                    DesEventPayload::ActionExecute { player_id, side } => {
                        if self.state.dead_ball.is_none() {
                            self.execute_player_decision(&player_id, side)?;
                        }
                    }
                    DesEventPayload::DribbleStep { carrier_id, side } => {
                        if self.state.dead_ball.is_none() {
                            self.finish_timed_dribble(&carrier_id, side)?;
                        }
                    }
                }

                Ok(())
            }
        }
    }

    fn sync_stage_from_clock(&mut self) {
        if let HalfStageV4::InHalf {
            period,
            regulation_duration_ms,
            ..
        } = self.stage
        {
            self.stage = HalfStageV4::InHalf {
                period,
                regulation_duration_ms,
                elapsed_in_half_ms: self.state.des_clock.period_elapsed_ms,
                added_time_announced: self.state.des_clock.added_time_locked,
                announced_added_ms: self.state.des_clock.announced_added_ms,
                additional_added_ms: self.state.des_clock.additional_added_ms,
            };
        }
    }

    /// CRITICAL-08: Single centralized continuous-time world integrator called strictly once per
    /// positive `dt_ms` advance before dispatching any scheduled DES event.
    fn advance_world_for_dt(&mut self, dt_ms: i64, is_ball_in_play: bool) -> Result<bool, V4Error> {
        if dt_ms <= 0 {
            return Ok(false);
        }
        let dt_sec = (dt_ms as f32) / 1000.0;
        let game_sec = self.state.des_clock.game_second();

        if is_ball_in_play {
            match self.possession_side {
                TeamSideV4::Home => {
                    self.home_possession_ms = self.home_possession_ms.saturating_add(dt_ms);
                    self.state.home.possession_seconds = (self.home_possession_ms / 1000) as u32;
                }
                TeamSideV4::Away => {
                    self.away_possession_ms = self.away_possession_ms.saturating_add(dt_ms);
                    self.state.away.possession_seconds = (self.away_possession_ms / 1000) as u32;
                }
            }
        }

        update_in_match_psychology(&mut self.state, &self.input, game_sec, dt_sec);

        if is_ball_in_play {
            let prev_positions: Vec<Vec2> = self.state.players.iter().map(|p| p.pos).collect();
            let defense=advance_off_ball_movement_with_report(
                &mut self.state,
                &self.input,
                self.possession_side,
                self.carrier_idx,
                dt_sec,
                &mut self.rng,
            );
            // Keep the predicted contact participant on the same motion
            // path used by the resolver, including across unrelated DES events.
            if let Some(motion) = self.pending_flight.as_ref().and_then(|f| f.pass_contact_motion.as_ref()) {
                let now = self.state.des_clock.sim_time_ms;
                if let Some((idx, p)) = self.state.players.iter_mut().enumerate()
                    .find(|(_, p)| p.player_id == motion.candidate.player_id && p.on_pitch && !p.sent_off) {
                    let old = prev_positions[idx];
                    let next = motion.position_at(now);
                    p.pos = next;
                    p.target_pos = motion.target;
                    p.vel = Vec2::new((next.x - old.x) / dt_sec, (next.y - old.y) / dt_sec);
                    if old.distance(next) > 0.05 {
                        p.facing_rad = (next.y - old.y).atan2(next.x - old.x);
                    }
                }
            }
            // A carry is a future, real movement interval, never an
            // additional displacement for an already integrated interval.
            if let Some(carry) = self.pending_carry.as_ref() {
                self.diagnostics.record_carry_interval(dt_ms,carry.is_hold);
                let idx = carry.carrier_idx;
                let p = &mut self.state.players[idx];
                if p.on_pitch && !p.sent_off && p.player_id == carry.motion.candidate.player_id {
                    let old = prev_positions[idx];
                    let next = carry.motion.position_at(self.state.des_clock.sim_time_ms.min(carry.resolves_at_ms));
                    p.pos = next;
                    p.target_pos = carry.motion.target;
                    p.vel = Vec2::new((next.x-old.x)/dt_sec, (next.y-old.y)/dt_sec);
                    if old.distance(next)>0.05 { p.facing_rad=(next.y-old.y).atan2(next.x-old.x); }
                    self.state.ball.pos=Vec3::ground(next.x,next.y);
                    self.state.ball.vel=Vec3::new(p.vel.x,p.vel.y,0.0);
                }
                if let Some(chase)=carry.challenger.as_ref() {
                    if let Some((di,def))=self.state.players.iter_mut().enumerate().find(|(_,p)|
                        p.player_id==chase.candidate.player_id && p.on_pitch && !p.sent_off) {
                        let old=prev_positions[di];
                        let next=chase.position_at(self.state.des_clock.sim_time_ms.min(carry.resolves_at_ms));
                        def.pos=next; def.target_pos=chase.target;
                        def.vel=Vec2::new((next.x-old.x)/dt_sec,(next.y-old.y)/dt_sec);
                        if old.distance(next)>0.05 {def.facing_rad=(next.y-old.y).atan2(next.x-old.x);}
                    }
                }
                if carry.shielding {
                    // Face away from the actual challenger so the body can
                    // protect the ball; do not change the ball position here.
                    if let Some(chase)=carry.challenger.as_ref() {
                        if let Some(def)=self.state.players.iter().find(|p|p.player_id==chase.candidate.player_id) {
                            let pos=self.state.players[idx].pos;
                            let angle=(pos.y-def.pos.y).atan2(pos.x-def.pos.x);
                            self.state.players[idx].facing_rad=angle;
                        }
                    }
                }
            }
            // Preparation contests use the same committed contact path
            // as carries. No extra movement or fatigue at resolution.
            if let Some(chase)=self.pending_decision.as_ref().and_then(|p|p.challenger.as_ref()) {
                if let Some((di,def))=self.state.players.iter_mut().enumerate()
                    .find(|(_,p)|p.player_id==chase.candidate.player_id && p.on_pitch && !p.sent_off) {
                    let old=prev_positions[di];
                    let next=chase.position_at(self.state.des_clock.sim_time_ms);
                    def.pos=next;
                    def.target_pos=chase.target;
                    def.vel=Vec2::new((next.x-old.x)/dt_sec,(next.y-old.y)/dt_sec);
                    if old.distance(next)>0.05 {def.facing_rad=(next.y-old.y).atan2(next.x-old.x);}
                }
            }
            for (idx, p) in self.state.players.iter_mut().enumerate() {
                if let Some(prev) = prev_positions.get(idx) {
                    p.last_moved_m = prev.distance(p.pos);
                }
            }
            // Measure the realized movement after physical contact path
            // overrides, rather than the preliminary tactical displacement.
            // A selected carry challenger is an independent physical race;
            // exclude that player's interval from tactical-role distance.
            for a in &defense {
                let p=&self.state.players[a.idx];
                let overridden=self.pending_carry.as_ref().and_then(|c|c.challenger.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id)
                    || self.pending_flight.as_ref().and_then(|f|f.pass_contact_motion.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id)
                    || self.pending_decision.as_ref().and_then(|p|p.challenger.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id);
                if !overridden {
                    self.diagnostics.record_defensive_interval(a.role,dt_ms,p.last_moved_m);
                }
            }
        } else {
            // NHIỆM VỤ 6: Simulate purposeful low-intensity dead-ball movement (celebration,
            // returning to own half for kickoff, entering box for corner/FK, forming wall).
            let db_ctx = self.state.dead_ball.as_ref().map(|db| (db.kind, db.phase, db.restart_side, db.restart_spot));
            for p in &mut self.state.players {
                if !p.on_pitch || p.sent_off {
                    p.last_moved_m = 0.0;
                    continue;
                }
                let prev = p.pos;
                let (target, walk_speed_mps) = if let Some((kind, phase, r_side, spot)) = db_ctx {
                    match (kind, phase) {
                        (DeadBallKind::GoalCelebration, DeadBallPhase::Celebrating) => {
                            if p.side == r_side.opposite() {
                                // Scoring team jogs toward corner/celebration zone
                                (Vec2::new(spot.x.clamp(12.0, 93.0), if spot.y < 34.0 { 6.0 } else { 62.0 }), 2.4f32)
                            } else {
                                // Conceding team walks back toward own half
                                let own_x = if p.side == TeamSideV4::Home { 36.0 } else { 69.0 };
                                (Vec2::new(own_x, p.pos.y.clamp(12.0, 56.0)), 1.4f32)
                            }
                        }
                        (_, DeadBallPhase::ReturningToHalf | DeadBallPhase::KickoffSetup) => {
                            let own_x = if p.side == TeamSideV4::Home {
                                p.pos.x.min(50.0).max(8.0)
                            } else {
                                p.pos.x.max(55.0).min(97.0)
                            };
                            (Vec2::new(own_x, p.pos.y.clamp(8.0, 60.0)), 1.8f32)
                        }
                        (DeadBallKind::Corner | DeadBallKind::DirectFreeKick | DeadBallKind::IndirectFreeKick, DeadBallPhase::SettingBall) => {
                            let box_x = if r_side == TeamSideV4::Home { 93.0 } else { 12.0 };
                            (Vec2::new((p.pos.x * 0.6 + box_x * 0.4).clamp(6.0, 99.0), (p.pos.y * 0.7 + 34.0 * 0.3).clamp(8.0, 60.0)), 2.1f32)
                        }
                        _ => (Vec2::new((p.pos.x * 0.85 + spot.x * 0.15).clamp(4.0, 101.0), (p.pos.y * 0.85 + spot.y * 0.15).clamp(4.0, 64.0)), 1.3f32),
                    }
                } else {
                    (prev, 0.0f32)
                };
                let max_step = walk_speed_mps * dt_sec;
                let delta = Vec2::new(target.x - prev.x, target.y - prev.y);
                let dist = (delta.x * delta.x + delta.y * delta.y).sqrt();
                if dist > 1e-3 && max_step > 0.0 {
                    let frac = (max_step / dist).min(1.0);
                    p.pos = Vec2::new(
                        (prev.x + delta.x * frac).clamp(1.0, PITCH_LENGTH_M - 1.0),
                        (prev.y + delta.y * frac).clamp(1.0, 67.0),
                    );
                }
                p.last_moved_m = prev.distance(p.pos);
            }
        }

        if let Some(flight) = &self.pending_flight {
            let now = self.state.des_clock.sim_time_ms;
            self.state.ball.pos = flight.trajectory.position_at_ms(now).clamp_to_pitch();
            self.state.ball.vel = flight.trajectory.velocity_at_ms(now);
            if matches!(&flight.action, FlightActionV4::LooseRecovery { .. }) && self.state.ball.pos.z <= 0.0 {
                self.state.ball.vel.z=0.0;
            }
        }
        self.update_interval_fatigue(dt_sec, self.possession_side, !is_ball_in_play);

        if is_ball_in_play {
            let avg_wr = self.team_avg_work_rate(self.possession_side);
            let ball_pos_now = self.state.ball.pos;
            let phase_events = update_tactical_phases(
                &mut self.state,
                self.possession_side,
                self.possession_side,
                ball_pos_now,
                dt_ms,
                avg_wr,
            );
            if !phase_events.is_empty() {
                let poss = self.possession_side;
                self.flush_play_events(phase_events, poss);
            }

            if let Some(muscle_inj) =
                check_muscle_injury(&mut self.state, &self.input, dt_sec, &mut self.rng)
            {
                let inj_pos = muscle_inj.ball_pos;
                let poss = self.possession_side;
                self.flush_play_events(vec![muscle_inj], poss);
                let inj_ai = evaluate_ai_managers(&mut self.state, &self.input);
                if !inj_ai.is_empty() {
                    self.flush_play_events(inj_ai, poss);
                    self.ensure_valid_carrier();
                }
                self.open_dead_ball(DeadBallKind::InjuryStoppage, poss, inj_pos, true, false)?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Proportionally credits elapsed compensable stoppage time if a dead-ball phase is active
    /// when added time is announced or period-end is checked, preventing lost stoppage without double counting.
    fn credit_ongoing_dead_ball_compensable_ms(&mut self) {
        let now_ms = self.state.des_clock.sim_time_ms;
        let credit = if let Some(db) = self.state.dead_ball.as_mut() {
            if db.phase_compensable_ms > 0 && db.phase_ends_at_ms > db.phase_started_at_ms {
                let total = (db.phase_ends_at_ms - db.phase_started_at_ms).max(1);
                let elapsed = now_ms.saturating_sub(db.phase_started_at_ms).clamp(0, total);
                let accrued = ((db.phase_compensable_ms as i128 * elapsed as i128) / total as i128) as i64;
                let take = accrued.saturating_sub(db.phase_compensable_credited_ms).max(0);
                db.phase_compensable_credited_ms = db.phase_compensable_credited_ms.saturating_add(take);
                take
            } else {
                0
            }
        } else {
            0
        };
        if credit > 0 {
            self.state.des_clock.record_compensable_lost_ms(credit);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();
        }
    }

    fn handle_announce_added_time(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        self.credit_ongoing_dead_ball_compensable_ms();
        if !self.state.des_clock.added_time_locked {
            let rounded_ms = self
                .ruleset
                .round_announced_added_time_ms(self.state.des_clock.compensable_lost_ms);
            let announced_ms = self.state.des_clock.lock_announced_added_ms(rounded_ms);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();

            if announced_ms > 0 {
                let mut meta = EventMetadataV4::default();
                meta.extra
                    .insert("addedSeconds".into(), (announced_ms / 1000).to_string());
                meta.extra
                    .insert("addedMs".into(), announced_ms.to_string());
                self.emit(
                    EventTypeV4::AddedTimeAnnounced,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    meta,
                );
            }
        }

        let target_ms = self
            .state
            .des_clock
            .period_started_at_ms
            .saturating_add(self.state.des_clock.period_target_ms());
        let now_ms = self.state.des_clock.sim_time_ms;
        if target_ms <= now_ms {
            self.handle_check_period_end(period, auto_advance_breaks)?;
        } else {
            self.scheduler
                .schedule(
                    target_ms,
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }
        Ok(())
    }

    fn handle_check_period_end(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        self.credit_ongoing_dead_ball_compensable_ms();
        // Dynamically read `effective_added_ms()` (no static added_target lock!).
        let dynamic_target_elapsed = self.state.des_clock.period_target_ms();
        let elapsed_now = self.state.des_clock.period_elapsed_ms;
        let now_ms = self.state.des_clock.sim_time_ms;

        if elapsed_now < dynamic_target_elapsed {
            let next_check_ms = self
                .state
                .des_clock
                .period_started_at_ms
                .saturating_add(dynamic_target_elapsed);
            self.scheduler
                .schedule(
                    next_check_ms.max(now_ms.saturating_add(1)),
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
            return Ok(());
        }

        // Law 7: never blow whistle during an awarded penalty kick, or an active dangerous attack
        // under `HalfEndPolicy::AllowAttackCompletion` (bounded by watchdog).
        let penalty_active = matches!(
            self.state.dead_ball.as_ref().map(|d| d.kind),
            Some(DeadBallKind::PenaltyKick)
        );
        let dangerous_attack = self.ruleset.half_end_policy == HalfEndPolicy::AllowAttackCompletion
            && self.state.dead_ball.is_none()
            && is_in_attacking_penalty_box(self.possession_side, self.state.ball.pos.xy())
            && elapsed_now < dynamic_target_elapsed.saturating_add(15_000);

        if penalty_active || dangerous_attack {
            let defer_ms = now_ms.saturating_add(3_000);
            self.scheduler
                .schedule(
                    defer_ms,
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
            return Ok(());
        }

        self.complete_current_half(period, auto_advance_breaks)
    }

    fn handle_tactical_command_effective(&mut self, command_index: usize, side: TeamSideV4) {
        let Some(entry) = self.pending_tactical_commands.get_mut(command_index) else {
            return;
        };
        if entry.status != CommandLifecycleStatus::Scheduled || entry.side != side {
            return;
        }
        let now_ms = self.state.des_clock.sim_time_ms;
        let cmd_id = entry.command_id;
        let instructions = entry.instructions.clone();
        entry.status = CommandLifecycleStatus::Applied;
        entry.actual_effective_sim_time_ms = Some(now_ms);

        for rec in &mut self.command_timeline {
            if rec.command_id == cmd_id {
                rec.status = CommandLifecycleStatus::Applied;
                rec.actual_effective_sim_time_ms = Some(now_ms);
            }
        }

        self.apply_tactical_instructions_now(side, instructions, "USER_TACTICS_UPDATE");
    }

    fn apply_tactical_instructions_now(
        &mut self,
        side: TeamSideV4,
        instructions: TeamInstructionsV4,
        cause: &str,
    ) {
        let high_line = instructions.line_height >= 68;
        let wasting = instructions.time_wasting >= 55;
        *self.state.active_instructions_mut(side) = instructions;
        let ball_pos = self.state.ball.pos;
        self.emit(
            EventTypeV4::TacticalPhaseChanged,
            HighlightTier::None,
            None,
            Some(side),
            None,
            None,
            EventMetadataV4 {
                high_defensive_line: if high_line { Some(true) } else { None },
                time_wasting: if wasting { Some(true) } else { None },
                cause: Some(cause.into()),
                ..EventMetadataV4::default()
            },
        );
        self.state.ball.pos = ball_pos;
    }

    fn handle_ai_manager_check(&mut self, period: Period) -> Result<(), V4Error> {
        let now_ms = self.state.des_clock.sim_time_ms;
        self.state.last_ai_eval_second = (now_ms / 1000) as i32;

        // Only apply AI tactical/substitution decisions when not mid-celebration;
        // AI substitutions during live play are routed cleanly through dead-ball or immediate AI hook.
        let ai_events = evaluate_ai_managers(&mut self.state, &self.input);
        if !ai_events.is_empty() {
            let poss = self.possession_side;
            self.flush_play_events(ai_events, poss);
            self.ensure_valid_carrier();
        }

        // Schedule next periodic AI manager evaluation (+300_000 ms) if still within period.
        let period_end_est = self
            .state
            .des_clock
            .period_started_at_ms
            .saturating_add(self.state.des_clock.period_target_ms());
        let next_ai_ms = now_ms.saturating_add(300_000);
        if next_ai_ms < period_end_est {
            let _ = self.scheduler.schedule(
                next_ai_ms,
                EventPriority::TacticalAndCommand,
                CancellationScope::PeriodScoped,
                DesEventPayload::AiManagerCheck {
                    side: TeamSideV4::Home,
                },
            );
        }
        let _ = period;
        Ok(())
    }

    fn open_dead_ball(
        &mut self,
        kind: DeadBallKind,
        restart_side: TeamSideV4,
        restart_spot: Vec3,
        has_injury: bool,
        has_discipline_review: bool,
    ) -> Result<(), V4Error> {
        self.scheduler.invalidate_live_play();

        let now_ms = self.state.des_clock.sim_time_ms;
        let has_pending_subs = self.has_pending_substitutions();
        let initial_phase = initial_dead_ball_phase(
            kind,
            has_injury,
            has_discipline_review,
            has_pending_subs,
        );
        let pending_count = self.count_pending_substitutions();
        let goal_diff = self.goal_diff_for(restart_side);
        let tw = self.state.active_instructions(restart_side).time_wasting;
        let ref_strict = self.input.referee_profile.strictness;

        let (phase_dur_ms, comp_ms) = estimate_dead_ball_phase_ms(
            &self.input.timing_model,
            kind,
            initial_phase,
            goal_diff,
            now_ms,
            tw,
            ref_strict,
            pending_count,
            &mut self.rng,
        );

        let db = DeadBallState::new(
            kind,
            initial_phase,
            restart_side,
            restart_spot.clamp_to_pitch(),
            now_ms,
            phase_dur_ms,
            comp_ms,
        );
        let phase_end_ms = db.phase_ends_at_ms;
        self.state.ball.pos = db.restart_spot;
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::DeadBall;
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.state.dead_ball = Some(db);

        self.scheduler
            .schedule(
                phase_end_ms,
                EventPriority::DeadBallAndSubstitution,
                CancellationScope::PeriodScoped,
                DesEventPayload::DeadBallPhaseComplete {
                    phase_tag: format!("{:?}::{:?}", kind, initial_phase),
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        Ok(())
    }

    fn handle_dead_ball_phase_complete(&mut self, dt_ms: i64) -> Result<(), V4Error> {
        let Some(db_snapshot) = self.state.dead_ball.clone() else {
            return Ok(());
        };
        let _ = dt_ms;
        let now_ms = self.state.des_clock.sim_time_ms;

        // Record compensable lost time accrued in this completed phase now that
        // `advance_ms(dt_ms, false)` has added the phase duration to `ball_out_of_play_ms`.
        let prev_comp_ms = db_snapshot.phase_compensable_ms.saturating_sub(db_snapshot.phase_compensable_credited_ms);
        if prev_comp_ms > 0 {
            self.state.des_clock.record_compensable_lost_ms(prev_comp_ms);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();
        }

        // Execute queued substitutions upon completing `ProcessingSubstitutions`.
        if db_snapshot.phase == DeadBallPhase::ProcessingSubstitutions {
            self.flush_pending_substitutions();
        }

        let has_pending_subs = self.has_pending_substitutions();
        let next_phase = next_dead_ball_phase(&db_snapshot, has_pending_subs, false, false);

        if let Some(next_p) = next_phase {
            let pending_count = self.count_pending_substitutions();
            let goal_diff = self.goal_diff_for(db_snapshot.restart_side);
            let tw = self
                .state
                .active_instructions(db_snapshot.restart_side)
                .time_wasting;
            let ref_strict = self.input.referee_profile.strictness;

            let (next_dur_ms, next_comp_ms) = estimate_dead_ball_phase_ms(
                &self.input.timing_model,
                db_snapshot.kind,
                next_p,
                goal_diff,
                now_ms,
                tw,
                ref_strict,
                pending_count,
                &mut self.rng,
            );

            if let Some(db) = self.state.dead_ball.as_mut() {
                db.advance_to(next_p, now_ms, next_dur_ms, next_comp_ms);
                let phase_end_ms = db.phase_ends_at_ms;
                let kind = db.kind;
                self.scheduler
                    .schedule(
                        phase_end_ms,
                        EventPriority::DeadBallAndSubstitution,
                        CancellationScope::PeriodScoped,
                        DesEventPayload::DeadBallPhaseComplete {
                            phase_tag: format!("{:?}::{:?}", kind, next_p),
                        },
                    )
                    .map_err(|e| V4Error::InvalidClock(e.into()))?;
            }
            return Ok(());
        }

        // Dead ball completed! Record total duration diagnostics & compensable lost time
        // for the initial phase if not yet counted.
        let total_db_ms = now_ms.saturating_sub(db_snapshot.started_at_ms).max(0);
        if let Some(diag_kind) = map_dead_ball_diag_kind(db_snapshot.kind) {
            self.diagnostics
                .record_dead_ball_duration(diag_kind, total_db_ms);
        }
        // Clear dead ball and execute the actual restart.
        self.state.dead_ball = None;
        self.execute_dead_ball_restart(db_snapshot)?;
        Ok(())
    }

    fn execute_dead_ball_restart(&mut self, db: DeadBallState) -> Result<(), V4Error> {
        let now_ms = self.state.des_clock.sim_time_ms;
        let restart_side = db.restart_side;
        let def_side = restart_side.opposite();

        match db.kind {
            DeadBallKind::GoalCelebration | DeadBallKind::Kickoff => {
                let kicker_idx = self.restart_after_goal_kickoff(restart_side);
                self.schedule_next_decision(kicker_idx, restart_side, now_ms, false)?;
            }
            DeadBallKind::Corner => {
                let corner_out =
                    execute_corner_v4(&mut self.state, &self.input, restart_side, &mut self.rng);
                let scored = corner_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(corner_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = corner_out.next_side;
                    self.carrier_idx = corner_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    self.schedule_next_decision(self.carrier_idx, self.possession_side, now_ms, false)?;
                }
            }
            DeadBallKind::PenaltyKick => {
                let sp_out = execute_penalty_kick_v4_with_rules(
                    &mut self.state,
                    &self.input,
                    restart_side,
                    self.ruleset.allow_penalty_rebound,
                    &mut self.rng,
                );
                let scored = sp_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(sp_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = sp_out.next_side;
                    self.carrier_idx = sp_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    let one_touch = self.state.ball.control == BallControlState::Loose;
                    self.schedule_next_decision(
                        self.carrier_idx,
                        self.possession_side,
                        now_ms,
                        one_touch,
                    )?;
                }
            }
            DeadBallKind::DirectFreeKick
            | DeadBallKind::IndirectFreeKick
            | DeadBallKind::QuickFreeKick => {
                let sp_out = execute_free_kick_v4(
                    &mut self.state,
                    &self.input,
                    restart_side,
                    db.restart_spot.xy(),
                    &mut self.rng,
                );
                let scored = sp_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(sp_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = sp_out.next_side;
                    self.carrier_idx = sp_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    self.schedule_next_decision(self.carrier_idx, self.possession_side, now_ms, false)?;
                }
            }
            DeadBallKind::ThrowIn
            | DeadBallKind::GoalKick
            | DeadBallKind::Offside
            | DeadBallKind::InjuryStoppage
            | DeadBallKind::SubstitutionWindow => {
                self.possession_side = restart_side;
                self.carrier_idx = if db.kind == DeadBallKind::GoalKick {
                    self.find_goalkeeper_index(restart_side)
                } else {
                    self.pick_outfield_index(restart_side)
                };
                self.ensure_valid_carrier();
                let cid = self.state.players[self.carrier_idx].player_id.clone();
                self.state.ball.pos = db.restart_spot.clamp_to_pitch();
                self.state.ball.control = BallControlState::Controlled;
                self.state.ball.owner_side = Some(restart_side);
                self.state.ball.owner_player_id = Some(cid);
                self.schedule_next_decision(self.carrier_idx, restart_side, now_ms, false)?;
            }
        }
        Ok(())
    }

    fn flush_pending_substitutions(&mut self) {
        let now_ms = self.state.des_clock.sim_time_ms;
        for side in [TeamSideV4::Home, TeamSideV4::Away] {
            let mut pairs: Vec<(String, String)> = Vec::new();
            let mut cmd_ids: Vec<u64> = Vec::new();
            for item in &self.pending_substitutions {
                if item.side == side && item.status == CommandLifecycleStatus::QueuedForDeadBall {
                    pairs.push((item.player_out_id.clone(), item.player_in_id.clone()));
                    cmd_ids.push(item.command_id);
                }
            }
            if pairs.is_empty() {
                continue;
            }
            match apply_manual_substitution_group(&mut self.state, &self.input, side, &pairs, false)
            {
                Ok(events) => {
                    let poss = self.possession_side;
                    self.flush_play_events(events, poss);
                    self.ensure_valid_carrier();
                    for item in &mut self.pending_substitutions {
                        if item.side == side
                            && item.status == CommandLifecycleStatus::QueuedForDeadBall
                        {
                            item.status = CommandLifecycleStatus::Applied;
                        }
                    }
                    for rec in &mut self.command_timeline {
                        if cmd_ids.contains(&rec.command_id) {
                            rec.status = CommandLifecycleStatus::Applied;
                            rec.actual_effective_sim_time_ms = Some(now_ms);
                        }
                    }
                }
                Err(_) => {
                    for item in &mut self.pending_substitutions {
                        if item.side == side
                            && item.status == CommandLifecycleStatus::QueuedForDeadBall
                        {
                            item.status = CommandLifecycleStatus::Rejected;
                        }
                    }
                    for rec in &mut self.command_timeline {
                        if cmd_ids.contains(&rec.command_id) {
                            rec.status = CommandLifecycleStatus::Rejected;
                        }
                    }
                }
            }
        }
    }

    /// PI chooses once from a read-only observation; DES owns the future execution.
    fn begin_player_decision(&mut self, fresh_receipt: bool) -> Result<(), V4Error> {
        if self.pending_decision.is_some() || self.pending_flight.is_some() || self.pending_carry.is_some()
            || self.state.dead_ball.is_some() { return Ok(()); }
        self.ensure_valid_carrier();
        let idx = self.carrier_idx;
        let p = &self.state.players[idx];
        let side = p.side;
        let id = p.player_id.clone();
        let pos = p.pos;
        self.state.ball.pos = Vec3::ground(pos.x,pos.y).clamp_to_pitch();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(side);
        self.state.ball.owner_player_id = Some(id.clone());
        self.possession_side = side;
        self.diagnostics.observe_controlled_ball(side,pos);
        let observed:Vec<_>=self.state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off)
            .map(|(idx,p)|crate::player_intelligence::ObservedPlayer{idx,side:p.side,pos:p.pos,is_gk:p.is_goalkeeper}).collect();
        let receiving_line=crate::v4::ball_play::offside_reference_progress_m(&self.state,side,pos);
        let open_outlets=crate::player_intelligence::count_open_outlets(side,idx,pos,&observed,receiving_line);
        self.diagnostics.record_open_outlets(open_outlets);
        let ctx = evaluate_local_context(&self.state,&self.input,side,pos);
        let mut plan = perceive_evaluate_and_decide_des(&self.state,&self.input,
            &self.input.timing_model,idx,ctx,&mut self.rng);
        self.diagnostics.record_outlet_choice(open_outlets,matches!(plan.action,PlannedActionV4::HoldAndRecycle));
        // A one-touch intention is a property of this receiver's action,
        // not of the passer who delivered the previous ball.
        if !fresh_receipt { plan.is_one_touch = false; }
        let settle = if fresh_receipt && !plan.is_one_touch {
            self.input.timing_model.first_touch_settle_ms.sample_triangular(&mut self.rng,1.0).max(150)
        } else { 0 };
        let now = self.state.des_clock.sim_time_ms;
        let release_at_ms = now.saturating_add(plan.decision_delay_ms.max(1)).saturating_add(settle);
        let (at,challenger)=self.preparation_contest(idx,release_at_ms);
        self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
            DesEventPayload::ActionExecute{player_id:id.clone(),side})
            .map_err(|e|V4Error::InvalidClock(e.into()))?;
        let observed_pass_risk=crate::player_intelligence::observe_pass_risk(&self.state,&self.input,idx,ctx,plan.action);
        self.pending_decision=Some(PendingDecisionV4{player_id:id,side,started_at_ms:now,executes_at_ms:at,
            plan,release_at_ms,challenger,observed_pass_risk,final_aim_committed:false});
        Ok(())
    }

    /// A defender finishing a tackle must recover before committing to
    /// another tackle. This does not prevent movement, screening or
    /// pass interception; it is a separate action readiness clock.
    fn tackle_candidates(&self,side:TeamSideV4,end:i64)->Vec<crate::v4::interaction::PassReachCandidate> {
        let now=self.state.des_clock.sim_time_ms;
        self.state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off
            && !p.is_goalkeeper && p.side!=side)
            .filter_map(|(i,p)|{
                let ready=self.tackle_ready_at.get(&p.player_id).copied().unwrap_or(now);
                let remaining=ready.saturating_sub(now).max(0);
                let mut candidate=pass_reach_candidate(&self.state,i,false);
                // A recovering player cannot actively chase/attempt a
                // second tackle until recovery plus the new reaction.
                candidate.reaction_ms=candidate.reaction_ms.saturating_add(remaining);
                if now.saturating_add(candidate.reaction_ms)>=end {None}else{Some(candidate)}
            }).collect()
    }

    fn mark_tackle_recovery(&mut self,idx:usize) {
        let p=&self.state.players[idx];
        let attrs=lookup_attrs(&self.input,p);
        let agility=normalize_attr(attrs.physical.agility);
        let balance=normalize_attr(attrs.physical.balance);
        // Explicit follow-through + regain balance, in milliseconds.
        // This kinematic prior is independent of score and population KPIs.
        let duration=(450.0+450.0*(1.0-agility)+250.0*(1.0-balance)).round() as i64;
        self.tackle_ready_at.insert(p.player_id.clone(),self.state.des_clock.sim_time_ms.saturating_add(duration));
    }

    /// Reachable candidates first, then the PI's tackle-vs-contain
    /// choice. Evaluate each defender independently; only committed
    /// candidates compete for the earliest physical event.
    fn choose_committed_tackle(&mut self,idx:usize,motion:&PassContactMotion,end:i64)
        ->Option<(usize,i64,PassContactMotion)> {
        let candidates=self.tackle_candidates(motion.candidate.side,end);
        let mut best:Option<(usize,i64,PassContactMotion)>=None;
        for candidate in &candidates {
            let Some((_,at,chase))=find_carry_contact(motion,std::slice::from_ref(candidate),end) else {continue;};
            let Some(def_idx)=self.state.players.iter().position(|p|p.player_id==candidate.player_id) else {continue;};
            let elapsed=at.saturating_sub(motion.started_at_ms) as f32/1000.0;
            let evaluation=crate::player_intelligence::evaluate_tackle(&self.state,idx,def_idx,elapsed);
            let commits=evaluation.commits();
            self.diagnostics.record_tackle_choice(commits,evaluation.access,
                evaluation.clean_win_probability,evaluation.foul_probability);
            if !commits {continue;}
            if best.as_ref().map_or(true,|(_,old_time,old_motion)|
                at<*old_time || (at==*old_time && candidate.player_id<old_motion.candidate.player_id)) {
                best=Some((def_idx,at,chase));
            }
        }
        best
    }

    fn preparation_contest(&mut self,idx:usize,release:i64)->(i64,Option<PassContactMotion>) {
        let now=self.state.des_clock.sim_time_ms;
        let mut carrier=pass_reach_candidate(&self.state,idx,false);
        carrier.speed_mps=0.0;
        carrier.reaction_ms=0;
        carrier.velocity=Vec2::default();
        let pos=carrier.pos;
        let motion=PassContactMotion{candidate:carrier,target:pos,started_at_ms:now};
        match self.choose_committed_tackle(idx,&motion,release) {
            // Exact tie goes to the already prepared release.
            Some((_,at,chase)) if at<release=>(at,Some(chase)),
            _=>(release,None),
        }
    }

    fn execute_player_decision(&mut self, player_id: &str, side: TeamSideV4) -> Result<(), V4Error> {
        let Some(mut pending) = self.pending_decision.take() else { return Ok(()); };
        let now_ms=self.state.des_clock.sim_time_ms;
        if pending.player_id!=player_id || pending.side!=side || pending.executes_at_ms!=now_ms {
            return Err(V4Error::InvalidClock("PI action execution does not match pending intention".into()));
        }
        self.ensure_valid_carrier();
        if self.state.players[self.carrier_idx].player_id!=player_id || self.possession_side!=side {
            return self.begin_player_decision(false);
        }
        let idx=self.carrier_idx;
        let pos=self.state.players[idx].pos;
        self.state.ball.pos=Vec3::ground(pos.x,pos.y).clamp_to_pitch();
        if let Some(chase)=pending.challenger.take() {
            let contact=self.state.players.iter().position(|p|p.player_id==chase.candidate.player_id
                && p.on_pitch && !p.sent_off && p.side!=side
                && p.pos.distance(pos)<=chase.candidate.reach_m+0.01);
            if let Some(def_idx)=contact {
                self.mark_tackle_recovery(def_idx);
                let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
                let mut outcome=self.resolve_audited_carry_contact(idx,Some(def_idx),ctx,
                    pending.plan.carry.map_or(false,|c|c.shielding));
                let foul=outcome.events.iter().any(|e|e.kind==EventTypeV4::Foul);
                let card=outcome.events.iter().any(|e|matches!(e.kind,EventTypeV4::YellowCard|EventTypeV4::RedCard));
                let injury=outcome.events.iter().any(|e|e.kind==EventTypeV4::Injury);
                let lost=outcome.next_side!=side;
                self.diagnostics.record_preparation_contact(lost,foul);
                for e in &mut outcome.events {
                    e.metadata.extra.insert("contestPhase".into(),"PREPARATION".into());
                    // The carrier retained control, not necessarily beat
                    // a defender or completed a dribble.
                    if e.kind==EventTypeV4::DribbleWon {
                        e.kind=EventTypeV4::Carry;
                        e.metadata.escaped_pressure=None;
                        e.metadata.outcome=Some("RETAINED_UNDER_CONTACT".into());
                    }
                }
                self.flush_play_events(outcome.events,side);
                if foul {
                    let kind=if is_in_attacking_penalty_box(side,pos){DeadBallKind::PenaltyKick}else{DeadBallKind::DirectFreeKick};
                    return self.open_dead_ball(kind,side,Vec3::ground(pos.x,pos.y),injury,card);
                }
                if self.state.ball.control==BallControlState::Loose {
                    return self.start_pass_recovery(side,Vec::new(),"TACKLE_CONTACT_LOOSE");
                }
                if lost {
                    self.last_passer_idx=None;
                    let avg_wr=self.team_avg_work_rate(outcome.next_side);
                    let ball=self.state.ball.pos;
                    let phase_events=update_tactical_phases(&mut self.state,side,outcome.next_side,ball,0,avg_wr);
                    self.flush_play_events(phase_events,outcome.next_side);
                    self.carrier_idx=outcome.next_carrier_idx;
                    self.possession_side=outcome.next_side;
                    return self.begin_player_decision(false);
                }
            }
            // A committed challenge was resolved or became invalid. If
            // the carrier retains control, keep the ORIGINAL planned
            // release time without rolling another tackle in this plan.
            pending.executes_at_ms=pending.release_at_ms;
            self.scheduler.schedule(pending.release_at_ms,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
                DesEventPayload::ActionExecute{player_id:player_id.into(),side})
                .map_err(|e|V4Error::InvalidClock(e.into()))?;
            self.pending_decision=Some(pending);
            return Ok(());
        }
        let mut plan=pending.plan;
        // Revalidate the intended target, and measure the physical situation
        // at execution. Do not re-roll the whole decision after every event.
        match &mut plan.action {
            PlannedActionV4::Pass{target_idx,kind,distance_m,progress_gain_m,lane_risk,interceptor_idx}=>{
                let Some(p)=self.state.players.get(*target_idx) else {return self.begin_player_decision(false);};
                if !p.on_pitch || p.sent_off || p.side!=side || *target_idx==idx {
                    return self.begin_player_decision(false);
                }
                *distance_m=pos.distance(p.pos);
                if *distance_m<0.5 {return self.begin_player_decision(false);}
                *progress_gain_m=attacking_progress_m(side,p.pos)-attacking_progress_m(side,pos);
                (*lane_risk,*interceptor_idx)=evaluate_passing_lane(&self.state,&self.input,side,pos,p.pos,kind.is_aerial());
            }
            PlannedActionV4::Shoot{distance_m,in_box}=>{
                *distance_m=distance_to_attacking_goal(side,pos);
                *in_box=is_in_attacking_penalty_box(side,pos);
            }
            _=>{}
        }
        // Audit the original intended release before deciding whether
        // there is a better compatible ground kick. No reroll of the
        // original action-family decision or the original timing draw.
        let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
        let release_risk=if pending.observed_pass_risk.is_some() {
            crate::player_intelligence::observe_pass_risk(&self.state,&self.input,idx,ctx,plan.action)
        }else{None};
        if let (Some(initial),Some(release))=(pending.observed_pass_risk,release_risk) {
            self.diagnostics.record_pass_risk_drift(initial,release);
        }
        if !plan.is_one_touch && !pending.final_aim_committed &&
            matches!(plan.action,PlannedActionV4::Pass{kind:PassKindV4::Short,..}) {
            self.diagnostics.record_reaim_review();
            if let Some(reaim)=crate::player_intelligence::reconsider_short_pass(
                &self.state,&self.input,idx,ctx,plan.action) {
                let updated_risk=crate::player_intelligence::observe_pass_risk(
                    &self.state,&self.input,idx,ctx,reaim.action);
                self.diagnostics.record_reaim_choice(reaim.delay_ms,release_risk,updated_risk);
                plan.action=reaim.action;
                let release_at_ms=now_ms.saturating_add(reaim.delay_ms);
                let (at,challenger)=self.preparation_contest(idx,release_at_ms);
                self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
                    DesEventPayload::ActionExecute{player_id:player_id.into(),side})
                    .map_err(|e|V4Error::InvalidClock(e.into()))?;
                self.pending_decision=Some(PendingDecisionV4{
                    player_id:player_id.into(),side,started_at_ms:now_ms,executes_at_ms:at,
                    plan,release_at_ms,challenger,observed_pass_risk:updated_risk,final_aim_committed:true,
                });
                return Ok(());
            }
        }
        if pending.final_aim_committed {self.diagnostics.record_reaim_release();}
        let prev_possession = self.possession_side;
        let (next_side, next_carrier, entered_dead_ball, next_delay_ms) =
            self.step_live_action_des(self.carrier_idx, plan)?;

        self.possession_side = next_side;
        self.carrier_idx = next_carrier;

        if prev_possession != self.possession_side {
            let avg_wr = self.team_avg_work_rate(self.possession_side);
            let ball_pos_now = self.state.ball.pos;
            let phase_events = update_tactical_phases(
                &mut self.state,
                prev_possession,
                self.possession_side,
                ball_pos_now,
                0,
                avg_wr,
            );
            if !phase_events.is_empty() {
                let poss = self.possession_side;
                self.flush_play_events(phase_events, poss);
            }
        }

        if !entered_dead_ball && self.state.dead_ball.is_none() && self.pending_flight.is_none()
            && self.pending_carry.is_none() {
            let cid = self.state.players[self.carrier_idx].player_id.clone();
            self.scheduler
                .schedule(
                    now_ms.saturating_add(next_delay_ms.max(1)),
                    EventPriority::ActionDecision,
                    CancellationScope::LivePlayOnly,
                    DesEventPayload::DecisionReady {
                        player_id: cid,
                        side: self.possession_side,
                    },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }

        Ok(())
    }

    /// CRITICAL-01: Resolves an early mid-flight interaction (interception, out-of-bounds, or
    /// shot block) at its exact `hit_time_ms` AFTER the clock and world have advanced to `hit_time_ms`.
    fn handle_early_interaction_resolved(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn handle_ball_in_flight_arrival(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn commit_pending_flight(&mut self) -> Result<(), V4Error> {
        let Some(mut flight) = self.pending_flight.take() else { return Ok(()); };
        let now_ms = self.state.des_clock.sim_time_ms;
        if now_ms != flight.resolves_at_ms {
            return Err(V4Error::InvalidClock("flight resolved at incorrect timestamp".into()));
        }
        let launch_ms = flight.trajectory.start_time_ms;
        for ev in &mut flight.outcome.events {
            ev.metadata.extra.insert("launchSimTimeMs".into(), launch_ms.to_string());
            ev.metadata.extra.insert("plannedArrivalSimTimeMs".into(), flight.planned_arrival_ms.to_string());
            if matches!(&flight.action, FlightActionV4::Shot { .. }) {
                ev.metadata.extra.insert("shotLaunchSimTimeMs".into(), launch_ms.to_string());
            }
        }
        // Decide control at actual contact, using live fatigue and pressure,
        // not the cloned launch-state. A failed control leaves a real loose ball.
        let pass_contact = matches!(&flight.action, FlightActionV4::Pass { .. })
            && flight.early_interaction.as_ref().map_or(false, |h| h.kind == InteractionKind::PassInterception);
        if pass_contact {
            let h = flight.early_interaction.as_ref().expect("contact");
            let idx = flight.outcome.next_carrier_idx;
            let p = &self.state.players[idx];
            let attrs = *lookup_attrs(&self.input, p);
            let profile = pass_reach_candidate(&self.state, idx, h.ball_pos.z > 0.8);
            let incoming = flight.trajectory.velocity_at_ms(now_ms);
            let speed = (incoming.x * incoming.x + incoming.y * incoming.y + incoming.z * incoming.z).sqrt();
            let skill = normalize_attr(attrs.technical.first_touch) * 0.45
                + normalize_attr(attrs.technical.technique) * 0.30
                + normalize_attr(attrs.mental.composure) * 0.25;
            let fatigue = (p.fatigue.aerobic * 0.6 + p.fatigue.anaerobic * 0.4) / 100.0;
            let pressure = evaluate_local_context(&self.state, &self.input, p.side, h.ball_pos.xy()).pressure / 100.0;
            let chance = contact_control_probability(skill, normalize_attr(attrs.physical.balance),
                fatigue, speed, h.ball_pos.z, h.distance_m / profile.reach_m.max(0.1), pressure);
            let controlled = (self.rng.next_double() as f32) < chance;
            if !controlled {
                let origin_side = p.side.opposite();
                let defender_id = p.player_id.clone();
                let defender_side = p.side;
                let spot = h.ball_pos.clamp_to_pitch();
                let rebound = blocked_ball_velocity(incoming, p.pos, spot);
                flight.counter_delta.apply(&mut self.state)?;
                self.state.ball.pos = spot;
                self.state.ball.vel = rebound;
                self.state.ball.control = BallControlState::Loose;
                self.state.ball.owner_side = None;
                self.state.ball.owner_player_id = None;
                self.state.ball.last_touch_side = Some(defender_side);
                self.state.ball.last_touch_player_id = Some(defender_id.clone());
                for ev in &mut flight.outcome.events {
                    if ev.kind == EventTypeV4::PassFailed {
                        ev.metadata.extra.insert("pendingPassRecovery".into(), "true".into());
                        ev.metadata.extra.insert("contactOutcome".into(), "BLOCKED_LOOSE".into());
                    }
                }
                // Interception means controlled interception here, not merely contact.
                flight.outcome.events.retain(|ev| ev.kind != EventTypeV4::Interception);
                self.flush_play_events(flight.outcome.events, origin_side);
                self.emit(EventTypeV4::LooseBall, HighlightTier::None, None, Some(defender_side),
                    Some(defender_id), None, EventMetadataV4 {
                        cause: Some("PASS_CONTACT_LOOSE".into()),
                        ..EventMetadataV4::default()
                    });
                return self.start_pass_recovery(origin_side, flight.offside_at_launch, "PASS_CONTACT_LOOSE");
            }
            for ev in &mut flight.outcome.events {
                ev.metadata.extra.insert("contactOutcome".into(), "CONTROLLED".into());
            }
            if h.time_ms < flight.planned_arrival_ms {
                self.diagnostics.record_early_interception();
            }
        }
        if let FlightActionV4::LooseRecovery { origin_side, cause } = &flight.action {
            let origin_side = *origin_side;
            let cause = cause.clone();
            if let Some(hit) = flight.early_interaction.as_ref() {
                return self.finish_loose_ball_boundary(origin_side, &cause, hit);
            }
            let idx = flight.outcome.next_carrier_idx;
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                return self.start_pass_recovery(origin_side, flight.offside_at_launch, &cause);
            }
            return self.finish_pass_recovery(origin_side, idx, &flight.offside_at_launch, &cause);
        }
        if matches!(&flight.action, FlightActionV4::Pass { .. }) && flight.ball_after.control == BallControlState::Loose {
            let origin_side = flight.outcome.next_side;
            let heavy = flight.outcome.events.iter().any(|ev| ev.kind == EventTypeV4::FirstTouch
                && ev.metadata.cause.as_deref() == Some("FIRST_TOUCH_ERROR"));
            let cause = if heavy { "FIRST_TOUCH_LOOSE" } else { "PASS_MISS_LOOSE" };
            let offside = if heavy {
                let actor = flight.outcome.next_carrier_idx;
                self.state.players.iter().enumerate().filter(|(i,p)| *i != actor
                    && p.on_pitch && !p.sent_off && p.side == origin_side
                    && is_receiver_offside(&self.state, origin_side, flight.ball_after.pos.xy(), p.pos))
                    .map(|(_,p)|p.player_id.clone()).collect()
            } else { flight.offside_at_launch };
            flight.counter_delta.apply(&mut self.state)?;
            self.state.ball = flight.ball_after;
            self.flush_play_events(flight.outcome.events, origin_side);
            self.emit(EventTypeV4::LooseBall, HighlightTier::None, None,
                self.state.ball.last_touch_side, self.state.ball.last_touch_player_id.clone(), None,
                EventMetadataV4 { cause: Some(cause.into()), ..EventMetadataV4::default() });
            return self.start_pass_recovery(origin_side, offside, cause);
        }
        flight.counter_delta.apply(&mut self.state)?;
        self.state.ball = flight.ball_after;
        match flight.action {
            FlightActionV4::Pass { passer_idx, kind, distance_m, is_one_touch } => {
                let side = self.state.players[passer_idx].side;
                self.complete_pass_outcome(side, passer_idx, kind, distance_m, is_one_touch, flight.outcome)
            }
            FlightActionV4::LooseRecovery { .. } => unreachable!("handled before normal flight commit"),
            FlightActionV4::Shot { shooter_idx } => {
                let side = self.state.players[shooter_idx].side;
                let (next_side, next_carrier, dead, delay) = self.finalize_shot_outcome_des(side, flight.outcome, flight.won_corner)?;
                self.possession_side = next_side;
                self.carrier_idx = next_carrier;
                if !dead && self.state.dead_ball.is_none() {
                    let cid = self.state.players[next_carrier].player_id.clone();
                    self.scheduler.schedule(now_ms.saturating_add(delay.max(150)), EventPriority::ActionDecision,
                        CancellationScope::LivePlayOnly, DesEventPayload::DecisionReady { player_id: cid, side: next_side })
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                }
                Ok(())
            }
        }
    }


    /// Both teams pursue the actual rolling/descending ball.
    fn start_pass_recovery(&mut self, origin_side: TeamSideV4, offside_at_launch: Vec<String>, cause: &str) -> Result<(), V4Error> {
        let now = self.state.des_clock.sim_time_ms;
        let indices: Vec<_> = self.state.players.iter().enumerate()
            .filter(|(_,p)| p.on_pitch && !p.sent_off).map(|(i,_)| i).collect();
        let candidates: Vec<_> = indices.iter().map(|&i| pass_reach_candidate(&self.state,i,true)).collect();
        let trajectory = BallTrajectory { start_time_ms:now, start_pos:self.state.ball.pos,
            initial_vel:self.state.ball.vel, gravity_z:9.81, drag:1.6, is_shot:false,is_carrier_dribble:false };
        let resolution = find_moving_ball_recovery(&trajectory,&candidates)
            .ok_or_else(|| V4Error::InvalidClock("no active player can reach loose ball".into()))?;
        let (idx,at,motion,boundary) = match resolution {
            LooseBallResolution::Player{candidate_idx,time_ms,ball_pos} => (indices[candidate_idx],time_ms,
                Some(PassContactMotion{candidate:candidates[candidate_idx].clone(),target:ball_pos.xy(),started_at_ms:now}),None),
            LooseBallResolution::Boundary(h) => (self.carrier_idx,h.time_ms.max(now+1),None,Some(h)),
        };
        let side=self.state.players[idx].side;
        let cid=self.state.players[idx].player_id.clone();
        self.last_passer_idx=None;
        self.possession_side=origin_side;
        self.state.ball.control=BallControlState::Loose;
        self.state.ball.owner_side=None;
        self.state.ball.owner_player_id=None;
        let flight=PendingFlightSnapshotV4 {
            trajectory,planned_arrival_ms:at,resolves_at_ms:at,
            action:FlightActionV4::LooseRecovery{origin_side,cause:cause.into()},
            outcome:crate::v4::ball_play::BallPlayOutcome{next_side:side,next_carrier_idx:idx,stoppage_seconds:0,events:Vec::new()},
            ball_after:self.state.ball.clone(),counter_delta:FlightCounterDeltaV4::between(&self.state,&self.state),
            won_corner:false,early_interaction:boundary,pass_contact_motion:motion,offside_at_launch,
        };
        self.scheduler.schedule(at,EventPriority::PhysicsAndBoundary,CancellationScope::LivePlayOnly,
            DesEventPayload::EarlyInteractionResolved{interaction_tag:"MOVING_PASS_RECOVERY".into(),actor_id:Some(cid)})
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        self.pending_flight=Some(flight);
        Ok(())
    }

    #[cfg(test)]
    fn start_grounded_pass_recovery(&mut self, origin_side: TeamSideV4, offside_at_launch: Vec<String>) -> Result<(), V4Error> {
        self.start_pass_recovery(origin_side,offside_at_launch,"PASS_CONTACT_LOOSE")
    }

    fn finish_pass_recovery(&mut self, origin_side: TeamSideV4, idx: usize, offside_at_launch: &[String], cause: &str) -> Result<(), V4Error> {
        let side = self.state.players[idx].side;
        let id = self.state.players[idx].player_id.clone();
        let spot = self.state.ball.pos;
        let now = self.state.des_clock.sim_time_ms;
        // An accidental block does not reset the original Law 11 snapshot.
        if side == origin_side && offside_at_launch.contains(&id) {
            self.state.team_state_mut(side).offsides += 1;
            let mut meta = EventMetadataV4 { outcome: Some("OFFSIDE_INVOLVEMENT".into()), cause: Some(cause.into()), ..EventMetadataV4::default() };
            meta.extra.insert("passAlreadyCounted".into(), "true".into());
            self.emit(EventTypeV4::Offside, HighlightTier::Extended, None, Some(side), Some(id), None, meta);
            return self.open_dead_ball(DeadBallKind::Offside, side.opposite(), spot, false, false);
        }
        self.possession_side = side;
        self.carrier_idx = idx;
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(side);
        self.state.ball.owner_player_id = Some(id.clone());
        self.state.ball.last_touch_side = Some(side);
        self.state.ball.last_touch_player_id = Some(id.clone());
        self.state.ball.vel = Vec3::default();
        let mut meta = EventMetadataV4 { cause: Some(cause.into()), ..EventMetadataV4::default() };
        meta.extra.insert("recoveryOutcome".into(), if side == origin_side { "ATTACK_REGAINED".into() } else { "DEFENCE_WON".into() });
        self.emit(EventTypeV4::SecondBallWon, HighlightTier::None, None, Some(side), Some(id.clone()), None, meta);
        if cause=="TACKLE_CONTACT_LOOSE" {
            self.diagnostics.record_tackle_loose_recovery(side==origin_side);
        }
        if cause=="TACKLE_CONTACT_LOOSE" && side!=origin_side {
            let avg_wr=self.team_avg_work_rate(side);
            let ball=self.state.ball.pos;
            let phase_events=update_tactical_phases(&mut self.state,origin_side,side,ball,0,avg_wr);
            self.flush_play_events(phase_events,side);
        }
        let settle = self.input.timing_model.first_touch_settle_ms.sample_triangular(&mut self.rng, 1.0).max(150);
        self.scheduler.schedule(now.saturating_add(settle), EventPriority::BallStateTransition, CancellationScope::LivePlayOnly,
            DesEventPayload::FirstTouchComplete { player_id: id, side })
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        Ok(())
    }

    fn finish_loose_ball_boundary(&mut self, origin_side: TeamSideV4, cause: &str, hit: &EarlyInteractionHit) -> Result<(), V4Error> {
        if cause=="TACKLE_CONTACT_LOOSE" {self.diagnostics.record_tackle_loose_boundary();}
        let spot=hit.ball_pos.clamp_to_pitch();
        let last_side=self.state.ball.last_touch_side.unwrap_or(origin_side);
        let last_id=self.state.ball.last_touch_player_id.clone();
        let line=hit.boundary.ok_or_else(|| V4Error::InvalidClock("missing loose ball boundary".into()))?;
        let defender=match line { BoundaryKind::HomeGoalLine=>Some(TeamSideV4::Home),
            BoundaryKind::AwayGoalLine=>Some(TeamSideV4::Away), _=>None };
        self.state.ball.pos=spot;
        self.state.ball.vel=Vec3::default();
        if hit.kind==InteractionKind::GoalScored {
            let defender=defender.ok_or_else(|| V4Error::InvalidClock("goal outside goal line".into()))?;
            let beneficiary=defender.opposite();
            let actor=last_id.as_deref().and_then(|id|self.state.players.iter().position(|p|p.player_id==id))
                .ok_or_else(||V4Error::InvalidClock("goal without last touch".into()))?;
            let seq=self.alloc_highlight_sequence();
            if last_side==defender {
                let event=crate::v4::own_goal::award_own_goal(&mut self.state,actor,beneficiary,
                    if cause=="TACKLE_CONTACT_LOOSE" {crate::v4::own_goal::DeflectionCause::TackleContact}
                    else {crate::v4::own_goal::DeflectionCause::PassContact},seq);
                self.flush_play_events(vec![event],beneficiary);
            } else {
                self.state.team_state_mut(beneficiary).goals+=1;
                self.state.players[actor].goals=self.state.players[actor].goals.saturating_add(1);
                self.emit(EventTypeV4::Goal,HighlightTier::GoalKey,Some(seq),Some(beneficiary),last_id,None,
                    EventMetadataV4{scored:Some(true),cause:Some("LOOSE_BALL_GOAL".into()),..EventMetadataV4::default()});
            }
            shift_momentum(&mut self.state,beneficiary,0.35);
            return self.open_dead_ball(DeadBallKind::GoalCelebration,defender,Vec3::center_spot(),false,false);
        }
        let (kind,ev,restart_side,restart_spot)=match defender {
            None=>(DeadBallKind::ThrowIn,EventTypeV4::ThrowIn,last_side.opposite(),spot),
            Some(d) if last_side==d=>(DeadBallKind::Corner,EventTypeV4::Corner,d.opposite(),
                Vec3::ground(spot.x,if spot.y<34.0 {0.5} else {67.5})),
            Some(d)=>(DeadBallKind::GoalKick,EventTypeV4::GoalKick,d,
                Vec3::ground(if d==TeamSideV4::Home {5.5} else {PITCH_LENGTH_M-5.5},34.0)),
        };
        match kind { DeadBallKind::ThrowIn=>self.state.team_state_mut(restart_side).throw_ins+=1,
            DeadBallKind::Corner=>self.state.team_state_mut(restart_side).corners+=1,_=>{} }
        let mut meta=EventMetadataV4{cause:Some(cause.into()),..EventMetadataV4::default()};
        meta.extra.insert("looseRecoveryBoundary".into(),"true".into());
        meta.extra.insert("recoveryOutcome".into(),if restart_side==origin_side {"ATTACK_RESTART".into()} else {"DEFENCE_RESTART".into()});
        self.emit(ev,HighlightTier::None,None,Some(restart_side),None,None,meta);
        self.open_dead_ball(kind,restart_side,restart_spot,false,false)
    }

    fn install_flight(&mut self, flight: PendingFlightSnapshotV4, payload: DesEventPayload, actor_idx: usize) -> Result<(), V4Error> {
        let time = flight.resolves_at_ms;
        self.state.ball.pos = flight.trajectory.start_pos;
        self.state.ball.vel = flight.trajectory.initial_vel;
        self.state.ball.control = BallControlState::InFlight;
        self.state.ball.owner_side = None;
        self.state.ball.owner_player_id = None;
        self.state.ball.last_touch_side = Some(self.state.players[actor_idx].side);
        self.state.ball.last_touch_player_id = Some(self.state.players[actor_idx].player_id.clone());
        let payload = if let Some(hit) = flight.early_interaction.as_ref() {
            DesEventPayload::EarlyInteractionResolved {
                interaction_tag: format!("{:?}", hit.kind), actor_id: hit.defender_id.clone(),
            }
        } else { payload };
        self.scheduler.schedule(time, EventPriority::PhysicsAndBoundary, CancellationScope::LivePlayOnly, payload)
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        self.pending_flight = Some(flight);
        Ok(())
    }

    fn complete_pass_outcome(
        &mut self,
        att_side: TeamSideV4,
        passer_idx: usize,
        kind: PassKindV4,
        distance_m: f32,
        is_one_touch: bool,
        outcome: crate::v4::ball_play::BallPlayOutcome,
    ) -> Result<(), V4Error> {
        let def_side = att_side.opposite();
        let now_ms = self.state.des_clock.sim_time_ms;
        let retained = outcome.next_side == att_side;
        self.last_passer_idx = if retained { Some(passer_idx) } else { None };
        self.possession_side = outcome.next_side;
        self.carrier_idx = outcome.next_carrier_idx;

        let had_offside = outcome.events.iter().any(|e| e.kind == EventTypeV4::Offside);
        let had_throw_in = outcome.events.iter().any(|e| e.kind == EventTypeV4::ThrowIn);
        let had_goal_kick = outcome.events.iter().any(|e| e.kind == EventTypeV4::GoalKick);
        let last_pos = outcome
            .events
            .last()
            .map(|e| e.ball_pos)
            .unwrap_or(self.state.ball.pos);

        self.flush_play_events(outcome.events, att_side);

        if had_offside {
            self.open_dead_ball(DeadBallKind::Offside, def_side, last_pos, false, false)?;
            return Ok(());
        }
        if had_throw_in {
            self.open_dead_ball(DeadBallKind::ThrowIn, outcome.next_side, last_pos, false, false)?;
            return Ok(());
        }
        if had_goal_kick {
            self.open_dead_ball(DeadBallKind::GoalKick, outcome.next_side, last_pos, false, false)?;
            return Ok(());
        }

        if retained
            && kind == PassKindV4::Cross
            && is_in_attacking_penalty_box(att_side, self.state.players[outcome.next_carrier_idx].pos)
        {
            self.schedule_shot_flight(outcome.next_carrier_idx, Some(passer_idx), Some(ShotKindV4::Header))?;
            return Ok(());
        }

        let _ = (distance_m, is_one_touch, now_ms);
        // Receiver chooses its own intent immediately after the contact.
        // A controlling touch is included only if the new plan requires it.
        self.begin_player_decision(true)
    }

    /// CRITICAL-01: Player finishes first touch and schedules the next decision.
    fn handle_first_touch_complete(&mut self) -> Result<(), V4Error> {
        self.begin_player_decision(false)
    }

    /// CRITICAL-01: Schedules a shot in-flight so `Goal`/`Save`/`ShotOffTarget` occurs at
    /// `t_shot + shot_flight_ms` (or early `ShotBlocked` at `t_block`), never at `t_shot`.
    fn schedule_shot_flight(&mut self, shooter_idx: usize, assist_idx: Option<usize>, shot_kind: Option<ShotKindV4>) -> Result<(), V4Error> {
        let now = self.state.des_clock.sim_time_ms;
        let side = self.state.players[shooter_idx].side;
        let mut resolved = self.state.clone();
        let shot = resolve_shot_v4_des(&mut resolved, &self.input, &self.input.timing_model,
            shooter_idx, assist_idx, shot_kind, false, &mut self.rng);
        let delta = FlightCounterDeltaV4::between(&self.state, &resolved);
        self.state.next_highlight_sequence_id = resolved.next_highlight_sequence_id;
        let flight = PendingFlightSnapshotV4 {
            trajectory: shot.trajectory,
            planned_arrival_ms: now.saturating_add(shot.planned_flight_ms.max(1)),
            resolves_at_ms: now.saturating_add(shot.flight_ms.max(1)),
            action: FlightActionV4::Shot { shooter_idx },
            outcome: shot.outcome, ball_after: resolved.ball, counter_delta: delta,
            won_corner: shot.won_corner, early_interaction: shot.early_interaction,
            pass_contact_motion: None,
            offside_at_launch: Vec::new(),
        };
        let payload = DesEventPayload::ShotArrival { shooter_id: self.state.players[shooter_idx].player_id.clone(), side,
            target_y_mm: 34_000, target_z_mm: 1_100 };
        self.install_flight(flight, payload, shooter_idx)
    }

    fn handle_shot_arrival(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn step_live_action_des(
        &mut self,
        carrier_idx: usize,
        plan: DecisionPlanV4,
    ) -> Result<(TeamSideV4, usize, bool, i64), V4Error> {
        let att_side = self.state.players[carrier_idx].side;
        let def_side = att_side.opposite();
        let carrier_pos = self.state.players[carrier_idx].pos;
        let carrier_ctx = evaluate_local_context(&self.state, &self.input, att_side, carrier_pos);

        // Fouls during preparation now arise from a scheduled, reachable
        // contact. Merely being within 7.5m is not a tackle or a foul.

        match plan.action {
            PlannedActionV4::Pass {
                target_idx,
                kind,
                distance_m,
                progress_gain_m,
                lane_risk,
                interceptor_idx,
            } => {
                let now = self.state.des_clock.sim_time_ms;
                let is_one_touch = plan.is_one_touch;
                if is_one_touch { self.diagnostics.record_one_touch_pass(); }
                let mut resolved = self.state.clone();
                let pass = execute_pass_and_first_touch_des(&mut resolved, &self.input, &self.input.timing_model,
                    carrier_idx, target_idx, kind, distance_m, progress_gain_m, lane_risk, interceptor_idx,
                    carrier_ctx, is_one_touch, &mut self.rng);
                let delta = FlightCounterDeltaV4::between(&self.state, &resolved);
                self.state.next_highlight_sequence_id = resolved.next_highlight_sequence_id;
                let trajectory = pass.trajectory.ok_or_else(|| V4Error::InvalidClock("missing pass trajectory".into()))?;
                let contact_idx = if let Some(hit) = pass.early_interaction.as_ref() {
                    if hit.kind == InteractionKind::PassInterception {
                        hit.defender_id.as_deref().and_then(|id|
                            self.state.players.iter().position(|p| p.player_id == id))
                    } else { None }
                } else if pass.outcome.events.iter().any(|e| matches!(e.kind,
                    EventTypeV4::PassCompleted | EventTypeV4::ThroughBall | EventTypeV4::Cross | EventTypeV4::Offside)) {
                    Some(target_idx)
                } else { None };
                let pass_contact_motion = contact_idx.map(|idx| PassContactMotion {
                    candidate: pass_reach_candidate(&self.state, idx, kind.is_aerial()),
                    target: trajectory.position_at_ms(now.saturating_add(pass.elapsed_ms.max(1))).xy(),
                    started_at_ms: now,
                });
                let offside_at_launch = self.state.players.iter().filter(|p|
                    p.on_pitch && !p.sent_off && p.side == att_side
                    && is_receiver_offside(&self.state, att_side, self.state.players[carrier_idx].pos, p.pos))
                    .map(|p| p.player_id.clone()).collect();
                let flight = PendingFlightSnapshotV4 {
                    trajectory,
                    planned_arrival_ms: now.saturating_add(pass.planned_flight_ms.max(1)),
                    resolves_at_ms: now.saturating_add(pass.elapsed_ms.max(1)),
                    action: FlightActionV4::Pass { passer_idx: carrier_idx, kind, distance_m, is_one_touch },
                    outcome: pass.outcome, ball_after: resolved.ball, counter_delta: delta,
                    won_corner: false, early_interaction: pass.early_interaction,
                    pass_contact_motion, offside_at_launch,
                };
                let target = self.state.players[target_idx].pos;
                let payload = DesEventPayload::BallInFlightArrival {
                    passer_id: Some(self.state.players[carrier_idx].player_id.clone()),
                    receiver_id: Some(self.state.players[target_idx].player_id.clone()),
                    target_x_mm: (target.x * 1000.0).round() as i32, target_y_mm: (target.y * 1000.0).round() as i32,
                };
                self.install_flight(flight, payload, carrier_idx)?;

                Ok((att_side, carrier_idx, false, 0))
            }
            PlannedActionV4::Dribble { .. } => {
                let intent=plan.carry.ok_or_else(||V4Error::InvalidClock("PI dribble lacks carry intention".into()))?;
                self.start_carry_intent(carrier_idx,intent,false)?;
                Ok((att_side,carrier_idx,false,0))
            }
            PlannedActionV4::Shoot { .. } => {
                let assist = self.last_passer_idx;
                self.last_passer_idx = None;
                self.schedule_shot_flight(carrier_idx, assist, None)?;
                Ok((att_side, carrier_idx, false, 0))
            }
            PlannedActionV4::Clearance => {
                self.last_passer_idx = None;
                let carrier_id = self.state.players[carrier_idx].player_id.clone();
                let dir = match att_side {
                    TeamSideV4::Home => 1.0f32,
                    TeamSideV4::Away => -1.0f32,
                };
                let clear_pos = Vec3::new(
                    (carrier_pos.x + dir * 32.0).clamp(5.0, PITCH_LENGTH_M - 5.0),
                    (carrier_pos.y + (self.rng.next_double() as f32 - 0.5) * 18.0)
                        .clamp(0.0, 68.0),
                    4.5,
                );
                let flight_ms = estimate_ball_flight_ms(
                    &self.input.timing_model,
                    carrier_pos.distance(clear_pos.xy()),
                    BallFlightKind::Clearance,
                    self.input.match_environment.weather,
                    self.input.match_environment.pitch_quality,
                    &mut self.rng,
                );
                self.state.ball.pos = clear_pos;
                let g_sec = self.state.des_clock.game_second();
                let mut meta = EventMetadataV4 {
                    pressure: Some(carrier_ctx.pressure.round() as u8),
                    ..EventMetadataV4::default()
                };
                attach_tactical_flags(&mut meta, compute_tactical_flags(&self.state, att_side, g_sec));
                self.emit(
                    EventTypeV4::Clearance,
                    HighlightTier::None,
                    None,
                    Some(att_side),
                    Some(carrier_id),
                    None,
                    meta,
                );
                let next_idx = self.pick_outfield_index(def_side);
                let next_id = self.state.players[next_idx].player_id.clone();
                self.state.ball.control = BallControlState::Controlled;
                self.state.ball.owner_side = Some(def_side);
                self.state.ball.owner_player_id = Some(next_id);
                Ok((def_side, next_idx, false, flight_ms))
            }
            PlannedActionV4::HoldAndRecycle => {
                let intent=plan.carry.ok_or_else(||V4Error::InvalidClock("PI hold lacks carry intention".into()))?;
                self.start_carry_intent(carrier_idx,intent,true)?;
                Ok((att_side,carrier_idx,false,0))
            }
        }
    }

    #[cfg(test)]
    fn start_timed_dribble(&mut self,idx:usize)->Result<(),V4Error>{
        let dir=if self.state.players[idx].side==TeamSideV4::Home{1.0}else{-1.0};
        self.start_carry_intent(idx,CarryIntentV4{direction:Vec2::new(dir,0.0),duration_ms:1600,shielding:false},false)
    }

    fn start_carry_intent(&mut self,idx:usize,intent:CarryIntentV4,is_hold:bool)->Result<(),V4Error>{
        let now=self.state.des_clock.sim_time_ms;
        let mut candidate=pass_reach_candidate(&self.state,idx,false);
        candidate.speed_mps*=if intent.shielding{0.42}else{0.88};
        candidate.reaction_ms=0;
        let target=carry_target(candidate.pos,candidate.speed_mps,intent);
        let duration=intent.duration_ms.max(150);
        let end=now.saturating_add(duration);
        let side=candidate.side;
        let id=candidate.player_id.clone();
        let start_pressure=evaluate_local_context(&self.state,&self.input,side,candidate.pos).pressure;
        let motion=PassContactMotion{candidate,target,started_at_ms:now};
        let (at,challenger)=match self.choose_committed_tackle(idx,&motion,end) {
            Some((_,at,chase))=>(at,Some(chase)),None=>(end,None),
        };
        self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
            DesEventPayload::DribbleStep{carrier_id:id,side}).map_err(|e|V4Error::InvalidClock(e.into()))?;
        self.diagnostics.record_carry_start(is_hold,intent.shielding,intent.direction.y.abs()>0.1);
        self.pending_carry=Some(PendingCarryV4{carrier_idx:idx,resolves_at_ms:at,motion,shielding:intent.shielding,
            is_hold,challenger,start_pressure});
        Ok(())
    }

    fn resolve_audited_carry_contact(&mut self,idx:usize,contact:Option<usize>,
        ctx:crate::v4::movement::LocalSpatialContext,shielding:bool)->crate::v4::ball_play::BallPlayOutcome {
        // Evaluate against the SAME physical snapshot and same actual
        // contact, not all candidate commits or hypothetical reaches.
        // No forecast is used to change the resolved outcome.
        let audit=contact.map(|def_idx|{
            let pi=crate::player_intelligence::evaluate_tackle(&self.state,idx,def_idx,0.0);
            let resolver=crate::v4::ball_play::duel_contact_probabilities(&self.state,&self.input,idx,def_idx,ctx,shielding);
            (pi,resolver)
        });
        let out=resolve_carry_contact(&mut self.state,&self.input,idx,contact,ctx,shielding,&mut self.rng);
        if let Some((pi,resolver))=audit {
            let has=|kind|out.events.iter().any(|e|e.kind==kind);
            let missed_ball_body=out.events.iter().any(|e|matches!(e.kind,EventTypeV4::Foul|EventTypeV4::AdvantagePlayed)
                && e.metadata.extra.get("contactBranch").map(String::as_str)==Some("MISSED_BALL_BODY"));
            let loose=out.events.iter().any(|e|e.kind==EventTypeV4::LooseBall
                && e.metadata.cause.as_deref()==Some("TACKLE_CONTACT_LOOSE"));
            self.diagnostics.record_tackle_contact_audit(pi.clean_win_probability,pi.foul_probability,
                resolver,has(EventTypeV4::TackleWon)||loose,has(EventTypeV4::Foul),has(EventTypeV4::AdvantagePlayed),missed_ball_body);
            self.diagnostics.record_tackle_control_split(loose,has(EventTypeV4::TackleWon));
            self.diagnostics.record_tackle_second_ball_prior(pi.controlled_probability,pi.loose_probability,
                pi.recovery_given_loose,loose);
            if loose {
                let def_idx=contact.expect("loose tackle has a defender");
                let spot=self.state.ball.pos.xy();
                let vel=self.state.ball.vel;
                let speed=(vel.x*vel.x+vel.y*vel.y).sqrt();
                self.diagnostics.record_tackle_loose_kinematics(speed,
                    spot.distance(self.state.players[idx].pos),spot.distance(self.state.players[def_idx].pos));
            }
        }
        out
    }

    fn finish_timed_dribble(&mut self, player_id:&str, side:TeamSideV4) -> Result<(),V4Error> {
        let Some(carry)=self.pending_carry.take() else {return Ok(());};
        let idx=carry.carrier_idx;
        let now=self.state.des_clock.sim_time_ms;
        if carry.resolves_at_ms!=now || carry.motion.candidate.player_id!=player_id || carry.motion.candidate.side!=side {
            return Err(V4Error::InvalidClock("carry completion does not match pending motion".into()));
        }
        if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
            return self.begin_player_decision(false);
        }
        let pos=self.state.players[idx].pos;
        let moved=carry.motion.candidate.pos.distance(pos);
        let gain=if side==TeamSideV4::Home{pos.x-carry.motion.candidate.pos.x}else{carry.motion.candidate.pos.x-pos.x};
        let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
        // World integration has already accounted for the entire carry.
        // The existing duel resolver is used only for its contact outcome
        // with zero displacement/time, not a second movement interval.
        let contact=carry.challenger.as_ref().and_then(|chase|
            self.state.players.iter().position(|p|p.player_id==chase.candidate.player_id
                && p.on_pitch && !p.sent_off && p.pos.distance(pos)<=chase.candidate.reach_m+0.01));
        if let Some(def_idx)=contact{
            self.mark_tackle_recovery(def_idx);
            self.diagnostics.record_carry_contact();
        }
        let mut outcome=self.resolve_audited_carry_contact(idx,contact,ctx,carry.shielding);
        self.diagnostics.record_carry_result(outcome.next_side!=side);
        let had_foul=outcome.events.iter().any(|e|e.kind==EventTypeV4::Foul);
        if had_foul {self.diagnostics.record_carry_foul();}
        let had_card=outcome.events.iter().any(|e|matches!(e.kind,EventTypeV4::YellowCard|EventTypeV4::RedCard));
        let had_injury=outcome.events.iter().any(|e|e.kind==EventTypeV4::Injury);
        for ev in &mut outcome.events {
            if ev.kind==EventTypeV4::DribbleWon {
                ev.metadata.progress_gain=Some(gain.round() as i32);
                ev.metadata.escaped_pressure=if carry.start_pressure>=45.0 && ctx.pressure<carry.start_pressure-15.0 {Some(true)}else{None};
            }
            ev.metadata.extra.insert("carryMode".into(),if carry.shielding{"SHIELD"}else if carry.is_hold{"HOLD"}else{"DRIBBLE"}.into());
            ev.metadata.extra.insert("carryDistanceM".into(),format!("{moved:.3}"));
        }
        if outcome.events.is_empty() {
            self.emit(EventTypeV4::Carry,HighlightTier::None,None,Some(side),Some(player_id.into()),None,
                EventMetadataV4{progress_gain:Some(gain.round() as i32),pressure:Some(ctx.pressure.round() as u8),
                    escaped_pressure:if carry.start_pressure>=45.0 && ctx.pressure<carry.start_pressure-15.0{Some(true)}else{None},
                    extra: [("carryMode".into(),if carry.shielding{"SHIELD"}else if carry.is_hold{"HOLD"}else{"DRIBBLE"}.into()),
                        ("carryDistanceM".into(),format!("{moved:.3}"))].into_iter().collect(),
                    ..EventMetadataV4::default()});
        }
        self.flush_play_events(outcome.events,side);
        if had_foul {
            let kind=if is_in_attacking_penalty_box(side,pos){DeadBallKind::PenaltyKick}else{DeadBallKind::DirectFreeKick};
            return self.open_dead_ball(kind,side,Vec3::ground(pos.x,pos.y),had_injury,had_card);
        }
        if self.state.ball.control==BallControlState::Loose {
            return self.start_pass_recovery(side,Vec::new(),"TACKLE_CONTACT_LOOSE");
        }
        if outcome.next_side!=side {
            self.last_passer_idx=None;
            let avg_wr=self.team_avg_work_rate(outcome.next_side);
            let ball=self.state.ball.pos;
            let phase_events=update_tactical_phases(&mut self.state,side,outcome.next_side,ball,0,avg_wr);
            self.flush_play_events(phase_events,outcome.next_side);
        }
        self.carrier_idx=outcome.next_carrier_idx;
        self.possession_side=outcome.next_side;
        self.begin_player_decision(false)
    }

    fn finalize_shot_outcome_des(
        &mut self,
        att_side: TeamSideV4,
        shot_out: crate::v4::ball_play::BallPlayOutcome,
        won_corner: bool,
    ) -> Result<(TeamSideV4, usize, bool, i64), V4Error> {
        let def_side = att_side.opposite();
        let scored = shot_out
            .events
            .iter()
            .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
        let had_goal_kick = shot_out.events.iter().any(|e| e.kind == EventTypeV4::GoalKick);
        let last_pos = shot_out
            .events
            .last()
            .map(|e| e.ball_pos)
            .unwrap_or(self.state.ball.pos);

        self.flush_play_events(shot_out.events, att_side);

        if scored {
            shift_momentum(&mut self.state, att_side, 0.35);
            // Strictly enter GoalCelebration lifecycle (Celebrating -> ReturningToHalf -> KickoffSetup -> ExecutingRestart).
            self.open_dead_ball(
                DeadBallKind::GoalCelebration,
                def_side,
                Vec3::center_spot(),
                false,
                false,
            )?;
            return Ok((def_side, shot_out.next_carrier_idx, true, 0));
        }

        if won_corner {
            let corner_x = match att_side {
                TeamSideV4::Home => PITCH_LENGTH_M,
                TeamSideV4::Away => 0.0,
            };
            let corner_spot = Vec3::new(corner_x, if self.rng.next_boolean() { 0.5 } else { 67.5 }, 0.0);
            self.open_dead_ball(DeadBallKind::Corner, att_side, corner_spot, false, false)?;
            return Ok((att_side, shot_out.next_carrier_idx, true, 0));
        }

        if had_goal_kick {
            self.open_dead_ball(DeadBallKind::GoalKick, def_side, last_pos, false, false)?;
            return Ok((def_side, shot_out.next_carrier_idx, true, 0));
        }

        Ok((
            shot_out.next_side,
            shot_out.next_carrier_idx,
            false,
            900,
        ))
    }

    fn complete_current_half(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        let elapsed_ms = self.state.des_clock.period_elapsed_ms;
        self.scheduler.invalidate_period();
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.state.dead_ball = None;

        match period {
            Period::FirstHalf => {
                self.first_half_elapsed_ms = elapsed_ms;
                self.emit(
                    EventTypeV4::HalfTime,
                    HighlightTier::Extended,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.stage = HalfStageV4::HalfTimeBreak;
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
            }
            Period::SecondHalf => {
                self.second_half_elapsed_ms = elapsed_ms;
                if self.ruleset.extra_time_enabled
                    && self.state.home.goals == self.state.away.goals
                {
                    self.stage = HalfStageV4::ExtraTimeBreak {
                        next_period: Period::ExtraTimeFirst,
                    };
                    if auto_advance_breaks {
                        self.begin_next_period_internal()?;
                    }
                } else {
                    self.finish_full_time();
                }
            }
            Period::ExtraTimeFirst => {
                self.et1_elapsed_ms = elapsed_ms;
                self.emit(
                    EventTypeV4::ExtraTimeHalfTime,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.stage = HalfStageV4::ExtraTimeBreak {
                    next_period: Period::ExtraTimeSecond,
                };
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
            }
            Period::ExtraTimeSecond => {
                self.et2_elapsed_ms = elapsed_ms;
                if self.ruleset.penalty_shootout_enabled
                    && self.state.home.goals == self.state.away.goals
                {
                    self.stage = HalfStageV4::PenaltyShootout;
                    self.run_penalty_shootout()?;
                }
                self.finish_full_time();
            }
            _ => {
                self.finish_full_time();
            }
        }
        Ok(())
    }

    fn finish_full_time(&mut self) {
        self.state.des_clock.period = Period::FullTime;
        self.state.sync_legacy_clock();
        self.emit(
            EventTypeV4::FullTime,
            HighlightTier::Extended,
            None,
            None,
            None,
            None,
            EventMetadataV4::default(),
        );
        self.stage = HalfStageV4::FullTime;
    }

    fn schedule_next_decision(
        &mut self,
        carrier_idx: usize,
        side: TeamSideV4,
        now_ms: i64,
        one_touch: bool,
    ) -> Result<(), V4Error> {
        let cid = self.state.players[carrier_idx].player_id.clone();
        let delay_ms: i64 = 1;
        let _ = one_touch; // actual intent belongs to the upcoming PI observation
        self.scheduler
            .schedule(
                now_ms.saturating_add(delay_ms),
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DecisionReady {
                    player_id: cid,
                    side,
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        Ok(())
    }

    fn flush_play_events(&mut self, events: Vec<EmittedPlayEvent>, att_side: TeamSideV4) {
        let g_sec = self.state.des_clock.game_second();
        let flags = compute_tactical_flags(&self.state, att_side, g_sec);
        let actual_ball_pos = self.state.ball.pos;
        for mut ev in events {
            attach_tactical_flags(&mut ev.metadata, flags);
            self.state.ball.pos = ev.ball_pos.clamp_to_pitch();
            self.emit(
                ev.kind,
                ev.tier,
                ev.seq_id,
                ev.side,
                ev.actor,
                ev.target,
                ev.metadata,
            );
        }
        self.state.ball.pos = actual_ball_pos;
    }

    /// Law 10: snapshot eligible on-pitch players, equalize team counts, and run shootout rounds.
    fn run_penalty_shootout(&mut self) -> Result<(), V4Error> {
        self.went_to_shootout = true;
        self.state
            .des_clock
            .begin_period(Period::Shootout, 0);
        self.state.sync_legacy_clock();
        let seq_id = self.alloc_highlight_sequence();
        self.emit(
            EventTypeV4::PenaltyShootoutStarted,
            HighlightTier::GoalKey,
            Some(seq_id),
            None,
            None,
            None,
            EventMetadataV4::default(),
        );

        let mut home_order = self.shootout_eligible_order(TeamSideV4::Home);
        let mut away_order = self.shootout_eligible_order(TeamSideV4::Away);
        let equal_count = home_order.len().min(away_order.len());
        if equal_count == 0 {
            return Err(V4Error::InvalidInput("no eligible penalty kickers".into()));
        }
        home_order.truncate(equal_count);
        away_order.truncate(equal_count);

        let mut home_kicks: u32 = 0;
        let mut away_kicks: u32 = 0;
        let mut round: u32 = 0;
        const MAX_ROUNDS: u32 = 200;
        'shootout: loop {
            if round >= MAX_ROUNDS {
                return Err(V4Error::InvalidInput(
                    "penalty shootout exceeded simulation safety limit without a winner".into(),
                ));
            }
            round += 1;
            for side in [TeamSideV4::Home, TeamSideV4::Away] {
                let order = if side == TeamSideV4::Home {
                    &home_order
                } else {
                    &away_order
                };
                let kick_number = if side == TeamSideV4::Home {
                    home_kicks
                } else {
                    away_kicks
                };
                let taker_idx = order[(kick_number as usize) % order.len()];
                let gk_idx = self.find_goalkeeper_index(side.opposite());
                let taker_id = self.state.players[taker_idx].player_id.clone();
                let gk_id = self.state.players[gk_idx].player_id.clone();
                let taker_attrs = *lookup_attrs(&self.input, &self.state.players[taker_idx]);
                let gk_attrs = *lookup_attrs(&self.input, &self.state.players[gk_idx]);
                let pen_skill = normalize_attr(taker_attrs.technical.penalties) * 0.50
                    + normalize_attr(taker_attrs.mental.composure) * 0.30
                    + normalize_attr(taker_attrs.hidden.pressure) * 0.20;
                let gk_skill = normalize_attr(gk_attrs.goalkeeping.reflexes) * 0.65
                    + normalize_attr(gk_attrs.goalkeeping.one_on_ones) * 0.35;
                let score_prob = (0.76 + (pen_skill - gk_skill) * 0.18).clamp(0.55, 0.92);
                let scored = (self.rng.next_double() as f32) < score_prob;
                match side {
                    TeamSideV4::Home => {
                        home_kicks += 1;
                        if scored {
                            self.state.shootout_home_goals += 1;
                        }
                    }
                    TeamSideV4::Away => {
                        away_kicks += 1;
                        if scored {
                            self.state.shootout_away_goals += 1;
                        }
                    }
                }
                let mut meta = EventMetadataV4 {
                    scored: Some(scored),
                    goalkeeper: Some(gk_id),
                    ..EventMetadataV4::default()
                };
                meta.extra.insert("round".into(), round.to_string());
                meta.extra.insert(
                    "kickNumber".into(),
                    kick_number.saturating_add(1).to_string(),
                );
                self.emit(
                    EventTypeV4::PenaltyShootoutKick,
                    HighlightTier::GoalKey,
                    Some(seq_id),
                    Some(side),
                    Some(taker_id),
                    None,
                    meta,
                );

                if round <= 5 {
                    let home_remaining = 5u32.saturating_sub(home_kicks);
                    let away_remaining = 5u32.saturating_sub(away_kicks);
                    if self.state.shootout_home_goals as u32
                        > self.state.shootout_away_goals as u32 + away_remaining
                        || self.state.shootout_away_goals as u32
                            > self.state.shootout_home_goals as u32 + home_remaining
                    {
                        break 'shootout;
                    }
                }
            }
            if round >= 5 && self.state.shootout_home_goals != self.state.shootout_away_goals {
                break;
            }
        }
        self.emit(
            EventTypeV4::PenaltyShootoutEnded,
            HighlightTier::GoalKey,
            Some(seq_id),
            None,
            None,
            None,
            EventMetadataV4::default(),
        );
        Ok(())
    }

    fn shootout_eligible_order(&self, side: TeamSideV4) -> Vec<usize> {
        let mut eligible: Vec<usize> = self
            .state
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == side && p.on_pitch && !p.sent_off)
            .map(|(i, _)| i)
            .collect();
        eligible.sort_by(|&a, &b| {
            let ar = lookup_attrs(&self.input, &self.state.players[a]);
            let br = lookup_attrs(&self.input, &self.state.players[b]);
            let ascore = ar.technical.penalties as u16 * 3
                + ar.mental.composure as u16 * 2
                + ar.hidden.pressure as u16;
            let bscore = br.technical.penalties as u16 * 3
                + br.mental.composure as u16 * 2
                + br.hidden.pressure as u16;
            bscore
                .cmp(&ascore)
                .then_with(|| self.state.players[a].player_id.cmp(&self.state.players[b].player_id))
        });
        eligible
    }

    /// Executes the post-celebration Kickoff once `DeadBallKind::GoalCelebration` finishes
    /// `ExecutingRestart` (strictly later in `sim_time_ms` than the `Goal` event).
    fn restart_after_goal_kickoff(&mut self, kickoff_side: TeamSideV4) -> usize {
        self.reset_kickoff_positions(kickoff_side);
        let kicker = self.kickoff_carrier_index(kickoff_side);
        let kicker_id = self.state.players[kicker].player_id.clone();
        self.state.ball.pos = Vec3::center_spot();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(kickoff_side);
        self.state.ball.owner_player_id = Some(kicker_id.clone());
        self.state.ball.last_touch_side = None;
        self.state.ball.last_touch_player_id = None;
        self.possession_side = kickoff_side;
        self.carrier_idx = kicker;
        self.last_passer_idx = None;
        self.emit(
            EventTypeV4::Kickoff,
            HighlightTier::None,
            None,
            Some(kickoff_side),
            Some(kicker_id),
            None,
            EventMetadataV4::default(),
        );
        kicker
    }

    fn reset_kickoff_positions(&mut self, _kickoff_side: TeamSideV4) {
        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                continue;
            }
            let side = self.state.players[idx].side;
            let slot_id = self.state.players[idx].slot_id.clone();
            let team = match side {
                TeamSideV4::Home => &self.input.home_team,
                TeamSideV4::Away => &self.input.away_team,
            };
            if let Some(slot) = team.tactical_slots.iter().find(|s| s.slot_id == slot_id) {
                let mut pos = Vec2::from_normalized_slot(side, slot.out_x as f32, slot.out_y as f32);
                match side {
                    TeamSideV4::Home => pos.x = pos.x.min(PITCH_LENGTH_M * 0.49),
                    TeamSideV4::Away => pos.x = pos.x.max(PITCH_LENGTH_M * 0.51),
                }
                self.state.players[idx].pos = pos;
                self.state.players[idx].target_pos = pos;
                self.state.players[idx].vel = Vec2::default();
                self.state.players[idx].last_moved_m = 0.0;
            }
        }
        self.state.ball.pos = Vec3::center_spot();
    }

    fn kickoff_carrier_index(&self, side: TeamSideV4) -> usize {
        self.state
            .players
            .iter()
            .rposition(|p| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .unwrap_or(0)
    }

    fn ensure_valid_carrier(&mut self) {
        if self.carrier_idx >= self.state.players.len()
            || !self.state.players[self.carrier_idx].on_pitch
            || self.state.players[self.carrier_idx].sent_off
        {
            self.carrier_idx = self.kickoff_carrier_index(self.possession_side);
        }
    }

    fn update_interval_fatigue(
        &mut self,
        dt_sec: f32,
        possession_side: TeamSideV4,
        is_dead_ball: bool,
    ) {
        if dt_sec <= 0.0 {
            return;
        }
        let home_press = self.state.active_home_ti.pressing_intensity as f32 / 100.0;
        let away_press = self.state.active_away_ti.pressing_intensity as f32 / 100.0;

        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                continue;
            }
            let side = self.state.players[idx].side;
            let is_gk = self.state.players[idx].is_goalkeeper;
            let press = match side {
                TeamSideV4::Home => home_press,
                TeamSideV4::Away => away_press,
            };
            let attrs = *lookup_attrs(&self.input, &self.state.players[idx]);
            let moved_m = if is_dead_ball {
                // Light walking/repositioning during stoppages (~0.35 m/s).
                dt_sec * if is_gk { 0.15 } else { 0.35 }
            } else {
                self.state.players[idx].last_moved_m.max(0.0)
            };
            let speed_mps = moved_m / dt_sec.max(0.05);
            let high_intensity = !is_dead_ball
                && !is_gk
                && (speed_mps >= 4.2 || (side != possession_side && press >= 0.70 && speed_mps >= 3.2));

            self.state.players[idx].fatigue.apply_interval(
                dt_sec,
                moved_m,
                high_intensity,
                is_dead_ball,
                attrs.physical.stamina,
                attrs.physical.natural_fitness,
            );
        }
    }

    fn apply_half_time_recovery(&mut self) {
        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch {
                continue;
            }
            let attrs = *lookup_attrs(&self.input, &self.state.players[idx]);
            self.state.players[idx]
                .fatigue
                .apply_half_time_recovery(attrs.physical.natural_fitness);
        }
    }

    fn team_tactical_readiness(&self, side: TeamSideV4) -> (f32, f32) {
        let mut tw_sum = 0.0f32;
        let mut dec_sum = 0.0f32;
        let mut count = 0u32;
        for p in &self.state.players {
            if p.side == side && p.on_pitch && !p.sent_off {
                let a = lookup_attrs(&self.input, p);
                tw_sum += normalize_attr(a.mental.teamwork);
                dec_sum += normalize_attr(a.mental.decisions);
                count += 1;
            }
        }
        if count == 0 {
            (0.5, 0.5)
        } else {
            (tw_sum / count as f32, dec_sum / count as f32)
        }
    }

    fn team_avg_work_rate(&self, side: TeamSideV4) -> f32 {
        let mut wr_sum = 0.0f32;
        let mut count = 0u32;
        for p in &self.state.players {
            if p.side == side && p.on_pitch && !p.sent_off && !p.is_goalkeeper {
                let a = lookup_attrs(&self.input, p);
                wr_sum += normalize_attr(a.mental.work_rate);
                count += 1;
            }
        }
        if count == 0 {
            0.5
        } else {
            wr_sum / count as f32
        }
    }

    fn goal_diff_for(&self, side: TeamSideV4) -> i32 {
        self.state.team_state(side).goals as i32
            - self.state.team_state(side.opposite()).goals as i32
    }

    fn has_pending_substitutions(&self) -> bool {
        self.pending_substitutions
            .iter()
            .any(|s| s.status == CommandLifecycleStatus::QueuedForDeadBall)
    }

    fn count_pending_substitutions(&self) -> usize {
        self.pending_substitutions
            .iter()
            .filter(|s| s.status == CommandLifecycleStatus::QueuedForDeadBall)
            .count()
    }

    fn find_goalkeeper_index(&self, side: TeamSideV4) -> usize {
        self.state
            .players
            .iter()
            .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .or_else(|| {
                self.state
                    .players
                    .iter()
                    .position(|p| p.side == side && p.on_pitch && !p.sent_off)
            })
            .unwrap_or(0)
    }

    fn pick_outfield_index(&mut self, side: TeamSideV4) -> usize {
        let candidates: Vec<usize> = self
            .state
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return 0;
        }
        let pick = self.rng.next_int(candidates.len() as i32) as usize;
        candidates[pick]
    }

    fn alloc_highlight_sequence(&mut self) -> u32 {
        let id = self.state.next_highlight_sequence_id;
        self.state.next_highlight_sequence_id =
            self.state.next_highlight_sequence_id.saturating_add(1);
        id
    }

    fn alloc_command_id(&mut self) -> u64 {
        let id = self.next_command_id;
        self.next_command_id = self.next_command_id.saturating_add(1);
        id
    }

    /// Emits an event: in `FastBatch` (`retain_events == false`), computes the exact same
    /// `timeline_fingerprint` inline without allocating a `MatchEventV4` entry in `self.events`.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        kind: EventTypeV4,
        highlight_tier: HighlightTier,
        highlight_sequence_id: Option<u32>,
        side: Option<TeamSideV4>,
        actor: Option<String>,
        target: Option<String>,
        metadata: EventMetadataV4,
    ) {
        self.event_count = self.event_count.saturating_add(1);
        let ball_pos = self.state.ball.pos.clamp_to_pitch();
        let sim_time_ms = self.state.des_clock.sim_time_ms;
        let period_elapsed_ms = self.state.des_clock.period_elapsed_ms;
        let g_sec = self.state.des_clock.game_second();
        let p_sec = self.state.des_clock.period_elapsed_second();
        let period = self.state.des_clock.period as i32;

        let ev_fp = fingerprint_event_parts(
            self.event_count,
            g_sec,
            sim_time_ms,
            period,
            kind,
            highlight_tier,
            highlight_sequence_id,
            side,
            actor.as_deref(),
            target.as_deref(),
            ball_pos,
            &metadata,
        );
        self.timeline_fingerprint = fingerprints::mix_i64(self.timeline_fingerprint, ev_fp as i64);

        self.diagnostics.on_event(kind, side, ball_pos, &metadata);

        // Use the authoritative emission boundary, not retained commentary.
        // Shootout kicks are a separate phase and do not feed open-play PI.
        if self.state.des_clock.period!=Period::Shootout {
            crate::v4::psychology::observe_personal_outcome(&mut self.state,kind,side,actor.as_deref(),&metadata);
        }

        if self.retain_events {
            self.events.push(MatchEventV4 {
                sequence: self.event_count,
                game_second: g_sec,
                period,
                period_elapsed_second: p_sec,
                sim_time_ms,
                period_elapsed_ms,
                dt_ms: self.last_dt_ms,
                kind,
                highlight_tier,
                highlight_sequence_id,
                side,
                actor,
                target,
                ball_pos,
                metadata,
            });
        }
    }

    pub(super) fn build_result(&mut self) -> MatchResultV4 {
        let total_poss_ms = (self.home_possession_ms + self.away_possession_ms).max(1);
        let home_poss = (self.home_possession_ms as f64 / total_poss_ms as f64 * 100.0) as f32;
        let away_poss = 100.0 - home_poss;
        let home_goals = self.state.home.goals;
        let away_goals = self.state.away.goals;

        // Populate DES clock totals and average outfield distance into diagnostics.
        let dc = &self.state.des_clock;
        self.diagnostics.record_des_clock_totals(
            dc.ball_in_play_ms,
            dc.ball_out_of_play_ms,
            dc.compensable_lost_ms,
            dc.announced_added_ms,
            dc.additional_added_ms,
        );
        let mut outfield_dist_sum = 0.0f32;
        let mut outfield_count = 0u32;
        for p in &self.state.players {
            if !p.is_goalkeeper {
                outfield_dist_sum += p.fatigue.distance_m;
                outfield_count += 1;
            }
        }
        if outfield_count > 0 {
            self.diagnostics
                .set_avg_player_distance_m(outfield_dist_sum / outfield_count as f32);
        }

        let summary = MatchSummaryV4 {
            home: TeamMatchSummaryV4 {
                team_id: self.input.home_team.team_id.clone(),
                goals: home_goals,
                shots: self.state.home.shots,
                shots_on_target: self.state.home.shots_on_target,
                shots_off_target: self.state.home.shots_off_target,
                shots_blocked: self.state.home.shots_blocked,
                saves: self.state.home.saves,
                own_goals_for: self.state.home.own_goals_for,
                own_goals_conceded: self.state.home.own_goals_conceded,
                passes_attempted: self.state.home.passes_attempted,
                passes_completed: self.state.home.passes_completed,
                possession_pct: (home_poss * 10.0).round() / 10.0,
                xg: (self.state.home.xg_millis as f32) / 1000.0,
                fouls: self.state.home.fouls,
                yellow_cards: self.state.home.yellow_cards,
                red_cards: self.state.home.red_cards,
                corners: self.state.home.corners,
                offsides: self.state.home.offsides,
                throw_ins: self.state.home.throw_ins,
            },
            away: TeamMatchSummaryV4 {
                team_id: self.input.away_team.team_id.clone(),
                goals: away_goals,
                shots: self.state.away.shots,
                shots_on_target: self.state.away.shots_on_target,
                shots_off_target: self.state.away.shots_off_target,
                shots_blocked: self.state.away.shots_blocked,
                saves: self.state.away.saves,
                own_goals_for: self.state.away.own_goals_for,
                own_goals_conceded: self.state.away.own_goals_conceded,
                passes_attempted: self.state.away.passes_attempted,
                passes_completed: self.state.away.passes_completed,
                possession_pct: (away_poss * 10.0).round() / 10.0,
                xg: (self.state.away.xg_millis as f32) / 1000.0,
                fouls: self.state.away.fouls,
                yellow_cards: self.state.away.yellow_cards,
                red_cards: self.state.away.red_cards,
                corners: self.state.away.corners,
                offsides: self.state.away.offsides,
                throw_ins: self.state.away.throw_ins,
            },
            final_game_second: self.state.des_clock.game_second(),
            first_half_elapsed_seconds: (self.first_half_elapsed_ms / 1000) as i32,
            second_half_elapsed_seconds: (self.second_half_elapsed_ms / 1000) as i32,
            extra_time_first_elapsed_seconds: (self.et1_elapsed_ms / 1000) as i32,
            extra_time_second_elapsed_seconds: (self.et2_elapsed_ms / 1000) as i32,
            went_to_extra_time: self.went_to_extra_time,
            went_to_shootout: self.went_to_shootout,
            shootout_home_goals: if self.went_to_shootout {
                Some(self.state.shootout_home_goals)
            } else {
                None
            },
            shootout_away_goals: if self.went_to_shootout {
                Some(self.state.shootout_away_goals)
            } else {
                None
            },
            players: self
                .state
                .players
                .iter()
                .map(|p| {
                    let attrs = lookup_attrs(&self.input, p);
                    let rating = (p.rating_tenths as f32) / 10.0;
                    let (won, drew) = match p.side {
                        TeamSideV4::Home => (home_goals > away_goals, home_goals == away_goals),
                        TeamSideV4::Away => (away_goals > home_goals, home_goals == away_goals),
                    };
                    PlayerMatchSummaryV4 {
                        player_id: p.player_id.clone(),
                        side: p.side,
                        goals: p.goals,
                        own_goals: p.own_goals,
                        assists: p.assists,
                        yellow_cards: p.yellow_cards,
                        sent_off: p.sent_off,
                        suspended_next_match: p.sent_off,
                        injury: p.injury,
                        final_aerobic: (p.fatigue.aerobic * 10.0).round() / 10.0,
                        final_anaerobic: (p.fatigue.anaerobic * 10.0).round() / 10.0,
                        post_match_condition: p.fatigue.aerobic.round().clamp(5.0, 100.0) as u8,
                        morale_delta: compute_post_match_morale_delta(attrs, won, drew, rating),
                        distance_m: p.fatigue.distance_m.round(),
                        rating,
                    }
                })
                .collect(),
        };

        MatchResultV4 {
            ok: true,
            protocol_version: ENGINE_PROTOCOL_VERSION_V4 as i32,
            engine_version: ENGINE_VERSION_V4.to_string(),
            event_schema_version: EVENT_SCHEMA_VERSION_V4 as i32,
            match_id: self.input.match_id.clone(),
            seed: self.input.seed,
            simulation_mode: self.input.simulation_mode,
            input_fingerprint: format!("{:016x}", self.input.input_fingerprint()),
            event_timeline_fingerprint: format!("{:016x}", self.timeline_fingerprint),
            event_count: self.event_count,
            command_timeline: self.command_timeline.clone(),
            summary,
            diagnostics: Some(self.diagnostics.finish()),
            events: std::mem::take(&mut self.events),
        }
    }
}

fn map_dead_ball_diag_kind(kind: DeadBallKind) -> Option<DeadBallDiagKind> {
    match kind {
        DeadBallKind::GoalCelebration => Some(DeadBallDiagKind::GoalCelebration),
        DeadBallKind::ThrowIn => Some(DeadBallDiagKind::ThrowIn),
        DeadBallKind::Corner => Some(DeadBallDiagKind::Corner),
        DeadBallKind::GoalKick => Some(DeadBallDiagKind::GoalKick),
        DeadBallKind::QuickFreeKick
        | DeadBallKind::DirectFreeKick
        | DeadBallKind::IndirectFreeKick
        | DeadBallKind::Offside => Some(DeadBallDiagKind::FreeKick),
        DeadBallKind::PenaltyKick => Some(DeadBallDiagKind::Penalty),
        DeadBallKind::InjuryStoppage => Some(DeadBallDiagKind::Injury),
        DeadBallKind::SubstitutionWindow => Some(DeadBallDiagKind::Substitution),
        DeadBallKind::Kickoff => None,
    }
}

#[cfg(test)]
include!("session_tests.rs");
