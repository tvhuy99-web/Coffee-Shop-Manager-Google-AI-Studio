//! Read-only exact adapter for PI's spatial queries. No RNG and no world-state writes.
use crate::contracts::{MatchInputV4, SpatialForecastPort, TeamSideV4, Vec2};
use crate::contracts::ports::LocalSpatialContext;
use crate::v4::state::MatchStateV4;
use crate::v4::{ball_play, movement,rules::{TimingModelConfig,estimate_reaction_decision_ms}};
use crate::contracts::{PlayerAttributes54,RngSampleSource};
use crate::player_intelligence::{PassKindV4,CarryIntentV4,carry_target};
use crate::player_intelligence::pass_forecast::PassForecast;
use std::cell::RefCell;

pub(crate) struct EngineForecastAdapter<'a> {
    state: &'a MatchStateV4,
    input: &'a MatchInputV4,
    timing: &'a TimingModelConfig,
    pass_cache: RefCell<Option<(TeamSideV4,u32,PassForecast)>>,
}

impl<'a> EngineForecastAdapter<'a> {
    pub(crate) fn new(state: &'a MatchStateV4, input: &'a MatchInputV4) -> Self {
        Self::with_timing(state,input,&input.timing_model)
    }
    pub(crate) fn with_timing(state:&'a MatchStateV4,input:&'a MatchInputV4,timing:&'a TimingModelConfig)->Self {
        Self {state,input,timing,pass_cache:RefCell::new(None)}
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
    fn pass_risk(&self,side:TeamSideV4,awareness:f32,from:Vec2,target_idx:usize,kind:PassKindV4)->f32 {
        let mut cache=self.pass_cache.borrow_mut();
        if cache.as_ref().map(|(s,a,_)|(*s,*a))!=Some((side,awareness.to_bits())) { *cache=Some((side,awareness.to_bits(),PassForecast::new(self.state,side,awareness))); }
        cache.as_ref().unwrap().2.lane_risk(self.state,self.input,self.timing,from,target_idx,kind)
    }
    fn carry_endpoint(&self,idx:usize,direction:Vec2,duration_ms:i64)->Vec2 {
        let p=&self.state.players[idx];let mut profile=ball_play::pass_reach_candidate(self.state,idx,false);
        profile.speed_mps*=0.88;profile.reaction_ms=0;
        let target=carry_target(p.pos,profile.speed_mps,CarryIntentV4{direction,duration_ms,shielding:false});
        profile.position_toward(target,duration_ms)
    }
    fn reaction_delay(&self,attrs:&PlayerAttributes54,pressure:f32,tempo:u8,one_touch:bool,rng:&mut impl RngSampleSource)->i64 {
        estimate_reaction_decision_ms(self.timing,attrs.technical.first_touch,attrs.mental.vision,attrs.mental.decisions,attrs.mental.composure,pressure,tempo,one_touch,rng)
    }
    fn reaction_bounds(&self)->(i64,i64){(self.timing.one_touch_reaction_ms.min_ms,self.timing.reaction_base_ms.max_ms)}
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
