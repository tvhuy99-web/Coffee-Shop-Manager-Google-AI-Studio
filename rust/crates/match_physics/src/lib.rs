use serde::{Deserialize, Serialize};

use match_contracts::spatial::{
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

/// Observable ground-contact geometry shared by PI and execution.
/// The ball is still co-located with the carrier in this engine.
/// Facing is therefore a proxy for exposure, NOT a tracked foot or
/// a literal line-of-contact with the ball. Relative speed is a proxy
/// for the difficulty of synchronizing the challenge.
#[derive(Debug,Clone,Copy)]
pub struct GroundContactGeometry {
    pub access:f32,
    pub relative_speed_mps:f32,
}

impl GroundContactGeometry {
    pub fn observe(carrier_pos:Vec2,carrier_vel:Vec2,carrier_facing_rad:f32,
        defender_pos:Vec2,defender_vel:Vec2)->Self {
        let dx=defender_pos.x-carrier_pos.x;
        let dy=defender_pos.y-carrier_pos.y;
        let distance=(dx*dx+dy*dy).sqrt();
        let facing=Vec2::new(carrier_facing_rad.cos(),carrier_facing_rad.sin());
        let front=if distance>0.05 {(dx*facing.x+dy*facing.y)/distance}else{0.0};
        let rvx=defender_vel.x-carrier_vel.x;
        let rvy=defender_vel.y-carrier_vel.y;
        Self{access:((front+1.0)*0.5).clamp(0.0,1.0),
            relative_speed_mps:(rvx*rvx+rvy*rvy).sqrt()}
    }

    // Existing PI exposure and relative-speed priors, now shared.
    // Preserve the PI factors and arithmetic ordering. These are not
    // empirically calibrated probabilities of a physical foot contact.
    pub fn access_factor(self)->f32 {
        0.18+0.82*self.access.clamp(0.0,1.0)
    }
    pub fn speed_factor(self)->f32 {
        1.0-0.24*(self.relative_speed_mps/7.0).clamp(0.0,1.0)
    }

    /// Conditional chance of getting the better of the duel, given that
    /// contact is reachable. A fully exposed, speed-matched ball leaves
    /// the skill-based chance unchanged. No second reachability roll.
    pub fn apply_to_duel(self,neutral_win:f32)->f32 {
        (neutral_win.clamp(0.0,1.0)*self.access_factor()*self.speed_factor()).clamp(0.0,1.0)
    }
}


/// Local ground-challenge proxy: estimate the ball in front of the
/// carrier and check whether a reach-limited challenge path crosses
/// the carrier's leg/body envelope. The main ball state is unchanged.
/// These dimensions are explicit geometric assumptions, not calibrated
/// tracking data or a literal animation of two articulated players.
pub fn missed_ball_body_overlap(
    carrier:Vec2,facing_rad:f32,defender:Vec2,reach_m:f32
)->f32 {
    const LEG_BODY_ENVELOPE_M:f32=0.35;
    if reach_m<=0.0 || defender.distance(carrier)>reach_m+LEG_BODY_ENVELOPE_M {
        return 0.0;
    }
    let ball=controlled_ball_proxy(carrier,facing_rad);
    let dx=ball.x-defender.x;
    let dy=ball.y-defender.y;
    let distance=(dx*dx+dy*dy).sqrt();
    let scale=if distance>1e-6 {(reach_m/distance).min(1.0)}else{0.0};
    let end=Vec2::new(defender.x+dx*scale,defender.y+dy*scale);
    let clearance=match_contracts::spatial::point_to_segment_distance_m(carrier,defender,end);
    (1.0-clearance/LEG_BODY_ENVELOPE_M).clamp(0.0,1.0)
}

/// Shared local estimate of the controlled ball, not an independent
/// foot/ball trajectory. Preserve the offset introduced for body overlap.
fn controlled_ball_proxy(carrier:Vec2,facing_rad:f32)->Vec2 {
    const CONTROLLED_BALL_OFFSET_M:f32=0.45;
    Vec2::new(carrier.x+facing_rad.cos()*CONTROLLED_BALL_OFFSET_M,
        carrier.y+facing_rad.sin()*CONTROLLED_BALL_OFFSET_M)
}

/// Approximate a legal tackle's release point by projecting the local
/// ball proxy onto the reachable disc if necessary. Conditional on
/// accepted legal contact; not a new reach criterion or player move.
pub fn tackle_loose_contact_point(carrier:Vec2,facing_rad:f32,
    defender:Vec2,reach_m:f32)->Vec3 {
    let proxy=controlled_ball_proxy(carrier,facing_rad).clamp_to_pitch();
    let distance=proxy.distance(defender);
    let reach=reach_m.max(0.0);
    let point=if distance>reach && distance>1e-6 {
        let scale=reach/distance;
        Vec2::new(defender.x+(proxy.x-defender.x)*scale,
            defender.y+(proxy.y-defender.y)*scale).clamp_to_pitch()
    }else{proxy};
    Vec3::ground(point.x,point.y)
}

/// Reuse the dissipative block law in the moving challenger frame.
/// Body velocity approximates foot velocity. No random kick speed.
pub fn tackle_loose_velocity(incoming:Vec2,defender_velocity:Vec2,
    defender:Vec2,contact:Vec3)->Vec3 {
    let relative=Vec3::new(incoming.x-defender_velocity.x,
        incoming.y-defender_velocity.y,0.0);
    let rebound=blocked_ball_velocity(relative,defender,contact);
    Vec3::new(rebound.x+defender_velocity.x,rebound.y+defender_velocity.y,0.0)
}

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
        let dt_sec = match_contracts::units::ms_to_seconds_f32(sim_time_ms.saturating_sub(self.start_time_ms).max(0));
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
        let dt_sec = match_contracts::units::ms_to_seconds_f32(sim_time_ms.saturating_sub(self.start_time_ms).max(0));
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
    let max_t_sec = match_contracts::units::ms_to_seconds_f32(span_total_ms);
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
                (t_now_ms + match_contracts::units::seconds_to_ms_nearest_f32(t_hit_sec)).clamp(t_now_ms, t_scheduled_ms);
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

    let window_t_sec = match_contracts::units::ms_to_seconds_f32(window_span_ms);
    let mut corridor_defenders: Vec<&DefenderCandidate> = Vec::new();

    for def in defenders {
        let (t_closest, dist_m) = closest_approach_time(p_now.xy(), v_now.xy(), def.pos, window_t_sec);
        let t_closest_ms = match_contracts::units::seconds_to_ms_nearest_f32(t_closest);
        let move_ms = (t_closest_ms - def.effective_reaction_ms()).max(0);
        let max_cover_m =
            def.effective_speed_mps() * match_contracts::units::ms_to_seconds_f32(move_ms) + def.effective_reach_m();

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
            let cover_m = def.effective_speed_mps() * match_contracts::units::ms_to_seconds_f32(move_ms);
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


/// Symmetric, acceleration-limited pass-contact prediction.
/// Contact alone does not establish tackle success or ball control.
/// Parameters describe movement, not a desired pass-success percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassReachCandidate {
    pub player_id: String,
    pub side: TeamSideV4,
    pub pos: Vec2,
    pub velocity: Vec2,
    pub speed_mps: f32,
    pub acceleration_mps2: f32,
    pub agility: f32,
    pub reaction_ms: i64,
    pub reach_m: f32,
    pub playable_height_m: f32,
}

impl PassReachCandidate {
    /// Approximate a player's movement towards a fixed contact point. Coast
    /// during reaction, then turn and accelerate with a bounded running speed.
    /// This same absolute-time path is used to move the selected participant
    /// in the live world, so a predicted contact cannot teleport a player.
    pub fn position_toward(&self, target: Vec2, elapsed_ms: i64) -> Vec2 {
        let t = match_contracts::units::ms_to_seconds_f32(elapsed_ms.max(0));
        let vmax = self.speed_mps.max(0.0);
        let raw_speed = (self.velocity.x * self.velocity.x + self.velocity.y * self.velocity.y).sqrt();
        let scale = if raw_speed > vmax && raw_speed > 0.0 { vmax / raw_speed } else { 1.0 };
        let vx = self.velocity.x * scale;
        let vy = self.velocity.y * scale;
        let reaction = match_contracts::units::ms_to_seconds_f32(self.reaction_ms.max(0));
        let coast_t = t.min(reaction);
        let origin = Vec2::new(self.pos.x + vx * coast_t, self.pos.y + vy * coast_t).clamp_to_pitch();
        if t <= reaction { return origin; }
        let dist = origin.distance(target);
        if dist <= 0.001 || vmax <= 0.001 { return origin; }
        let ux = (target.x - origin.x) / dist;
        let uy = (target.y - origin.y) / dist;
        let projected = vx * ux + vy * uy;
        let actual_speed = raw_speed.min(vmax);
        let alignment = if actual_speed > 0.15 { (projected / actual_speed).clamp(-1.0, 1.0) } else { 1.0 };
        // A player running the other way must brake/turn before accelerating.
        let turn_s = (1.0 - alignment) * (0.10 + 0.24 * (1.0 - self.agility.clamp(0.0, 1.0)));
        let run_s = (t - reaction - turn_s).max(0.0);
        let v0 = projected.clamp(0.0, vmax);
        let accel = self.acceleration_mps2.max(0.1);
        let ramp_s = ((vmax - v0) / accel).min(run_s);
        let cover = (v0 * ramp_s + 0.5 * accel * ramp_s * ramp_s
            + vmax * (run_s - ramp_s)).min(dist);
        Vec2::new(origin.x + ux * cover, origin.y + uy * cover).clamp_to_pitch()
    }

    fn contact_distance(&self, ball: Vec3, elapsed_ms: i64) -> Option<f32> {
        if ball.z > self.playable_height_m { return None; }
        let residual = self.position_toward(ball.xy(), elapsed_ms).distance(ball.xy());
        if residual <= self.reach_m { Some(residual) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassContactMotion {
    pub candidate: PassReachCandidate,
    pub target: Vec2,
    pub started_at_ms: i64,
}

impl PassContactMotion {
    pub fn position_at(&self, now_ms: i64) -> Vec2 {
        self.candidate.position_toward(self.target, now_ms.saturating_sub(self.started_at_ms))
    }
}

pub enum PassContact {
    Receiver { time_ms: i64, ball_pos: Vec3 },
    Other(EarlyInteractionHit),
}

/// Find the first playable contact along the same trajectory for both sides.
/// The intended receiver may meet the pass near its planned destination,
/// rather than automatically waiting for the ball at a fixed timestamp.
/// Only the intended receiver is eligible here; other recovery and contested balls
/// require the separate loose-ball or deflection resolver.
/// Boundary detection remains analytic. Sampling is local, not a global tick.
pub fn find_pass_contact(
    traj: &BallTrajectory,
    defenders: &[PassReachCandidate],
    receiver: Option<&PassReachCandidate>,
    t_now_ms: i64,
    t_arrival_ms: i64,
) -> Option<PassContact> {
    if t_arrival_ms <= t_now_ms { return None; }
    let boundary = find_earliest_ball_interaction(traj, &[], t_now_ms, t_arrival_ms);
    let end = boundary.as_ref().map_or(t_arrival_ms, |h| h.time_ms);
    let destination = traj.position_at_ms(t_arrival_ms).xy();
    let start = traj.position_at_ms(t_now_ms).xy();
    let horizon_s = match_contracts::units::ms_to_seconds_f32((end - t_now_ms).max(0));
    // Conservative geometric filter: include everyone who could reach the
    // segment at full speed, even if acceleration later rules them out.
    let nearby: Vec<_> = defenders.iter().filter(|p| {
        match_contracts::spatial::point_to_segment_distance_m(p.pos, start, destination)
            <= p.speed_mps * horizon_s + p.reach_m + 0.01
    }).collect();
    let contact_at = |time: i64| -> Option<PassContact> {
        // Boundary has precedence if the ball is already out.
        if boundary.as_ref().map_or(false, |h| time >= h.time_ms) { return None; }
        let ball = traj.position_at_ms(time);
        let elapsed = time - t_now_ms;
        let receiving = receiver.filter(|_| ball.xy().distance(destination) <= 2.0)
            .and_then(|p| p.contact_distance(ball, elapsed));
        let mut best: Option<(&PassReachCandidate, f32)> = None;
        for &p in &nearby {
            if let Some(d) = p.contact_distance(ball, elapsed) {
                if best.map_or(true, |(prev, prev_d)| d < prev_d - 1e-4
                    || ((d - prev_d).abs() <= 1e-4 && p.player_id < prev.player_id)) {
                    best = Some((p, d));
                }
            }
        }
        if let Some(d) = receiving {
            if best.map_or(true, |(_, def_d)| d <= def_d) {
                return Some(PassContact::Receiver { time_ms: time, ball_pos: ball });
            }
        }
        best.map(|(p, residual)| PassContact::Other(EarlyInteractionHit {
            time_ms: time, kind: InteractionKind::PassInterception, ball_pos: ball,
            defender_id: Some(p.player_id.clone()), defender_side: Some(p.side),
            boundary: None, distance_m: residual, used_adaptive_substep: true,
        }))
    };
    // Bounded even with a custom timing model. Refine a detected contact in
    // the preceding bracket; do not truncate the end of long trajectories.
    let step = ((end - t_now_ms + 179) / 180).max(25);
    let mut prev = t_now_ms;
    while prev < end {
        let sample = prev.saturating_add(step).min(end);
        if let Some(hit) = contact_at(sample) {
            let mut fine = prev.saturating_add(1);
            while fine < sample {
                if let Some(earlier) = contact_at(fine) { return Some(earlier); }
                fine = fine.saturating_add((step / 20).max(5));
            }
            return Some(hit);
        }
        prev = sample;
    }
    boundary.map(PassContact::Other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use match_contracts::spatial::PITCH_HALF_WIDTH_M;

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


#[cfg(test)]
mod pass_reach_tests {
    use super::*;
    fn runner(id: &str, side: TeamSideV4, x: f32, y: f32) -> PassReachCandidate {
        PassReachCandidate {
            player_id: id.into(), side, pos: Vec2::new(x, y), velocity: Vec2::default(),
            speed_mps: 8.0, acceleration_mps2: 4.0, agility: 0.65,
            reaction_ms: 200, reach_m: 1.0, playable_height_m: 1.2,
        }
    }
    fn trajectory() -> BallTrajectory {
        BallTrajectory { start_time_ms: 0, start_pos: Vec3::new(40.0, 34.0, 0.1),
            initial_vel: Vec3::new(20.0, 0.0, 0.0), gravity_z: 9.81, drag: 0.0,
            is_shot: false, is_carrier_dribble: false }
    }

    #[test]
    fn des_pass_reach_acceleration_and_turn_direction() {
        let p = runner("P", TeamSideV4::Home, 40.0, 34.0);
        let target = Vec2::new(50.0, 34.0);
        assert!(p.position_toward(target, 500).distance(p.pos) < 0.3);
        let mut toward = p.clone();
        toward.velocity.x = 5.0;
        let mut away = p.clone();
        away.velocity.x = -5.0;
        assert!(toward.position_toward(target, 800).distance(target)
            < p.position_toward(target, 800).distance(target));
        assert!(away.position_toward(target, 800).distance(target)
            > p.position_toward(target, 800).distance(target));
        let mut tired = p.clone();
        tired.acceleration_mps2 = 2.0;
        tired.speed_mps = 4.0;
        assert!(tired.position_toward(target, 1500).distance(target)
            > p.position_toward(target, 1500).distance(target));
        let mut mirrored = p.clone();
        mirrored.side = TeamSideV4::Away;
        assert_eq!(p.position_toward(target, 1500), mirrored.position_toward(target, 1500));
    }

    #[test]
    fn des_pass_reach_receiver_and_defender_race() {
        let traj = trajectory();
        let receiver = runner("R", TeamSideV4::Home, 60.0, 34.0);
        let unreachable = runner("D", TeamSideV4::Away, 50.0, 38.0);
        assert!(matches!(find_pass_contact(&traj, &[unreachable], Some(&receiver), 0, 1000),
            Some(PassContact::Receiver { time_ms, .. }) if time_ms > 800 && time_ms <= 1000));
        let on_lane = runner("D", TeamSideV4::Away, 50.0, 34.0);
        match find_pass_contact(&traj, &[on_lane.clone()], Some(&receiver), 0, 1000) {
            Some(PassContact::Other(h)) => {
                assert_eq!(h.kind, InteractionKind::PassInterception);
                assert!(h.time_ms > 0 && h.time_ms < 1000);
                let motion = PassContactMotion { candidate: on_lane, target: h.ball_pos.xy(), started_at_ms: 0 };
                assert!(motion.position_at(h.time_ms).distance(h.ball_pos.xy()) <= 1.001);
            }
            _ => panic!("defender already on the lane must intercept before receiver"),
        }
        let close_receiver = runner("R", TeamSideV4::Home, 59.0, 34.0);
        let late_defender = runner("D", TeamSideV4::Away, 60.0, 36.5);
        assert!(matches!(find_pass_contact(&traj, &[late_defender], Some(&close_receiver), 0, 1000),
            Some(PassContact::Receiver { .. })));
    }

    #[test]
    fn des_pass_reach_checks_end_of_long_flight_and_boundary() {
        let mut traj = trajectory();
        traj.initial_vel.x = 12.0;
        // Stationary defender near the destination: the first playable
        // contact cannot occur in the old detector's first 3.2 seconds.
        let mut late = runner("D", TeamSideV4::Away, 92.0, 34.0);
        late.speed_mps = 0.0;
        match find_pass_contact(&traj, &[late], None, 0, 4500) {
            Some(PassContact::Other(h)) => {
                assert_eq!(h.kind, InteractionKind::PassInterception);
                assert!(h.time_ms > 3200 && h.time_ms <= 4500);
            }
            _ => panic!("long flight tail must not be dropped"),
        }
        traj.start_pos = Vec3::new(40.0, 67.0, 0.1);
        traj.initial_vel = Vec3::new(0.0, 10.0, 0.0);
        match find_pass_contact(&traj, &[], None, 0, 1000) {
            Some(PassContact::Other(h)) => {
                assert_eq!(h.kind, InteractionKind::BallOutOfBounds);
                assert_eq!(h.time_ms, 100);
            }
            _ => panic!("pitch boundary must remain authoritative"),
        }
    }
}


/// Probability of controlling a ball AFTER a physically valid contact.
/// Technique and body balance matter; reaching with the tip of a boot
/// at high ball speed is not equivalent to a cushioned reception.
pub fn contact_control_probability(
    skill: f32, balance: f32, fatigue: f32, speed_mps: f32,
    height_m: f32, reach_fraction: f32, pressure: f32,
) -> f32 {
    let quality = 0.72 * skill.clamp(0.0, 1.0) + 0.28 * balance.clamp(0.0, 1.0);
    let load = 0.24 * (speed_mps / 30.0).clamp(0.0, 1.0)
        + 0.16 * (height_m / 2.4).clamp(0.0, 1.0)
        + 0.26 * reach_fraction.clamp(0.0, 1.0)
        + 0.12 * pressure.clamp(0.0, 1.0);
    (0.38 + 0.58 * quality * fatigue.clamp(0.45, 1.0) - load).clamp(0.04, 0.96)
}

/// When a blocked ball drops at the contact point, find the first active
/// player able to reach the grounded loose ball; no team gets priority.
/// A later stage will extend this to rolling/airborne deflections.
pub fn find_grounded_ball_recovery(
    candidates: &[PassReachCandidate], spot: Vec2,
) -> Option<(usize, i64)> {
    // Allow the ball to drop and players to react before awarding possession.
    // A bounded local search, not an engine-wide simulation tick.
    for ms in (250_i64..=120_000).step_by(50) {
        let mut best: Option<(usize, f32)> = None;
        for (idx, p) in candidates.iter().enumerate() {
            let residual = p.position_toward(spot, ms).distance(spot);
            if residual > p.reach_m { continue; }
            if best.map_or(true, |(old_idx, d)| residual < d - 1e-4
                || ((residual - d).abs() <= 1e-4 && p.player_id < candidates[old_idx].player_id)) {
                best = Some((idx, residual));
            }
        }
        if let Some((idx, _)) = best { return Some((idx, ms)); }
    }
    None
}

#[cfg(test)]
mod contact_control_tests {
    use super::*;
    #[test]
    fn des_contact_control_skill_and_contact_difficulty() {
        let easy = contact_control_probability(0.9, 0.9, 1.0, 8.0, 0.0, 0.2, 0.0);
        let hard = contact_control_probability(0.9, 0.9, 1.0, 25.0, 1.5, 0.95, 0.8);
        let weak = contact_control_probability(0.2, 0.2, 0.6, 8.0, 0.0, 0.2, 0.0);
        assert!(easy > hard && easy > weak);
        assert!(hard > 0.0 && easy < 1.0);
    }
    #[test]
    fn des_contact_control_recovery_race_is_side_neutral() {
        let candidate = |id: &str, side, x| PassReachCandidate {
            player_id: id.into(), side, pos: Vec2::new(x, 34.0),
            velocity: Vec2::default(), speed_mps: 7.0, acceleration_mps2: 4.0,
            agility: 0.65, reaction_ms: 200, reach_m: 1.0, playable_height_m: 1.2,
        };
        let spot = Vec2::new(50.0, 34.0);
        let mut players = vec![candidate("H", TeamSideV4::Home, 51.8),
                               candidate("A", TeamSideV4::Away, 55.0)];
        let (winner, ms) = find_grounded_ball_recovery(&players, spot).unwrap();
        assert_eq!(winner, 0);
        assert!(ms > 250);
        assert!(players[winner].position_toward(spot, ms).distance(spot) <= players[winner].reach_m);
        players[0].side = TeamSideV4::Away;
        players[1].side = TeamSideV4::Home;
        assert_eq!(find_grounded_ball_recovery(&players, spot), Some((winner, ms)));
        players.swap(0, 1);
        assert_eq!(find_grounded_ball_recovery(&players, spot), Some((1, ms)));
    }
}


/// Inelastic block: dissipate energy, reflect normal motion and preserve
/// some tangential motion. The contact normal is independent of team side.
pub fn blocked_ball_velocity(v: Vec3, player: Vec2, contact: Vec3) -> Vec3 {
    let dx = contact.x - player.x;
    let dy = contact.y - player.y;
    let len = (dx*dx + dy*dy).sqrt();
    let speed = (v.x*v.x + v.y*v.y).sqrt();
    let (nx, ny) = if len > 0.05 { (dx/len, dy/len) }
        else if speed > 0.01 { (-v.x/speed, -v.y/speed) } else { (0.0,0.0) };
    let dot = v.x*nx + v.y*ny;
    if dot < 0.0 {
        Vec3::new(0.55*(v.x-dot*nx)-0.35*dot*nx,
                  0.55*(v.y-dot*ny)-0.35*dot*ny, v.z.abs()*0.12)
    } else {
        Vec3::new(v.x*0.45, v.y*0.45, v.z.abs()*0.12)
    }
}

pub enum LooseBallResolution {
    Player { candidate_idx: usize, time_ms: i64, ball_pos: Vec3 },
    Boundary(EarlyInteractionHit),
}

/// Local bounded search for a moving loose ball. A routine recovery requires
/// playable height and reduced horizontal speed; rapid repeated contacts
/// remain a subsequent model refinement, not automatic possession.
pub fn find_moving_ball_recovery(traj: &BallTrajectory, candidates: &[PassReachCandidate]) -> Option<LooseBallResolution> {
    let now = traj.start_time_ms;
    let end = now.saturating_add(120_000);
    let boundary = find_earliest_ball_interaction(traj, &[], now, end);
    let stop = boundary.as_ref().map_or(end, |h| h.time_ms);
    let mut t = now.saturating_add(250);
    while t <= stop {
        if boundary.as_ref().map_or(false, |h| t >= h.time_ms) { break; }
        let ball = traj.position_at_ms(t);
        let v = traj.velocity_at_ms(t);
        if (v.x*v.x+v.y*v.y).sqrt() <= 7.0 {
            let mut best: Option<(usize,f32)> = None;
            for (idx,p) in candidates.iter().enumerate() {
                if let Some(d) = p.contact_distance(ball, t-now) {
                    if best.map_or(true, |(old,old_d)| d < old_d-1e-4
                        || ((d-old_d).abs() <= 1e-4 && p.player_id < candidates[old].player_id)) {
                        best = Some((idx,d));
                    }
                }
            }
            if let Some((idx,_)) = best {
                return Some(LooseBallResolution::Player { candidate_idx: idx, time_ms: t, ball_pos: ball });
            }
        }
        t = t.saturating_add(50);
    }
    boundary.map(LooseBallResolution::Boundary)
}

#[cfg(test)]
mod moving_ball_tests {
    use super::*;
    #[test]
    fn des_loose_motion_block_dissipates_energy_and_mirrors() {
        let v=Vec3::new(20.0,4.0,0.0);
        let spot=Vec3::ground(50.0,34.0);
        let r=blocked_ball_velocity(v,Vec2::new(51.0,34.0),spot);
        let m=blocked_ball_velocity(Vec3::new(-20.0,4.0,0.0),Vec2::new(49.0,34.0),spot);
        assert!(r.x<0.0 && r.y>0.0);
        assert!(r.x*r.x+r.y*r.y<v.x*v.x+v.y*v.y);
        assert!((r.x+m.x).abs()<0.001 && r.y==m.y);
    }
    #[test]
    fn des_loose_motion_actual_position_and_boundary() {
        let traj=BallTrajectory { start_time_ms:1000,start_pos:Vec3::ground(50.0,34.0),
            initial_vel:Vec3::new(12.0,0.0,0.0),gravity_z:9.81,drag:1.6,is_shot:false,is_carrier_dribble:false };
        let p=|id:&str,side,x| PassReachCandidate {player_id:id.into(),side,pos:Vec2::new(x,34.0),
            velocity:Vec2::default(),speed_mps:7.0,acceleration_mps2:4.0,agility:0.65,
            reaction_ms:200,reach_m:1.0,playable_height_m:1.2};
        let ps=[p("D",TeamSideV4::Away,50.0),p("R",TeamSideV4::Home,54.0)];
        match find_moving_ball_recovery(&traj,&ps).unwrap() {
            LooseBallResolution::Player{candidate_idx,time_ms,ball_pos} => {
                assert_eq!(candidate_idx,1);
                assert!(time_ms>1250 && ball_pos.x>53.0);
                assert_eq!(ball_pos,traj.position_at_ms(time_ms));
                assert!(ps[candidate_idx].position_toward(ball_pos.xy(),time_ms-1000).distance(ball_pos.xy())<=1.0);
            }
            _=>panic!("receiver must reach rolling ball"),
        }
        let exit=BallTrajectory{start_pos:Vec3::ground(50.0,67.0),initial_vel:Vec3::new(0.0,12.0,0.0),..traj};
        match find_moving_ball_recovery(&exit,&ps).unwrap() {
            LooseBallResolution::Boundary(h)=>{
                assert_eq!(h.boundary,Some(BoundaryKind::RightTouchline));
                assert!(h.time_ms>1000 && h.time_ms<1250);
            }
            _=>panic!("boundary before recovery"),
        }
    }
}


/// Local contact search along an absolute-time carrier path. This is a
/// bounded event prediction, not a global simulation tick. All eligible
/// defenders have the same reach rules, independent of team side.
pub fn find_carry_contact(
    carrier:&PassContactMotion, defenders:&[PassReachCandidate], end_ms:i64,
)->Option<(usize,i64,PassContactMotion)> {
    let start=carrier.started_at_ms;
    let horizon=match_contracts::units::ms_to_seconds_f32((end_ms-start).max(0));
    let end=carrier.position_at(end_ms);
    let eligible:Vec<_>=defenders.iter().enumerate().filter(|(_,p)|
        match_contracts::spatial::point_to_segment_distance_m(p.pos,carrier.candidate.pos,end)
            <=p.speed_mps*horizon+p.reach_m+0.01).collect();
    let mut now=start.saturating_add(150).min(end_ms);
    while now<=end_ms && now>start {
        let pos=carrier.position_at(now);
        let elapsed=now-start;
        let mut best:Option<(usize,f32)>=None;
        for &(idx,p) in &eligible {
            if elapsed<p.reaction_ms {continue;}
            let distance=p.position_toward(pos,elapsed).distance(pos);
            if distance<=p.reach_m && best.map_or(true,|(old,d)|
                distance<d-1e-4 || ((distance-d).abs()<=1e-4 && p.player_id<defenders[old].player_id)) {
                best=Some((idx,distance));
            }
        }
        if let Some((idx,_))=best {
            return Some((idx,now,PassContactMotion{candidate:defenders[idx].clone(),target:pos,started_at_ms:start}));
        }
        if now==end_ms {break;}
        now=now.saturating_add(50).min(end_ms);
    }
    None
}

#[cfg(test)]
mod carry_contact_tests {
    use super::*;
    fn runner(id:&str,side:TeamSideV4,x:f32,y:f32)->PassReachCandidate {
        PassReachCandidate{player_id:id.into(),side,pos:Vec2::new(x,y),velocity:Vec2::default(),
            speed_mps:6.0,acceleration_mps2:4.0,agility:0.7,reaction_ms:200,
            reach_m:1.0,playable_height_m:1.2}
    }
    #[test]
    fn pi_carry_contact_requires_reach_and_precedes_endpoint() {
        let motion=PassContactMotion{candidate:runner("H",TeamSideV4::Home,50.0,34.0),
            target:Vec2::new(60.0,34.0),started_at_ms:1000};
        let nearby=runner("A",TeamSideV4::Away,52.0,34.0);
        let far=runner("F",TeamSideV4::Away,90.0,60.0);
        assert!(find_carry_contact(&motion,&[far.clone()],2600).is_none());
        let (idx,time,def)=find_carry_contact(&motion,&[far,nearby],2600).unwrap();
        assert_eq!(idx,1);
        assert!(time>=1200 && time<2600);
        assert!(def.position_at(time).distance(motion.position_at(time))<=1.001);
    }
}
