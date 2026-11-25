use crate::rng::DeterministicRng;
use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::TeamInstructionsV4;
use crate::v4::spatial::{attacking_progress_m, TeamSideV4, Vec3};
use crate::v4::state::{MatchStateV4, TacticalPhaseV4};

/// Transition window bounds (milliseconds) following a possession turnover.
pub const TRANSITION_WINDOW_MIN_MS: i32 = 12_000;
pub const TRANSITION_WINDOW_MAX_MS: i32 = 18_000;

/// Continuous sustained pressure accumulation threshold (milliseconds) in attacking half.
pub const SUSTAINED_PRESSURE_THRESHOLD_MS: i32 = 25_000;

/// Live-ball tactical command propagation delay bounds (1.5–4.5 simulated minutes).
pub use match_tactics::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct TacticalMetadataFlags {
    pub sustained_pressure: bool,
    pub high_defensive_line: bool,
    pub time_wasting: bool,
    pub ten_men: bool,
}

/// Updates the 4-phase tactical state machine (`InPossession`, `AttackingTransition`,
/// `DefensiveTransition`, `OutOfPossession`) over a DES interval `dt_ms` and emits
/// `TransitionStarted` / `TransitionEnded` events when phases shift.
///
/// NOTE (Wave 1 -> Wave 2 handoff): Uses `state.transition_remaining_ms: i32` and
/// `TeamRuntimeStateV4::sustained_pressure_ms: i32` (synchronized in `v4/state.rs` by Agent 1+5).
pub fn update_tactical_phases(
    state: &mut MatchStateV4,
    prev_possession: TeamSideV4,
    new_possession: TeamSideV4,
    ball_pos: Vec3,
    dt_ms: i64,
    avg_work_rate_norm: f32,
) -> Vec<EmittedPlayEvent> {
    let mut events = Vec::with_capacity(2);
    let flipped = prev_possession != new_possession;
    let dt_i32 = dt_ms.clamp(0, i32::MAX as i64) as i32;

    if flipped {
        // Turnover triggers a 12_000..=18_000 ms transition window scaled by work_rate and tempo
        let tempo_norm = (state.active_instructions(new_possession).tempo as f32 / 100.0).clamp(0.0, 1.0);
        let wr_norm = avg_work_rate_norm.clamp(0.0, 1.0);
        let speed_factor = (wr_norm * 0.55 + tempo_norm * 0.45).clamp(0.0, 1.0);
        let transition_ms = (TRANSITION_WINDOW_MAX_MS as f32
            - speed_factor * (TRANSITION_WINDOW_MAX_MS - TRANSITION_WINDOW_MIN_MS) as f32)
            .round() as i32;
        state.transition_remaining_ms =
            transition_ms.clamp(TRANSITION_WINDOW_MIN_MS, TRANSITION_WINDOW_MAX_MS);

        state.team_state_mut(new_possession).phase = TacticalPhaseV4::AttackingTransition;
        state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::DefensiveTransition;
        state.team_state_mut(new_possession.opposite()).sustained_pressure_ms = 0;

        events.push(EmittedPlayEvent {
            kind: EventTypeV4::TransitionStarted,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(new_possession),
            actor: None,
            target: None,
            ball_pos,
            metadata: EventMetadataV4 {
                outcome: Some("ATTACKING_TRANSITION".into()),
                ..EventMetadataV4::default()
            },
        });
    } else if state.transition_remaining_ms > 0 {
        state.transition_remaining_ms = state.transition_remaining_ms.saturating_sub(dt_i32);
        if state.transition_remaining_ms == 0 {
            state.team_state_mut(new_possession).phase = TacticalPhaseV4::InPossession;
            state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::OutOfPossession;
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::TransitionEnded,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(new_possession),
                actor: None,
                target: None,
                ball_pos,
                metadata: EventMetadataV4::default(),
            });
        }
    } else {
        state.team_state_mut(new_possession).phase = TacticalPhaseV4::InPossession;
        state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::OutOfPossession;
    }

    // Track continuous sustained pressure time in attacking half (progress >= 65m)
    let progress = attacking_progress_m(new_possession, ball_pos.xy());
    if progress >= 65.0 {
        let acc = &mut state.team_state_mut(new_possession).sustained_pressure_ms;
        *acc = acc.saturating_add(dt_i32);
    } else {
        state.team_state_mut(new_possession).sustained_pressure_ms = 0;
    }

    events
}

/// Computes live tactical commentary flags for the attacking & defending teams.
pub fn compute_tactical_flags(
    state: &MatchStateV4,
    att_side: TeamSideV4,
    game_second: i32,
) -> TacticalMetadataFlags {
    let def_side = att_side.opposite();
    let att_ti = state.active_instructions(att_side);
    let def_ti = state.active_instructions(def_side);

    let att_team = state.team_state(att_side);
    let def_team = state.team_state(def_side);

    let active_att_players = state
        .players
        .iter()
        .filter(|p| p.side == att_side && p.on_pitch && !p.sent_off)
        .count();
    let active_def_players = state
        .players
        .iter()
        .filter(|p| p.side == def_side && p.on_pitch && !p.sent_off)
        .count();

    let sustained_pressure = att_team.sustained_pressure_ms >= SUSTAINED_PRESSURE_THRESHOLD_MS;
    let high_defensive_line = def_ti.line_height >= 68;
    let leading = att_team.goals > def_team.goals;
    let time_wasting = leading && game_second >= 4200 && att_ti.time_wasting >= 55;
    let ten_men = active_att_players <= 10 || active_def_players <= 10;

    TacticalMetadataFlags {
        sustained_pressure,
        high_defensive_line,
        time_wasting,
        ten_men,
    }
}

/// Applies live tactical metadata flags onto an existing `EventMetadataV4` without overwriting
/// explicitly set fields.
pub fn attach_tactical_flags(meta: &mut EventMetadataV4, flags: TacticalMetadataFlags) {
    if flags.sustained_pressure && meta.sustained_pressure.is_none() {
        meta.sustained_pressure = Some(true);
    }
    if flags.high_defensive_line && meta.high_defensive_line.is_none() {
        meta.high_defensive_line = Some(true);
    }
    if flags.time_wasting && meta.time_wasting.is_none() {
        meta.time_wasting = Some(true);
    }
    if flags.ten_men && meta.ten_men.is_none() {
        meta.ten_men = Some(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tactical_delay_respects_live_dead_ball_and_halftime_bounds() {
        let base = TeamInstructionsV4::default();
        let aggressive = TeamInstructionsV4 {
            pressing_intensity: 90,
            tempo: 85,
            line_height: 80,
            offside_trap: true,
            ..TeamInstructionsV4::default()
        };
        let mut rng = DeterministicRng::new(42);

        for _ in 0..32 {
            let (live_start, live_ramp) =
                compute_tactical_delay_ms(&base, &aggressive, false, 0.6, 0.6, &mut rng);
            assert!((TACTICAL_LIVE_START_MIN_MS..=TACTICAL_LIVE_START_MAX_MS).contains(&live_start));
            assert!((TACTICAL_LIVE_RAMP_MIN_MS..=TACTICAL_LIVE_RAMP_MAX_MS).contains(&live_ramp));

            let (dead_start, dead_ramp) =
                compute_tactical_delay_ms(&base, &aggressive, true, 0.6, 0.6, &mut rng);
            assert!((TACTICAL_DEAD_BALL_START_MIN_MS..=TACTICAL_DEAD_BALL_START_MAX_MS).contains(&dead_start));
            assert!((TACTICAL_DEAD_BALL_RAMP_MIN_MS..=TACTICAL_DEAD_BALL_RAMP_MAX_MS).contains(&dead_ramp));
        }

        let ht = compute_tactical_delay_for_context_ms(&base, &aggressive, true, true, 0.5, 0.5, &mut rng);
        assert_eq!(ht, (0, 0));
    }

    #[test]
    fn blend_tactical_instructions_interpolates_endpoints_and_midpoint() {
        let from = TeamInstructionsV4 {
            pressing_intensity: 20,
            tempo: 40,
            offside_trap: false,
            ..TeamInstructionsV4::default()
        };
        let to = TeamInstructionsV4 {
            pressing_intensity: 80,
            tempo: 100,
            offside_trap: true,
            ..TeamInstructionsV4::default()
        };

        assert_eq!(blend_tactical_instructions(&from, &to, 0.0), from);
        assert_eq!(blend_tactical_instructions(&from, &to, 1.0), to);

        let mid = blend_tactical_instructions(&from, &to, 0.5);
        assert_eq!(mid.pressing_intensity, 50);
        assert_eq!(mid.tempo, 70);
        assert!(mid.offside_trap);
    }
}
