//! Canonical simulation units, typed identifiers, and domain bounds for Match Engine 4.0.
//!
//! Pure types and conversion helpers only; no gameplay formulas or world state.

pub use crate::spatial::{TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M, PITCH_WIDTH_M};

/// Monotonic discrete-event simulation clock in integer milliseconds.
pub type SimTimeMs = i64;

/// Non-negative elapsed or scheduled duration in integer milliseconds.
pub type DurationMs = i64;

/// Canonical identifiers across engine, telemetry, and presentation boundaries.
pub type MatchId = String;
pub type TeamId = String;
pub type PlayerId = String;
pub type EventId = u64;
pub type CommandId = u64;

/// Canonical attribute scale bounds (`PlayerAttributes54` uses 1..=20).
pub use crate::attributes::{ATTR_MIN as ATTRIBUTE_MIN, ATTR_MAX as ATTRIBUTE_MAX};

/// Canonical regulation half and extra-time durations in milliseconds.
pub const REGULATION_HALF_MS: DurationMs = 2_700_000;
pub const EXTRA_TIME_HALF_MS: DurationMs = 900_000;

/// Converts integer milliseconds to seconds as `f64` for continuous kinematics.
#[inline]
pub fn ms_to_seconds_f64(ms: DurationMs) -> f64 {
    (ms as f64) / 1000.0
}

/// Converts integer milliseconds to seconds as `f32`.
#[inline]
pub fn ms_to_seconds_f32(ms: DurationMs) -> f32 {
    (ms as f32) / 1000.0
}

/// Validates that a floating-point value is finite (neither NaN nor infinite).
#[inline]
pub fn is_finite_f32(v: f32) -> bool {
    v.is_finite()
}

/// Validates that a 2D vector has strictly finite coordinates.
#[inline]
pub fn is_finite_vec2(v: Vec2) -> bool {
    v.x.is_finite() && v.y.is_finite()
}

/// Validates that a 3D vector has strictly finite coordinates.
#[inline]
pub fn is_finite_vec3(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions_and_finiteness_checks() {
        assert_eq!(ms_to_seconds_f64(1500), 1.5);
        assert_eq!(ms_to_seconds_f32(250), 0.25);
        assert!(is_finite_f32(10.5));
        assert!(!is_finite_f32(f32::NAN));
        assert!(!is_finite_f32(f32::INFINITY));
        assert!(is_finite_vec2(Vec2::new(52.5, 34.0)));
        assert!(!is_finite_vec2(Vec2::new(f32::NAN, 0.0)));
        assert!(is_finite_vec3(Vec3::new(0.0, 0.0, 1.2)));
    }
}
