//! Narrow trait ports for Phase A3/A5 crate isolation (ADR-004).
//!
//! `match_contracts` defines only pure query/response DTOs and trait signatures so
//! `player_intelligence` and `coach_intelligence` can depend strictly on `match_contracts`
//! without duplicating physics/rules formulas or turning `match_contracts` into a monolith.

use serde::{Deserialize, Serialize};

use crate::units::{DurationMs, TeamSideV4, Vec2};

/// Narrow random sampling capability provided by `match_engine` to subsystems.
/// Subsystems never inspect or mutate the master RNG seed or state directly.
pub trait RngSampleSource {
    fn next_long(&mut self) -> i64;
    fn next_int(&mut self, bound: i32) -> i32;
    fn next_boolean(&mut self) -> bool;
    fn next_double(&mut self) -> f64;
}

/// Flight classification used when querying nominal travel duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BallFlightKindContract {
    GroundPass,
    ThroughPass,
    LoftedPass,
    Cross,
    Clearance,
    Shot,
}

/// Query for subjective pass-reach forecasting from observed positions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassReachQuery {
    pub passer_pos: Vec2,
    pub target_pos: Vec2,
    pub attacking_side: TeamSideV4,
    pub flight_kind: BallFlightKindContract,
}

/// Forecast returned by `SpatialForecastPort::estimate_pass_reach`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassReachForecast {
    pub estimated_flight_ms: DurationMs,
    pub interception_risk: f32,
    pub lane_clearance_m: f32,
    pub receiver_margin_m: f32,
}

/// Query for local spatial pressure/space around a pitch point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalContextQuery {
    pub side: TeamSideV4,
    pub point: Vec2,
}

/// Pure data projection of local pressure and spacing around a point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalContextView {
    pub nearest_opponent_m: f32,
    pub pressure_score: f32,
    pub nearby_opponents: u8,
    pub nearby_teammates: u8,
}

/// Query for tackle approach geometry from observed positions/velocities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TackleGeometryQuery {
    pub tackler_pos: Vec2,
    pub carrier_pos: Vec2,
    pub carrier_vel: Vec2,
    pub attacking_side: TeamSideV4,
}

/// Pure geometric forecast for tackle vs. contain evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TackleGeometryForecast {
    pub distance_m: f32,
    pub reach_probability: f32,
    pub body_contact_risk: f32,
    pub in_penalty_box: bool,
}

/// Read-only spatial & timing forecast port implemented by `match_engine`
/// (delegating authoritative math to `match_physics` and `match_rules`).
/// Exact legacy spatial query contract, preserving pressure units, index identity
/// and float operation order. More restrictive observations can be introduced
/// in B2; A3 must not silently replace the original forecast with approximations.
#[derive(Debug, Clone, Copy)]
pub struct LocalSpatialContext {
    pub pressure: f32,
    pub nearest_defender_idx: Option<usize>,
    pub nearest_defender_dist_m: f32,
    pub overload: bool,
}
pub trait SpatialForecastPort {
    fn local_context(&self, side: TeamSideV4, point: Vec2) -> LocalSpatialContext;
    fn passing_lane(&self, side: TeamSideV4, from: Vec2, to: Vec2, aerial: bool) -> (f32, Option<usize>);
    fn offside_reference(&self, side: TeamSideV4, ball: Vec2) -> Option<f32>;
    fn pass_risk(&self, side: TeamSideV4, awareness: f32, from: Vec2, target_idx: usize, kind: crate::rule_types::PassKindV4) -> f32;
    fn carry_endpoint(&self, idx: usize, direction: Vec2, duration_ms: i64) -> Vec2;
    fn reaction_delay(&self, attrs: &crate::attributes::PlayerAttributes54, pressure: f32, tempo: u8, one_touch: bool, rng: &mut impl RngSampleSource) -> i64;
    fn reaction_bounds(&self) -> (i64, i64);
}
