//! Engine adapter. PI receives an immutable observation and narrow forecast capability.
use crate::contracts::{MatchInputV4,RngSampleSource};
use crate::contracts::ports::LocalSpatialContext;
use crate::v4::{observation::EngineForecastAdapter,state::MatchStateV4,movement::player_attrs,rules::TimingModelConfig};
use match_contracts::decision_observation::{DecisionObservation,DecisionPlayer};
use ::player_intelligence::{decision as policy,intent::{DecisionPlanV4,PlannedActionV4}};
pub use ::player_intelligence::decision::PassReaim;

fn observation(state:&MatchStateV4,idx:usize)->DecisionObservation<'_>{
    let p=&state.players[idx];
    let players=state.players.iter().map(|q|{
        let (first_touch,heading)=if q.side==p.side&&q.on_pitch&&!q.sent_off {
            let a=player_attrs(state,q);(a.technical.first_touch,a.technical.heading)
        } else {(0,0)};
        DecisionPlayer{pos:q.pos,side:q.side,on_pitch:q.on_pitch,sent_off:q.sent_off,is_goalkeeper:q.is_goalkeeper,first_touch,heading}
    }).collect();
    DecisionObservation{players,attrs:player_attrs(state,p),own_instructions:state.active_instructions(p.side),
        opponent_line_height:state.active_instructions(p.side.opposite()).line_height,phase:state.team_state(p.side).phase,
        position_familiarity:p.position_familiarity,anaerobic:p.fatigue.anaerobic,
        concentration_level:p.psychology.concentration_level,match_form_multiplier:p.psychology.match_form_multiplier,
        composure_delta:p.psychology.composure_delta,personal_confidence_delta:p.psychology.personal_confidence_delta,
        personal_confidence_bound:crate::v4::psychology::PERSONAL_CONFIDENCE_BOUND}
}
pub fn perceive_evaluate_and_decide_des(state:&MatchStateV4,input:&MatchInputV4,timing:&TimingModelConfig,idx:usize,ctx:LocalSpatialContext,rng:&mut impl RngSampleSource)->DecisionPlanV4{
    perceive_evaluate_and_decide_with_motion(state,input,timing,idx,ctx,rng,false)
}
pub(crate) fn perceive_evaluate_and_decide_with_motion(state:&MatchStateV4,input:&MatchInputV4,timing:&TimingModelConfig,
    idx:usize,ctx:LocalSpatialContext,rng:&mut impl RngSampleSource,experimental:bool)->DecisionPlanV4 {
    policy::decide_with_forecast(&observation(state,idx),idx,ctx,&EngineForecastAdapter::with_motion(state,input,timing,experimental),rng)
}
pub fn perceive_evaluate_and_decide(state:&MatchStateV4,input:&MatchInputV4,idx:usize,ctx:LocalSpatialContext,rng:&mut impl RngSampleSource)->PlannedActionV4{
    perceive_evaluate_and_decide_des(state,input,&TimingModelConfig::default(),idx,ctx,rng).action
}
pub(crate) fn observe_pass_risk(state:&MatchStateV4,input:&MatchInputV4,idx:usize,ctx:LocalSpatialContext,action:PlannedActionV4)->Option<f32>{
    observe_pass_risk_with_motion(state,input,idx,ctx,action,false)
}
pub(crate) fn observe_pass_risk_with_motion(state:&MatchStateV4,input:&MatchInputV4,idx:usize,ctx:LocalSpatialContext,
    action:PlannedActionV4,experimental:bool)->Option<f32>{
    state.players.get(idx)?;
    policy::observe_pass_risk(&observation(state,idx),
        &EngineForecastAdapter::with_motion(state,input,&input.timing_model,experimental),idx,ctx,action)
}
pub(crate) fn reconsider_short_pass(state:&MatchStateV4,input:&MatchInputV4,idx:usize,ctx:LocalSpatialContext,action:PlannedActionV4)->Option<PassReaim>{
    reconsider_short_pass_with_motion(state,input,idx,ctx,action,false)
}
pub(crate) fn reconsider_short_pass_with_motion(state:&MatchStateV4,input:&MatchInputV4,idx:usize,ctx:LocalSpatialContext,
    action:PlannedActionV4,experimental:bool)->Option<PassReaim>{
    state.players.get(idx)?;
    policy::reconsider_short_pass(&observation(state,idx),
        &EngineForecastAdapter::with_motion(state,input,&input.timing_model,experimental),idx,ctx,action)
}
#[cfg(test)]
pub(crate) fn decision_uncertainty(state:&MatchStateV4,idx:usize,pressure:f32)->f32{
    policy::decision_uncertainty(&observation(state,idx),idx,pressure)
}
