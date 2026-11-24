//! PI defensive action selection. Reachability is necessary, not sufficient.
//! The defender compares a committed tackle with continued containment.
//! All estimates are subjective priors in goal-equivalent units; actual
//! duel, foul, cards and penalty outcomes belong to the physics/rules engine.
use crate::v4::attributes::normalize_attr;
use crate::v4::movement::player_attrs;
use crate::v4::spatial::{attacking_progress_m,is_in_attacking_penalty_box,Vec2};
use crate::v4::state::MatchStateV4;

#[derive(Debug,Clone,Copy)]
struct TackleObservation {
    access:f32, // visible approach angle relative to the carrier's facing
    relative_speed:f32,
    reach_time_s:f32,
    cover:f32,
    threat:f32,
    own_box:bool,
    skill:f32,
    decisions:f32,
    aggression:f32,
    composure:f32,
    energy:f32,
    intensity:f32,
}

#[derive(Debug,Clone,Copy)]
pub(crate) struct TackleEvaluation {
    pub tackle_utility:f32,
    pub contain_utility:f32,
}
impl TackleEvaluation {
    pub(crate) fn commits(self)->bool {self.tackle_utility>self.contain_utility}
}

fn evaluate(o:TackleObservation)->TackleEvaluation {
    let access=o.access.clamp(0.0,1.0);
    let skill=o.skill.clamp(0.0,1.0);
    let speed=(o.relative_speed/7.0).clamp(0.0,1.0);
    // An approach from behind or against a fast-moving body is less
    // likely to expose the ball to a clean standing tackle. A defender
    // can misjudge that risk; better decisions/composure reduce it.
    let win=((0.24+0.48*skill)*(0.18+0.82*access)
        *(0.75+0.25*o.energy)*(1.0-0.24*speed)).clamp(0.02,0.85);
    let foul=(0.035+0.19*(1.0-access)+0.10*speed
        +0.06*o.aggression-0.045*skill-0.025*o.composure)
        .clamp(0.015,(1.0-win).max(0.015));
    let miss=(1.0-win-foul).max(0.0);
    // Losing a goal-side defender is expensive without a covering player.
    let gain=0.10+0.22*o.threat;
    let missed_cost=(0.035+0.16*o.threat)*(1.0-0.60*o.cover);
    let foul_cost=if o.own_box {0.76} else {0.035+0.15*o.threat};
    let commitment_cost=0.012+0.016*o.reach_time_s.min(3.0)
        +0.020*(1.0-o.energy);
    // Decisions and temperament affect risk perception, not physical odds.
    let risk_weight=0.75+0.40*o.decisions+0.15*o.composure-0.15*o.aggression;
    let tackle_utility=win*gain-risk_weight*(miss*missed_cost+foul*foul_cost)-commitment_cost;
    // Containment has positive value: it denies progression without
    // conceding a free kick or abandoning the defensive position.
    let contain_utility=(0.012+0.036*o.threat)*(0.8+0.4*o.cover)
        *(1.15-0.4*o.intensity);
    TackleEvaluation{tackle_utility,contain_utility}
}

/// Read current visible geometry and the defender's own attributes.
/// Do not read the opponent's planned action or hidden attributes.
/// reach_time is an estimate of the time needed to enter foot reach,
/// supplied by the physical reach model before commitment.
pub(crate) fn evaluate_tackle(state:&MatchStateV4,carrier_idx:usize,def_idx:usize,reach_time_s:f32)->TackleEvaluation {
    let carrier=&state.players[carrier_idx];
    let defender=&state.players[def_idx];
    let attrs=player_attrs(state,defender);
    let dx=defender.pos.x-carrier.pos.x;
    let dy=defender.pos.y-carrier.pos.y;
    let distance=(dx*dx+dy*dy).sqrt();
    let facing=Vec2::new(carrier.facing_rad.cos(),carrier.facing_rad.sin());
    // Current facing is only a coarse proxy for ball exposure.
    let front=if distance>0.05 {(dx*facing.x+dy*facing.y)/distance}else{0.0};
    let access=((front+1.0)*0.5).clamp(0.0,1.0);
    let rvx=defender.vel.x-carrier.vel.x;
    let rvy=defender.vel.y-carrier.vel.y;
    let relative_speed=(rvx*rvx+rvy*rvy).sqrt();
    let progress=attacking_progress_m(carrier.side,carrier.pos);
    let central=(1.0-(carrier.pos.y-34.0).abs()/34.0).clamp(0.0,1.0);
    let threat=(progress/105.0).clamp(0.0,1.0).powi(2)*(0.5+0.5*central);
    let cover=state.players.iter().enumerate().filter(|(i,p)|*i!=def_idx && p.on_pitch
        && !p.sent_off && !p.is_goalkeeper && p.side==defender.side
        && attacking_progress_m(carrier.side,p.pos)>progress
        && p.pos.distance(carrier.pos)<12.0)
        .map(|(_,p)|(1.0-p.pos.distance(carrier.pos)/12.0).clamp(0.0,1.0))
        .fold(0.0f32,f32::max);
    let skill=normalize_attr(attrs.technical.tackling)*0.55
        +normalize_attr(attrs.mental.anticipation)*0.25
        +normalize_attr(attrs.physical.balance)*0.20;
    evaluate(TackleObservation{access,relative_speed,reach_time_s,cover,threat,
        own_box:is_in_attacking_penalty_box(carrier.side,carrier.pos),skill,
        decisions:normalize_attr(attrs.mental.decisions),aggression:normalize_attr(attrs.mental.aggression),
        composure:normalize_attr(attrs.mental.composure),
        energy:(defender.fatigue.anaerobic/100.0).clamp(0.0,1.0),
        intensity:state.active_instructions(defender.side).pressing_intensity as f32/100.0})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opening()->TackleObservation {
        TackleObservation{access:0.98,relative_speed:0.3,reach_time_s:0.25,cover:0.6,threat:0.5,
            own_box:false,skill:0.85,decisions:0.85,aggression:0.5,composure:0.8,energy:0.9,intensity:0.65}
    }
    #[test]
    fn pi_defense_tackle_prefers_exposed_ball_to_blind_challenge(){
        let o=opening();
        let good=evaluate(o);
        let bad=evaluate(TackleObservation{access:0.05,relative_speed:5.0,..o});
        assert!(good.commits());
        assert!(!bad.commits());
    }
    #[test]
    fn pi_defense_tackle_values_penalty_risk_and_cover(){
        let o=TackleObservation{access:0.56,relative_speed:2.5,skill:0.65,..opening()};
        let outside=evaluate(o);
        let inside=evaluate(TackleObservation{own_box:true,..o});
        assert!(inside.tackle_utility<outside.tackle_utility);
        assert!(!inside.commits());
        let covered=evaluate(TackleObservation{cover:0.9,..o});
        let alone=evaluate(TackleObservation{cover:0.0,..o});
        assert!(covered.tackle_utility>alone.tackle_utility);
        assert!(evaluate(TackleObservation{own_box:true,threat:0.98,..opening()}).commits(),
            "a clean exposed-ball opportunity must remain possible inside the box");
    }
}
