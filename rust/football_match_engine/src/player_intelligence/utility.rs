//! Common dimensionless action value. This is a fallible policy estimate,
//! never an execution probability or a measured expected-goals model.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionValue {
    pub success: f32,
    pub benefit: f32,
    pub loss: f32,
    pub cost: f32,
}

impl ActionValue {
    pub(crate) fn expected(self) -> f32 {
        let p = self.success.clamp(0.0, 1.0);
        p * self.benefit - (1.0 - p) * self.loss - self.cost
    }
}

/// Value of a controlled position, not the absolute merit of a pass.
/// Central attacking positions are generally more useful than equally
/// advanced wide positions. No reward just for crossing the pitch.
pub(crate) fn territory_value(progress: f32, y: f32) -> f32 {
    let x = (progress / 105.0).clamp(0.0, 1.0);
    let central = (1.0 - (y - 34.0).abs() / 34.0).clamp(0.0, 1.0);
    x * 0.36 + x * x * (0.18 + 0.32 * central)
}

/// Losing possession near one's own goal is worse than losing it high up.
pub(crate) fn turnover_cost(progress: f32) -> f32 {
    let own_threat = 1.0 - (progress / 105.0).clamp(0.0, 1.0);
    0.16 + 0.38 * own_threat * own_threat
}

pub(crate) fn position_gain(from_progress: f32, from_y: f32, to_progress: f32, to_y: f32,
    pressure_before: f32, pressure_after: f32) -> f32 {
    territory_value(to_progress, to_y) - territory_value(from_progress, from_y)
        + 0.28 * (pressure_before.clamp(0.0, 1.0) - pressure_after.clamp(0.0, 1.0))
}

/// Switch is a genuine move between opposite wide channels, not every
/// diagonal pass across the central strip.
pub(crate) fn is_switch(from_y: f32, to_y: f32, distance: f32) -> bool {
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
