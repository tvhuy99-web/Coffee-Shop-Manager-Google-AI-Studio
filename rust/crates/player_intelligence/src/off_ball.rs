//! Read-only position intentions. The engine alone applies movement, time and fatigue.
use match_contracts::ports::RngSampleSource;
use match_contracts::attributes::{normalize_attr, PlayerAttributes54};
use match_contracts::team_instructions::TeamInstructionsV4;
use match_contracts::spatial::{attacking_progress_m, TeamSideV4, Vec2, PITCH_LENGTH_M};
use match_contracts::tactical_phase::TacticalPhaseV4;

/// Inputs already available to the pre-extraction movement policy.
/// Values are borrowed or copied; no mutable match state is exposed.
pub struct MovementObservation<'a> {
    pub is_carrier: bool,
    pub is_gk: bool,
    pub in_poss: bool,
    pub side: TeamSideV4,
    pub phase: TacticalPhaseV4,
    pub attrs: &'a PlayerAttributes54,
    pub instructions: &'a TeamInstructionsV4,
    pub anchor: Vec2,
    pub ball_xy: Vec2,
    pub ball_progress_home: f32,
    pub receiving_line: Option<f32>,
    pub noise_scale: f32,
    pub slot_id: &'a str,
    pub position_familiarity: f32,
    pub aerobic: f32,
}

#[inline]
pub fn select_movement_target(observation: MovementObservation<'_>, rng: &mut impl RngSampleSource) -> Vec2 {
    let MovementObservation {
        is_carrier, is_gk, in_poss, side, phase, attrs, instructions: ti,
        anchor, ball_xy, ball_progress_home, receiving_line, noise_scale,
        slot_id, position_familiarity, aerobic,
    } = observation;
    if is_carrier {
            ball_xy
        } else if is_gk {
            let line_shift = (ti.line_height as f32 / 100.0) * 6.5;
            let rush_bonus = normalize_attr(attrs.goalkeeping.rushing_out) * 3.5;
            let gk_x = match side {
                TeamSideV4::Home => {
                    (4.5 + ball_progress_home * (line_shift + rush_bonus)).clamp(2.5, 16.5)
                }
                TeamSideV4::Away => (PITCH_LENGTH_M
                    - 4.5
                    - (1.0 - ball_progress_home) * (line_shift + rush_bonus))
                    .clamp(PITCH_LENGTH_M - 16.5, PITCH_LENGTH_M - 2.5),
            };
            let gk_y = (34.0 + (ball_xy.y - 34.0) * 0.22).clamp(24.0, 44.0);
            Vec2::new(gk_x, gk_y)
        } else if in_poss {
            let otb = normalize_attr(attrs.mental.off_the_ball);
            let work = normalize_attr(attrs.mental.work_rate);
            let width_factor = 0.80 + (ti.width as f32 / 100.0) * 0.40;
            let transition_surge = if phase == TacticalPhaseV4::AttackingTransition {
                (ti.transition_risk as f32 / 100.0) * 6.5
            } else {
                0.0
            };

            let shift_x = (ball_xy.x - PITCH_LENGTH_M * 0.5) * 0.34;
            let dir = match side {
                TeamSideV4::Home => 1.0f32,
                TeamSideV4::Away => -1.0f32,
            };
            let ball_prog_team = attacking_progress_m(side, ball_xy);
            let sid = slot_id;
            let (mut cut_in, mut extra_fwd) = (0.0f32, 0.0f32);
            if (sid == "LW" || sid == "RW") && ball_prog_team > 52.0 {
                cut_in = (34.0 - anchor.y) * (0.38 + otb * 0.25);
                extra_fwd = 4.5 + otb * 4.0;
            } else if (sid == "LCM" || sid == "RCM" || sid == "CM") && ball_prog_team > 58.0 {
                extra_fwd = 8.0 + otb * 9.0;
            } else if (sid == "LB" || sid == "RB") && ball_prog_team > 52.0 {
                extra_fwd = 9.0 + otb * 6.0;
            }
            let pocket_run = dir
                * (otb * 6.0 + work * 2.0 + extra_fwd + transition_surge)
                * position_familiarity;
            let jitter_x = (rng.next_double() as f32 - 0.5) * (6.0 - otb * 3.0) * noise_scale;
            let jitter_y = (rng.next_double() as f32 - 0.5) * (7.0 - otb * 3.0) * noise_scale;
            let stretched_y = 34.0 + (anchor.y - 34.0) * width_factor + cut_in;
            let mut target = Vec2::new(
                anchor.x + shift_x + pocket_run + jitter_x,
                stretched_y + jitter_y,
            )
            .clamp_to_pitch();
            if let Some(line) = receiving_line {
                // Better off-ball movement, anticipation and decisions improve run timing.
                // A weak reader can still overshoot; a defender stepping up can still trap
                // anyone. Only the intended destination changes, never teleport the player.
                let awareness = (otb * 0.50
                    + normalize_attr(attrs.mental.anticipation) * 0.25
                    + normalize_attr(attrs.mental.decisions) * 0.25)
                    * position_familiarity;
                let awareness = awareness.clamp(0.0, 1.0);
                let margin_m = 0.6 + 1.8 * awareness;
                let timing_error_m = 3.0 * (1.0 - awareness);
                let intended_limit = (line - margin_m + timing_error_m).clamp(0.0, PITCH_LENGTH_M);
                if attacking_progress_m(side, target) > intended_limit {
                    target.x = match side {
                        TeamSideV4::Home => intended_limit,
                        TeamSideV4::Away => PITCH_LENGTH_M - intended_limit,
                    };
                }
            }
            target
        } else {
            let pos_skill = normalize_attr(attrs.mental.positioning);
            let conc = normalize_attr(attrs.mental.concentration);
            let line_bias = ((ti.line_height as f32 - 50.0) / 50.0) * 12.0;
            let counter_press_push =
                if phase == TacticalPhaseV4::DefensiveTransition && ti.pressing_intensity >= 62 {
                    4.5
                } else {
                    0.0
                };
            let dir = match side {
                TeamSideV4::Home => 1.0f32,
                TeamSideV4::Away => -1.0f32,
            };
            let shift_x = (ball_xy.x - PITCH_LENGTH_M * 0.5) * 0.42
                + dir * (line_bias + counter_press_push);
            let compact_y = anchor.y + (ball_xy.y - anchor.y) * 0.28;

            let fatigue_penalty = ((100.0 - aerobic) / 100.0) * 0.35;
            let lapse_risk = ((1.0 - conc * 0.65 - pos_skill * 0.35)
                + fatigue_penalty
                + (1.0 - position_familiarity) * 0.6)
                .clamp(0.05, 0.85);
            let lapse_x = if (rng.next_double() as f32) < lapse_risk * 0.35 * noise_scale {
                (rng.next_double() as f32 - 0.5) * 9.5 * lapse_risk * noise_scale
            } else {
                (rng.next_double() as f32 - 0.5) * 2.4 * noise_scale
            };
            let lapse_y =
                (rng.next_double() as f32 - 0.5) * (3.0 + 4.0 * lapse_risk) * noise_scale;

            Vec2::new(anchor.x + shift_x + lapse_x, compact_y + lapse_y).clamp_to_pitch()
        }
}
