//! Continuous fatigue integration and match-day form/career carryover.
//! Engine owns condition state and RNG streams; no world-state access.
use match_contracts::attributes::{normalize_attr,PlayerAttributes54};
use match_contracts::ports::RngSampleSource;
use serde::{Deserialize,Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwoPoolFatigueState {
    pub aerobic: f32,
    pub anaerobic: f32,
    pub distance_m: f32,
    pub sprint_count: u32,
    pub high_intensity_actions: u32,
}

impl TwoPoolFatigueState {
    pub fn from_condition(condition: u8) -> Self {
        Self {
            aerobic: (condition as f32).clamp(10.0, 100.0),
            anaerobic: 100.0,
            distance_m: 0.0,
            sprint_count: 0,
            high_intensity_actions: 0,
        }
    }

    /// Continuous-time two-pool fatigue integration over an arbitrary DES interval `dt_sec`
    /// and actual distance covered `distance_m`.
    ///
    /// Strictly step-length invariant: no fixed per-call penalty constants (`4.8` / `0.08`).
    /// During `is_dead_ball == true` or low-intensity movement, anaerobic reserves recover
    /// proportionally to `dt_sec` and `natural_fitness`.
    pub fn apply_interval(
        &mut self,
        dt_sec: f32,
        distance_m: f32,
        is_high_intensity: bool,
        is_dead_ball: bool,
        stamina: u8,
        natural_fitness: u8,
    ) {
        let dt = dt_sec.max(0.0);
        let dist = distance_m.max(0.0);
        if dt <= 0.0 && dist <= 0.0 {
            return;
        }

        let stam = normalize_attr(stamina);
        let nat = normalize_attr(natural_fitness);
        let aerobic_efficiency = 1.24 - (stam * 0.52);

        if is_dead_ball {
            let idle_aerobic_drain = (dist * 0.0011 + dt * 0.0007) * aerobic_efficiency;
            self.aerobic = (self.aerobic - idle_aerobic_drain).clamp(5.0, 100.0);

            let recovery = dt * (0.42 + nat * 0.52);
            self.anaerobic = (self.anaerobic + recovery).clamp(0.0, 100.0);
            self.distance_m += dist;
            return;
        }

        let base_aerobic_drain = (dist * 0.0021 + dt * 0.0038) * aerobic_efficiency;
        let intensity_aerobic_drain = if is_high_intensity {
            (dt * 0.011 + dist * 0.0014) * aerobic_efficiency
        } else {
            0.0
        };
        self.aerobic =
            (self.aerobic - base_aerobic_drain - intensity_aerobic_drain).clamp(5.0, 100.0);

        if is_high_intensity {
            let ana_drain = (dt * 0.85 + dist * 0.16) * (1.20 - stam * 0.35);
            self.anaerobic = (self.anaerobic - ana_drain).clamp(0.0, 100.0);
            if dist >= 1.5 || dt >= 0.4 {
                self.sprint_count = self.sprint_count.saturating_add(1);
                self.high_intensity_actions = self.high_intensity_actions.saturating_add(1);
            }
        } else {
            let recovery = dt * (0.24 + nat * 0.38);
            self.anaerobic = (self.anaerobic + recovery).clamp(0.0, 100.0);
        }
        self.distance_m += dist;
    }

    pub fn apply_half_time_recovery(&mut self, natural_fitness: u8) {
        let nat = normalize_attr(natural_fitness);
        self.aerobic = (self.aerobic + 5.0 + nat * 7.0).clamp(5.0, 100.0);
        self.anaerobic = (self.anaerobic + 45.0 + nat * 35.0).clamp(0.0, 100.0);
    }
}

/// Computes deterministic pre-match form multiplier from `consistency`, `big_matches`,
/// `match_importance`, and the meta/career readiness hook (`professionalism` + `adaptability`).
pub fn compute_prematch_form_multiplier(
    attrs: &PlayerAttributes54,
    match_importance: u8,
    rng: &mut impl RngSampleSource,
) -> f32 {
    sample_prematch_form(attrs,match_importance,rng).1
}

/// One draw, two explicitly separated outputs:
/// (football match-day form, legacy career-influenced confidence factor).
/// Only the first is consumed by match decision uncertainty.
pub fn sample_prematch_form(
    attrs:&PlayerAttributes54,match_importance:u8,rng:&mut impl RngSampleSource
)->(f32,f32) {
    let cons = normalize_attr(attrs.hidden.consistency);
    let big = normalize_attr(attrs.hidden.big_matches);
    let readiness = attrs.career_readiness_hook();

    // Higher consistency shrinks random match-day variance
    let spread = (0.14 - cons * 0.11).clamp(0.025, 0.14);
    let variance = (rng.next_double() as f32 - 0.5) * 2.0 * spread;

    // Big-match temperament activates when match_importance >= 65
    let big_delta = if match_importance >= 65 {
        let weight = ((match_importance as f32 - 60.0) / 40.0).clamp(0.0, 1.0);
        (big - 0.5) * 0.16 * weight
    } else {
        0.0
    };

    let career_stability = (readiness - 0.5) * 0.04;
    let match_only=(1.0+variance+big_delta).clamp(0.84,1.16);
    let legacy=(1.0+variance+big_delta+career_stability).clamp(0.84,1.16);
    (match_only,legacy)
}

/// Computes post-match career carryover delta (`morale_delta`) using `ambition` & `loyalty`
/// (completing the 4 meta/career attribute hooks alongside `professionalism` & `adaptability`).
pub fn compute_post_match_morale_delta(
    attrs: &PlayerAttributes54,
    won: bool,
    drew: bool,
    rating: f32,
) -> i8 {
    let amb = normalize_attr(attrs.hidden.ambition);
    let loy = normalize_attr(attrs.hidden.loyalty);
    let base: f32 = if won {
        4.0 + amb * 3.0
    } else if drew {
        (loy - amb) * 2.0
    } else {
        -4.0 - amb * 3.0 + loy * 1.5
    };
    let perf = (rating - 6.5) * 2.0;
    (base + perf).round().clamp(-12.0, 12.0) as i8
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn condition_bounds_and_dead_ball_recovery() {
        let mut s=TwoPoolFatigueState::from_condition(80);
        s.apply_interval(15.0,80.0,true,false,10,10);
        assert!(s.aerobic<80.0 && s.anaerobic<100.0);
        let sprint=s.sprint_count;
        let before=s.anaerobic;
        s.apply_interval(20.0,0.0,false,true,10,10);
        assert!(s.anaerobic>before);
        assert_eq!(s.sprint_count,sprint);
        s.apply_half_time_recovery(20);
        assert!(s.aerobic>=5.0 && s.aerobic<=100.0 && s.anaerobic<=100.0);
        s.apply_interval(1000000.0,1000000.0,true,false,1,1);
        assert_eq!(s.aerobic,5.0);assert_eq!(s.anaerobic,0.0);
    }
}
