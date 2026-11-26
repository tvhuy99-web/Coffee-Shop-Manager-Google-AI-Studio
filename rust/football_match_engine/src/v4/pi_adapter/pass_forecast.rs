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
}

impl PassForecast {
    pub(crate) fn new(state:&MatchStateV4,side:TeamSideV4,awareness:f32)->Self {
        let opponents=state.players.iter().filter(|p|p.on_pitch&&!p.sent_off&&!p.is_goalkeeper&&p.side!=side)
            .map(|p|{
                let observed_speed=(p.vel.x*p.vel.x+p.vel.y*p.vel.y).sqrt();
                // Neutral motion priors; actual opponent attributes belong
                // to physical execution, not the passer's perception.
                PassReachCandidate{player_id:p.player_id.clone(),side:p.side,pos:p.pos,velocity:p.vel,
                    speed_mps:6.5f32.max(observed_speed).min(9.0),acceleration_mps2:4.0,
                    agility:0.65,reaction_ms:220,reach_m:1.0,playable_height_m:1.2,locomotion:None}
            }).collect();
        Self{opponents,awareness:awareness.clamp(0.0,1.0)}
    }

    pub(crate) fn lane_risk(&self,state:&MatchStateV4,input:&MatchInputV4,timing:&TimingModelConfig,
        from:Vec2,target_idx:usize,kind:PassKindV4)->f32 {
        let to=state.players[target_idx].pos;
        let flight=nominal_ball_flight_ms(timing,from.distance(to),BallFlightKind::from(kind),
            input.match_environment.weather,input.match_environment.pitch_quality);
        let traj=build_pass_trajectory(kind,from,to,0,flight);
        let receiver=pass_reach_candidate(state,target_idx,kind.is_aerial());
        predicted_contact_risk(&traj,flight,kind.is_aerial(),&self.opponents,&receiver,self.awareness)
    }
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
