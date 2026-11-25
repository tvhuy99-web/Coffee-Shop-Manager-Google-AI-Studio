//! PI defensive action selection. Reachability is necessary, not sufficient.
//! The defender compares a committed tackle with continued containment.
//! All estimates are subjective priors in goal-equivalent units; actual
//! duel, foul, cards and penalty outcomes belong to the physics/rules engine.
use crate::v4::attributes::normalize_attr;
use crate::v4::movement::player_attrs;
use crate::v4::spatial::{attacking_progress_m,is_in_attacking_penalty_box,Vec2,TeamSideV4};
use crate::v4::interaction::{GroundContactGeometry,missed_ball_body_overlap,
    tackle_loose_contact_point,tackle_loose_velocity,contact_control_probability,
    BallTrajectory,PassReachCandidate,find_earliest_ball_interaction};
use crate::v4::ball_play::pass_reach_candidate;
use crate::v4::state::MatchStateV4;
use super::utility::territory_value;

use ::player_intelligence::tackle::TackleObservation;
pub use ::player_intelligence::tackle::TackleEvaluation;
fn evaluate(o:TackleObservation)->TackleEvaluation {
    let geometry=GroundContactGeometry{access:o.access.clamp(0.0,1.0),relative_speed_mps:o.relative_speed};
    ::player_intelligence::tackle::evaluate(o,geometry.access_factor(),geometry.speed_factor())
}

/// For a future reachable candidate, estimate contact on the observed
/// approach ray at the existing foot reach distance. This is a local
/// visible-geometry approximation, not the opponent's planned motion.
/// At actual contact use the measured positions without projection.
fn projected_challenger(carrier:Vec2,defender:Vec2,reach_m:f32,reach_time_s:f32)->Vec2 {
    let distance=defender.distance(carrier);
    if reach_time_s>0.0 && distance>reach_m && distance>1e-6 {
        let scale=reach_m/distance;
        Vec2::new(carrier.x+(defender.x-carrier.x)*scale,
            carrier.y+(defender.y-carrier.y)*scale)
    }else{defender}
}

fn forecast_body_overlap(carrier:Vec2,facing:f32,defender:Vec2,reach_m:f32,reach_time_s:f32)->f32 {
    missed_ball_body_overlap(carrier,facing,projected_challenger(carrier,defender,reach_m,reach_time_s),reach_m)
}

/// Compare the earliest plausible access of each side, not the eventual
/// authoritative winner. Use a short, symmetric observation horizon and
/// neutral motion priors for everyone except the deciding defender.
/// A soft margin represents uncertainty in observed motion and contact.
fn loose_recovery_prior(traj:&BallTrajectory,players:&[PassReachCandidate],def_side:TeamSideV4)->f32 {
    let horizon=traj.start_time_ms+2500;
    let boundary=find_earliest_ball_interaction(traj,&[],traj.start_time_ms,horizon);
    let end=boundary.as_ref().map_or(horizon,|h|h.time_ms.saturating_sub(1));
    let mut def_time=None;
    let mut att_time=None;
    for elapsed in (250_i64..=2500).step_by(100) {
        let t=traj.start_time_ms+elapsed;
        if t>end {break;}
        let ball=traj.position_at_ms(t);
        let vel=traj.velocity_at_ms(t);
        if (vel.x*vel.x+vel.y*vel.y).sqrt()>7.0 {continue;}
        for p in players {
            let slot=if p.side==def_side {&mut def_time}else{&mut att_time};
            if slot.is_none() && ball.z<=p.playable_height_m
                && p.position_toward(ball.xy(),elapsed).distance(ball.xy())<=p.reach_m {
                *slot=Some(elapsed);
            }
        }
        if def_time.is_some()&&att_time.is_some(){break;}
    }
    match (def_time,att_time) {
        (Some(d),Some(a))=>(0.5+(a-d) as f32/1000.0).clamp(0.0,1.0),
        (Some(_),None)=>1.0,
        (None,Some(_))=>0.0,
        // No player can reach before a visible exit: do not value a
        // restart as controlled possession. Outside our horizon unknown.
        (None,None)=>if boundary.is_some(){0.0}else{0.5},
    }
}

/// Forecast conditional control and loose recovery from visible geometry.
/// No opponent attributes, planned intentions, outcome RNG or aggregate
/// recovery percentages enter this estimate.
fn forecast_second_ball(state:&MatchStateV4,carrier_idx:usize,def_idx:usize,reach_time_s:f32,reach_m:f32)->(f32,f32) {
    let carrier=&state.players[carrier_idx];
    let defender=&state.players[def_idx];
    let attrs=player_attrs(state,defender);
    let projected=projected_challenger(carrier.pos,defender.pos,reach_m,reach_time_s);
    let spot=tackle_loose_contact_point(carrier.pos,carrier.facing_rad,projected,reach_m);
    let vel=tackle_loose_velocity(carrier.vel,defender.vel,projected,spot);
    let relative=GroundContactGeometry::observe(carrier.pos,carrier.vel,carrier.facing_rad,projected,defender.vel).relative_speed_mps;
    let skill=normalize_attr(attrs.technical.first_touch)*0.45
        +normalize_attr(attrs.technical.technique)*0.30
        +normalize_attr(attrs.mental.composure)*0.25;
    let fatigue=(defender.fatigue.aerobic*0.6+defender.fatigue.anaerobic*0.4)/100.0;
    // Same proximity shape as pressure, without hidden opponent skills,
    // fatigue or team instructions. A subjective neutral pressure prior.
    let pressure=state.players.iter().filter(|p|p.on_pitch&&!p.sent_off&&p.side!=defender.side)
        .map(|p|{let near=(1.0-p.pos.distance(carrier.pos)/12.0).clamp(0.0,1.0);
            (62.0*near*near+18.0*near)/100.0}).sum::<f32>().clamp(0.0,1.0);
    let control=contact_control_probability(skill,normalize_attr(attrs.physical.balance),fatigue,
        relative,0.0,carrier.pos.distance(projected)/reach_m.max(0.1),pressure);
    let candidates:Vec<_>=state.players.iter().enumerate().filter(|(_,p)|p.on_pitch&&!p.sent_off)
        .filter(|(i,p)|*i==def_idx||p.pos.distance(spot.xy())<=25.0)
        .map(|(i,p)|{
            if i==def_idx {
                let mut c=pass_reach_candidate(state,i,false);
                c.pos=projected;
                c
            }else{
                let speed=(p.vel.x*p.vel.x+p.vel.y*p.vel.y).sqrt();
                PassReachCandidate{player_id:p.player_id.clone(),side:p.side,pos:p.pos,velocity:p.vel,
                    speed_mps:6.5f32.max(speed).min(9.0),acceleration_mps2:4.0,
                    agility:0.65,reaction_ms:220,reach_m:1.0,playable_height_m:1.2}
            }
        }).collect();
    let trajectory=BallTrajectory{start_time_ms:0,start_pos:spot,initial_vel:vel,gravity_z:9.81,
        drag:1.6,is_shot:false,is_carrier_dribble:false};
    (control,loose_recovery_prior(&trajectory,&candidates,defender.side))
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
    let (control_given_clean,recovery_given_loose)=forecast_second_ball(state,carrier_idx,def_idx,reach_time_s,reach_m);
    evaluate(TackleObservation{access,relative_speed,body_overlap,reach_time_s,cover,threat,progress,y:carrier.pos.y,
        control_given_clean,recovery_given_loose,
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
            progress:55.0,y:34.0,own_box:false,skill:0.85,tackling:0.85,decisions:0.85,aggression:0.5,composure:0.8,energy:0.9,intensity:0.65,control_given_clean:1.0,recovery_given_loose:0.5}
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
                decisions:0.65,aggression:0.65,composure:0.65,energy:0.8,intensity:0.65,control_given_clean:1.0,recovery_given_loose:0.5};
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
            decisions:0.656,aggression:0.656,composure:0.656,energy:0.8,intensity:0.65,control_given_clean:1.0,recovery_given_loose:0.5};
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
            energy:0.9,intensity:0.65,control_given_clean:1.0,recovery_given_loose:0.5};
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


#[cfg(test)]
mod second_ball_tests {
    use super::*;
    use crate::v4::spatial::Vec3;
    fn opening()->TackleObservation {
        TackleObservation{access:0.9,relative_speed:0.4,body_overlap:0.0,reach_time_s:0.3,
            cover:0.5,threat:0.3,progress:55.0,y:34.0,own_box:false,skill:0.8,tackling:0.8,
            decisions:0.7,aggression:0.5,composure:0.7,energy:0.9,intensity:0.65,
            control_given_clean:0.35,recovery_given_loose:0.5}
    }
    #[test]
    fn pi_defense_second_ball_partitions_legal_contact_without_changing_foul() {
        let o=opening();
        let r=evaluate(o);
        assert!((r.controlled_probability+r.loose_probability-r.clean_win_probability).abs()<1e-6);
        assert!((r.controlled_probability-r.clean_win_probability*0.35).abs()<1e-6);
        let certain=evaluate(TackleObservation{control_given_clean:1.0,..o});
        let lost=evaluate(TackleObservation{recovery_given_loose:0.0,..o});
        let won=evaluate(TackleObservation{recovery_given_loose:1.0,..o});
        assert_eq!(r.clean_win_probability,certain.clean_win_probability);
        assert_eq!(r.foul_probability,certain.foul_probability);
        assert_eq!(r.contain_utility,certain.contain_utility);
        assert!(lost.tackle_utility<r.tackle_utility && r.tackle_utility<won.tackle_utility);
        assert!((certain.tackle_utility-won.tackle_utility).abs()<1e-6);
        let good_control=evaluate(TackleObservation{control_given_clean:0.8,..o});
        assert!(good_control.tackle_utility>r.tackle_utility);
    }
    #[test]
    fn pi_defense_second_ball_race_is_symmetric_and_geometry_sensitive() {
        let traj=BallTrajectory{start_time_ms:0,start_pos:Vec3::ground(50.0,34.0),
            initial_vel:Vec3::new(5.0,0.0,0.0),gravity_z:9.81,drag:1.6,
            is_shot:false,is_carrier_dribble:false};
        let p=|id:&str,side,x|PassReachCandidate{player_id:id.into(),side,pos:Vec2::new(x,34.0),
            velocity:Vec2::default(),speed_mps:6.5,acceleration_mps2:4.0,agility:0.65,
            reaction_ms:220,reach_m:1.0,playable_height_m:1.2};
        let mut players=vec![p("D",TeamSideV4::Home,53.0),p("A",TeamSideV4::Away,45.0)];
        let home=loose_recovery_prior(&traj,&players,TeamSideV4::Home);
        let away=loose_recovery_prior(&traj,&players,TeamSideV4::Away);
        assert!(home>0.5 && away<0.5);
        assert!((home+away-1.0).abs()<1e-6);
        players.swap(0,1);
        assert_eq!(home,loose_recovery_prior(&traj,&players,TeamSideV4::Home));
        players[0].pos=Vec2::new(52.0,34.0);
        players[1].pos=Vec2::new(44.0,34.0);
        assert!(loose_recovery_prior(&traj,&players,TeamSideV4::Home)<home);
        let exit=BallTrajectory{start_pos:Vec3::ground(50.0,67.9),initial_vel:Vec3::new(0.0,5.0,0.0),..traj};
        assert_eq!(loose_recovery_prior(&exit,&players,TeamSideV4::Home),0.0);
    }
}
