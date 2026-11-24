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
pub const TACTICAL_LIVE_START_MIN_MS: i64 = 90_000;
pub const TACTICAL_LIVE_START_MAX_MS: i64 = 270_000;
pub const TACTICAL_LIVE_RAMP_MIN_MS: i64 = 30_000;
pub const TACTICAL_LIVE_RAMP_MAX_MS: i64 = 90_000;

/// Dead-ball tactical command propagation delay bounds (faster communication during stoppage).
pub const TACTICAL_DEAD_BALL_START_MIN_MS: i64 = 20_000;
pub const TACTICAL_DEAD_BALL_START_MAX_MS: i64 = 60_000;
pub const TACTICAL_DEAD_BALL_RAMP_MIN_MS: i64 = 15_000;
pub const TACTICAL_DEAD_BALL_RAMP_MAX_MS: i64 = 40_000;

/// Context flags emitted onto V4 events to feed `LabCommentaryTemplatePolicy.kt`.
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

/// Computes natural tactical propagation delay `(start_delay_ms, ramp_duration_ms)`
/// when a manager issues new `TeamInstructionsV4`:
/// - Live ball: `start_delay_ms` in `90_000..=270_000 ms` (1.5–4.5 min),
///   `ramp_duration_ms` in `30_000..=90_000 ms`.
/// - Dead ball: `start_delay_ms` in `20_000..=60_000 ms`,
///   `ramp_duration_ms` in `15_000..=40_000 ms`.
/// For `HalfTimeBreak` / `ExtraTimeBreak`, use `compute_tactical_delay_for_context_ms`
/// (returns `(0, 0)` so instructions apply immediately at kickoff).
pub fn compute_tactical_delay_ms(
    old_instr: &TeamInstructionsV4,
    new_instr: &TeamInstructionsV4,
    is_dead_ball: bool,
    avg_teamwork_norm: f32,
    avg_decisions_norm: f32,
    rng: &mut DeterministicRng,
) -> (i64, i64) {
    let mag = tactical_change_magnitude(old_instr, new_instr);
    let readiness = (avg_teamwork_norm.clamp(0.0, 1.0) * 0.55
        + avg_decisions_norm.clamp(0.0, 1.0) * 0.45)
        .clamp(0.0, 1.0);

    let jitter_start = (rng.next_double() as f32 - 0.5) * 2.0;
    let jitter_ramp = (rng.next_double() as f32 - 0.5) * 2.0;

    if is_dead_ball {
        let start_ms = (28_000.0
            + mag * 22_000.0
            + (1.0 - readiness) * 12_000.0
            + jitter_start * 6_000.0)
            .round() as i64;
        let ramp_ms = (20_000.0
            + mag * 12_000.0
            + (1.0 - readiness) * 7_000.0
            + jitter_ramp * 4_000.0)
            .round() as i64;
        (
            start_ms.clamp(TACTICAL_DEAD_BALL_START_MIN_MS, TACTICAL_DEAD_BALL_START_MAX_MS),
            ramp_ms.clamp(TACTICAL_DEAD_BALL_RAMP_MIN_MS, TACTICAL_DEAD_BALL_RAMP_MAX_MS),
        )
    } else {
        let start_ms = (125_000.0
            + mag * 85_000.0
            + (1.0 - readiness) * 55_000.0
            + jitter_start * 22_000.0)
            .round() as i64;
        let ramp_ms = (45_000.0
            + mag * 28_000.0
            + (1.0 - readiness) * 16_000.0
            + jitter_ramp * 8_000.0)
            .round() as i64;
        (
            start_ms.clamp(TACTICAL_LIVE_START_MIN_MS, TACTICAL_LIVE_START_MAX_MS),
            ramp_ms.clamp(TACTICAL_LIVE_RAMP_MIN_MS, TACTICAL_LIVE_RAMP_MAX_MS),
        )
    }
}

/// Context-aware wrapper for `compute_tactical_delay_ms` that returns `(0, 0)`
/// during `HalfTimeBreak` / `ExtraTimeBreak` so changes take effect immediately at kickoff.
pub fn compute_tactical_delay_for_context_ms(
    old_instr: &TeamInstructionsV4,
    new_instr: &TeamInstructionsV4,
    is_dead_ball: bool,
    is_half_time_break: bool,
    avg_teamwork_norm: f32,
    avg_decisions_norm: f32,
    rng: &mut DeterministicRng,
) -> (i64, i64) {
    if is_half_time_break {
        return (0, 0);
    }
    compute_tactical_delay_ms(
        old_instr,
        new_instr,
        is_dead_ball,
        avg_teamwork_norm,
        avg_decisions_norm,
        rng,
    )
}

/// Linearly blends `TeamInstructionsV4` from `from` to `to` with progress `alpha` in `[0.0, 1.0]`
/// during the `Propagating` ramp phase.
pub fn blend_tactical_instructions(
    from: &TeamInstructionsV4,
    to: &TeamInstructionsV4,
    alpha: f32,
) -> TeamInstructionsV4 {
    let t = alpha.clamp(0.0, 1.0);
    if t <= 0.0 {
        return from.clone();
    }
    if t >= 1.0 {
        return to.clone();
    }
    TeamInstructionsV4 {
        pressing_intensity: lerp_u8(from.pressing_intensity, to.pressing_intensity, t),
        tempo: lerp_u8(from.tempo, to.tempo, t),
        line_height: lerp_u8(from.line_height, to.line_height, t),
        width: lerp_u8(from.width, to.width, t),
        time_wasting: lerp_u8(from.time_wasting, to.time_wasting, t),
        offside_trap: if t >= 0.5 { to.offside_trap } else { from.offside_trap },
        aggression: lerp_u8(from.aggression, to.aggression, t),
        transition_risk: lerp_u8(from.transition_risk, to.transition_risk, t),
        build_up_risk: lerp_u8(from.build_up_risk, to.build_up_risk, t),
        concentration: lerp_u8(from.concentration, to.concentration, t),
    }
}

fn tactical_change_magnitude(old: &TeamInstructionsV4, new: &TeamInstructionsV4) -> f32 {
    let numeric_sum = (old.pressing_intensity as i32 - new.pressing_intensity as i32).abs()
        + (old.tempo as i32 - new.tempo as i32).abs()
        + (old.line_height as i32 - new.line_height as i32).abs()
        + (old.width as i32 - new.width as i32).abs()
        + (old.time_wasting as i32 - new.time_wasting as i32).abs()
        + (old.aggression as i32 - new.aggression as i32).abs()
        + (old.transition_risk as i32 - new.transition_risk as i32).abs()
        + (old.build_up_risk as i32 - new.build_up_risk as i32).abs()
        + (old.concentration as i32 - new.concentration as i32).abs();
    let avg_delta = (numeric_sum as f32) / (9.0 * 100.0);
    let trap_delta = if old.offside_trap != new.offside_trap { 0.35 } else { 0.0 };
    (avg_delta * 1.8 + trap_delta).clamp(0.0, 1.0)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 100.0) as u8
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

/// Returns effective pressing multiplier during `DefensiveTransition` (counter-pressing)
/// vs settled `OutOfPossession`.
pub fn effective_press_multiplier(phase: TacticalPhaseV4, ti: &TeamInstructionsV4) -> f32 {
    let base = 0.72 + (ti.pressing_intensity as f32 / 100.0) * 0.56;
    match phase {
        TacticalPhaseV4::DefensiveTransition if ti.pressing_intensity >= 62 => base * 1.18,
        TacticalPhaseV4::DefensiveTransition if ti.pressing_intensity <= 40 => base * 0.85,
        _ => base,
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
