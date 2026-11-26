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

/// Compatibility conversion for existing nearest-ms callers.
/// Preserves f32 multiply, ties-away-from-zero rounding, and Rust saturating cast.
/// Caller owns domain validation; this must not be used to validate input.
#[inline]
pub fn seconds_to_ms_nearest_f32(seconds: f32) -> DurationMs {
    (seconds * 1000.0).round() as i64
}

/// Checked conversion for future physical-event scheduling: never schedule
/// before the continuous offset. Reject negative, non-finite or overflowing
/// offsets rather than clamping. Existing nearest-ms resolvers are unchanged.
#[inline]
pub fn physical_offset_ms_ceil(seconds: f64) -> Option<DurationMs> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let ms = (seconds * 1000.0).ceil();
    if !ms.is_finite() || ms >= i64::MAX as f64 {
        return None;
    }
    Some(ms as i64)
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
    fn b11_numeric_compatibility_and_split() {
        for ms in [0_i64, 1, 17, 100, 500, 999, 1000, 2700000, i64::MAX] {
            assert_eq!(ms_to_seconds_f32(ms).to_bits(), ((ms as f32) / 1000.0).to_bits());
            assert_eq!(ms_to_seconds_f64(ms).to_bits(), ((ms as f64) / 1000.0).to_bits());
        }
        for sec in [0.0_f32, 0.00049, 0.0005, 0.0015, 1.2002, -0.0005, f32::MAX, f32::NAN, f32::INFINITY] {
            assert_eq!(seconds_to_ms_nearest_f32(sec), (sec * 1000.0).round() as i64);
        }
        assert_eq!(seconds_to_ms_nearest_f32(0.0005), 1);
        assert_eq!(ms_to_seconds_f32(1000), ms_to_seconds_f32(500) + ms_to_seconds_f32(500));
        assert_eq!(REGULATION_HALF_MS, 45 * 60 * 1000);
        assert_eq!(EXTRA_TIME_HALF_MS, 15 * 60 * 1000);
    }

    #[test]
    fn b11_physical_event_never_rounds_early() {
        assert_eq!(physical_offset_ms_ceil(0.0), Some(0));
        assert_eq!(physical_offset_ms_ceil(0.0001), Some(1));
        assert_eq!(physical_offset_ms_ceil(1.2002), Some(1201));
        assert_eq!(physical_offset_ms_ceil(1.2008), Some(1201));
        assert_eq!(physical_offset_ms_ceil(1.25), Some(1250));
        for sec in [-1.0, f64::NAN, f64::INFINITY, f64::MAX, i64::MAX as f64] {
            assert_eq!(physical_offset_ms_ceil(sec), None);
        }
    }

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
