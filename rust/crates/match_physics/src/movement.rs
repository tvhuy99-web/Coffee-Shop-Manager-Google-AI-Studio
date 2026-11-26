//! Pure locomotion capability. Values are derived per motion plan, not saved attributes.
use match_contracts::attributes::{normalize_attr, PhysicalAttributes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionError {
    InvalidCapability,
    InvalidState,
    InvalidIntent,
    InvalidHorizon,
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
