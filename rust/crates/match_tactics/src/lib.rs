//! Instruction propagation, interpolation and phase-dependent pressing.
use match_contracts::team_instructions::TeamInstructionsV4;
use match_contracts::tactical_phase::TacticalPhaseV4;
use match_contracts::ports::RngSampleSource;

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
    rng: &mut impl RngSampleSource,
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
    rng: &mut impl RngSampleSource,
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

