//! Pure directional carry and hold policy.
use match_contracts::{attributes::normalize_attr,decision_observation::DecisionObservation,ports::{SpatialForecastPort,LocalSpatialContext},spatial::{attacking_progress_m,TeamSideV4,Vec2}};
use super::intent::CarryIntentV4;
use super::utility::{ActionValue,possession_value,turnover_cost};

/// Generate a small, symmetric set of directions. Select one option by
/// deterministic utility before comparing action families; adding directions
/// must not confer extra independent random draws.
pub fn choose_carry(state:&DecisionObservation<'_>,forecast:&impl SpatialForecastPort,idx:usize,ctx:LocalSpatialContext,
    loss:f32,urgency:f32,counter:f32,readiness:f32)->Option<(CarryIntentV4,f32,f32)> {
    let p=&state.players[idx];
    let attrs=state.attrs;
    let dir=if p.side==TeamSideV4::Home {1.0} else {-1.0};
    let progress=attacking_progress_m(p.side,p.pos);
    let pressure=(ctx.pressure/100.0).clamp(0.0,1.0);
    let current=possession_value(progress,p.pos.y,pressure);
    let drib=normalize_attr(attrs.technical.dribbling);
    let agility=normalize_attr(attrs.physical.agility);
    let accel=normalize_attr(attrs.physical.acceleration);
    let mut best:Option<(CarryIntentV4,f32,f32)>=None;
    for (dx,dy) in [(1.0,0.0),(0.707,-0.707),(0.707,0.707),(0.0,-1.0),(0.0,1.0),(-0.707,-0.707),(-0.707,0.707),(-1.0,0.0)] {
        let intent=CarryIntentV4{direction:Vec2::new(dir*dx,dy),duration_ms:1600,shielding:false};
        let end=forecast.carry_endpoint(idx,intent.direction,intent.duration_ms);
        if p.pos.distance(end)<0.4 {continue;}
        let end_ctx=forecast.local_context(p.side,end);
        let (risk,_)=forecast.passing_lane(p.side,p.pos,end,false);
        let target_pressure=(end_ctx.pressure/100.0).clamp(0.0,1.0);
        let next_progress=attacking_progress_m(p.side,end);
        let success=(0.70+0.20*drib+0.08*agility+0.05*accel-0.40*risk
            -0.25*pressure-0.15*target_pressure-0.12*(1.0-readiness)).clamp(0.06,0.98);
        let value=ActionValue{success,benefit:possession_value(next_progress,end.y,target_pressure)
            +counter*((next_progress-progress)/12.0).clamp(0.0,1.0),
            loss:(loss+turnover_cost(next_progress))*0.5,cost:urgency*0.008+0.006*(1.0-readiness)}.expected()-current;
        if best.as_ref().map_or(true,|(_,old,_)|value>*old) {best=Some((intent,value,1.0-success));}
    }
    best
}

/// Shielding is an intention to protect and reposition, not automatic
/// evasion. In open space retain a stationary hold; under pressure step
/// away from the nearest challenger. Body-facing is applied by the engine.
pub fn choose_hold(state:&DecisionObservation<'_>,idx:usize,ctx:LocalSpatialContext)->CarryIntentV4 {
    let p=&state.players[idx];
    if !p.is_goalkeeper && ctx.pressure>=20.0 {
        if let Some(def)=ctx.nearest_defender_idx {
            let q=state.players[def].pos;
            let len=q.distance(p.pos);
            if len>0.05 {
                return CarryIntentV4{direction:Vec2::new((p.pos.x-q.x)/len,(p.pos.y-q.y)/len),duration_ms:1600,shielding:true};
            }
        }
    }
    CarryIntentV4{direction:Vec2::default(),duration_ms:1600,shielding:false}
}

/// Same geometry for the policy forecast and the executor. Use a modest
/// protected walking speed in a shielding stance, not a sprint.
pub fn carry_target(pos:Vec2,speed:f32,intent:CarryIntentV4)->Vec2 {
    let distance=speed*(intent.duration_ms as f32/1000.0);
    Vec2::new(pos.x+intent.direction.x*distance,pos.y+intent.direction.y*distance).clamp_to_pitch()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pi_carry_target_is_directional_and_bounded() {
        let p=Vec2::new(50.0,34.0);
        let left=CarryIntentV4{direction:Vec2::new(0.0,-1.0),duration_ms:1600,shielding:false};
        assert_eq!(carry_target(p,5.0,left),Vec2::new(50.0,26.0));
        let right=CarryIntentV4{direction:Vec2::new(0.0,1.0),..left};
        assert_eq!(carry_target(p,5.0,right),Vec2::new(50.0,42.0));
        assert_eq!(carry_target(Vec2::new(50.0,1.0),5.0,left).y,0.0);
    }
}
