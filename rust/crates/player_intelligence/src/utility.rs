//! Common dimensionless action value. This is a fallible policy estimate,
//! never an execution probability or a measured expected-goals model.
#[derive(Debug, Clone, Copy)]
pub struct ActionValue {
    pub success: f32,
    pub benefit: f32,
    pub loss: f32,
    pub cost: f32,
}

impl ActionValue {
    pub fn expected(self) -> f32 {
        let p = self.success.clamp(0.0, 1.0);
        p * self.benefit - (1.0 - p) * self.loss - self.cost
    }
}

/// Approximate continuation value of possession in goal-equivalent units.
/// This is a bounded prior, NOT empirical expected threat. All actions must
/// compare successor states in the same units, including a goal (1.0).
pub fn territory_value(progress: f32, y: f32) -> f32 {
    let x = (progress / 105.0).clamp(0.0, 1.0);
    let central = (1.0 - (y - 34.0).abs() / 34.0).clamp(0.0, 1.0);
    0.025 + 0.045*x + 0.15*x*x*(0.45+0.55*central)
}

pub fn possession_value(progress:f32,y:f32,pressure:f32)->f32 {
    territory_value(progress,y)*(1.0-0.42*pressure.clamp(0.0,1.0))
}

/// Value of the opponent's ensuing possession at a location.
pub fn turnover_cost(progress: f32) -> f32 {
    territory_value(105.0-progress,34.0)
}

pub fn position_gain(from_progress: f32, from_y: f32, to_progress: f32, to_y: f32,
    pressure_before: f32, pressure_after: f32) -> f32 {
    possession_value(to_progress,to_y,pressure_after)
        - possession_value(from_progress,from_y,pressure_before)
}

/// The engine checks block, on-target and conditional goalkeeper outcome
/// in sequence. Treating the last conditional estimate as unconditional
/// scoring chance systematically overvalues shooting.
pub fn shot_goal_probability(unblocked:f32,on_target:f32,conditional_goal:f32)->f32 {
    unblocked.clamp(0.0,1.0)*on_target.clamp(0.0,1.0)*conditional_goal.clamp(0.0,1.0)
}

/// Expected successor value: goal, attacking continuation, or lost ball.
/// All outcome probabilities sum to one. Subtract the current opportunity.
pub fn shot_value(goal:f32,retain_if_not_goal:f32,retained_value:f32,
    opponent_value:f32,current_value:f32,time_cost:f32)->f32 {
    let g=goal.clamp(0.0,1.0);
    let retain=(1.0-g)*retain_if_not_goal.clamp(0.0,1.0);
    g + retain*retained_value - (1.0-g-retain)*opponent_value - current_value - time_cost
}

/// Switch is a genuine move between opposite wide channels, not every
/// diagonal pass across the central strip.
pub fn is_switch(from_y: f32, to_y: f32, distance: f32) -> bool {
    distance >= 32.0 && (from_y - to_y).abs() >= 30.0
        && ((from_y <= 22.0 && to_y >= 46.0) || (to_y <= 22.0 && from_y >= 46.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pi_on_ball_value_uses_risk_benefit_and_time() {
        let a = ActionValue{success:0.8,benefit:0.35,loss:0.25,cost:0.04};
        assert!(a.expected() > ActionValue{success:0.5,..a}.expected());
        assert!(a.expected() > ActionValue{cost:0.12,..a}.expected());
        assert!(a.expected() < ActionValue{benefit:0.5,..a}.expected());
        assert!(turnover_cost(12.0)>turnover_cost(80.0));
    }
    #[test]
    fn pi_on_ball_shot_probability_and_opportunity_cost() {
        let p=shot_goal_probability(0.8,0.5,0.2);
        assert!((p-0.08).abs()<1e-6);
        assert!(shot_goal_probability(0.4,0.5,0.2)<p);
        let speculative=shot_value(p,0.15,0.12,0.04,0.16,0.005);
        let clear=shot_value(0.35,0.15,0.12,0.04,0.16,0.005);
        assert!(speculative<0.0 && clear>0.0);
        assert!(shot_value(p,0.15,0.12,0.04,0.20,0.005)<speculative);
    }
    #[test]
    fn pi_on_ball_switch_requires_real_advantage() {
        assert!(is_switch(10.0,58.0,48.0));
        assert!(is_switch(58.0,10.0,48.0));
        assert!(!is_switch(4.0,35.0,38.0));
        assert!(!is_switch(10.0,58.0,29.0));
        let empty = position_gain(55.0,10.0,55.0,58.0,0.1,0.1);
        let relief = position_gain(55.0,10.0,55.0,58.0,0.8,0.1);
        let forward = position_gain(55.0,10.0,72.0,34.0,0.1,0.1);
        assert!(empty.abs()<1e-6);
        assert!(relief>empty && forward>empty);
    }
}
