use serde::{Deserialize, Serialize};

use crate::v4::spatial::{
    closest_approach_time, is_in_goal_mouth, ray_pitch_boundary_hit_time, PitchBoundarySide,
    TeamSideV4, Vec2, Vec3,
};

/// Minimum adaptive substep duration in milliseconds (DES v1.4 Contract A Tier 2).
pub const ADAPTIVE_SUBSTEP_MIN_MS: i64 = 50;
/// Maximum adaptive substep duration in milliseconds (DES v1.4 Contract A Tier 2).
pub const ADAPTIVE_SUBSTEP_MAX_MS: i64 = 200;
/// Hard upper bound on adaptive substep iterations per continuous-interaction query.
pub const MAX_ADAPTIVE_SUBSTEPS: usize = 16;

/// Horizontal corridor threshold (meters) within which Tier 2 adaptive substepping activates.
pub const CONTEST_CORRIDOR_M: f32 = 6.5;
/// Distance threshold (meters) for pressing pressure on a ball carrier.
pub const PRESSING_THRESHOLD_M: f32 = 2.8;
/// Distance threshold (meters) for a direct tackle contest on a ball carrier.
pub const TACKLE_CONTEST_RADIUS_M: f32 = 1.65;
/// Maximum playable ball height (meters) for outfield interception / shot block.
pub const MAX_INTERCEPTABLE_HEIGHT_M: f32 = 2.40;
/// Standard downward gravitational acceleration (m/s^2).
pub const DEFAULT_GRAVITY_Z_MPS2: f32 = 9.81;

/// Analytic 3D ball trajectory parameterized by start timestamp, initial kinematics,
/// gravitational acceleration (`gravity_z >= 0.0`), and linear drag coefficient (`drag >= 0.0`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BallTrajectory {
    pub start_time_ms: i64,
    pub start_pos: Vec3,
    pub initial_vel: Vec3,
    /// Downward gravitational acceleration in m/s^2 (e.g. `9.81`).
    pub gravity_z: f32,
    /// Linear aerodynamic/turf drag coefficient in `1/s` (`0.0` = no drag).
    pub drag: f32,
    /// Marks whether this trajectory represents a shot on goal (produces `ShotBlocked`).
    pub is_shot: bool,
    /// Marks whether the ball is carried in a dribble (produces `TackleContest` / `PressingThresholdReached`).
    pub is_carrier_dribble: bool,
}

impl BallTrajectory {
    /// Evaluates the ball's continuous 3D position at `sim_time_ms`, accounting for
    /// linear drag and downward gravity, with `z` clamped to `>= 0.0`.
    pub fn position_at_ms(&self, sim_time_ms: i64) -> Vec3 {
        let dt_sec = ((sim_time_ms.saturating_sub(self.start_time_ms)).max(0) as f32) / 1000.0;
        if dt_sec <= 0.0 {
            return Vec3::new(self.start_pos.x, self.start_pos.y, self.start_pos.z.max(0.0));
        }

        let k = self.drag.max(0.0);
        let drag_integral = if k > 1e-5 {
            (1.0 - (-k * dt_sec).exp()) / k
        } else {
            dt_sec
        };

        let x = self.start_pos.x + self.initial_vel.x * drag_integral;
        let y = self.start_pos.y + self.initial_vel.y * drag_integral;
        let g = self.gravity_z.max(0.0);
        let z = (self.start_pos.z + self.initial_vel.z * drag_integral - 0.5 * g * dt_sec * dt_sec)
            .max(0.0);

        Vec3::new(x, y, z)
    }

    /// Evaluates instantaneous 3D velocity at `sim_time_ms`.
    pub fn velocity_at_ms(&self, sim_time_ms: i64) -> Vec3 {
        let dt_sec = ((sim_time_ms.saturating_sub(self.start_time_ms)).max(0) as f32) / 1000.0;
        if dt_sec <= 0.0 {
            return self.initial_vel;
        }
        let k = self.drag.max(0.0);
        let decay = if k > 1e-5 { (-k * dt_sec).exp() } else { 1.0 };
        let g = self.gravity_z.max(0.0);
        Vec3::new(
            self.initial_vel.x * decay,
            self.initial_vel.y * decay,
            self.initial_vel.z * decay - g * dt_sec,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BoundaryKind {
    HomeGoalLine,
    AwayGoalLine,
    LeftTouchline,
    RightTouchline,
}

impl From<PitchBoundarySide> for BoundaryKind {
    fn from(side: PitchBoundarySide) -> Self {
        match side {
            PitchBoundarySide::HomeGoalLine => BoundaryKind::HomeGoalLine,
            PitchBoundarySide::AwayGoalLine => BoundaryKind::AwayGoalLine,
            PitchBoundarySide::LeftTouchline => BoundaryKind::LeftTouchline,
            PitchBoundarySide::RightTouchline => BoundaryKind::RightTouchline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InteractionKind {
    GoalScored,
    BallOutOfBounds,
    PassInterception,
    ShotBlocked,
    PressingThresholdReached,
    TackleContest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarlyInteractionHit {
    pub time_ms: i64,
    pub kind: InteractionKind,
    pub ball_pos: Vec3,
    pub defender_id: Option<String>,
    pub defender_side: Option<TeamSideV4>,
    pub boundary: Option<BoundaryKind>,
    pub distance_m: f32,
    pub used_adaptive_substep: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefenderCandidate {
    pub player_id: String,
    pub side: TeamSideV4,
    pub pos: Vec2,
    pub max_speed_mps: f32,
    pub reach_m: f32,
    pub reaction_delay_ms: i64,
    /// Normalized anticipation attribute in `0.0..=1.0`.
    pub anticipation_norm: f32,
    /// Fatigue speed multiplier in `0.0..=1.0` (`1.0` = fresh).
    pub fatigue_factor: f32,
}

impl DefenderCandidate {
    #[inline]
    pub fn effective_speed_mps(&self) -> f32 {
        self.max_speed_mps.max(0.0) * self.fatigue_factor.clamp(0.45, 1.0)
    }

    #[inline]
    pub fn effective_reaction_ms(&self) -> i64 {
        let ant = self.anticipation_norm.clamp(0.0, 1.0);
        let scale = 1.15 - 0.35 * ant;
        ((self.reaction_delay_ms.max(0) as f32) * scale).round() as i64
    }

    #[inline]
    pub fn effective_reach_m(&self) -> f32 {
        let ant = self.anticipation_norm.clamp(0.0, 1.0);
        self.reach_m.max(0.2) * (0.95 + 0.15 * ant)
    }
}

/// Pure read-only continuous-interaction detector (DES v1.4 Contract A).
///
/// Scans `[t_now_ms, t_scheduled_ms]` in two tiers without mutating any simulation state:
/// 1. **Tier 1 (Analytic)**:
///    - Solves earliest pitch boundary / goal-mouth crossing (`GoalScored` vs `BallOutOfBounds`),
///      including exact inverse-drag mapping when `traj.drag > 0.0`.
///    - Screens candidate defenders via `closest_approach_time` and kinematic reachability
///      (`max_speed_mps`, `reach_m`, `reaction_delay_ms`, `anticipation_norm`, `fatigue_factor`).
/// 2. **Tier 2 (Bounded Adaptive Substep)**:
///    - Activates only when at least one defender enters the `6.5m` contest corridor.
///    - Steps in `50..=200ms` increments (at most `16` substeps) to pinpoint the earliest
///      `PassInterception`, `ShotBlocked`, `TackleContest`, or `PressingThresholdReached`.
pub fn find_earliest_ball_interaction(
    traj: &BallTrajectory,
    defenders: &[DefenderCandidate],
    t_now_ms: i64,
    t_scheduled_ms: i64,
) -> Option<EarlyInteractionHit> {
    if t_scheduled_ms <= t_now_ms {
        return None;
    }

    let span_total_ms = t_scheduled_ms - t_now_ms;
    let max_t_sec = (span_total_ms as f32) / 1000.0;
    let p_now = traj.position_at_ms(t_now_ms);
    let v_now = traj.velocity_at_ms(t_now_ms);
    let k = traj.drag.max(0.0);

    // --- Tier 1A: Analytic pitch boundary / goal-line crossing ---
    let mut best_hit: Option<EarlyInteractionHit> = None;

    let u_max = if k > 1e-5 {
        (1.0 - (-k * max_t_sec).exp()) / k
    } else {
        max_t_sec
    };

    if let Some((u_hit, side)) = ray_pitch_boundary_hit_time(p_now.xy(), v_now.xy(), u_max) {
        let t_hit_sec_opt = if k > 1e-5 {
            let arg = 1.0 - k * u_hit;
            if arg > 1e-6 {
                Some((-arg.ln() / k).clamp(0.0, max_t_sec))
            } else {
                None
            }
        } else {
            Some(u_hit.clamp(0.0, max_t_sec))
        };

        if let Some(t_hit_sec) = t_hit_sec_opt {
            let hit_time_ms =
                (t_now_ms + (t_hit_sec * 1000.0).round() as i64).clamp(t_now_ms, t_scheduled_ms);
            let ball_pos = traj.position_at_ms(hit_time_ms);
            let boundary = BoundaryKind::from(side);
            let is_goal_line = matches!(
                boundary,
                BoundaryKind::HomeGoalLine | BoundaryKind::AwayGoalLine
            );
            let kind = if is_goal_line && is_in_goal_mouth(ball_pos.y, ball_pos.z) {
                InteractionKind::GoalScored
            } else {
                InteractionKind::BallOutOfBounds
            };

            best_hit = Some(EarlyInteractionHit {
                time_ms: hit_time_ms,
                kind,
                ball_pos,
                defender_id: None,
                defender_side: None,
                boundary: Some(boundary),
                distance_m: 0.0,
                used_adaptive_substep: false,
            });
        }
    }

    // --- Tier 1B: Analytic defender screening ---
    let window_end_ms = best_hit
        .as_ref()
        .map_or(t_scheduled_ms, |hit| hit.time_ms)
        .min(t_scheduled_ms);
    let window_span_ms = window_end_ms - t_now_ms;
    if window_span_ms <= 0 || defenders.is_empty() {
        return best_hit;
    }

    let window_t_sec = (window_span_ms as f32) / 1000.0;
    let mut corridor_defenders: Vec<&DefenderCandidate> = Vec::new();

    for def in defenders {
        let (t_closest, dist_m) = closest_approach_time(p_now.xy(), v_now.xy(), def.pos, window_t_sec);
        let t_closest_ms = (t_closest * 1000.0).round() as i64;
        let move_ms = (t_closest_ms - def.effective_reaction_ms()).max(0);
        let max_cover_m =
            def.effective_speed_mps() * (move_ms as f32 / 1000.0) + def.effective_reach_m();

        // Enter Tier 2 if defender is within the 6.5m contest corridor or kinematically reachable.
        if dist_m <= CONTEST_CORRIDOR_M || dist_m <= max_cover_m {
            corridor_defenders.push(def);
        }
    }

    if corridor_defenders.is_empty() {
        return best_hit;
    }

    // --- Tier 2: Bounded adaptive substep (50..=200ms, max 16 substeps) ---
    let raw_step_ms =
        (window_span_ms + (MAX_ADAPTIVE_SUBSTEPS as i64) - 1) / (MAX_ADAPTIVE_SUBSTEPS as i64);
    let step_ms = raw_step_ms.clamp(ADAPTIVE_SUBSTEP_MIN_MS, ADAPTIVE_SUBSTEP_MAX_MS);

    for step_idx in 1..=MAX_ADAPTIVE_SUBSTEPS {
        let t_sample_ms = (t_now_ms + (step_idx as i64) * step_ms).min(window_end_ms);
        let dt_sample_ms = t_sample_ms - t_now_ms;
        let ball_pos = traj.position_at_ms(t_sample_ms);

        let mut step_best: Option<(f32, EarlyInteractionHit)> = None;

        for def in &corridor_defenders {
            let move_ms = (dt_sample_ms - def.effective_reaction_ms()).max(0);
            let cover_m = def.effective_speed_mps() * (move_ms as f32 / 1000.0);
            let dist_to_ball = def.pos.distance(ball_pos.xy());
            let residual_m = (dist_to_ball - cover_m).max(0.0);
            let reach_m = def.effective_reach_m();

            let matched_kind = if traj.is_carrier_dribble {
                if residual_m <= reach_m.max(TACKLE_CONTEST_RADIUS_M) && dist_to_ball - cover_m <= TACKLE_CONTEST_RADIUS_M {
                    Some(InteractionKind::TackleContest)
                } else if residual_m <= PRESSING_THRESHOLD_M {
                    Some(InteractionKind::PressingThresholdReached)
                } else {
                    None
                }
            } else if ball_pos.z <= MAX_INTERCEPTABLE_HEIGHT_M && residual_m <= reach_m {
                if traj.is_shot {
                    Some(InteractionKind::ShotBlocked)
                } else {
                    Some(InteractionKind::PassInterception)
                }
            } else {
                None
            };

            if let Some(kind) = matched_kind {
                let candidate_hit = EarlyInteractionHit {
                    time_ms: t_sample_ms,
                    kind,
                    ball_pos,
                    defender_id: Some(def.player_id.clone()),
                    defender_side: Some(def.side),
                    boundary: None,
                    distance_m: residual_m,
                    used_adaptive_substep: true,
                };

                let replace = match &step_best {
                    None => true,
                    Some((best_res, prev)) => {
                        residual_m < *best_res - 1e-4
                            || ((residual_m - *best_res).abs() <= 1e-4
                                && candidate_hit.defender_id < prev.defender_id)
                    }
                };
                if replace {
                    step_best = Some((residual_m, candidate_hit));
                }
            }
        }

        if let Some((_, def_hit)) = step_best {
            // Earlier than any boundary hit (or ties at window_end_ms, where defender beat the line).
            let beats_boundary = match &best_hit {
                None => true,
                Some(b_hit) => def_hit.time_ms < b_hit.time_ms,
            };
            if beats_boundary {
                return Some(def_hit);
            }
            break;
        }

        if t_sample_ms >= window_end_ms {
            break;
        }
    }

    best_hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v4::spatial::PITCH_HALF_WIDTH_M;

    fn make_defender(id: &str, x: f32, y: f32) -> DefenderCandidate {
        DefenderCandidate {
            player_id: id.into(),
            side: TeamSideV4::Away,
            pos: Vec2::new(x, y),
            max_speed_mps: 8.5,
            reach_m: 1.15,
            reaction_delay_ms: 140,
            anticipation_norm: 0.75,
            fatigue_factor: 0.95,
        }
    }

    #[test]
    fn trajectory_respects_gravity_and_drag() {
        let traj = BallTrajectory {
            start_time_ms: 1_000,
            start_pos: Vec3::new(50.0, 34.0, 0.2),
            initial_vel: Vec3::new(20.0, 0.0, 5.0),
            gravity_z: DEFAULT_GRAVITY_Z_MPS2,
            drag: 0.25,
            is_shot: false,
            is_carrier_dribble: false,
        };
        let p_mid = traj.position_at_ms(1_500);
        assert!(p_mid.x > 50.0 && p_mid.x < 60.0); // slowed by drag relative to 60.0m
        assert!(p_mid.z > 0.2);

        let p_late = traj.position_at_ms(3_000);
        assert_eq!(p_late.z, 0.0); // clamped to turf
    }

    #[test]
    fn detects_goal_scored_vs_out_of_bounds() {
        let goal_shot = BallTrajectory {
            start_time_ms: 0,
            start_pos: Vec3::new(95.0, PITCH_HALF_WIDTH_M, 0.5),
            initial_vel: Vec3::new(25.0, 0.0, 1.0),
            gravity_z: DEFAULT_GRAVITY_Z_MPS2,
            drag: 0.0,
            is_shot: true,
            is_carrier_dribble: false,
        };
        let hit = find_earliest_ball_interaction(&goal_shot, &[], 0, 1_000)
            .expect("expected goal crossing");
        assert_eq!(hit.kind, InteractionKind::GoalScored);
        assert_eq!(hit.boundary, Some(BoundaryKind::AwayGoalLine));
        assert_eq!(hit.time_ms, 400);

        let wide_shot = BallTrajectory {
            start_pos: Vec3::new(95.0, 50.0, 0.5),
            ..goal_shot
        };
        let oob = find_earliest_ball_interaction(&wide_shot, &[], 0, 1_000)
            .expect("expected out-of-bounds crossing");
        assert_eq!(oob.kind, InteractionKind::BallOutOfBounds);
        assert_eq!(oob.boundary, Some(BoundaryKind::AwayGoalLine));
    }

    #[test]
    fn defender_intercepts_pass_before_scheduled_arrival() {
        let pass = BallTrajectory {
            start_time_ms: 0,
            start_pos: Vec3::new(40.0, 34.0, 0.1),
            initial_vel: Vec3::new(18.0, 0.0, 0.0),
            gravity_z: DEFAULT_GRAVITY_Z_MPS2,
            drag: 0.05,
            is_shot: false,
            is_carrier_dribble: false,
        };
        let defender = make_defender("A04", 54.0, 36.2);
        let hit = find_earliest_ball_interaction(&pass, &[defender], 0, 1_500)
            .expect("expected pass interception");
        assert_eq!(hit.kind, InteractionKind::PassInterception);
        assert_eq!(hit.defender_id.as_deref(), Some("A04"));
        assert!(hit.used_adaptive_substep);
        assert!(hit.time_ms < 1_500);
    }

    #[test]
    fn defender_blocks_shot_before_goal_line() {
        let shot = BallTrajectory {
            start_time_ms: 0,
            start_pos: Vec3::new(88.0, PITCH_HALF_WIDTH_M, 0.4),
            initial_vel: Vec3::new(26.0, 0.0, 1.2),
            gravity_z: DEFAULT_GRAVITY_Z_MPS2,
            drag: 0.0,
            is_shot: true,
            is_carrier_dribble: false,
        };
        let blocker = make_defender("A05", 94.0, PITCH_HALF_WIDTH_M + 0.8);
        let hit = find_earliest_ball_interaction(&shot, &[blocker], 0, 1_000)
            .expect("expected shot block");
        assert_eq!(hit.kind, InteractionKind::ShotBlocked);
        assert_eq!(hit.defender_id.as_deref(), Some("A05"));
    }

    #[test]
    fn dribble_detects_tackle_or_pressing_threshold() {
        let dribble = BallTrajectory {
            start_time_ms: 0,
            start_pos: Vec3::new(60.0, 34.0, 0.0),
            initial_vel: Vec3::new(6.0, 0.0, 0.0),
            gravity_z: DEFAULT_GRAVITY_Z_MPS2,
            drag: 0.0,
            is_shot: false,
            is_carrier_dribble: true,
        };
        let tackler = make_defender("A06", 63.0, 34.6);
        let hit = find_earliest_ball_interaction(&dribble, &[tackler], 0, 1_000)
            .expect("expected dribble contest");
        assert!(matches!(
            hit.kind,
            InteractionKind::TackleContest | InteractionKind::PressingThresholdReached
        ));
    }
}
