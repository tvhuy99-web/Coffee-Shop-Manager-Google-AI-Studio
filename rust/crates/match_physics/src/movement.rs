//! Pure locomotion capability. Values are derived per motion plan, not saved attributes.
use match_contracts::attributes::{normalize_attr, PhysicalAttributes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionError {
    InvalidCapability,
    InvalidState,
    InvalidIntent,
    InvalidHorizon,
    InvalidReach,
    NonFiniteResult,
}

/// Physical limits in SI units. Initial coefficients are hypotheses, not calibration results.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementCapability {
    pub max_speed_mps: f32,
    pub acceleration_mps2: f32,
    pub braking_mps2: f32,
    pub lateral_mps2: f32,
    pub body_turn_radps: f32,
}

impl MovementCapability {
    /// Energy values use the existing two 0..100 pools. Pace sets top speed;
    /// Acceleration sets forward acceleration; Agility sets braking/steering/turning.
    /// Injury constraints and perception delay remain the caller's responsibility.
    pub fn from_attributes(
        attrs: &PhysicalAttributes,
        aerobic: f32,
        anaerobic: f32,
    ) -> Result<Self, MotionError> {
        if !(1..=20).contains(&attrs.pace)
            || !(1..=20).contains(&attrs.acceleration)
            || !(1..=20).contains(&attrs.agility)
            || !(0.0..=100.0).contains(&aerobic)
            || !(0.0..=100.0).contains(&anaerobic)
        {
            return Err(MotionError::InvalidCapability);
        }
        let pace = normalize_attr(attrs.pace);
        let acceleration = normalize_attr(attrs.acceleration);
        let agility = normalize_attr(attrs.agility);
        let endurance = 0.65 + 0.35 * (aerobic / 100.0);
        let burst = 0.45 + 0.55 * (anaerobic / 100.0);
        Ok(Self {
            max_speed_mps: (5.0 + 4.4 * pace) * endurance,
            acceleration_mps2: (2.0 + 3.4 * acceleration) * burst,
            braking_mps2: (3.5 + 3.5 * agility) * burst,
            lateral_mps2: (2.5 + 4.0 * agility) * burst,
            body_turn_radps: (2.5 + 3.5 * agility) * burst,
        })
    }

    pub fn validate(self) -> Result<(), MotionError> {
        if ![
            self.max_speed_mps,
            self.acceleration_mps2,
            self.braking_mps2,
            self.lateral_mps2,
            self.body_turn_radps,
        ].iter().all(|v| v.is_finite() && *v > 0.0) {
            return Err(MotionError::InvalidCapability);
        }
        Ok(())
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    fn attrs() -> PhysicalAttributes {
        PhysicalAttributes { pace: 10, acceleration: 10, strength: 10, stamina: 10,
            balance: 10, agility: 10, jumping_reach: 10, natural_fitness: 10 }
    }

    #[test]
    fn b12_pace_and_acceleration_are_independent() {
        let base = attrs();
        let low = MovementCapability::from_attributes(&base, 100.0, 100.0).unwrap();
        let pace = MovementCapability::from_attributes(&PhysicalAttributes { pace: 20, ..base }, 100.0, 100.0).unwrap();
        let acceleration = MovementCapability::from_attributes(&PhysicalAttributes { acceleration: 20, ..base }, 100.0, 100.0).unwrap();
        assert!(pace.max_speed_mps > low.max_speed_mps);
        assert_eq!(pace.acceleration_mps2, low.acceleration_mps2);
        assert!(acceleration.acceleration_mps2 > low.acceleration_mps2);
        assert_eq!(acceleration.max_speed_mps, low.max_speed_mps);
    }

    #[test]
    fn b12_agility_and_energy_have_distinct_roles() {
        let base = attrs();
        let full = MovementCapability::from_attributes(&base, 100.0, 100.0).unwrap();
        let agile = MovementCapability::from_attributes(&PhysicalAttributes { agility: 20, ..base }, 100.0, 100.0).unwrap();
        let endurance = MovementCapability::from_attributes(&base, 40.0, 100.0).unwrap();
        let burst = MovementCapability::from_attributes(&base, 100.0, 40.0).unwrap();
        assert_eq!(agile.max_speed_mps, full.max_speed_mps);
        assert_eq!(agile.acceleration_mps2, full.acceleration_mps2);
        assert!(agile.braking_mps2 > full.braking_mps2);
        assert!(agile.lateral_mps2 > full.lateral_mps2);
        assert!(agile.body_turn_radps > full.body_turn_radps);
        assert!(endurance.max_speed_mps < full.max_speed_mps);
        assert_eq!(endurance.acceleration_mps2, full.acceleration_mps2);
        assert_eq!(burst.max_speed_mps, full.max_speed_mps);
        assert!(burst.acceleration_mps2 < full.acceleration_mps2);
        assert!(burst.braking_mps2 < full.braking_mps2);
    }

    #[test]
    fn b12_invalid_capability_is_rejected() {
        let a = attrs();
        for energy in [-1.0, 101.0, f32::NAN, f32::INFINITY] {
            assert_eq!(MovementCapability::from_attributes(&a, energy, 100.0), Err(MotionError::InvalidCapability));
            assert_eq!(MovementCapability::from_attributes(&a, 100.0, energy), Err(MotionError::InvalidCapability));
        }
        assert_eq!(MovementCapability::from_attributes(&PhysicalAttributes { pace: 0, ..a }, 100.0, 100.0), Err(MotionError::InvalidCapability));
        let cap = MovementCapability::from_attributes(&a, 0.0, 0.0).unwrap();
        assert!(cap.validate().is_ok());
        assert!(MovementCapability { braking_mps2: 0.0, ..cap }.validate().is_err());
    }
}

use match_contracts::spatial::Vec2;
use match_contracts::units::is_finite_vec2;

/// Local trajectory resolution, not a global simulation tick.
const LOCAL_STEP_MS: i64 = 50;
/// Technical bound per plan query. Longer world intervals need explicit replanning.
pub const MAX_MOTION_HORIZON_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub facing_rad: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementIntent {
    pub target: Vec2,
    /// Requested fraction of effective maximum speed, not the resulting speed.
    pub intensity: f32,
    pub desired_facing_rad: Option<f32>,
    /// Delay from the plan origin before the new intent takes effect.
    pub reaction_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionResult {
    pub state: MotionState,
    /// Cumulative path length since the plan origin, not endpoint displacement.
    pub distance_m: f32,
    /// Cumulative integral of absolute acceleration (m/s), not an energy pool.
    pub acceleration_load_mps: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachHit {
    pub elapsed_ms: i64,
    pub motion: MotionResult,
}

/// Immutable snapshot for both prediction and execution. Sampling at an absolute
/// elapsed time never replans, consumes RNG or depends on previous queries.
/// Not yet wired to production or serialized in checkpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionPlan {
    initial: MotionState,
    intent: MovementIntent,
    capability: MovementCapability,
}

#[derive(Clone, Copy)]
struct D2(f64, f64);

impl D2 {
    fn from_vec(v: Vec2) -> Self { Self(v.x as f64, v.y as f64) }
    fn to_vec(self) -> Vec2 { Vec2::new(self.0 as f32, self.1 as f32) }
    fn add(self, other: Self) -> Self { Self(self.0 + other.0, self.1 + other.1) }
    fn sub(self, other: Self) -> Self { Self(self.0 - other.0, self.1 - other.1) }
    fn scale(self, k: f64) -> Self { Self(self.0 * k, self.1 * k) }
    fn dot(self, other: Self) -> f64 { self.0 * other.0 + self.1 * other.1 }
    fn norm(self) -> f64 { self.0.hypot(self.1) }
}

fn angle_delta(from: f64, to: f64) -> f64 {
    // At an exact half turn choose the negative direction deterministically.
    (to - from + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI
}

/// Bounded vector change over one local controller segment. The velocity is
/// continuous; slowing, accelerating and lateral steering share one effort bound.
fn velocity_delta(v: D2, desired: D2, cap: MovementCapability, dt: f64) -> D2 {
    let delta = desired.sub(v);
    let speed = v.norm();
    if speed <= 1e-9 {
        let n = delta.norm();
        return if n > 0.0 {
            delta.scale((cap.acceleration_mps2 as f64 * dt / n).min(1.0))
        } else { delta };
    }
    let forward = v.scale(1.0 / speed);
    let longitudinal = delta.dot(forward);
    let lateral = delta.sub(forward.scale(longitudinal));
    let longitudinal_limit = if longitudinal < 0.0 {
        cap.braking_mps2
    } else {
        cap.acceleration_mps2
    } as f64 * dt;
    let lateral_limit = cap.lateral_mps2 as f64 * dt;
    let effort = (longitudinal / longitudinal_limit).hypot(lateral.norm() / lateral_limit);
    let mut scale = if effort > 1.0 { 1.0 / effort } else { 1.0 };
    // Do not switch from braking to reverse acceleration within the same
    // segment. A fresh controller segment handles the new forward direction.
    if longitudinal < -speed {
        scale = scale.min(speed / -longitudinal);
    }
    delta.scale(scale)
}

impl MotionPlan {
    pub fn new(
        initial: MotionState,
        intent: MovementIntent,
        capability: MovementCapability,
    ) -> Result<Self, MotionError> {
        capability.validate()?;
        if !is_finite_vec2(initial.position) || !is_finite_vec2(initial.velocity)
            || !initial.facing_rad.is_finite()
        {
            return Err(MotionError::InvalidState);
        }
        if !is_finite_vec2(intent.target)
            || !(0.0..=1.0).contains(&intent.intensity)
            || intent.desired_facing_rad.is_some_and(|a| !a.is_finite())
            || intent.reaction_ms < 0
        {
            return Err(MotionError::InvalidIntent);
        }
        Ok(Self { initial, intent, capability })
    }

    pub fn target(&self) -> Vec2 { self.intent.target }

    /// Convex interpolation of bounded velocity segments cannot exceed the
    /// larger of initial speed and requested capability. Overspeed can
    /// legitimately persist during braking or reaction.
    pub fn maximum_path_speed_mps(&self) -> f32 {
        self.initial.velocity.x.hypot(self.initial.velocity.y).max(self.capability.max_speed_mps)
    }

    /// Sample the same plan at elapsed milliseconds. Controller segments are
    /// anchored at the origin/reaction boundary; a partial query evaluates the
    /// same segment rather than changing the integration step or target.
    /// Position is not clamped to pitch or target: contact and rules own boundaries.
    pub fn sample_at(&self, elapsed_ms: i64) -> Result<MotionResult, MotionError> {
        self.walk(elapsed_ms, |_, _, _, _, _| false)
    }

    /// Search an already committed path, not a second movement model.
    /// Returns the first integer millisecond at which the fixed target is
    /// within reach. Contact with a moving ball and sub-ms order are separate.
    pub fn first_reach(&self, target: Vec2, reach_m: f32, horizon_ms: i64)
        -> Result<Option<ReachHit>, MotionError>
    {
        if !is_finite_vec2(target) || !reach_m.is_finite() || reach_m < 0.0 {
            return Err(MotionError::InvalidReach);
        }
        if !(0..=MAX_MOTION_HORIZON_MS).contains(&horizon_ms) {
            return Err(MotionError::InvalidHorizon);
        }
        let target64 = D2::from_vec(target);
        let radius = reach_m as f64;
        let within = |p: D2| D2::from_vec(p.to_vec()).sub(target64).norm() <= radius;
        let mut hit = if within(D2::from_vec(self.initial.position)) { Some(0) } else { None };
        if hit.is_none() {
            self.walk(horizon_ms, |at, span, p, v, dv| {
                let dt = span as f64 / 1000.0;
                let max_speed = v.norm().max(v.add(dv).norm());
                // Broad-phase bound plus f32 position rounding allowance.
                let rounding = 4.0 * f32::EPSILON as f64 *
                    (p.norm() + max_speed * dt + target64.norm() + 1.0);
                if p.sub(target64).norm() > radius + max_speed * dt + rounding {
                    return false;
                }
                for ms in 1..=span {
                    let fraction = ms as f64 / span as f64;
                    let pos = p.add(v.add(dv.scale(0.5 * fraction)).scale(ms as f64 / 1000.0));
                    if within(pos) { hit = Some(at + ms); return true; }
                }
                false
            })?;
        }
        hit.map(|elapsed_ms| {
            let motion = self.sample_at(elapsed_ms)?;
            Ok(ReachHit { elapsed_ms, motion })
        }).transpose()
    }

    fn walk(&self, elapsed_ms: i64, mut visit: impl FnMut(i64, i64, D2, D2, D2) -> bool)
        -> Result<MotionResult, MotionError>
    {
        if !(0..=MAX_MOTION_HORIZON_MS).contains(&elapsed_ms) {
            return Err(MotionError::InvalidHorizon);
        }
        let mut p = D2::from_vec(self.initial.position);
        let mut v = D2::from_vec(self.initial.velocity);
        let target = D2::from_vec(self.intent.target);
        let mut facing = self.initial.facing_rad as f64;
        let mut distance = 0.0_f64;
        let mut load = 0.0_f64;
        let mut at = 0_i64;
        while at < elapsed_ms {
            let reacting = at < self.intent.reaction_ms;
            let segment_ms = if reacting { self.intent.reaction_ms - at } else { LOCAL_STEP_MS };
            let used_ms = (elapsed_ms - at).min(segment_ms);
            let dt = used_ms as f64 / 1000.0;
            let (dv, turn) = if reacting {
                // Reaction preserves momentum and body orientation.
                (D2(0.0, 0.0), 0.0)
            } else {
                let full_dt = segment_ms as f64 / 1000.0;
                let to = target.sub(p);
                let dist = to.norm();
                let brake = self.capability.braking_mps2 as f64;
                // Conservative braking envelope with one controller segment of
                // lookahead. No position snap is needed even when overshooting.
                let lookahead = brake * full_dt;
                let stop_speed = (2.0 * brake * dist + lookahead * lookahead).sqrt() - lookahead;
                let requested = self.capability.max_speed_mps as f64 * self.intent.intensity as f64;
                let desired = if dist > 1e-9 {
                    to.scale(stop_speed.min(requested) / dist)
                } else { D2(0.0, 0.0) };
                let desired_facing = self.intent.desired_facing_rad.map(|a| a as f64)
                    .or_else(|| if dist > 1e-9 && requested > 0.0 {
                        Some(to.1.atan2(to.0))
                    } else { None });
                let max_turn = self.capability.body_turn_radps as f64 * full_dt;
                let turn = desired_facing.map_or(0.0, |a| angle_delta(facing, a).clamp(-max_turn, max_turn));
                let fraction = dt / full_dt;
                (velocity_delta(v, desired, self.capability, full_dt).scale(fraction), turn * fraction)
            };
            if visit(at, used_ms, p, v, dv) { break; }
            let next_v = v.add(dv);
            // Constant acceleration within a segment: integrate the vector
            // exactly and the speed norm with Simpson quadrature.
            let middle_v = v.add(dv.scale(0.5));
            p = p.add(middle_v.scale(dt));
            distance += dt * (v.norm() + 4.0 * middle_v.norm() + next_v.norm()) / 6.0;
            load += dv.norm();
            v = next_v;
            facing += turn;
            at += used_ms;
        }
        let result = MotionResult {
            state: MotionState { position: p.to_vec(), velocity: v.to_vec(), facing_rad: facing as f32 },
            distance_m: distance as f32,
            acceleration_load_mps: load as f32,
        };
        if !is_finite_vec2(result.state.position) || !is_finite_vec2(result.state.velocity)
            || !result.state.facing_rad.is_finite() || !result.distance_m.is_finite()
            || !result.acceleration_load_mps.is_finite()
        {
            return Err(MotionError::NonFiniteResult);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod solver_tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn cap() -> MovementCapability {
        MovementCapability { max_speed_mps: 8.0, acceleration_mps2: 4.0,
            braking_mps2: 5.0, lateral_mps2: 3.0, body_turn_radps: 3.0 }
    }

    fn initial(vx: f32, vy: f32) -> MotionState {
        MotionState { position: Vec2::new(40.0, 34.0), velocity: Vec2::new(vx, vy), facing_rad: 0.0 }
    }

    fn intent(x: f32, y: f32, intensity: f32) -> MovementIntent {
        MovementIntent { target: Vec2::new(x, y), intensity, desired_facing_rad: None, reaction_ms: 0 }
    }

    fn plan(s: MotionState, i: MovementIntent) -> MotionPlan {
        MotionPlan::new(s, i, cap()).unwrap()
    }

    fn near(a: f32, b: f32, tolerance: f32) {
        assert!((a - b).abs() <= tolerance, "{a} differs from {b} by more than {tolerance}");
    }

    fn speed(v: Vec2) -> f32 { v.x.hypot(v.y) }

    #[test]
    fn b12_accelerates_then_respects_top_speed() {
        let p = plan(initial(0.0, 0.0), intent(100.0, 34.0, 1.0));
        for (ms, x, v) in [(0, 40.0, 0.0), (500, 40.5, 2.0), (1000, 42.0, 4.0),
                            (2000, 48.0, 8.0), (3000, 56.0, 8.0)] {
            let r = p.sample_at(ms).unwrap();
            near(r.state.position.x, x, 1e-4);
            near(r.state.velocity.x, v, 1e-4);
            near(r.distance_m, x - 40.0, 1e-4);
        }
    }

    #[test]
    fn b12_brakes_near_target_without_snapping() {
        let p = plan(initial(8.0, 0.0), intent(48.0, 34.0, 1.0));
        let early = p.sample_at(100).unwrap();
        assert!(early.state.position.x > 40.0 && early.state.position.x < 41.0);
        assert!(early.state.velocity.x >= 7.49);
        let settled = p.sample_at(15_000).unwrap();
        near(settled.state.position.x, 48.0, 0.08);
        assert!(speed(settled.state.velocity) < 0.08);
        assert!(settled.distance_m >= 7.9);
    }

    #[test]
    fn b12_reversal_brakes_before_accelerating_back() {
        let p = plan(initial(8.0, 0.0), intent(10.0, 34.0, 1.0));
        let early = p.sample_at(1000).unwrap();
        near(early.state.velocity.x, 3.0, 1e-4);
        near(early.state.position.x, 45.5, 1e-4);
        let stopped = p.sample_at(1600).unwrap();
        near(stopped.state.velocity.x, 0.0, 1e-4);
        let later = p.sample_at(3500).unwrap();
        assert!(later.state.velocity.x < 0.0);
        assert!(later.distance_m > initial(8.0, 0.0).position.distance(later.state.position) + 5.0);
    }

    #[test]
    fn b12_right_angle_turn_is_laterally_bounded() {
        let p = plan(initial(8.0, 0.0), intent(40.0, 65.0, 1.0));
        let a = p.sample_at(50).unwrap();
        assert!(a.state.velocity.x > 7.7);
        assert!(a.state.velocity.y > 0.0 && a.state.velocity.y <= 0.15001);
        assert!(speed(a.state.velocity) <= 8.00001);
        assert!(a.state.facing_rad > 0.0 && a.state.facing_rad <= 0.15001);
    }

    #[test]
    fn b12_reaction_and_facing_are_independent_of_velocity() {
        let i = MovementIntent { reaction_ms: 225, desired_facing_rad: Some(FRAC_PI_2),
            ..intent(100.0, 34.0, 1.0) };
        let p = plan(initial(4.0, 0.0), i);
        let coast = p.sample_at(225).unwrap();
        near(coast.state.position.x, 40.9, 1e-4);
        near(coast.state.velocity.x, 4.0, 1e-4);
        near(coast.state.facing_rad, 0.0, 1e-6);
        let turn = p.sample_at(725).unwrap();
        assert!(turn.state.velocity.x > 4.0);
        near(turn.state.velocity.y, 0.0, 1e-6);
        near(turn.state.facing_rad, 1.5, 1e-4);
    }

    #[test]
    fn b12_intensity_and_overspeed_do_not_snap_velocity() {
        let walk = plan(initial(0.0, 0.0), intent(100.0, 34.0, 0.25)).sample_at(2000).unwrap();
        near(speed(walk.state.velocity), 2.0, 1e-4);
        let stop = plan(initial(8.0, 0.0), intent(100.0, 34.0, 0.0));
        near(stop.sample_at(100).unwrap().state.velocity.x, 7.5, 1e-4);
        near(stop.sample_at(2000).unwrap().state.velocity.x, 0.0, 1e-4);
        let fast = plan(initial(10.0, 0.0), intent(100.0, 34.0, 1.0));
        near(fast.sample_at(100).unwrap().state.velocity.x, 9.5, 1e-4);
        near(fast.sample_at(1000).unwrap().state.velocity.x, 8.0, 1e-4);
    }

    #[test]
    fn b12_absolute_plan_queries_are_order_independent_and_account_path() {
        let p = plan(initial(3.0, 1.0), intent(55.0, 48.0, 0.8));
        let whole = p.sample_at(1377).unwrap();
        let a = p.sample_at(217).unwrap();
        let b = p.sample_at(831).unwrap();
        assert_eq!(whole, p.sample_at(1377).unwrap());
        assert_eq!(a, p.sample_at(217).unwrap());
        assert!(a.distance_m < b.distance_m && b.distance_m < whole.distance_m);
        near(a.distance_m + (b.distance_m - a.distance_m) + (whole.distance_m - b.distance_m),
             whole.distance_m, 1e-5);
        assert!(whole.distance_m + 1e-5 >= initial(3.0, 1.0).position.distance(whole.state.position));
    }

    #[test]
    fn b12_mirror_geometry_and_speed_budget() {
        let s = initial(3.0, 1.0);
        let i = intent(55.0, 48.0, 0.8);
        let reflected = MotionState { position: Vec2::new(105.0-s.position.x, 68.0-s.position.y),
            velocity: Vec2::new(-s.velocity.x, -s.velocity.y), facing_rad: PI };
        let ri = MovementIntent { target: Vec2::new(105.0-i.target.x, 68.0-i.target.y), ..i };
        let a = plan(s, i).sample_at(1377).unwrap();
        let b = plan(reflected, ri).sample_at(1377).unwrap();
        near(a.state.position.x+b.state.position.x, 105.0, 1e-4);
        near(a.state.position.y+b.state.position.y, 68.0, 1e-4);
        near(a.state.velocity.x+b.state.velocity.x, 0.0, 1e-4);
        near(a.state.velocity.y+b.state.velocity.y, 0.0, 1e-4);
        near(a.state.facing_rad.cos()+b.state.facing_rad.cos(), 0.0, 1e-4);
        near(a.state.facing_rad.sin()+b.state.facing_rad.sin(), 0.0, 1e-4);
        near(a.distance_m, b.distance_m, 1e-4);
        assert!(a.distance_m <= 8.0*1.377+1e-4);
    }

    #[test]
    fn b12_reach_matches_execution_and_earliest_ms() {
        let target = Vec2::new(45.0, 34.0);
        let p = plan(initial(0.0, 0.0), intent(60.0, 34.0, 1.0));
        let hit = p.first_reach(target, 0.5, 3000).unwrap().unwrap();
        assert_eq!(hit.motion, p.sample_at(hit.elapsed_ms).unwrap());
        assert!(hit.motion.state.position.distance(target) <= 0.50001);
        assert!(p.sample_at(hit.elapsed_ms-1).unwrap().state.position.distance(target)>0.5);
        assert!(p.first_reach(target, 0.5, hit.elapsed_ms-1).unwrap().is_none());
        assert_eq!(p.first_reach(initial(0.0,0.0).position, 0.0, 0).unwrap().unwrap().elapsed_ms,0);
        assert_eq!(p.first_reach(target, -1.0, 3000), Err(MotionError::InvalidReach));
    }

    #[test]
    fn b12_reach_respects_momentum_and_transient_crossings() {
        let target=Vec2::new(45.0,34.0);
        let toward=plan(initial(5.0,0.0),intent(70.0,34.0,1.0));
        let away=plan(initial(-5.0,0.0),intent(70.0,34.0,1.0));
        assert!(toward.first_reach(target,0.5,6000).unwrap().unwrap().elapsed_ms
            < away.first_reach(target,0.5,6000).unwrap().unwrap().elapsed_ms);
        // Crossing between 50-ms controller endpoints must still be found.
        let coast=plan(initial(8.0,0.0),MovementIntent{reaction_ms:100,..intent(70.0,34.0,1.0)});
        let hit=coast.first_reach(Vec2::new(40.2,34.0),0.01,50).unwrap().unwrap();
        assert!((23..=26).contains(&hit.elapsed_ms));
        assert_eq!(hit.motion,coast.sample_at(hit.elapsed_ms).unwrap());
    }

    #[test]
    fn b12_invalid_inputs_and_horizons_are_rejected() {
        let s = initial(0.0, 0.0);
        let i = intent(100.0, 34.0, 1.0);
        assert_eq!(MotionPlan::new(MotionState { facing_rad: f32::NAN, ..s }, i, cap()), Err(MotionError::InvalidState));
        assert_eq!(MotionPlan::new(s, MovementIntent { intensity: 1.01, ..i }, cap()), Err(MotionError::InvalidIntent));
        assert_eq!(MotionPlan::new(s, MovementIntent { reaction_ms: -1, ..i }, cap()), Err(MotionError::InvalidIntent));
        assert_eq!(plan(s, i).sample_at(-1), Err(MotionError::InvalidHorizon));
        assert_eq!(plan(s, i).sample_at(MAX_MOTION_HORIZON_MS+1), Err(MotionError::InvalidHorizon));
    }
}
