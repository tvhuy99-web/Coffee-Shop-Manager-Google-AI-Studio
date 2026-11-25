//! PI defensive action selection. Reachability is necessary, not sufficient.
//! The defender compares a committed tackle with continued containment.
//! All estimates are subjective priors in goal-equivalent units; actual
//! duel, foul, cards and penalty outcomes belong to the physics/rules engine.
use crate::v4::attributes::normalize_attr;
use crate::v4::movement::player_attrs;
use crate::v4::spatial::{attacking_progress_m,is_in_attacking_penalty_box,Vec2};
use crate::v4::interaction::{GroundContactGeometry,missed_ball_body_overlap};
use crate::v4::state::MatchStateV4;
use super::utility::territory_value;

#[derive(Debug,Clone,Copy)]
struct TackleObservation {
    access:f32, // visible approach angle relative to the carrier's facing
    relative_speed:f32,
    body_overlap:f32,
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
    let geometry=GroundContactGeometry{access,relative_speed_mps:o.relative_speed};
    let cover=o.cover.clamp(0.0,1.0);
    let energy=o.energy.clamp(0.0,1.0);
    let threat=o.threat.clamp(0.0,1.0);

    // A neutral opponent prior and observable ball access. Estimate the
    // chance of getting the better of the duel before judging legality.
    // This is a fallible PI forecast, not the engine's actual roll.
    let duel_win=((0.50+0.65*(skill-0.55))*geometry.access_factor()
        *(0.75+0.25*energy)*geometry.speed_factor()).clamp(0.02,0.82);
    // As in execution, a ball win can be unsafe, and a missed ball
    // can independently produce an unsafe body contact. Retain the
    // existing neutral discipline/referee prior: PI is not an oracle.
    let foul_propensity=(0.14+0.14*o.aggression+0.07-0.10*o.tackling).clamp(0.05,0.42);
    let missed_ball=1.0-duel_win;
    let missed_ball_foul=missed_ball*o.body_overlap.clamp(0.0,1.0)*foul_propensity;
    let foul=duel_win*foul_propensity+missed_ball_foul;
    let clean=duel_win*(1.0-foul_propensity);
    // Exclusive outcomes: a missed-ball foul must not also pay the
    // ordinary miss cost in the utility comparison.
    let miss=missed_ball-missed_ball_foul;

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

/// For a future reachable candidate, estimate contact on the observed
/// approach ray at the existing foot reach distance. This is a local
/// visible-geometry approximation, not the opponent's planned motion.
/// At actual contact use the measured positions without projection.
fn forecast_body_overlap(carrier:Vec2,facing:f32,defender:Vec2,reach_m:f32,reach_time_s:f32)->f32 {
    let distance=defender.distance(carrier);
    let contact=if reach_time_s>0.0 && distance>reach_m && distance>1e-6 {
        let scale=reach_m/distance;
        Vec2::new(carrier.x+(defender.x-carrier.x)*scale,
            carrier.y+(defender.y-carrier.y)*scale)
    }else{defender};
    missed_ball_body_overlap(carrier,facing,contact,reach_m)
}

/// Read current visible geometry and the defender's own attributes.
/// Do not read the opponent's planned action or hidden attributes.
/// reach_time is an estimate of the time needed to enter foot reach,
/// supplied by the physical reach model before commitment.
pub(crate) fn evaluate_tackle(state:&MatchStateV4,carrier_idx:usize,def_idx:usize,reach_time_s:f32)->TackleEvaluation {
    let carrier=&state.players[carrier_idx];
    let defender=&state.players[def_idx];
    let attrs=player_attrs(state,defender);
    let geometry=GroundContactGeometry::observe(carrier.pos,carrier.vel,carrier.facing_rad,
        defender.pos,defender.vel);
    let access=geometry.access;
    let relative_speed=geometry.relative_speed_mps;
    let reach_m=0.85+normalize_attr(attrs.physical.agility)*0.25;
    let body_overlap=forecast_body_overlap(carrier.pos,carrier.facing_rad,defender.pos,reach_m,reach_time_s);
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
    evaluate(TackleObservation{access,relative_speed,body_overlap,reach_time_s,cover,threat,progress,y:carrier.pos.y,
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
        TackleObservation{access:0.98,relative_speed:0.3,body_overlap:0.0,reach_time_s:0.25,cover:0.6,threat:0.5,
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
            let o=TackleObservation{access,relative_speed:1.0,body_overlap:0.0,reach_time_s:0.4,cover:0.4,
                threat:0.3,progress:55.0,y:34.0,own_box:false,skill:0.65,tackling:0.65,
                decisions:0.65,aggression:0.65,composure:0.65,energy:0.8,intensity:0.65};
            let r=evaluate(o);
            assert!(r.clean_win_probability>0.0 && r.foul_probability>0.0);
            assert!(r.clean_win_probability+r.foul_probability<=1.0);
            let expected=(0.14+0.14*o.aggression+0.07-0.10*o.tackling).clamp(0.05,0.42);
            assert!((r.foul_probability/(r.clean_win_probability+r.foul_probability)-expected).abs()<1e-6);
            let ball_win=r.clean_win_probability+r.foul_probability;
            for overlap in [0.25,0.5,1.0] {
                let body=evaluate(TackleObservation{body_overlap:overlap,..o});
                let extra=(1.0-ball_win)*overlap*expected;
                assert_eq!(body.clean_win_probability,r.clean_win_probability);
                assert!((body.foul_probability-r.foul_probability-extra).abs()<1e-6);
                assert!(body.clean_win_probability+body.foul_probability<=1.0);
                // Explicitly check the ordinary miss is not charged twice.
                let before=territory_value(o.progress,o.y);
                let miss_cost=before*(0.18+0.52*o.threat)*(1.0-0.65*o.cover)+0.012*(1.0-o.cover);
                let foul_cost=0.012+0.25*before;
                let risk=0.75+0.40*o.decisions+0.15*o.composure-0.15*o.aggression;
                assert!((body.tackle_utility-r.tackle_utility+extra*risk*(foul_cost-miss_cost)).abs()<1e-6);
            }
        }
    }
    #[test]
    fn pi_defense_tackle_typical_front_and_rear_opportunities_differ() {
        let o=TackleObservation{access:1.0,relative_speed:0.4,body_overlap:0.0,reach_time_s:0.5,cover:0.3,
            threat:0.1,progress:28.0,y:34.0,own_box:false,skill:0.656,tackling:0.656,
            decisions:0.656,aggression:0.656,composure:0.656,energy:0.8,intensity:0.65};
        assert!(evaluate(o).commits(),"ordinary player can exploit a reachable exposed ball");
        assert!(!evaluate(TackleObservation{access:0.0,..o}).commits(),
            "rear challenge is not justified just because the defender can reach");
    }
}


#[cfg(test)]
mod body_forecast_tests {
    use super::*;
    #[test]
    fn pi_defense_body_forecast_uses_observable_reachable_approach() {
        let c=Vec2::new(50.0,34.0);
        let far=Vec2::new(53.0,34.0);
        let close=Vec2::new(51.0,34.0);
        let rear=std::f32::consts::PI;
        assert_eq!(forecast_body_overlap(c,0.0,far,1.0,0.5),0.0);
        assert_eq!(forecast_body_overlap(c,rear,far,1.0,0.0),0.0);
        let actual=forecast_body_overlap(c,rear,close,1.0,0.0);
        let predicted=forecast_body_overlap(c,rear,far,1.0,0.5);
        assert!(actual>0.95);
        assert!((predicted-actual).abs()<1e-6);
        assert_eq!(actual,missed_ball_body_overlap(c,rear,close,1.0));
        let mirror=forecast_body_overlap(Vec2::new(55.0,34.0),0.0,Vec2::new(52.0,34.0),1.0,0.5);
        assert!((mirror-predicted).abs()<1e-5);
    }
    #[test]
    fn pi_defense_body_risk_does_not_remove_clean_ball_branch() {
        let o=TackleObservation{access:0.3,relative_speed:2.0,body_overlap:0.0,
            reach_time_s:0.5,cover:0.5,threat:0.8,progress:94.0,y:34.0,own_box:true,
            skill:0.8,tackling:0.8,decisions:0.8,aggression:0.65,composure:0.8,
            energy:0.9,intensity:0.65};
        let clear=evaluate(o);
        let blocked=evaluate(TackleObservation{body_overlap:1.0,..o});
        assert_eq!(clear.clean_win_probability,blocked.clean_win_probability);
        assert!(blocked.foul_probability>clear.foul_probability);
        assert!(blocked.tackle_utility<clear.tackle_utility);
        assert_eq!(clear.contain_utility,blocked.contain_utility);
        println!("PI_TACKLE_BODY_FORECAST clean={} clear_foul={} blocked_foul={} clear_utility={} blocked_utility={}",
            clear.clean_win_probability,clear.foul_probability,blocked.foul_probability,clear.tackle_utility,blocked.tackle_utility);
    }
}
