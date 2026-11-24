//! PI defensive action selection. Reachability is necessary, not sufficient.
//! The defender compares a committed tackle with continued containment.
//! All estimates are subjective priors in goal-equivalent units; actual
//! duel, foul, cards and penalty outcomes belong to the physics/rules engine.
use crate::v4::attributes::normalize_attr;
use crate::v4::movement::player_attrs;
use crate::v4::spatial::{attacking_progress_m,is_in_attacking_penalty_box,Vec2};
use crate::v4::state::MatchStateV4;
use super::utility::territory_value;

#[derive(Debug,Clone,Copy)]
struct TackleObservation {
    access:f32, // visible approach angle relative to the carrier's facing
    relative_speed:f32,
    reach_time_s:f32,
    cover:f32,
    threat:f32,
    progress:f32,
    y:f32,
    own_box:bool,
    skill:f32,
    tackling:f32,
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
    pub access:f32,
    pub clean_win_probability:f32,
    pub foul_probability:f32,
}
impl TackleEvaluation {
    pub(crate) fn commits(self)->bool {self.tackle_utility>self.contain_utility}
}

fn evaluate(o:TackleObservation)->TackleEvaluation {
    let access=o.access.clamp(0.0,1.0);
    let skill=o.skill.clamp(0.0,1.0);
    let speed=(o.relative_speed/7.0).clamp(0.0,1.0);
    let cover=o.cover.clamp(0.0,1.0);
    let energy=o.energy.clamp(0.0,1.0);
    let threat=o.threat.clamp(0.0,1.0);

    // A neutral opponent prior and observable ball access. Estimate the
    // chance of getting the better of the duel before judging legality.
    // This is a fallible PI forecast, not the engine's actual roll.
    let duel_win=((0.50+0.65*(skill-0.55))*(0.18+0.82*access)
        *(0.75+0.25*energy)*(1.0-0.24*speed)).clamp(0.02,0.82);
    // Execution tests the foul only on the defender-winning branch.
    // Use the same conditional structure and neutral priors for hidden
    // temperament/sportsmanship and referee strictness.
    let foul_given_win=(0.14+0.14*o.aggression+0.07-0.10*o.tackling).clamp(0.05,0.42);
    let foul=duel_win*foul_given_win;
    let clean=duel_win*(1.0-foul_given_win);
    let miss=1.0-duel_win;

    // Both choices are measured relative to current opponent possession.
    // Winning changes the sign of possession value, including the value
    // of regaining the ball high up the pitch. Missing a tackle incurs
    // only the incremental exposure, not the full existing threat again.
    let before=territory_value(o.progress,o.y);
    let recovered=territory_value(105.0-o.progress,o.y);
    let gain=before+recovered;
    let missed_cost=before*(0.18+0.52*threat)*(1.0-0.65*cover)
        +0.012*(1.0-cover);
    // A penalty replaces (rather than adds to) the existing opportunity.
    // A non-penalty free kick is a modest incremental restart risk.
    let foul_cost=if o.own_box {(0.76-before).max(0.0)} else {0.012+0.25*before};
    let commitment_cost=0.006+0.008*o.reach_time_s.clamp(0.0,3.0)+0.010*(1.0-energy);
    let risk_weight=0.75+0.40*o.decisions+0.15*o.composure-0.15*o.aggression;
    let tackle_utility=clean*gain-risk_weight*(miss*missed_cost+foul*foul_cost)-commitment_cost;
    // Continued containment does not guarantee a turnover. Its marginal
    // benefit is denial of a portion of the current possession value.
    let contain_utility=before*(0.10+0.10*cover)*(1.15-0.40*o.intensity);
    TackleEvaluation{tackle_utility,contain_utility,access,
        clean_win_probability:clean,foul_probability:foul}
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
    evaluate(TackleObservation{access,relative_speed,reach_time_s,cover,threat,progress,y:carrier.pos.y,
        own_box:is_in_attacking_penalty_box(carrier.side,carrier.pos),skill,
        tackling:normalize_attr(attrs.technical.tackling),
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
            progress:55.0,y:34.0,own_box:false,skill:0.85,tackling:0.85,decisions:0.85,aggression:0.5,composure:0.8,energy:0.9,intensity:0.65}
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
        let o=TackleObservation{access:0.56,relative_speed:2.5,skill:0.65,tackling:0.65,..opening()};
        let outside=evaluate(o);
        let inside=evaluate(TackleObservation{own_box:true,..o});
        assert!(inside.tackle_utility<outside.tackle_utility);
        assert!(!inside.commits());
        let covered=evaluate(TackleObservation{cover:0.9,..o});
        let alone=evaluate(TackleObservation{cover:0.0,..o});
        assert!(covered.tackle_utility>alone.tackle_utility);
        // A fully exposed ball, a highly skilled balanced defender and
        // real cover near goal can still justify a legal challenge.
        let emergency=TackleObservation{own_box:true,threat:0.98,progress:103.0,access:1.0,
            relative_speed:0.0,reach_time_s:0.15,skill:1.0,tackling:1.0,cover:0.85,
            aggression:0.25,energy:1.0,..opening()};
        assert!(evaluate(emergency).commits(),"a clean exposed-ball opportunity must remain possible inside the box");
    }
}


#[cfg(test)]
mod state_value_tests {
    use super::*;
    #[test]
    fn pi_defense_tackle_outcome_probabilities_are_exclusive() {
        for access in [0.0,0.25,0.5,0.75,1.0] {
            let o=TackleObservation{access,relative_speed:1.0,reach_time_s:0.4,cover:0.4,
                threat:0.3,progress:55.0,y:34.0,own_box:false,skill:0.65,tackling:0.65,
                decisions:0.65,aggression:0.65,composure:0.65,energy:0.8,intensity:0.65};
            let r=evaluate(o);
            assert!(r.clean_win_probability>0.0 && r.foul_probability>0.0);
            assert!(r.clean_win_probability+r.foul_probability<=1.0);
            let expected=(0.14+0.14*o.aggression+0.07-0.10*o.tackling).clamp(0.05,0.42);
            assert!((r.foul_probability/(r.clean_win_probability+r.foul_probability)-expected).abs()<1e-6);
        }
    }
    #[test]
    fn pi_defense_tackle_typical_front_and_rear_opportunities_differ() {
        let o=TackleObservation{access:1.0,relative_speed:0.4,reach_time_s:0.5,cover:0.3,
            threat:0.1,progress:28.0,y:34.0,own_box:false,skill:0.656,tackling:0.656,
            decisions:0.656,aggression:0.656,composure:0.656,energy:0.8,intensity:0.65};
        assert!(evaluate(o).commits(),"ordinary player can exploit a reachable exposed ball");
        assert!(!evaluate(TackleObservation{access:0.0,..o}).commits(),
            "rear challenge is not justified just because the defender can reach");
    }
}
