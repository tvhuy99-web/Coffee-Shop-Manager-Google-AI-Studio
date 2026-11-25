//! Read-only exact adapter for PI's spatial queries. No RNG and no world-state writes.
use crate::contracts::{MatchInputV4, SpatialForecastPort, TeamSideV4, Vec2};
use crate::contracts::ports::LocalSpatialContext;
use crate::v4::state::MatchStateV4;
use crate::v4::{ball_play, movement};

pub(crate) struct EngineForecastAdapter<'a> {
    state: &'a MatchStateV4,
    input: &'a MatchInputV4,
}

impl<'a> EngineForecastAdapter<'a> {
    pub(crate) fn new(state: &'a MatchStateV4, input: &'a MatchInputV4) -> Self {
        Self { state, input }
    }
}

impl SpatialForecastPort for EngineForecastAdapter<'_> {
    fn local_context(&self, side: TeamSideV4, point: Vec2) -> LocalSpatialContext {
        movement::evaluate_local_context(self.state, self.input, side, point)
    }
    fn passing_lane(&self, side: TeamSideV4, from: Vec2, to: Vec2, aerial: bool) -> (f32, Option<usize>) {
        movement::evaluate_passing_lane(self.state, self.input, side, from, to, aerial)
    }
    fn offside_reference(&self, side: TeamSideV4, ball: Vec2) -> Option<f32> {
        ball_play::offside_reference_progress_m(self.state, side, ball)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v4::tests::make_v4_input;
    use crate::contracts::{RulesModeV4, SimulationMode};

    #[test]
    fn a3_spatial_adapter_is_bit_exact_and_read_only() {
        let input = make_v4_input(4242, SimulationMode::FullEvent, RulesModeV4::Standard);
        let state = MatchStateV4::from_input(&input);
        let before = serde_json::to_string(&state).unwrap();
        let port = EngineForecastAdapter::new(&state, &input);
        for p in &state.players {
            let a = port.local_context(p.side, p.pos);
            let b = movement::evaluate_local_context(&state, &input, p.side, p.pos);
            assert_eq!(a.pressure.to_bits(), b.pressure.to_bits());
            assert_eq!(a.nearest_defender_idx, b.nearest_defender_idx);
            assert_eq!(a.nearest_defender_dist_m.to_bits(), b.nearest_defender_dist_m.to_bits());
            assert_eq!(a.overload, b.overload);
            assert_eq!(port.offside_reference(p.side, p.pos), ball_play::offside_reference_progress_m(&state,p.side,p.pos));
            for to in [Vec2::new(20.0,34.0),Vec2::new(80.0,34.0)] {
                for aerial in [false,true] {
                    let x=port.passing_lane(p.side,p.pos,to,aerial);
                    let y=movement::evaluate_passing_lane(&state,&input,p.side,p.pos,to,aerial);
                    assert_eq!((x.0.to_bits(),x.1),(y.0.to_bits(),y.1));
                }
            }
        }
        assert_eq!(serde_json::to_string(&state).unwrap(), before);
    }
}
