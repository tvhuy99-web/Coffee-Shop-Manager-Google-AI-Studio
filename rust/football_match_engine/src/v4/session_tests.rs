#[cfg(test)]
mod contact_control_lifecycle_tests {
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
            executes_at_ms:at,plan,release_at_ms:release,challenger:chase,observed_pass_risk:None,final_aim_committed:false,preparation_motion:None});
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
            // Deliberately explore RNG cursor values, not a different match identity.
            // Same stream state as the old fixture; retain the immutable root seed.
            s.rng=DeterministicRng::from_seed_and_state(s.input.seed,seed*7907);
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
}


#[cfg(test)]
mod a4_atomic_api_tests {
    use super::*;
    use crate::v4::input::{SimulationMode,RulesModeV4};
    use crate::v4::tests::make_v4_input;

    fn new_session()->MatchSessionV4{
        MatchSessionV4::new(make_v4_input(4242,SimulationMode::FullEvent,RulesModeV4::Standard)).unwrap()
    }
    fn snapshot(s:&MatchSessionV4)->String{
        serde_json::to_string(&s.export_checkpoint().unwrap()).unwrap()
    }
    #[test]
    fn a4_rejected_command_does_not_consume_id_or_mutate_any_state(){
        let mut s=new_session();
        let before=snapshot(&s);
        let id=s.next_command_id;
        let err=s.submit_command(MatchCommandV4::Substitute{side:TeamSideV4::Home,
            player_out_id:"missing-player".into(),player_in_id:"missing-bench".into()});
        assert!(err.is_err());
        assert_eq!(snapshot(&s),before);
        assert_eq!(s.next_command_id,id);
        s.submit_command(MatchCommandV4::StartNextPeriod).unwrap();
        assert_eq!(s.command_timeline.last().unwrap().command_id,id);
    }
    #[test]
    fn a4_failed_advance_rolls_back_queue_rng_ledger_and_event_cursor(){
        let mut s=new_session();
        s.start_next_period().unwrap();
        // Force a real scheduler failure after a number of successful
        // dispatcher calls: 65 same-time events exceed the watchdog.
        let at=s.sim_time_ms()+1;
        s.scheduler=EventScheduler::new();
        s.scheduler.advance_cursor_to(at).unwrap();
        for _ in 0..65 {
            s.scheduler.schedule(at,EventPriority::TacticalAndCommand,CancellationScope::Persistent,
                DesEventPayload::TacticalCommandEffective{command_index:u32::MAX,side:TeamSideV4::Home}).unwrap();
        }
        let before=snapshot(&s);
        let beats=s.completed_beats;
        assert!(s.advance_steps(100).is_err());
        assert_eq!(s.completed_beats,beats);
        assert_eq!(snapshot(&s),before);
    }
    #[test]
    fn a4_recomputed_checksum_cannot_hide_invalid_checkpoint_references(){
        let s=new_session();
        let original=s.export_checkpoint().unwrap();
        for mode in 0..5 {
            let mut bad=original.clone();
            match mode {
                0=>bad.payload.stats.carrier_idx=usize::MAX,
                1=>bad.payload.state.players[0].profile_idx=usize::MAX,
                2=>bad.payload.rng.root_seed=bad.payload.input.seed.wrapping_add(1),
                3=>bad.sim_time_ms=bad.sim_time_ms+1,
                _=>bad.payload.next_event_sequence=bad.payload.stats.event_count+1,
            }
            bad.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&bad.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(bad).is_err(),"corruption mode {mode}");
        }
        assert_eq!(snapshot(&MatchSessionV4::from_checkpoint(original).unwrap()),snapshot(&s));
    }
    #[test]
    fn a4_flight_delta_with_invalid_or_duplicate_id_changes_no_counters(){
        let mut s=new_session();
        let baseline=serde_json::to_string(&s.state).unwrap();
        let mut delta=FlightCounterDeltaV4::between(&s.state,&s.state);
        delta.home.goals=1;
        let valid=s.state.players[0].player_id.clone();
        let row=crate::v4::checkpoint::PlayerFlightDeltaV4{player_id:valid,goals:1,own_goals:0,assists:0,rating_delta:0};
        delta.players.push(row.clone());
        delta.players.push(crate::v4::checkpoint::PlayerFlightDeltaV4{player_id:"unknown".into(),..row.clone()});
        assert!(delta.apply(&mut s.state).is_err());
        assert_eq!(serde_json::to_string(&s.state).unwrap(),baseline);
        delta.players[1]=row;
        assert!(delta.apply(&mut s.state).is_err());
        assert_eq!(serde_json::to_string(&s.state).unwrap(),baseline);
    }

    #[test]
    fn a4_invalid_candidate_commit_is_rejected_without_side_effects(){
        let mut s=new_session();
        let before=snapshot(&s);
        let result=s.transact(|candidate|{
            candidate.state.players[0].pos.x=f32::NAN;
            candidate.rng.next_double();
            candidate.next_command_id+=1;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(snapshot(&s),before);
    }
    #[test]
    fn a4_checkpoint_rejects_duplicate_identity_and_invalid_condition(){
        let s=new_session();
        for mode in 0..2{
            let mut ck=s.export_checkpoint().unwrap();
            if mode==0 {ck.payload.state.players[1].player_id=ck.payload.state.players[0].player_id.clone();}
            else {ck.payload.state.players[0].fatigue.aerobic=101.0;}
            ck.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&ck.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(ck).is_err());
        }
    }

    #[test]
    fn a4_checkpoint_rejects_invalid_accounting_journal_and_terminal_state(){
        let mut s=new_session();
        s.submit_command(MatchCommandV4::StartNextPeriod).unwrap();
        s.advance_steps(20).unwrap();
        let original=s.export_checkpoint().unwrap();
        assert!(MatchSessionV4::from_checkpoint(original.clone()).is_ok());
        for mode in 0..6 {
            let mut bad=original.clone();
            match mode {
                0=>bad.payload.state.des_clock.ball_out_of_play_ms+=1,
                1=>bad.payload.stats.home_possession_ms+=1,
                2=>bad.payload.applied_commands.push(bad.payload.applied_commands[0].clone()),
                3=>bad.payload.applied_commands[0].actual_effective_sim_time_ms=Some(bad.sim_time_ms+1),
                4=>bad.payload.stats.retained_events[0].sequence=2,
                _=>{bad.payload.stats.finished=true;bad.payload.stats.retained_events.clear();}
            }
            bad.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&bad.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(bad).is_err(),"corruption mode {mode}");
        }
    }

    #[test]
    fn a4_reject_invalid_commit_accounting_without_publishing(){
        let mut s=new_session();
        s.start_next_period().unwrap();
        let before=snapshot(&s);
        let result=s.transact(|candidate|{
            candidate.home_possession_ms+=1;
            candidate.rng.next_double();
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(snapshot(&s),before);
        let result=s.transact(|candidate|{
            candidate.finished=true;
            candidate.events.clear();
            Ok(())
        });
        assert!(result.is_err(),"premature finalization must not pass");
        assert_eq!(snapshot(&s),before);
    }

    #[test]
    fn b11_checkpoint_rejects_period_and_scheduler_corruption() {
        let mut s = new_session();
        s.start_next_period().unwrap();
        s.advance_steps(20).unwrap();
        let original = s.export_checkpoint().unwrap();
        assert!(MatchSessionV4::from_checkpoint(original.clone()).is_ok());
        for mode in 0..5 {
            let mut bad = original.clone();
            match mode {
                0 => bad.payload.state.des_clock.period_started_at_ms += 1,
                1 => bad.payload.state.des_clock.period_elapsed_ms += 1,
                2 => bad.payload.state.des_clock.announced_added_ms = -1,
                3 => bad.payload.state.des_clock.additional_added_ms = i64::MAX,
                _ => bad.payload.scheduler.advance_cursor_to(bad.sim_time_ms + 1).unwrap(),
            }
            bad.checksum_hex = compute_checkpoint_checksum(&serde_json::to_string(&bad.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(bad).is_err(), "corruption mode {mode}");
        }
    }

    #[test]
    fn b11_rejected_period_commit_preserves_session_and_rng() {
        let mut s = new_session();
        s.start_next_period().unwrap();
        let before = snapshot(&s);
        let result = s.transact(|candidate| {
            candidate.state.des_clock.period_elapsed_ms += 1;
            candidate.rng.next_double();
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(snapshot(&s), before);
        let result = s.transact(|candidate| {
            let invalid_cursor = candidate.sim_time_ms() + 1;
            candidate.scheduler.advance_cursor_to(invalid_cursor)
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(snapshot(&s), before);
    }

    #[test]
    fn b11_read_only_invariants_detect_invalid_world_without_repair() {
        use crate::v4::transition::verify_authoritative_invariants;
        let mut s = new_session();
        s.start_next_period().unwrap();
        assert!(verify_authoritative_invariants(&s.state).is_ok());
        s.state.players[0].fatigue.aerobic = 100.25;
        s.state.players[0].fatigue.distance_m = -1.0;
        s.state.ball.control = BallControlState::Loose;
        assert!(s.state.ball.owner_player_id.is_some());
        let before = serde_json::to_string(&s.state).unwrap();
        let errors = verify_authoritative_invariants(&s.state).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "INV-01"));
        assert!(errors.iter().any(|e| e.code == "INV-07"));
        assert_eq!(serde_json::to_string(&s.state).unwrap(), before);
        s.state.players[0].facing_rad = f32::NAN;
        let errors = verify_authoritative_invariants(&s.state).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "INV-02"));
    }

    #[test]
    fn a4_result_closes_once_and_rejects_post_match_commands(){
        let mut s=new_session();
        let result=s.run_to_full_time().unwrap();
        assert!(s.is_full_time());
        assert_eq!(result.events.iter().filter(|e|e.kind==EventTypeV4::FullTime).count(),1);
        let before=snapshot(&s);
        assert!(s.run_to_full_time().is_err());
        assert!(s.submit_command(MatchCommandV4::StartNextPeriod).is_err());
        assert_eq!(snapshot(&s),before);
        assert!(MatchSessionV4::from_checkpoint(s.export_checkpoint().unwrap()).is_ok());
    }

    #[test]
    fn a4_goal_audit_detects_missing_duplicate_and_wrong_beneficiary(){
        use crate::v4::transition::{GoalDeltaAudit,goal_counters};
        let mut s=new_session();
        let home=s.state.players.iter().find(|p|p.side==TeamSideV4::Home).unwrap().player_id.clone();
        let away=s.state.players.iter().find(|p|p.side==TeamSideV4::Away).unwrap().player_id.clone();
        let before=goal_counters(&s.state);
        s.state.home.goals+=1;
        assert!(GoalDeltaAudit::default().verify(before,&s.state).is_err());
        let mut audit=GoalDeltaAudit::default();
        audit.record(&s.state,EventTypeV4::Goal,Some(TeamSideV4::Home),Some(&home));
        assert!(audit.verify(before,&s.state).is_ok());
        audit.record(&s.state,EventTypeV4::Goal,Some(TeamSideV4::Home),Some(&home));
        assert!(audit.verify(before,&s.state).is_err());
        s.state.home.own_goals_for+=1;
        let mut own=GoalDeltaAudit::default();
        own.record(&s.state,EventTypeV4::OwnGoal,Some(TeamSideV4::Home),Some(&away));
        assert!(own.verify(before,&s.state).is_ok());
        let mut wrong=GoalDeltaAudit::default();
        wrong.record(&s.state,EventTypeV4::OwnGoal,Some(TeamSideV4::Home),Some(&home));
        assert!(wrong.verify(before,&s.state).is_err());
        let score=goal_counters(&s.state);
        let mut shootout=GoalDeltaAudit::default();
        shootout.record(&s.state,EventTypeV4::PenaltyShootoutKick,Some(TeamSideV4::Home),Some(&home));
        assert!(shootout.verify(score,&s.state).is_ok());
    }
    #[test]
    fn a4_uncontrolled_ball_cannot_have_owner_at_commit_or_restore(){
        let mut s=new_session();
        let before=snapshot(&s);
        let owner=s.state.players[0].player_id.clone();
        let side=s.state.players[0].side;
        for control in [BallControlState::Loose,BallControlState::InFlight]{
            assert!(s.transact(|c|{
                c.state.ball.control=control.clone();
                c.state.ball.owner_side=Some(side);
                c.state.ball.owner_player_id=Some(owner.clone());
                Ok(())
            }).is_err());
            assert_eq!(snapshot(&s),before);
            let mut ck=s.export_checkpoint().unwrap();
            ck.payload.state.ball.control=control;
            ck.payload.state.ball.owner_side=Some(side);
            ck.payload.state.ball.owner_player_id=Some(owner.clone());
            ck.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&ck.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(ck).is_err());
        }
    }

    #[test]
    fn a4_base_profile_and_roster_identity_are_immutable(){
        let mut s=new_session();
        let before=snapshot(&s);
        let idx=s.state.players[0].profile_idx;
        assert!(s.transact(|c|{
            c.state.player_profiles[idx].weak_foot=0;
            Ok(())
        }).is_err());
        assert_eq!(snapshot(&s),before);
        for mode in 0..2{
            let mut ck=s.export_checkpoint().unwrap();
            if mode==0 {ck.payload.state.player_profiles[idx].weak_foot=0;}
            else {ck.payload.state.players[0].player_id="unknown-player".into();}
            ck.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&ck.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(ck).is_err());
        }
    }

    #[test]
    fn a4_action_actor_guard_rejects_sent_off_and_substituted_players(){
        let mut s=new_session();
        use crate::v4::transition::verify_action_actor;
        assert!(verify_action_actor(&s.state,0).is_ok());
        s.state.players[0].sent_off=true;
        assert!(verify_action_actor(&s.state,0).is_err());
        s.state.players[0].sent_off=false;
        s.state.players[0].on_pitch=false;
        assert!(verify_action_actor(&s.state,0).is_err());
        assert!(verify_action_actor(&s.state,usize::MAX).is_err());
    }

    #[test]
    fn a4_movement_budget_rejects_teleport_and_inactive_displacement(){
        use crate::v4::transition::verify_motion_budget;
        let p=Vec2::new(50.0,34.0);
        assert!(verify_motion_budget(p,Vec2::new(50.008,34.0),0.008,"p").is_ok());
        assert!(verify_motion_budget(p,Vec2::new(51.0,34.0),0.008,"p").is_err());
        assert!(verify_motion_budget(p,p,0.0,"inactive").is_ok());
        assert!(verify_motion_budget(p,Vec2::new(50.01,34.0),0.0,"inactive").is_err());
        assert!(verify_motion_budget(p,Vec2::new(f32::NAN,34.0),1.0,"p").is_err());
    }
    #[test]
    fn a4_controlled_actor_checks_identity_side_and_eligibility(){
        use crate::v4::transition::verify_controlled_actor;
        let mut s=new_session();
        let p=&s.state.players[0];
        s.state.ball.control=BallControlState::Controlled;
        s.state.ball.owner_side=Some(p.side);
        s.state.ball.owner_player_id=Some(p.player_id.clone());
        assert!(verify_controlled_actor(&s.state,0).is_ok());
        s.state.ball.owner_side=Some(p.side.opposite());
        assert!(verify_controlled_actor(&s.state,0).is_err());
        s.state.ball.owner_side=Some(p.side);
        s.state.players[0].sent_off=true;
        assert!(verify_controlled_actor(&s.state,0).is_err());
    }

    /// Host benchmark with the actual transactional public API. Report only,
    /// no arbitrary wall-clock threshold on shared runners. ARM64 device
    /// latency and RSS must be measured separately.
    #[test]
    #[ignore = "explicit A4 release performance report"]
    fn a4_transaction_performance_report(){
        use std::time::Instant;
        let mut rows=Vec::new();
        fn percentile(sorted:&[f64],p:f64)->f64 {
            sorted[((sorted.len() as f64*p).ceil() as usize).saturating_sub(1).min(sorted.len()-1)]
        }
        for seed in [4242,4243,4244] {
            let input=make_v4_input(seed,SimulationMode::FullEvent,RulesModeV4::Standard);
            let mut reference=MatchSessionV4::new(input.clone()).unwrap();
            let start=Instant::now();
            let expected=reference.run_to_full_time().unwrap();
            rows.push(serde_json::json!({"seed":seed,"mode":"whole_match","elapsedMs":start.elapsed().as_secs_f64()*1000.0,
                "events":expected.event_count,"fingerprint":expected.event_timeline_fingerprint}));
            let mut fast_input=input.clone();
            fast_input.simulation_mode=SimulationMode::FastBatch;
            let mut fast=MatchSessionV4::new(fast_input).unwrap();
            let start=Instant::now();
            let fast_result=fast.run_to_full_time().unwrap();
            let fast_ms=start.elapsed().as_secs_f64()*1000.0;
            assert_eq!(fast_result.event_timeline_fingerprint,expected.event_timeline_fingerprint);
            assert_eq!(fast_result.event_count,expected.event_count);
            assert_eq!(serde_json::to_string(&fast_result.summary).unwrap(),serde_json::to_string(&expected.summary).unwrap());
            rows.push(serde_json::json!({"seed":seed,"mode":"fast_batch","elapsedMs":fast_ms,"matchesPerSecond":1000.0/fast_ms,
                "events":fast_result.event_count,"fingerprint":fast_result.event_timeline_fingerprint}));
            let mut restore_source=MatchSessionV4::new(input.clone()).unwrap();
            restore_source.start_next_period().unwrap();
            restore_source.advance_steps(500).unwrap();
            let start=Instant::now();
            let checkpoint=restore_source.export_checkpoint().unwrap();
            let bytes=serde_json::to_vec(&checkpoint).unwrap();
            let save_ms=start.elapsed().as_secs_f64()*1000.0;
            let start=Instant::now();
            let decoded=serde_json::from_slice(&bytes).unwrap();
            let mut restored=MatchSessionV4::from_checkpoint(decoded).unwrap();
            let restore_ms=start.elapsed().as_secs_f64()*1000.0;
            assert_eq!(snapshot(&restore_source),snapshot(&restored));
            assert_eq!(restored.run_to_full_time().unwrap().event_timeline_fingerprint,expected.event_timeline_fingerprint);
            rows.push(serde_json::json!({"seed":seed,"mode":"checkpoint","bytes":bytes.len(),"serializeMs":save_ms,"restoreMs":restore_ms}));
            for batch in [1usize,32,500] {
                let mut s=MatchSessionV4::new(input.clone()).unwrap();
                let start=Instant::now();
                let mut calls=0u64;
                let mut max_call_ms=0.0f64;
                let mut latency=Vec::new();
                let mut delivered=0usize;
                while !s.is_full_time(){
                    let call_start=Instant::now();
                    let step=if s.stage.is_break() {s.start_next_period().unwrap()} else {s.advance_steps(batch).unwrap()};
                    let call_ms=call_start.elapsed().as_secs_f64()*1000.0;
                    max_call_ms=max_call_ms.max(call_ms);
                    latency.push(call_ms);
                    calls+=1;
                    delivered+=step.new_events.len();
                    assert!(calls<=100_000,"interactive benchmark watchdog");
                }
                let actual=s.run_to_full_time().unwrap();
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                assert_eq!(actual.event_timeline_fingerprint,expected.event_timeline_fingerprint);
                assert_eq!(actual.event_count,expected.event_count);
                assert_eq!(delivered,actual.event_count as usize);
                assert_eq!(serde_json::to_string(&actual.summary).unwrap(),serde_json::to_string(&expected.summary).unwrap());
                latency.sort_by(f64::total_cmp);
                rows.push(serde_json::json!({"seed":seed,"mode":"interactive","batch":batch,"calls":calls,
                    "p50CallMs":percentile(&latency,0.50),"p95CallMs":percentile(&latency,0.95),"p99CallMs":percentile(&latency,0.99),"eventsPerSecond":actual.event_count as f64*1000.0/elapsed,
                    "elapsedMs":elapsed,"maxCallMs":max_call_ms,"events":actual.event_count,
                    "fingerprint":actual.event_timeline_fingerprint}));
            }
        }
        let report=serde_json::json!({"schema":2,"scope":"host release build, three seeds, FastBatch/FullEvent/checkpoint; not Android device performance","measurements":rows});
        let json=serde_json::to_string_pretty(&report).unwrap();
        println!("{json}");
        if let Ok(path)=std::env::var("A4_PERF_REPORT_PATH"){std::fs::write(path,json).unwrap();}
    }

    #[test]
    fn a4_single_des_step_is_atomic_on_scheduler_failure(){
        let mut s=new_session();
        s.start_next_period().unwrap();
        let now=s.sim_time_ms();
        s.scheduler=EventScheduler::new();
        s.scheduler.advance_cursor_to(now).unwrap();
        for _ in 0..65 {
            s.scheduler.schedule(now,EventPriority::TacticalAndCommand,CancellationScope::Persistent,
                DesEventPayload::TacticalCommandEffective{command_index:u32::MAX,side:TeamSideV4::Home}).unwrap();
        }
        let mut failure_seen=false;
        for _ in 0..65 {
            let before=snapshot(&s);
            if s.step_one_des_event(false).is_err(){
                assert_eq!(snapshot(&s),before);
                failure_seen=true;
                break;
            }
        }
        assert!(failure_seen,"scheduler watchdog must fail");
    }

}


#[cfg(test)]
mod locomotion_calibration_tests {
    use super::*;
    use crate::v4::input::{RulesModeV4, SimulationMode};
    use crate::v4::movement::{advance_candidate_live_motion, advance_off_ball_movement_with_report};
    use crate::v4::tests::{make_v4_input_teams, make_v4_team};
    use serde_json::{json, Value};
    use std::time::Instant;

    // Compare the isolated live-motion adapters, not two full-match engines.
    // Run the unchanged reference session to full time; sample its world once
    // per seed. The candidate copy never feeds back into that match.
    fn shadow_pair(index: usize) -> Value {
        let seed = 104729_i64 + index as i64 * 7907;
        let (name, hl, al, press) = match index % 4 {
            0 => ("reference", 14, 12, None),
            1 => ("balanced", 14, 14, None),
            2 => ("strong_away", 12, 14, None),
            _ => ("high_press", 14, 12, Some(85)),
        };
        let mut home = make_v4_team("H", "HOME_V4", hl);
        let mut away = make_v4_team("A", "AWAY_V4", al);
        if let Some(press) = press {
            home.team_instructions.pressing_intensity = press;
            away.team_instructions.pressing_intensity = press;
        }
        let input = make_v4_input_teams(seed, SimulationMode::FastBatch, RulesModeV4::Standard, home, away);
        let mut session = MatchSessionV4::new(input).unwrap();
        let requested_ms = ((index / 4) % 6) as i64 * 900_000;
        let interval_ms = [250_i64, 500, 1000, 2000][(index / 24) % 4];
        let began = Instant::now();
        // Only choose a live controlled snapshot. No real pending contact
        // path is supplied: this report measures tactical locomotion alone.
        let mut steps = 0;
        loop {
            if session.sim_time_ms() >= requested_ms
                && matches!(session.stage, HalfStageV4::InHalf { .. })
                && session.state.dead_ball.is_none()
                && session.state.ball.control == BallControlState::Controlled
            {
                break;
            }
            assert!(!session.is_full_time(), "no live sample for seed {seed}");
            assert!(steps < 100_000, "snapshot watchdog for seed {seed}");
            session.step_one_des_event(true).unwrap();
            session.completed_beats += 1;
            steps += 1;
        }
        let before = session.state.clone();
        let now = session.sim_time_ms();
        let original_rng = session.rng.state();
        let mut legacy = before.clone();
        let mut candidate = before.clone();
        let mut legacy_rng = session.rng.clone();
        let mut candidate_rng = session.rng.clone();
        let dt = match_contracts::units::ms_to_seconds_f32(interval_ms);
        let start = Instant::now();
        advance_off_ball_movement_with_report(&mut legacy, &session.input, session.possession_side,
            session.carrier_idx, dt, &mut legacy_rng);
        let legacy_us = start.elapsed().as_secs_f64() * 1e6;
        let start = Instant::now();
        let attempt = advance_candidate_live_motion(&mut candidate, &session.input, session.possession_side,
            session.carrier_idx, interval_ms, &mut candidate_rng, &[]);
        let candidate_us = start.elapsed().as_secs_f64() * 1e6;
        let mut players = Vec::new();
        let mut failure = None;
        match attempt {
            Ok((rows, _)) => {
                assert_eq!(legacy_rng.state(), candidate_rng.state(), "RNG mismatch seed {seed}");
                assert_eq!(rows.len(), before.players.iter().filter(|p| p.on_pitch && !p.sent_off).count());
                for row in rows {
                    let p = &before.players[row.player_idx];
                    let a = &legacy.players[row.player_idx];
                    let b = &candidate.players[row.player_idx];
                    assert_eq!(a.target_pos, b.target_pos, "target mismatch seed {seed}");
                    let speed = |v: Vec2| v.x.hypot(v.y);
                    players.push(json!({
                        "id": p.player_id, "side": p.side, "slot": p.slot_id, "goalkeeper": p.is_goalkeeper,
                        "initial_speed_mps": speed(p.vel), "aerobic": p.fatigue.aerobic,
                        "anaerobic": p.fatigue.anaerobic, "legacy_distance_m": a.last_moved_m,
                        "candidate_distance_m": row.distance_m(), "legacy_terminal_speed_proxy_mps": speed(a.vel),
                        "candidate_terminal_speed_mps": speed(b.vel), "acceleration_load_mps": row.acceleration_load_mps(),
                        "candidate_aerobic_delta": b.fatigue.aerobic-p.fatigue.aerobic,
                        "candidate_anaerobic_delta": b.fatigue.anaerobic-p.fatigue.anaerobic,
                        "candidate_outside_pitch": b.pos.x<0.0 || b.pos.x>PITCH_LENGTH_M
                            || b.pos.y<0.0 || b.pos.y>match_contracts::spatial::PITCH_WIDTH_M
                    }));
                }
            }
            Err(err) => failure = Some(format!("{err:?}")),
        }
        // Observational harness must not mutate the baseline session.
        assert_eq!(session.rng.state(), original_rng);
        assert_eq!(session.sim_time_ms(), now);
        assert_eq!(session.state, before);
        let result = session.run_to_full_time().unwrap();
        let baseline_players: Vec<_> = session.state.players.iter().map(|p| json!({
            "id": p.player_id, "side": p.side, "slot": p.slot_id, "goalkeeper": p.is_goalkeeper,
            "distance_m": p.fatigue.distance_m, "legacy_sprint_counter": p.fatigue.sprint_count,
            "aerobic": p.fatigue.aerobic, "anaerobic": p.fatigue.anaerobic,
        })).collect();
        json!({
            "seed": seed, "cohort": name, "requested_ms": requested_ms, "sample_ms": now,
            "interval_ms": interval_ms, "legacy_adapter_us": legacy_us, "candidate_adapter_us": candidate_us,
            "baseline_session_wall_ms_including_probe": began.elapsed().as_secs_f64()*1e3,
            "baseline_timeline_fingerprint": result.event_timeline_fingerprint,
            "failure": failure, "players": players, "baseline_players": baseline_players
        })
    }

    #[test]
    fn b12_shadow_report_smoke() {
        let row = shadow_pair(0);
        assert!(row["failure"].is_null());
        assert_eq!(row["players"].as_array().unwrap().len(), 22);
        assert_eq!(row["baseline_players"].as_array().unwrap().len(), 22);
    }

    #[test]
    #[ignore = "release-mode paired interval evidence; run explicitly in L1"]
    fn b12_shadow_locomotion_report() {
        let n = std::env::var("B12_SHADOW_SEEDS").ok().map(|s| s.parse::<usize>().unwrap()).unwrap_or(1000);
        assert!((1..=10_000).contains(&n));
        let rows: Vec<_> = (0..n).map(shadow_pair).collect();
        let failures = rows.iter().filter(|r| !r["failure"].is_null()).count();
        let report = json!({
            "schema": 1, "scope": "reference_full_matches_plus_paired_live_intervals",
            "candidate_full_matches": 0, "paired_intervals": n, "reference_full_matches": n,
            "failures": failures, "rows": rows
        });
        if let Ok(path) = std::env::var("B12_SHADOW_REPORT_PATH") {
            std::fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
        }
        println!("B12_SHADOW_SUMMARY seeds={n} failures={failures} candidate_full_matches=0");
        assert_eq!(failures, 0, "inspect report for candidate failures");
    }
}


#[cfg(test)]
mod locomotion_full_match_experiment {
    use super::*;
    use crate::v4::checkpoint::LOCOMOTION_EXPERIMENT_ID;
    use crate::v4::input::{SimulationMode, RulesModeV4};
    use crate::v4::tests::{make_v4_input, make_v4_input_teams, make_v4_team};
    use serde_json::{json, Value};
    use std::time::Instant;

    #[test]
    fn b12_experiment_checkpoint_identity_and_resume() {
        let input = make_v4_input(12051, SimulationMode::FastBatch, RulesModeV4::Standard);
        let legacy = MatchSessionV4::new(input.clone()).unwrap().export_checkpoint().unwrap();
        assert!(legacy.payload.locomotion_experiment_id.is_none());
        assert!(!serde_json::to_string(&legacy).unwrap().contains("locomotionExperimentId"));
        let mut candidate = MatchSessionV4::new_locomotion_experiment(input).unwrap();
        candidate.advance_steps(25).unwrap();
        let checkpoint = candidate.export_checkpoint().unwrap();
        assert_eq!(checkpoint.engine_version, LOCOMOTION_EXPERIMENT_ID);
        assert_eq!(checkpoint.payload.locomotion_experiment_id.as_deref(), Some(LOCOMOTION_EXPERIMENT_ID));
        let mut invalid = checkpoint.clone();
        invalid.payload.locomotion_experiment_id = None;
        invalid.checksum_hex = compute_checkpoint_checksum(&serde_json::to_string(&invalid.payload).unwrap());
        assert!(MatchSessionV4::from_checkpoint(invalid).is_err());
        let mut invalid = checkpoint.clone();
        invalid.payload.locomotion_experiment_id = Some("unknown".into());
        invalid.checksum_hex = compute_checkpoint_checksum(&serde_json::to_string(&invalid.payload).unwrap());
        assert!(MatchSessionV4::from_checkpoint(invalid).is_err());
        let encoded = serde_json::to_string(&checkpoint).unwrap();
        let mut resumed = MatchSessionV4::from_checkpoint(serde_json::from_str(&encoded).unwrap()).unwrap();
        assert!(resumed.uses_locomotion_experiment());
        candidate.advance_steps(25).unwrap();
        resumed.advance_steps(25).unwrap();
        assert_eq!(candidate.export_checkpoint().unwrap().checksum_hex, resumed.export_checkpoint().unwrap().checksum_hex);
    }

    #[test]
    fn b12_batch1_motion_lifetime_rejects_corrupt_reservations_and_cancels_on_stoppage() {
        let input=make_v4_input(12051,SimulationMode::FastBatch,RulesModeV4::Standard);
        let mut session=MatchSessionV4::new_locomotion_experiment(input).unwrap();
        session.step_one_des_event(true).unwrap();
        session.begin_player_decision(false).unwrap();
        let valid=session.export_checkpoint().unwrap();
        assert!(valid.payload.pending_decision.is_some());
        let corruptions:Vec<Box<dyn Fn(&mut MatchCheckpointV4)>>=vec![
            Box::new(|c|{let d=c.payload.pending_decision.as_mut().unwrap();
                d.preparation_motion=None;}),
            Box::new(|c|{let d=c.payload.pending_decision.as_mut().unwrap();
                d.challenger=d.preparation_motion.clone();}),
            Box::new(|c|{let d=c.payload.pending_decision.as_mut().unwrap();
                d.executes_at_ms=d.release_at_ms+1;}),
            Box::new(|c|{let d=c.payload.pending_decision.as_mut().unwrap();
                d.preparation_motion.as_mut().unwrap().started_at_ms+=1;}),
            Box::new(|c|{let id=c.payload.pending_decision.as_ref().unwrap().player_id.clone();
                c.payload.state.players.iter_mut().find(|p|p.player_id==id).unwrap().on_pitch=false;}),
        ];
        for corrupt in corruptions {
            let mut c=valid.clone();
            corrupt(&mut c);
            c.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&c.payload).unwrap());
            assert!(MatchSessionV4::from_checkpoint(c).is_err());
        }
        let side=session.possession_side;
        let spot=session.state.ball.pos;
        session.open_dead_ball(DeadBallKind::InjuryStoppage,side,spot,true,false).unwrap();
        assert!(session.pending_decision.is_none() && session.pending_flight.is_none() && session.pending_carry.is_none());
        let checkpoint=session.export_checkpoint().unwrap();
        assert!(MatchSessionV4::from_checkpoint(checkpoint).is_ok());
    }

    #[test]
    fn b12_exp5_contact_outcome_is_atomic_in_both_directions() {
        use crate::v4::shooting::{resolve_shot_v4_des_with_motion,ShotContactResolution};
        use crate::rng::DeterministicRng;
        let input=make_v4_input(12056,SimulationMode::FastBatch,RulesModeV4::Standard);
        for side in [TeamSideV4::Home,TeamSideV4::Away] {
            for kind in [ShotKindV4::Driven,ShotKindV4::Header] {
                let mut initial=MatchStateV4::from_input(&input);
                let shooter=initial.players.iter().position(|p|p.side==side && p.on_pitch && !p.is_goalkeeper).unwrap();
                let defender=initial.players.iter().position(|p|p.side!=side && p.on_pitch && !p.is_goalkeeper).unwrap();
                let mirror=|x:f32|if side==TeamSideV4::Home {x} else {105.0-x};
                for p in &mut initial.players {
                    p.pos=Vec2::new(mirror(60.0),8.0);
                    p.vel=Vec2::default();
                }
                initial.players[shooter].pos=Vec2::new(mirror(86.0),34.0);
                // At the start of either a header or driven shot a nearby
                // high-reach defender has a real playable opportunity.
                initial.players[defender].pos=Vec2::new(mirror(86.6),34.0);
                let profile=initial.players[defender].profile_idx;
                initial.player_profiles[profile].attrs.physical.jumping_reach=20;
                let candidates=vec![crate::v4::ball_play::pass_reach_candidate_for_motion(&initial,defender,true,true)];
                let mut accepted=0;
                let mut declined=0;
                for seed in 0..64 {
                    let mut state=initial.clone();
                    let mut rng=DeterministicRng::new(seed);
                    let shot=resolve_shot_v4_des_with_motion(&mut state,&input,&input.timing_model,
                        shooter,None,Some(kind),false,true,&mut rng);
                    let had_block=shot.outcome.events.iter().any(|e|e.kind==EventTypeV4::Block);
                    let launch=shot.trajectory.start_time_ms;
                    let available=match_physics::find_locomotion_shot_block(&shot.trajectory,&candidates,
                        launch,launch+shot.planned_flight_ms);
                    match shot.early_interaction {
                        ShotContactResolution::CommittedBlock(witness)=>{
                            accepted+=1;
                            assert!(had_block);
                            assert!(available.is_some());
                            assert_eq!(witness.hit.defender_id.as_deref(),Some(initial.players[defender].player_id.as_str()));
                            assert_eq!(witness.motion.candidate,candidates[0]);
                            assert_eq!(witness.hit.time_ms,launch+shot.flight_ms);
                            let sample=witness.motion.experimental_plan().unwrap().unwrap()
                                .sample_at(shot.flight_ms).unwrap();
                            assert!(sample.state.position.distance(witness.hit.ball_pos.xy())<=witness.motion.candidate.reach_m+1e-5);
                        }
                        ShotContactResolution::None=>{
                            assert!(!had_block);
                            if available.is_some() {declined+=1;}
                        }
                        ShotContactResolution::CommittedSave(witness)=>{
                            assert!(!had_block);
                            assert!(shot.outcome.events.iter().any(|e|e.kind==EventTypeV4::Save));
                            assert_eq!(witness.time_ms,launch+shot.flight_ms);
                            let sample=witness.motion.experimental_plan().unwrap().unwrap().sample_at(shot.flight_ms).unwrap();
                            assert!(sample.state.position.distance(witness.ball_pos.xy())<=witness.motion.candidate.reach_m+1e-5);
                            if available.is_some() {declined+=1;}
                        }
                        ShotContactResolution::LegacyHit(_)=>panic!("legacy contact leaked into experiment"),
                    }
                }
                assert!(accepted>0,"no accepted contact for {side:?} {kind:?}");
                assert!(declined>0,"no declined opportunity for {side:?} {kind:?}");
            }
        }
    }

    #[test]
    fn b12_exp6_keeper_reach_and_held_save_are_separate_actions() {
        use crate::v4::shooting::{resolve_shot_v4_des_with_motion,ShotContactResolution};
        use crate::rng::DeterministicRng;
        let input=make_v4_input(12061,SimulationMode::FastBatch,RulesModeV4::Standard);
        for side in [TeamSideV4::Home,TeamSideV4::Away] {
            let mut initial=MatchStateV4::from_input(&input);
            let shooter=initial.players.iter().position(|p|p.side==side && p.on_pitch && !p.is_goalkeeper).unwrap();
            let keeper=initial.players.iter().position(|p|p.side!=side && p.on_pitch && p.is_goalkeeper).unwrap();
            let mirror=|x:f32|if side==TeamSideV4::Home {x}else{105.0-x};
            for p in &mut initial.players {
                p.pos=Vec2::new(mirror(50.0),5.0);
                p.vel=Vec2::default();
            }
            initial.players[shooter].pos=Vec2::new(mirror(85.0),34.0);
            initial.players[keeper].pos=Vec2::new(mirror(102.0),34.0);
            let profile=initial.players[keeper].profile_idx;
            initial.player_profiles[profile].attrs.goalkeeping.aerial_reach=20;
            initial.player_profiles[profile].attrs.goalkeeping.reflexes=20;
            let mut held=0;
            for seed in 0..96 {
                let mut near=initial.clone();
                let mut far=initial.clone();
                far.players[keeper].pos.y=4.0;
                let mut a=DeterministicRng::new(seed);
                let mut b=a.clone();
                let close=resolve_shot_v4_des_with_motion(&mut near,&input,&input.timing_model,
                    shooter,None,Some(ShotKindV4::Driven),false,true,&mut a);
                let distant=resolve_shot_v4_des_with_motion(&mut far,&input,&input.timing_model,
                    shooter,None,Some(ShotKindV4::Driven),false,true,&mut b);
                assert!(!distant.outcome.events.iter().any(|ev|ev.kind==EventTypeV4::Save));
                if let ShotContactResolution::CommittedSave(contact)=close.early_interaction {
                    assert_eq!(contact.motion.candidate.player_id,initial.players[keeper].player_id);
                    assert_eq!(contact.ball_pos,close.trajectory.position_at_ms(contact.time_ms));
                    assert_eq!(contact.time_ms,close.trajectory.start_time_ms+close.flight_ms);
                    if near.ball.control==BallControlState::Controlled {
                        held+=1;
                        assert_eq!(near.ball.owner_player_id.as_deref(),Some(initial.players[keeper].player_id.as_str()));
                        assert_eq!(close.outcome.next_carrier_idx,keeper);
                        assert!(!close.outcome.events.iter().any(|ev|ev.kind==EventTypeV4::GoalkeeperDistribution));
                        assert_eq!(near.ball.pos,contact.ball_pos);
                    }
                }
            }
            assert!(held>0,"no held save in deterministic reachable fixture {side:?}");
        }
    }

    #[test]
    fn b12_exp5_parry_ends_one_flight_then_schedules_recovery() {
        // Explicit reachable shot fixture, not a dependency on the event
        // distribution of an older behavior ID. The real scheduler, RNG,
        // shot resolver and recovery run unchanged. The historical failing
        // seed 326125 remains in b12_exp5_previous_failure_regressions.
        for side in [TeamSideV4::Home, TeamSideV4::Away] {
            let mut exercised = false;
            for seed in 12061_i64..12157 {
                let input = make_v4_input(seed, SimulationMode::FastBatch, RulesModeV4::Standard);
                let mut session = MatchSessionV4::new_locomotion_experiment(input).unwrap();
                session.step_one_des_event(true).unwrap();
                session.scheduler.invalidate_live_play();
                let shooter_idx = session.state.players.iter().position(|p|
                    p.side == side && p.on_pitch && !p.is_goalkeeper).unwrap();
                let keeper_idx = session.state.players.iter().position(|p|
                    p.side != side && p.on_pitch && p.is_goalkeeper).unwrap();
                let mirror = |x: f32| if side == TeamSideV4::Home { x } else { PITCH_LENGTH_M-x };
                for p in &mut session.state.players {
                    p.pos = Vec2::new(mirror(50.0), 5.0);
                    p.target_pos = p.pos;
                    p.vel = Vec2::default();
                }
                session.state.players[shooter_idx].pos = Vec2::new(mirror(85.0), 34.0);
                session.state.players[shooter_idx].target_pos = session.state.players[shooter_idx].pos;
                session.state.players[keeper_idx].pos = Vec2::new(mirror(102.0), 34.0);
                session.state.players[keeper_idx].target_pos = session.state.players[keeper_idx].pos;
                session.carrier_idx = shooter_idx;
                session.possession_side = side;
                session.state.ball.pos = Vec3::ground(mirror(85.0), 34.0);
                session.state.ball.vel = Vec3::default();
                session.state.ball.control = BallControlState::Controlled;
                session.state.ball.owner_side = Some(side);
                session.state.ball.owner_player_id = Some(session.state.players[shooter_idx].player_id.clone());
                session.schedule_shot_flight(shooter_idx, None, Some(ShotKindV4::Driven)).unwrap();
                let flight = session.pending_flight.as_ref().expect("scheduled test shot");
                if flight.ball_after.control != BallControlState::Loose { continue; }
                assert!(matches!(flight.action, FlightActionV4::Shot { shooter_idx: i } if i == shooter_idx));
                let at=flight.resolves_at_ms;
                let launch=flight.trajectory.start_time_ms;
                let offside=flight.offside_at_launch.clone();
                assert!(at<=flight.planned_arrival_ms && at>launch);
                assert!(flight.early_interaction.is_none());
                let save_path=flight.pass_contact_motion.clone().expect("reserved saving keeper");
                let terminal=save_path.experimental_plan().unwrap().unwrap().sample_at(at-launch).unwrap().state;
                assert!(terminal.position.distance(save_path.target)<=save_path.candidate.reach_m+1e-5);
                assert!(flight.ball_after.owner_player_id.is_none());
                assert!(flight.ball_after.owner_side.is_none());
                assert_eq!(flight.ball_after.last_touch_side,Some(side.opposite()));
                assert_eq!(flight.outcome.events.iter().filter(|e|e.kind==EventTypeV4::ShotOnTarget).count(),1);
                assert_eq!(flight.outcome.events.iter().filter(|e|e.kind==EventTypeV4::Rebound).count(),1);
                assert!(!flight.outcome.events.iter().any(|e|e.kind==EventTypeV4::Block));
                let ck=session.export_checkpoint().unwrap();
                let mut restored=MatchSessionV4::from_checkpoint(
                    serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
                for current in [&mut session,&mut restored] {
                    for _ in 0..100 {
                        if current.sim_time_ms()>=at {break;}
                        current.step_one_des_event(true).unwrap();
                        current.completed_beats+=1;
                    }
                    assert_eq!(current.sim_time_ms(),at);
                    let keeper=current.state.players.iter().find(|p|p.player_id==save_path.candidate.player_id).unwrap();
                    assert!(keeper.is_goalkeeper);
                    assert_eq!(keeper.pos,terminal.position);
                    assert_eq!(keeper.vel,terminal.velocity);
                    let pending=current.pending_flight.as_ref().expect("scheduled loose rebound race");
                    assert_eq!(pending.trajectory.start_time_ms,at);
                    assert!(pending.resolves_at_ms>at);
                    assert_eq!(pending.offside_at_launch,offside);
                    assert!(matches!(&pending.action,FlightActionV4::LooseRecovery{origin_side,cause}
                        if *origin_side==side && cause=="SHOT_REBOUND_LOOSE"));
                    assert_eq!(current.state.ball.control,BallControlState::Loose);
                    assert!(current.state.ball.owner_player_id.is_none());
                    assert!(current.state.ball.owner_side.is_none());
                    let recovery_at=pending.resolves_at_ms;
                    for _ in 0..100 {
                        if current.sim_time_ms()>=recovery_at {break;}
                        current.step_one_des_event(true).unwrap();
                        current.completed_beats+=1;
                    }
                    assert_eq!(current.sim_time_ms(),recovery_at);
                    assert!(current.pending_flight.is_none());
                    // Recovery may yield possession or an offside restart;
                    // neither may resolve a synthetic second shot.
                    assert!(matches!(current.state.ball.control,BallControlState::Controlled|BallControlState::DeadBall));
                }
                assert_eq!(session.export_checkpoint().unwrap().checksum_hex,restored.export_checkpoint().unwrap().checksum_hex);
                exercised = true;
                break;
            }
            assert!(exercised, "no scheduled parry in reachable fixture for {side:?}");
        }
    }

    #[test]
    fn b12_exp5_shot_block_path_survives_checkpoint_and_real_des() {
        // Find a real committed block in deterministic match fixtures;
        // no synthetic player displacement, mocked contact, or forced RNG.
        for index in 0..4 {
            let (input,_)=fixture(index,SimulationMode::FastBatch);
            let mut session=MatchSessionV4::new_locomotion_experiment(input).unwrap();
            for _ in 0..3000 {
                if session.is_full_time() {break;}
                session.step_one_des_event(true).unwrap();
                session.completed_beats+=1;
                let Some(f)=session.pending_flight.as_ref() else {continue;};
                if !matches!(f.action,FlightActionV4::Shot{..}) {continue;}
                let Some(hit)=f.early_interaction.as_ref().filter(|h|h.kind==InteractionKind::ShotBlocked) else {continue;};
                let path=f.pass_contact_motion.clone().expect("reserved shot-block actor");
                let at=f.resolves_at_ms;
                assert_eq!(hit.time_ms,at);
                assert_eq!(hit.defender_id.as_deref(),Some(path.candidate.player_id.as_str()));
                let target=hit.ball_pos.xy();
                let plan=path.experimental_plan().unwrap().unwrap();
                let terminal=plan.sample_at(at-path.started_at_ms).unwrap().state;
                assert!(terminal.position.distance(target)<=path.candidate.reach_m+1e-5);
                let ck=session.export_checkpoint().unwrap();
                // A checksum-valid snapshot cannot hide a scheduled block
                // by deleting both contact fields while leaving its event.
                let mut corrupt=ck.clone();
                let flight=corrupt.payload.pending_flight.as_mut().unwrap();
                flight.early_interaction=None;
                flight.pass_contact_motion=None;
                corrupt.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&corrupt.payload).unwrap());
                assert!(MatchSessionV4::from_checkpoint(corrupt).is_err());
                let mut resumed=MatchSessionV4::from_checkpoint(
                    serde_json::from_str(&serde_json::to_string(&ck).unwrap()).unwrap()).unwrap();
                for current in [&mut session,&mut resumed] {
                    for _ in 0..100 {
                        if current.sim_time_ms()>=at {break;}
                        current.step_one_des_event(true).unwrap();
                        current.completed_beats+=1;
                    }
                    assert_eq!(current.sim_time_ms(),at);
                    let actor=current.state.players.iter().find(|p|p.player_id==path.candidate.player_id).unwrap();
                    assert_eq!(actor.pos,terminal.position);
                    assert_eq!(actor.vel,terminal.velocity);
                    assert_eq!(actor.facing_rad,terminal.facing_rad);
                }
                assert_eq!(session.export_checkpoint().unwrap().checksum_hex,resumed.export_checkpoint().unwrap().checksum_hex);
                return;
            }
        }
        panic!("no committed shot block in the deterministic regression fixtures");
    }

    fn fixture(index: usize, mode: SimulationMode) -> (MatchInputV4, &'static str) {
        let seed = 104729_i64 + index as i64 * 7907;
        let (name, hl, al, press) = match index % 4 {
            0 => ("reference",14,12,None),
            1 => ("balanced",14,14,None),
            2 => ("strong_away",12,14,None),
            _ => ("high_press",14,12,Some(85)),
        };
        let mut home = make_v4_team("H","HOME_V4",hl);
        let mut away = make_v4_team("A","AWAY_V4",al);
        if let Some(press) = press {
            home.team_instructions.pressing_intensity = press;
            away.team_instructions.pressing_intensity = press;
        }
        (make_v4_input_teams(seed,mode,RulesModeV4::Standard,home,away),name)
    }

    fn run(input: MatchInputV4, experiment: bool) -> Value {
        let start = Instant::now();
        let mut session = if experiment {
            MatchSessionV4::new_locomotion_experiment(input).unwrap()
        } else { MatchSessionV4::new(input).unwrap() };
        let mut last_probe_launch = None;
        let (mut probe_on_target, mut probe_no_reach, mut probe_reachable_goal) = (0_u32,0_u32,0_u32);
        let (mut probe_segment, mut probe_depth, mut probe_speed, mut probe_ms) = (0.0_f64,0.0_f64,0.0_f64,0.0_f64);
        // Advance individual atomic events for a useful first-failure clock.
        // Full time still uses the same result builder and session lifecycle.
        while !session.is_full_time() {
            if session.stage.is_break() {
                if let Err(err) = session.begin_next_period_internal() {
                    return json!({"ok":false,"time_ms":session.sim_time_ms(),"error":format!("{err:?}")});
                }
                continue;
            }
            if let Err(err) = session.step_one_des_event(true) {
                let outside: Vec<_> = session.state.players.iter().filter(|p| p.on_pitch && !p.sent_off &&
                    (p.pos.x<0.0 || p.pos.x>PITCH_LENGTH_M || p.pos.y<0.0 ||
                        p.pos.y>match_contracts::spatial::PITCH_WIDTH_M))
                    .map(|p|json!({"id":p.player_id,"pos":p.pos,"vel":p.vel})).collect();
                return json!({"ok":false,"time_ms":session.sim_time_ms(),"error":format!("{err:?}"),
                    "next_event_ms":session.scheduler.peek_next_valid_time_ms(),
                    "ball_control":session.state.ball.control,"ball_position":session.state.ball.pos,
                    "pending_flight":session.pending_flight.as_ref().map(|f| &f.pass_contact_motion),
                    "pending_carry":session.pending_carry.as_ref().map(|c| &c.motion),
                    "pending_preparation":session.pending_decision.as_ref().map(|d| &d.preparation_motion),
                    "outside_players":outside});
            }
            // Read-only diagnostics at the actual scheduled shot launch.
            // Recompute reach only for goals; a committed save already owns its witness.
            if experiment {
                if let Some(flight) = session.pending_flight.as_ref() {
                    if let FlightActionV4::Shot { shooter_idx } = &flight.action {
                        let launch = flight.trajectory.start_time_ms;
                        if last_probe_launch != Some(launch) {
                            last_probe_launch = Some(launch);
                            let side = session.state.players[*shooter_idx].side;
                            let attacking = if side == TeamSideV4::Home { &flight.counter_delta.home } else { &flight.counter_delta.away };
                            if attacking.shots_on_target > 0 {
                                let timer = Instant::now();
                                probe_on_target += 1;
                                let keeper = session.state.players.iter().position(|p|p.side==side.opposite() && p.is_goalkeeper && p.on_pitch && !p.sent_off);
                                if let Some(idx) = keeper {
                                    let p = &session.state.players[idx];
                                    let goal = crate::v4::spatial::attacking_goal_center(side);
                                    probe_segment += crate::v4::spatial::point_to_segment_distance_m(p.pos,flight.trajectory.start_pos.xy(),goal) as f64;
                                    probe_depth += (p.pos.x-goal.x).abs() as f64;
                                    probe_speed += p.vel.x.hypot(p.vel.y) as f64;
                                }
                                if attacking.goals > 0 {
                                    let reach = keeper.and_then(|idx|crate::v4::goalkeeper::shot_contact_candidate(&session.state,idx))
                                        .and_then(|candidate|match_physics::find_locomotion_goalkeeper_contact(&flight.trajectory,&candidate,launch,flight.planned_arrival_ms));
                                    if reach.is_some() { probe_reachable_goal += 1; } else { probe_no_reach += 1; }
                                }
                                probe_ms += timer.elapsed().as_secs_f64()*1000.0;
                            }
                        }
                    }
                }
            }
            session.completed_beats += 1;
            if session.completed_beats > 250_000 {
                return json!({"ok":false,"time_ms":session.sim_time_ms(),"error":"experiment watchdog"});
            }
        }
        session.finished = true;
        let result = session.build_result();
        let players: Vec<_> = session.state.players.iter().map(|p| json!({
            "id":p.player_id, "slot":p.slot_id,"goalkeeper":p.is_goalkeeper,
            "distance_m":p.fatigue.distance_m,"aerobic":p.fatigue.aerobic,
            "anaerobic":p.fatigue.anaerobic,"sprint_counter":p.fatigue.sprint_count,
            "high_intensity_counter":p.fatigue.high_intensity_actions,
        })).collect();
        let diag=result.diagnostics.as_ref().expect("paired result must have diagnostics");
        let mut report = json!({
            "ok":true,"behavior_id":result.engine_version,"wall_ms":(start.elapsed().as_secs_f64()*1000.0-probe_ms).max(0.0),
            "pass_receiver_unreached":diag.home.pass_receiver_unreached+diag.away.pass_receiver_unreached,
            "pass_execution_errors":diag.home.pass_execution_errors+diag.away.pass_execution_errors,
            "pass_intercepted":diag.home.turnovers.pass_intercepted+diag.away.turnovers.pass_intercepted,
            "pass_out_of_play":diag.home.turnovers.pass_out_of_play+diag.away.turnovers.pass_out_of_play,
            "first_touch_errors":diag.home.turnovers.first_touch_error+diag.away.turnovers.first_touch_error,
            "pass_contacts_loose":diag.home.pass_contacts_loose+diag.away.pass_contacts_loose,
            "pass_risk_pairs":diag.des.pass_risk_pairs,
            "pass_risk_initial_sum":diag.des.pass_risk_initial_sum,
            "pass_risk_release_sum":diag.des.pass_risk_release_sum,
            "reaim_choices":diag.des.reaim_choices,
            "possessions":diag.home.possessions+diag.away.possessions,
            "final_third_possessions":diag.home.final_third_possessions+diag.away.final_third_possessions,
            "penalty_area_possessions":diag.home.penalty_area_possessions+diag.away.penalty_area_possessions,
            "possessions_with_shot":diag.home.possessions_with_shot+diag.away.possessions_with_shot,
            "shots_from_box":diag.home.shots_from_box+diag.away.shots_from_box,
            "shots_blocked":diag.home.shots_blocked+diag.away.shots_blocked,
            "shots_on_target":diag.home.shots_on_target+diag.away.shots_on_target,
            "shots_off_target":diag.home.shots_off_target+diag.away.shots_off_target,
            "shot_xg":diag.home.shot_xg+diag.away.shot_xg,
            "saves":session.state.home.saves+session.state.away.saves,
            "carry_starts":diag.des.carry_starts,
            "carry_contacts":diag.des.carry_contacts,
            "carry_turnovers":diag.des.carry_turnovers,
            "tackle_opportunities":diag.des.tackle_opportunities,
            "tackle_commit_choices":diag.des.tackle_commit_choices,
            "counter_semantics":if experiment {"upward_speed_crossings_4_2_and_7_mps"} else {"legacy_interval_counts"},
            "fingerprint":result.event_timeline_fingerprint,"events":result.event_count,
            "time_ms":session.sim_time_ms(),"live_ms":session.state.des_clock.ball_in_play_ms,
            "dead_ms":session.state.des_clock.ball_out_of_play_ms,
            "home_goals":result.summary.home.goals,"away_goals":result.summary.away.goals,
            "shots":u32::from(result.summary.home.shots)+u32::from(result.summary.away.shots),
            "passes":u32::from(result.summary.home.passes_attempted)+u32::from(result.summary.away.passes_attempted),
            "completed_passes":u32::from(result.summary.home.passes_completed)+u32::from(result.summary.away.passes_completed),
            "fouls":u32::from(result.summary.home.fouls)+u32::from(result.summary.away.fouls),
            "players":players,
        });
        // Keep the flat report schema, but split macro expansion into
        // small objects rather than raising the crate's recursion limit.
        let probes = json!({
            "shot_probe_on_target":probe_on_target,"shot_probe_no_reach":probe_no_reach,
            "shot_probe_reachable_goal":probe_reachable_goal,"shot_probe_keeper_segment_m":probe_segment,
            "shot_probe_keeper_depth_m":probe_depth,"shot_probe_keeper_speed_mps":probe_speed,
            "shot_probe_ms":probe_ms,
        });
        let fields = report.as_object_mut().expect("match report object");
        for (key, value) in probes.as_object().expect("probe report object") {
            assert!(fields.insert(key.clone(), value.clone()).is_none(), "duplicate report field {key}");
        }
        report
    }

    #[test]
    #[ignore = "experimental full-match cross-mode test, not production golden"]
    fn b12_experiment_cross_mode() {
        let (input,_) = fixture(0,SimulationMode::FastBatch);
        let mut full = input.clone();
        full.simulation_mode = SimulationMode::FullEvent;
        let a = run(input,true);
        let b = run(full,true);
        assert_eq!(a["ok"],true,"{a}");
        assert_eq!(b["ok"],true,"{b}");
        for key in ["fingerprint","events","time_ms","home_goals","away_goals","players",
            "shot_probe_on_target","shot_probe_no_reach","shot_probe_reachable_goal",
            "shot_probe_keeper_segment_m","shot_probe_keeper_depth_m","shot_probe_keeper_speed_mps"] {
            assert_eq!(a[key],b[key],"mismatch {key}");
        }
    }

    #[test]
    #[ignore = "full matches reproducing exp1 failures"]
    fn b12_exp2_previous_failure_regressions() {
        // One recorded exp1 failure from each cohort, identical fixture and seed.
        let mut failures = Vec::new();
        for seed in [167985_i64,397288,183799,191706] {
            let index=((seed-104729)/7907) as usize;
            let (input,cohort)=fixture(index,SimulationMode::FastBatch);
            assert_eq!(input.seed,seed);
            let report=run(input,true);
            if report["ok"] != true {
                failures.push(json!({"seed":seed,"cohort":cohort,"failure":report}));
            }
        }
        assert!(failures.is_empty(),"exp2 regression failures: {failures:?}");
    }

    #[test]
    #[ignore = "full matches reproducing exp5 shot-block handoff failures"]
    fn b12_exp5_previous_failure_regressions() {
        let mut failures=Vec::new();
        for seed in [326125_i64,270776,152171,128450,515893,112636,1322407,349846] {
            let index=((seed-104729)/7907) as usize;
            let (input,cohort)=fixture(index,SimulationMode::FastBatch);
            assert_eq!(input.seed,seed);
            let result=run(input,true);
            if result["ok"]!=true {failures.push(json!({"seed":seed,"cohort":cohort,"failure":result}));}
        }
        assert!(failures.is_empty(),"exp5 handoff regression failures: {failures:?}");
    }

    #[test]
    #[ignore = "paired full-match experiment, run in separate evidence workflow"]
    fn b12_experiment_paired_report() {
        let shard = std::env::var("B12_EXPERIMENT_SHARD").ok().map(|s|s.parse::<usize>().unwrap()).unwrap_or(0);
        let count = std::env::var("B12_EXPERIMENT_COUNT").ok().map(|s|s.parse::<usize>().unwrap()).unwrap_or(250);
        assert!(shard<4 && (1..=250).contains(&count));
        let mut rows=Vec::with_capacity(count);
        let mut failures=0;
        for i in 0..count {
            let index=4*i+shard;
            let (input,cohort)=fixture(index,SimulationMode::FastBatch);
            let seed=input.seed;
            let legacy=run(input.clone(),false);
            let experiment=run(input,true);
            if legacy["ok"]!=true || experiment["ok"]!=true { failures+=1; }
            rows.push(json!({"seed":seed,"cohort":cohort,"legacy":legacy,"experiment":experiment}));
        }
        let report=json!({"schema":1,"scope":"hybrid_experiment_full_match_pairs",
            "behavior_id":LOCOMOTION_EXPERIMENT_ID,"shard":shard,"attempted_pairs":count,
            "failed_pairs":failures,"rows":rows});
        if let Ok(path)=std::env::var("B12_EXPERIMENT_REPORT_PATH") {
            std::fs::write(path,serde_json::to_vec(&report).unwrap()).unwrap();
        }
        println!("B12_EXPERIMENT shard={shard} attempted={count} failures={failures}");
        assert_eq!(failures,0,"inspect paired experiment failure rows");
    }
}
