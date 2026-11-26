//! Read-only flight perception. No access to sampled kick errors,
//! opponent hidden attributes, future intents or RNG.
use crate::v4::ball_play::{build_pass_trajectory,pass_reach_candidate};
use crate::v4::input::MatchInputV4;
use crate::v4::interaction::{BallTrajectory,PassReachCandidate};
use crate::v4::rules::{nominal_ball_flight_ms,BallFlightKind,TimingModelConfig};
use crate::v4::spatial::{point_to_segment_distance_m,TeamSideV4,Vec2};
use crate::v4::state::MatchStateV4;
use super::intent::PassKindV4;

pub(crate) struct PassForecast {
    opponents:Vec<PassReachCandidate>,
    awareness:f32,
    experimental:bool,
}

impl PassForecast {
    pub(crate) fn new(state:&MatchStateV4,side:TeamSideV4,awareness:f32)->Self {
        Self::with_motion(state,side,awareness,false)
    }

    pub(crate) fn with_motion(state:&MatchStateV4,side:TeamSideV4,awareness:f32,experimental:bool)->Self {
        let opponents=state.players.iter().filter(|p|p.on_pitch&&!p.sent_off&&!p.is_goalkeeper&&p.side!=side)
            .map(|p|{
                let observed_speed=(p.vel.x*p.vel.x+p.vel.y*p.vel.y).sqrt();
                // Neutral motion priors; actual opponent attributes belong
                // to physical execution, not the passer's perception.
                if experimental { return observed_runner(p,false); }
                PassReachCandidate{player_id:p.player_id.clone(),side:p.side,pos:p.pos,velocity:p.vel,
                    speed_mps:6.5f32.max(observed_speed).min(9.0),acceleration_mps2:4.0,
                    agility:0.65,reaction_ms:220,reach_m:1.0,playable_height_m:1.2,locomotion:None}
            }).collect();
        Self{opponents,awareness:awareness.clamp(0.0,1.0),experimental}
    }

    pub(crate) fn lane_risk(&self,state:&MatchStateV4,input:&MatchInputV4,timing:&TimingModelConfig,
        from:Vec2,target_idx:usize,kind:PassKindV4)->f32 {
        let to=state.players[target_idx].pos;
        let flight=nominal_ball_flight_ms(timing,from.distance(to),BallFlightKind::from(kind),
            input.match_environment.weather,input.match_environment.pitch_quality);
        let traj=build_pass_trajectory(kind,from,to,0,flight);
        if self.experimental {
            let receiver=observed_runner(&state.players[target_idx],kind.is_aerial());
            predicted_motion_risk(&traj,flight,kind.is_aerial(),&self.opponents,&receiver,self.awareness)
        } else {
            let receiver=pass_reach_candidate(state,target_idx,kind.is_aerial());
            predicted_contact_risk(&traj,flight,kind.is_aerial(),&self.opponents,&receiver,self.awareness)
        }
    }
}

/// The experiment uses the same motion law, not authoritative capabilities.
/// Only visible position/velocity/facing and identity enter these neutral
/// priors. Do not read actual fatigue, attributes, future intents or kick errors.
fn observed_runner(p:&crate::v4::state::PlayerRuntimeStateV4,aerial:bool)->PassReachCandidate {
    let speed=6.5_f32.max(p.vel.x.hypot(p.vel.y)).min(9.0);
    let capability=match_physics::movement::MovementCapability {
        max_speed_mps:speed, acceleration_mps2:4.0, braking_mps2:5.5,
        lateral_mps2:4.5, body_turn_radps:4.5,
    };
    PassReachCandidate{player_id:p.player_id.clone(),side:p.side,pos:p.pos,velocity:p.vel,
        speed_mps:speed,acceleration_mps2:4.0,agility:0.65,reaction_ms:220,reach_m:1.0,
        playable_height_m:if aerial {2.05} else {1.2},
        locomotion:Some(match_physics::ReachLocomotion{capability,facing_rad:p.facing_rad})}
}

/// Soft feasibility estimate, not a probability of the resolved pass.
/// Reject impossible samples cheaply before integrating the actual solver.
/// The positive margin represents limited perception, not added physical reach.
fn observed_residual(p:&PassReachCandidate, target:Vec2, elapsed_ms:i64, margin:f32)->Option<f32> {
    let bound=p.motion_speed_bound_mps()*match_contracts::units::ms_to_seconds_f32(elapsed_ms);
    if p.pos.distance(target)>bound+p.reach_m+margin {return None;}
    p.contact_position_toward(target,elapsed_ms).map(|pos|pos.distance(target))
}

fn predicted_motion_risk(traj:&BallTrajectory,flight:i64,aerial:bool,
    opponents:&[PassReachCandidate],receiver:&PassReachCandidate,awareness:f32)->f32 {
    if flight<=0 || flight>match_physics::movement::MAX_MOTION_HORIZON_MS {return 1.0;}
    let start=traj.position_at_ms(0).xy();
    let end=traj.position_at_ms(flight).xy();
    let horizon=match_contracts::units::ms_to_seconds_f32(flight);
    let margin=0.65+0.70*(1.0-awareness.clamp(0.0,1.0));
    let nearby:Vec<_>=opponents.iter().filter(|p|
        point_to_segment_distance_m(p.pos,start,end)<=p.motion_speed_bound_mps()*horizon+p.reach_m+margin).collect();
    let mut threat_max=0.0_f32;
    let mut receive_max=0.0_f32;
    let step=((flight+35)/36).max(75);
    let mut at=step.min(flight);
    loop {
        let ball=traj.position_at_ms(at);
        let xy=ball.xy();
        let receiver_distance=if xy.distance(end)<=2.0 && ball.z<=receiver.playable_height_m {
            observed_residual(receiver,xy,at,margin)
        } else {None};
        if let Some(d)=receiver_distance {
            receive_max=receive_max.max(((receiver.reach_m+margin-d)/(2.0*margin)).clamp(0.0,1.0));
        }
        let receiving=receiver_distance.is_some_and(|d|d<=receiver.reach_m);
        for p in &nearby {
            let height=if aerial{2.05}else{p.playable_height_m};
            if ball.z>height {continue;}
            let Some(residual)=observed_residual(p,xy,at,margin) else {continue;};
            let mut threat=((p.reach_m+margin-residual)/(2.0*margin)).clamp(0.0,1.0);
            if receiving {
                let d=receiver_distance.unwrap();
                threat*=((d+margin-residual)/(2.0*margin)).clamp(0.0,1.0);
            }
            threat_max=threat_max.max(threat);
        }
        if at>=flight||receiving {break;}
        at=at.saturating_add(step).min(flight);
    }
    // A clear lane is not a viable pass if the receiver cannot get there.
    // Keep a single risk value for the existing PI utility; no new success quota.
    threat_max.max(1.0-receive_max)
}

/// Bounded local forecast with a soft reach margin. The authoritative
/// engine still refines contacts with real attributes and the sampled flight.
fn predicted_contact_risk(traj:&BallTrajectory,flight:i64,aerial:bool,
    opponents:&[PassReachCandidate],receiver:&PassReachCandidate,awareness:f32)->f32 {
    let start=traj.position_at_ms(0).xy();
    let end=traj.position_at_ms(flight).xy();
    let horizon=flight as f32/1000.0;
    let margin=0.65+0.70*(1.0-awareness.clamp(0.0,1.0));
    let nearby:Vec<_>=opponents.iter().filter(|p|
        point_to_segment_distance_m(p.pos,start,end)<=p.speed_mps*horizon+p.reach_m+margin).collect();
    let mut risk=0.0f32;
    let step=((flight+35)/36).max(75);
    let mut at=step.min(flight);
    loop {
        let ball=traj.position_at_ms(at);
        let xy=ball.xy();
        let receiver_distance=receiver.position_toward(xy,at).distance(xy);
        let receiving=xy.distance(end)<=2.0 && ball.z<=receiver.playable_height_m
            && receiver_distance<=receiver.reach_m;
        for p in &nearby {
            let height=if aerial{2.05}else{p.playable_height_m};
            if ball.z>height {continue;}
            let residual=p.position_toward(xy,at).distance(xy);
            let mut threat=((p.reach_m+margin-residual)/(2.0*margin)).clamp(0.0,1.0);
            if receiving {
                // A receiver winning the arrival race reduces risk, but
                // never erases an earlier potential interception.
                threat*=((receiver_distance+margin-residual)/(2.0*margin)).clamp(0.0,1.0);
            }
            risk=risk.max(threat);
        }
        if at>=flight||receiving {break;}
        at=at.saturating_add(step).min(flight);
    }
    risk
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runner(id:&str,side:TeamSideV4,pos:Vec2,vel:Vec2)->PassReachCandidate {
        PassReachCandidate{player_id:id.into(),side,pos,velocity:vel,speed_mps:6.5,
            acceleration_mps2:4.0,agility:0.65,reaction_ms:220,reach_m:1.0,playable_height_m:1.2,locomotion:None}
    }
    #[test]
    fn pi_pass_forecast_closing_velocity_changes_risk() {
        let start=Vec2::new(40.0,34.0);
        let end=Vec2::new(66.0,34.0);
        let flight=1500;
        let traj=build_pass_trajectory(PassKindV4::Short,start,end,0,flight);
        let recv=runner("R",TeamSideV4::Home,end,Vec2::default());
        let closing=runner("D",TeamSideV4::Away,Vec2::new(55.0,38.0),Vec2::new(0.0,-5.0));
        let away=runner("D",TeamSideV4::Away,closing.pos,Vec2::new(0.0,5.0));
        let risk=|p:PassReachCandidate|predicted_contact_risk(&traj,flight,false,&[p],&recv,0.8);
        assert!(risk(closing)>risk(away)+0.20,"same static distance, different movement risk");
    }
    #[test]
    fn pi_pass_forecast_open_lane_and_mirror() {
        let start=Vec2::new(40.0,34.0);let end=Vec2::new(60.0,34.0);
        let flight=1150;
        let traj=build_pass_trajectory(PassKindV4::Short,start,end,0,flight);
        let recv=runner("R",TeamSideV4::Home,end,Vec2::default());
        let def=runner("D",TeamSideV4::Away,Vec2::new(50.0,34.0),Vec2::default());
        let risk=predicted_contact_risk(&traj,flight,false,&[def],&recv,0.8);
        assert!(risk>0.5);
        assert_eq!(predicted_contact_risk(&traj,flight,false,&[],&recv,0.8),0.0);
        let mirror=build_pass_trajectory(PassKindV4::Short,Vec2::new(65.0,34.0),Vec2::new(45.0,34.0),0,flight);
        let r=runner("R",TeamSideV4::Away,Vec2::new(45.0,34.0),Vec2::default());
        let d=runner("D",TeamSideV4::Home,Vec2::new(55.0,34.0),Vec2::default());
        assert!((risk-predicted_contact_risk(&mirror,flight,false,&[d],&r,0.8)).abs()<0.001);
    }
    #[test]
    fn pi_pass_forecast_uses_playable_ball_height() {
        let start=Vec2::new(25.0,34.0);let end=Vec2::new(80.0,34.0);
        let flight=3200;
        let ground=build_pass_trajectory(PassKindV4::Short,start,end,0,flight);
        let air=build_pass_trajectory(PassKindV4::Long,start,end,0,flight);
        let recv=runner("R",TeamSideV4::Home,end,Vec2::default());
        let mut def=runner("D",TeamSideV4::Away,Vec2::new(52.5,34.0),Vec2::default());
        def.speed_mps=0.0;
        assert!(predicted_contact_risk(&ground,flight,false,&[def.clone()],&recv,0.8)>0.5);
        assert_eq!(predicted_contact_risk(&air,flight,true,&[def],&recv,0.8),0.0);
    }
}


#[cfg(test)]
mod b12_motion_forecast_tests {
    use super::*;
    use crate::v4::tests::make_v4_input;
    use crate::contracts::{RulesModeV4,SimulationMode};

    fn runner(pos:Vec2,vel:Vec2)->PassReachCandidate {
        PassReachCandidate{player_id:"R".into(),side:TeamSideV4::Home,pos,velocity:vel,
            speed_mps:8.0,acceleration_mps2:4.0,agility:0.65,reaction_ms:220,
            reach_m:1.0,playable_height_m:1.2,locomotion:Some(match_physics::ReachLocomotion{
                capability:match_physics::movement::MovementCapability{
                    max_speed_mps:8.0,acceleration_mps2:4.0,braking_mps2:5.5,lateral_mps2:4.5,body_turn_radps:4.5},
                facing_rad:0.0})}
    }

    #[test]
    fn b12_open_lane_still_requires_a_reachable_receiver() {
        let end=Vec2::new(50.0,34.0);
        let traj=build_pass_trajectory(PassKindV4::Short,Vec2::new(40.0,34.0),end,0,500);
        let ready=runner(end,Vec2::default());
        let escaping=runner(end,Vec2::new(8.0,0.0));
        let a=predicted_motion_risk(&traj,500,false,&[],&ready,0.8);
        let b=predicted_motion_risk(&traj,500,false,&[],&escaping,0.8);
        assert!(a<0.3,"ready receiver: {a}");
        assert!(b>a+0.4,"escaping receiver: {b}, ready: {a}");
    }

    #[test]
    fn b12_observed_forecast_does_not_use_hidden_condition_or_attributes() {
        let input=make_v4_input(12046,SimulationMode::FastBatch,RulesModeV4::Standard);
        let state=MatchStateV4::from_input(&input);
        let mut altered=state.clone();
        for p in &mut altered.players {
            p.fatigue.aerobic=5.0;
            p.fatigue.anaerobic=0.0;
        }
        for profile in &mut altered.player_profiles {
            profile.attrs.physical.pace=1;
            profile.attrs.physical.acceleration=20;
            profile.attrs.physical.agility=1;
            profile.attrs.mental.anticipation=20;
        }
        let a=PassForecast::with_motion(&state,TeamSideV4::Home,0.8,true);
        let b=PassForecast::with_motion(&altered,TeamSideV4::Home,0.8,true);
        let from=Vec2::new(48.0,34.0);
        let before=serde_json::to_string(&state).unwrap();
        for idx in 0..state.players.len() {
            if state.players[idx].side!=TeamSideV4::Home {continue;}
            for kind in [PassKindV4::Short,PassKindV4::Long] {
                let x=a.lane_risk(&state,&input,&input.timing_model,from,idx,kind);
                let y=b.lane_risk(&altered,&input,&input.timing_model,from,idx,kind);
                assert_eq!(x.to_bits(),y.to_bits());
                assert!((0.0..=1.0).contains(&x));
            }
        }
        assert_eq!(serde_json::to_string(&state).unwrap(),before);
    }
}
