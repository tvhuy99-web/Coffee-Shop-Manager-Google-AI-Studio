use super::*;
use crate::v4::input::{SimulationMode, RulesModeV4};
use crate::v4::tests::make_v4_input;

#[test]
fn des_contact_control_loose_ball_scheduled_and_checkpointed() {
    let mut session = MatchSessionV4::new(make_v4_input(16001, SimulationMode::FullEvent, RulesModeV4::Standard)).unwrap();
    session.step_one_des_event(true).unwrap();
    session.scheduler.invalidate_live_play();
    let spot = Vec3::ground(50.0, 34.0);
    for p in &mut session.state.players {
        p.pos = Vec2::new(if p.side == TeamSideV4::Home { 15.0 } else { 90.0 }, 10.0);
        p.vel = Vec2::default();
    }
    let winner = session.state.players.iter().position(|p| p.on_pitch && p.player_id == "H06").unwrap();
    session.state.players[winner].pos = Vec2::new(52.5, 34.0);
    session.state.ball.pos = spot;
    session.state.ball.vel = Vec3::default();
    session.start_grounded_pass_recovery(TeamSideV4::Home, Vec::new()).unwrap();
    let launch = session.state.des_clock.sim_time_ms;
    let resolves = session.pending_flight.as_ref().unwrap().resolves_at_ms;
    assert!(resolves > launch);
    assert_eq!(session.state.ball.control, BallControlState::Loose);
    assert!(session.state.ball.owner_player_id.is_none());
    let checkpoint = session.export_checkpoint().unwrap();
    let restored: MatchCheckpointV4 = serde_json::from_str(&serde_json::to_string(&checkpoint).unwrap()).unwrap();
    let mut restored = MatchSessionV4::from_checkpoint(restored).unwrap();
    // Advance the real DES event(s), not a fake clock or counter.
    for s in [&mut session, &mut restored] {
        for _ in 0..20 {
            if s.pending_flight.is_none() { break; }
            s.step_one_des_event(true).unwrap();
        }
        assert_eq!(s.state.des_clock.sim_time_ms, resolves);
        assert_eq!(s.state.ball.owner_player_id.as_deref(), Some("H06"));
        assert_eq!(s.state.ball.control, BallControlState::Controlled);
        assert!(s.events.iter().any(|e| e.kind == EventTypeV4::SecondBallWon
            && e.sim_time_ms == resolves && e.actor.as_deref() == Some("H06")));
    }
    assert_eq!(session.timeline_fingerprint, restored.timeline_fingerprint);
}
}


#[cfg(test)]
mod moving_recovery_tests {
use super::*;
use crate::v4::input::{SimulationMode, RulesModeV4};
use crate::v4::tests::make_v4_input;
#[test]
fn des_loose_motion_checkpoint_and_touchline_restart() {
    let mut s=MatchSessionV4::new(make_v4_input(16002,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    s.state.ball.pos=Vec3::ground(50.0,67.0);
    s.state.ball.vel=Vec3::new(0.0,12.0,0.0);
    s.state.ball.last_touch_side=Some(TeamSideV4::Away);
    s.start_pass_recovery(TeamSideV4::Home,Vec::new(),"PASS_CONTACT_LOOSE").unwrap();
    let at=s.pending_flight.as_ref().unwrap().resolves_at_ms;
    assert!(at>s.state.des_clock.sim_time_ms && s.state.ball.owner_side.is_none());
    let ck=s.export_checkpoint().unwrap();
    let mut r=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    for session in [&mut s,&mut r] {
        session.step_one_des_event(true).unwrap();
        assert_eq!(session.state.des_clock.sim_time_ms,at);
        let db=session.state.dead_ball.as_ref().unwrap();
        assert_eq!(db.kind,DeadBallKind::ThrowIn);
        assert_eq!(db.restart_side,TeamSideV4::Home);
    }
    assert_eq!(s.timeline_fingerprint,r.timeline_fingerprint);
}
}


#[cfg(test)]
mod pi_timing_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;

#[test]
fn pi_timing_plan_waits_and_survives_checkpoint() {
    let mut s=MatchSessionV4::new(make_v4_input(18001,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    // Isolate the timing contract from a legal early contest.
    for p in &mut s.state.players {
        if p.side!=s.possession_side {p.pos=Vec2::new(95.0,5.0);p.vel=Vec2::default();}
    }
    let now=s.sim_time_ms();
    s.begin_player_decision(false).unwrap();
    let p=s.pending_decision.clone().unwrap();
    assert_eq!(p.executes_at_ms,now+p.plan.decision_delay_ms);
    assert_eq!(p.release_at_ms,p.executes_at_ms);
    assert!(p.challenger.is_none());
    assert!(p.executes_at_ms>now && s.pending_flight.is_none());
    let ck=s.export_checkpoint().unwrap();
    let mut restored=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    for m in [&mut s,&mut restored] {
        m.step_one_des_event(true).unwrap();
        assert_eq!(m.sim_time_ms(),p.executes_at_ms);
        assert!(m.pending_decision.is_none());
    }
    assert_eq!(s.timeline_fingerprint,restored.timeline_fingerprint);
    assert_eq!(s.rng.state(),restored.rng.state());
}

#[test]
fn pi_timing_carry_moves_only_during_future_interval() {
    let mut s=MatchSessionV4::new(make_v4_input(18002,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    let idx=s.carrier_idx;
    for p in &mut s.state.players {
        if p.side!=s.possession_side {p.pos=Vec2::new(95.0,5.0);}
    }
    s.state.players[idx].pos=Vec2::new(40.0,34.0);
    s.state.players[idx].vel=Vec2::default();
    s.state.ball.pos=Vec3::ground(40.0,34.0);
    let before=s.state.players[idx].fatigue.distance_m;
    let launch=s.sim_time_ms();
    s.start_timed_dribble(idx).unwrap();
    let carry=s.pending_carry.clone().unwrap();
    assert_eq!(s.state.players[idx].pos,carry.motion.candidate.pos);
    assert_eq!(s.state.players[idx].fatigue.distance_m,before);
    let duration=carry.resolves_at_ms-launch;
    s.step_one_des_event(true).unwrap();
    assert_eq!(s.sim_time_ms(),carry.resolves_at_ms);
    assert!(s.pending_carry.is_none());
    let moved=s.state.players[idx].pos.distance(carry.motion.candidate.pos);
    assert!(moved>0.0 && moved<=carry.motion.candidate.speed_mps*duration as f32/1000.0+0.01);
    let charged=s.state.players[idx].fatigue.distance_m-before;
    assert!((charged-moved).abs()<0.01,"movement distance charged once");
}

#[test]
fn pi_timing_dead_ball_cancels_pending_intention() {
    let mut s=MatchSessionV4::new(make_v4_input(18003,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    s.begin_player_decision(false).unwrap();
    assert!(s.pending_decision.is_some());
    s.open_dead_ball(DeadBallKind::ThrowIn,TeamSideV4::Away,Vec3::ground(40.0,0.0),false,false).unwrap();
    assert!(s.pending_decision.is_none() && s.pending_carry.is_none());
}
}


#[cfg(test)]
mod pi_carry_lifecycle_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;
#[test]
fn pi_carry_early_contact_and_checkpoint() {
    let mut s=MatchSessionV4::new(make_v4_input(19401,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    let idx=s.carrier_idx;
    let def=s.state.players.iter().position(|p|p.on_pitch && !p.is_goalkeeper && p.side!=s.possession_side).unwrap();
    for p in &mut s.state.players {p.pos=Vec2::new(90.0,60.0);p.vel=Vec2::default();}
    s.state.players[idx].pos=Vec2::new(50.0,34.0);
    s.state.players[def].pos=Vec2::new(50.0,32.5);
    s.state.players[idx].facing_rad=-std::f32::consts::FRAC_PI_2;
    s.state.ball.pos=Vec3::ground(50.0,34.0);
    let now=s.sim_time_ms();
    let dist=s.state.players[idx].fatigue.distance_m;
    s.start_carry_intent(idx,CarryIntentV4{direction:Vec2::new(0.0,-1.0),duration_ms:1600,shielding:false},false).unwrap();
    let carry=s.pending_carry.clone().unwrap();
    assert!(carry.challenger.is_some());
    assert!(carry.resolves_at_ms>now && carry.resolves_at_ms<now+1600);
    assert_eq!(s.state.players[idx].fatigue.distance_m,dist);
    let ck=s.export_checkpoint().unwrap();
    let mut r=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    for session in [&mut s,&mut r] {
        session.step_one_des_event(true).unwrap();
        assert_eq!(session.sim_time_ms(),carry.resolves_at_ms);
        assert!(session.pending_carry.is_none());
    }
    assert_eq!(s.timeline_fingerprint,r.timeline_fingerprint);
    assert_eq!(s.rng.state(),r.rng.state());
}
}


#[cfg(test)]
mod pi_defense_commitment_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;

fn fixture()->(MatchSessionV4,usize,usize) {
    let mut s=MatchSessionV4::new(make_v4_input(19701,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    let car=s.carrier_idx;
    let def=s.state.players.iter().position(|p|p.on_pitch&&!p.is_goalkeeper&&p.side!=s.possession_side).unwrap();
    for p in &mut s.state.players {p.pos=Vec2::new(95.0,5.0);p.vel=Vec2::default();}
    s.state.players[car].pos=Vec2::new(50.0,34.0);
    s.state.players[def].pos=Vec2::new(50.0,34.7);
    s.state.players[car].facing_rad=std::f32::consts::FRAC_PI_2;
    s.state.ball.pos=Vec3::ground(50.0,34.0);
    (s,car,def)
}
#[test]
fn pi_defense_recovery_blocks_repeat_tackle_but_not_future_contact() {
    let (mut s,car,def)=fixture();
    let now=s.sim_time_ms();
    let (at,chase)=s.preparation_contest(car,now+1200);
    assert!(at>now&&at<now+1200&&chase.is_some());
    s.mark_tackle_recovery(def);
    let ready=s.tackle_ready_at[&s.state.players[def].player_id];
    assert!(ready>now);
    assert!(s.preparation_contest(car,ready).1.is_none());
    let (after,chase)=s.preparation_contest(car,ready+1200);
    assert!(after>=ready&&chase.is_some());
    let ck=s.export_checkpoint().unwrap();
    let r=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    assert_eq!(s.tackle_ready_at,r.tackle_ready_at);
}
#[test]
fn pi_defense_reachable_but_protected_ball_is_contained() {
    let (mut s,car,def)=fixture();
    let now=s.sim_time_ms();
    // Same reachable geometry, but approach from behind the carrier.
    s.state.players[car].facing_rad=-std::f32::consts::FRAC_PI_2;
    let mut carrier=pass_reach_candidate(&s.state,car,false);
    carrier.speed_mps=0.0; carrier.reaction_ms=0; carrier.velocity=Vec2::default();
    let motion=PassContactMotion{target:carrier.pos,candidate:carrier,started_at_ms:now};
    let reachable=s.tackle_candidates(s.state.players[car].side,now+1200);
    assert!(find_carry_contact(&motion,&reachable,now+1200).is_some());
    assert!(!crate::player_intelligence::evaluate_tackle(&s.state,car,def,0.25).commits());
    assert!(s.choose_committed_tackle(car,&motion,now+1200).is_none());
}
#[test]
fn pi_defense_preparation_requires_reach_not_proximity() {
    let (mut s,car,def)=fixture();
    let now=s.sim_time_ms();
    s.state.players[def].pos=Vec2::new(50.0,40.0);
    let (at,chase)=s.preparation_contest(car,now+500);
    assert_eq!(at,now+500);
    assert!(chase.is_none(),"nearby pressure alone is not physical contact");
}
#[test]
fn pi_defense_preparation_contact_checkpoint_and_original_release() {
    let (mut s,car,_)=fixture();
    let now=s.sim_time_ms();
    let release=now+1200;
    let (at,chase)=s.preparation_contest(car,release);
    assert!(at<release&&chase.is_some());
    let side=s.state.players[car].side;
    let id=s.state.players[car].player_id.clone();
    let ctx=evaluate_local_context(&s.state,&s.input,side,s.state.players[car].pos);
    let plan=perceive_evaluate_and_decide_des(&s.state,&s.input,&s.input.timing_model,car,ctx,&mut s.rng);
    s.pending_decision=Some(PendingDecisionV4{player_id:id.clone(),side,started_at_ms:now,
        executes_at_ms:at,plan,release_at_ms:release,challenger:chase,observed_pass_risk:None,final_aim_committed:false});
    s.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
        DesEventPayload::ActionExecute{player_id:id.clone(),side}).unwrap();
    let ck=s.export_checkpoint().unwrap();
    let mut r=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    for session in [&mut s,&mut r] {
        session.step_one_des_event(true).unwrap();
        assert_eq!(session.sim_time_ms(),at);
        if session.state.dead_ball.is_none() && session.state.players[session.carrier_idx].player_id==id {
            let pending=session.pending_decision.as_ref().unwrap();
            assert_eq!(pending.executes_at_ms,release);
            assert!(pending.challenger.is_none());
            assert_eq!(pending.plan,plan);
        }
    }
    assert_eq!(s.timeline_fingerprint,r.timeline_fingerprint);
    assert_eq!(s.rng.state(),r.rng.state());
    assert_eq!(s.tackle_ready_at,r.tackle_ready_at);
}
}


#[cfg(test)]
mod pi_clock_contract_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;

#[test]
fn pi_clock_stale_and_between_event_polls_do_not_advance() {
    let mut s=MatchSessionV4::new(make_v4_input(19901,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.advance_until_ms(60_000,1000).unwrap();
    // Process any same-time scheduled events before selecting a
    // strictly interior polling horizon.
    while s.scheduler.peek_next_valid_time_ms()==Some(s.sim_time_ms()) {
        s.step_one_des_event(false).unwrap();
    }
    let now=s.sim_time_ms();
    let next=s.scheduler.peek_next_valid_time_ms().unwrap();
    assert!(now>0 && next>now);
    let before=s.export_checkpoint().unwrap();
    for target in [now-1,now+(next-now)/2] {
        // Equal timestamps may legally dispatch due events; here none
        // remains due at now.
        let step=s.advance_until_ms(target,1000).unwrap();
        assert_eq!(step.steps_executed,0);
        assert_eq!(step.sim_time_ms,now);
        assert_eq!(step.ball_in_play_ms+step.ball_out_of_play_ms,now);
    }
    let after=s.export_checkpoint().unwrap();
    assert_eq!(before.payload.rng,after.payload.rng);
    assert_eq!(before.payload.stats.timeline_fingerprint,after.payload.stats.timeline_fingerprint);
    assert_eq!(before.payload.scheduler.total_popped(),after.payload.scheduler.total_popped());
    assert_eq!(before.payload.state.des_clock.sim_time_ms,after.payload.state.des_clock.sim_time_ms);
}

#[test]
fn pi_clock_split_committed_carry_accounts_distance_and_time_once() {
    let mut s=MatchSessionV4::new(make_v4_input(18002,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    let idx=s.carrier_idx;
    for p in &mut s.state.players {
        if p.side!=s.possession_side {p.pos=Vec2::new(95.0,5.0);p.vel=Vec2::default();}
    }
    s.state.players[idx].pos=Vec2::new(40.0,34.0);
    s.state.players[idx].vel=Vec2::default();
    s.state.ball.pos=Vec3::ground(40.0,34.0);
    s.start_timed_dribble(idx).unwrap();
    let carry=s.pending_carry.clone().unwrap();
    assert!(carry.challenger.is_none());
    let start=s.sim_time_ms();
    let end=carry.resolves_at_ms;
    let mid=start+(end-start)/2;
    let before=s.state.players[idx].fatigue.distance_m;
    let live_before=s.state.des_clock.ball_in_play_ms;
    let mut split=MatchSessionV4::from_checkpoint(s.export_checkpoint().unwrap()).unwrap();
    split.schedule_test_clock_probe(mid).unwrap();
    split.step_one_des_event(false).unwrap();
    assert_eq!(split.sim_time_ms(),mid);
    assert!(split.pending_carry.is_some());
    assert!(split.state.players[idx].pos.distance(carry.motion.position_at(mid))<0.002);
    let mid_distance=carry.motion.candidate.pos.distance(carry.motion.position_at(mid));
    assert!((split.state.players[idx].fatigue.distance_m-before-mid_distance).abs()<0.01);
    for session in [&mut s,&mut split] {
        session.step_one_des_event(false).unwrap();
        assert_eq!(session.sim_time_ms(),end);
        assert!(session.pending_carry.is_none());
        assert_eq!(session.state.des_clock.ball_in_play_ms-live_before,end-start);
        assert!(session.state.players[idx].pos.distance(carry.motion.position_at(end))<0.002);
        let traveled=carry.motion.candidate.pos.distance(carry.motion.position_at(end));
        assert!((session.state.players[idx].fatigue.distance_m-before-traveled).abs()<0.01);
    }
    assert!((s.state.players[idx].fatigue.distance_m-split.state.players[idx].fatigue.distance_m).abs()<0.01);
}
}


#[cfg(test)]
mod pi_personal_feedback_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;
use crate::player_intelligence::decision_uncertainty;
use crate::v4::psychology::{update_in_match_psychology,PERSONAL_CONFIDENCE_BOUND};

#[test]
fn pi_psychology_personal_outcome_actor_and_mode_parity() {
    let mut full=MatchSessionV4::new(make_v4_input(22801,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    let mut fast=MatchSessionV4::new(make_v4_input(22801,SimulationMode::FastBatch,RulesModeV4::Standard)).unwrap();
    let a=full.state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let b=full.state.players.iter().position(|p|p.side==TeamSideV4::Away&&!p.is_goalkeeper).unwrap();
    let mate=full.state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper&&p.player_id!=full.state.players[a].player_id).unwrap();
    let aid=full.state.players[a].player_id.clone();
    let bid=full.state.players[b].player_id.clone();
    for sess in [&mut full,&mut fast] {
        let rng_before=sess.rng.state();
        let before_a=decision_uncertainty(&sess.state,a,0.4);
        let before_b=decision_uncertainty(&sess.state,b,0.4);
        // These are fixture emissions, not generated match evidence.
        // A real clean tackle emits these two events, one per actor.
        sess.emit(EventTypeV4::DribbleLost,HighlightTier::None,None,Some(TeamSideV4::Home),Some(aid.clone()),Some(bid.clone()),
            EventMetadataV4{outcome:Some("TACKLED".into()),..EventMetadataV4::default()});
        sess.emit(EventTypeV4::TackleWon,HighlightTier::None,None,Some(TeamSideV4::Away),Some(bid.clone()),Some(aid.clone()),EventMetadataV4::default());
        assert!((sess.state.players[a].psychology.personal_confidence_delta+0.024).abs()<1e-6);
        assert!((sess.state.players[b].psychology.personal_confidence_delta-0.024).abs()<1e-6);
        assert_eq!(sess.state.players[mate].psychology.personal_confidence_delta,0.0);
        assert!(decision_uncertainty(&sess.state,a,0.4)>before_a);
        assert!(decision_uncertainty(&sess.state,b,0.4)<before_b);
        let prior=sess.state.players[a].psychology;
        for kind in [EventTypeV4::Shot,EventTypeV4::ShotOnTarget,EventTypeV4::PassFailed,EventTypeV4::LooseBall,EventTypeV4::Foul,EventTypeV4::OwnGoal,EventTypeV4::Carry] {
            sess.emit(kind,HighlightTier::None,None,Some(TeamSideV4::Home),Some(aid.clone()),None,EventMetadataV4::default());
        }
        // Actor/side mismatch must not influence a different team.
        sess.emit(EventTypeV4::Goal,HighlightTier::None,None,Some(TeamSideV4::Away),Some(aid.clone()),None,EventMetadataV4::default());
        assert_eq!(sess.state.players[a].psychology,prior);
        assert_eq!(sess.rng.state(),rng_before);
        assert_eq!(sess.state.home.momentum,0.0);
        assert_eq!(sess.state.home.goals,0);
    }
    assert_eq!(full.timeline_fingerprint,fast.timeline_fingerprint);
    assert_eq!(full.state.players[a].psychology,fast.state.players[a].psychology);
    assert_eq!(full.state.players[b].psychology,fast.state.players[b].psychology);
    assert!(!full.events.is_empty());
    assert!(fast.events.is_empty());
    let ck=full.export_checkpoint().unwrap();
    let restored=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
    assert_eq!(ck.checksum_hex,restored.export_checkpoint().unwrap().checksum_hex);
}

#[test]
fn pi_psychology_personal_feedback_decay_and_bounds() {
    let input=make_v4_input(22802,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut s=MatchSessionV4::new(input.clone()).unwrap();
    let idx=s.state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let id=s.state.players[idx].player_id.clone();
    for _ in 0..100 {
        s.emit(EventTypeV4::Goal,HighlightTier::None,None,Some(TeamSideV4::Home),Some(id.clone()),None,EventMetadataV4::default());
    }
    let initial=s.state.players[idx].psychology.personal_confidence_delta;
    assert!(initial>0.0&&initial<=PERSONAL_CONFIDENCE_BOUND);
    let mut split=s.state.clone();
    update_in_match_psychology(&mut s.state,&input,1200,120.0);
    for k in 1..=120 {update_in_match_psychology(&mut split,&input,1080+k,1.0);}
    let recovered=s.state.players[idx].psychology.personal_confidence_delta;
    assert!(recovered>0.0&&recovered<initial);
    assert!((recovered-split.players[idx].psychology.personal_confidence_delta).abs()<1e-6);
    for _ in 0..100 {
        s.emit(EventTypeV4::DribbleLost,HighlightTier::None,None,Some(TeamSideV4::Home),Some(id.clone()),None,
            EventMetadataV4{outcome:Some("TACKLED".into()),..EventMetadataV4::default()});
    }
    let negative=s.state.players[idx].psychology.personal_confidence_delta;
    assert!(negative<0.0&&negative>=-PERSONAL_CONFIDENCE_BOUND);
    update_in_match_psychology(&mut s.state,&input,1320,120.0);
    let recovering=s.state.players[idx].psychology.personal_confidence_delta;
    assert!(negative<recovering&&recovering<0.0);
    s.state.des_clock.begin_period(Period::Shootout,0);
    s.emit(EventTypeV4::Goal,HighlightTier::None,None,Some(TeamSideV4::Home),Some(id),None,EventMetadataV4::default());
    assert_eq!(recovering,s.state.players[idx].psychology.personal_confidence_delta);
    println!("PI_PSYCHOLOGY personal_feedback initial={initial} recovered={recovered} negative={negative} recovering={recovering}");
}
}


#[cfg(test)]
mod pi_tackle_loose_tests {
use super::*;
use crate::v4::input::{SimulationMode,RulesModeV4};
use crate::v4::tests::make_v4_input;
#[test]
fn pi_defense_tackle_loose_recovery_checkpoint_and_third_player() {
    let mut s=MatchSessionV4::new(make_v4_input(23102,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap();
    s.step_one_des_event(true).unwrap();
    s.scheduler.invalidate_live_play();
    let side=s.possession_side;
    let a=s.carrier_idx;
    let b=s.state.players.iter().position(|p|p.side!=side&&!p.is_goalkeeper).unwrap();
    let third=s.state.players.iter().enumerate().position(|(i,p)|i!=a&&i!=b&&p.side==side&&!p.is_goalkeeper).unwrap();
    for p in &mut s.state.players {p.pos=Vec2::new(95.0,60.0);p.vel=Vec2::default();}
    s.state.players[a].pos=Vec2::new(50.0,34.0);
    s.state.players[b].pos=Vec2::new(51.0,34.0);
    s.state.players[a].facing_rad=0.0;
    s.state.players[a].vel=Vec2::new(4.0,0.0);
    let ctx=evaluate_local_context(&s.state,&s.input,side,s.state.players[a].pos);
    let initial=s.state.clone();
    let mut loose=None;
    for seed in 1..=512 {
        s.state=initial.clone();
        s.rng=DeterministicRng::new(seed*7907);
        let out=resolve_carry_contact(&mut s.state,&s.input,a,Some(b),ctx,false,&mut s.rng);
        if s.state.ball.control==BallControlState::Loose {loose=Some(out);break;}
    }
    s.flush_play_events(loose.expect("legal uncontrolled touch").events,side);
    // Place an uninvolved teammate at the projected recovery spot;
    // both original duel participants are too far to recover first.
    s.state.players[a].pos=Vec2::new(20.0,10.0);
    s.state.players[b].pos=Vec2::new(80.0,10.0);
    s.state.players[third].pos=s.state.ball.pos.xy();
    let now=s.sim_time_ms();
    s.start_pass_recovery(side,Vec::new(),"TACKLE_CONTACT_LOOSE").unwrap();
    let at=s.pending_flight.as_ref().unwrap().resolves_at_ms;
    assert!(at>now);
    assert!(s.state.ball.owner_side.is_none());
    assert_eq!(s.pending_flight.as_ref().unwrap().outcome.next_carrier_idx,third);
    let mut r=MatchSessionV4::from_checkpoint(serde_json::from_str(&serde_json::to_string(&s.export_checkpoint().unwrap()).unwrap()).unwrap()).unwrap();
    for x in [&mut s,&mut r] {
        x.step_one_des_event(true).unwrap();
        assert_eq!(x.sim_time_ms(),at);
        assert_eq!(x.carrier_idx,third);
        assert_eq!(x.state.ball.control,BallControlState::Controlled);
        assert_eq!(x.state.ball.owner_player_id.as_deref(),Some(x.state.players[third].player_id.as_str()));
    }
    assert_eq!(s.timeline_fingerprint,r.timeline_fingerprint);
    assert_eq!(s.rng.state(),r.rng.state());
}
