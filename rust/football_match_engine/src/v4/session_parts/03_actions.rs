// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    /// PI chooses once from a read-only observation; DES owns the future execution.
    fn begin_player_decision(&mut self, fresh_receipt: bool) -> Result<(), V4Error> {
        if self.pending_decision.is_some() || self.pending_flight.is_some() || self.pending_carry.is_some()
            || self.state.dead_ball.is_some() { return Ok(()); }
        self.ensure_valid_carrier();
        let idx = self.carrier_idx;
        crate::v4::transition::verify_action_actor(&self.state,idx)?;
        let p = &self.state.players[idx];
        let side = p.side;
        let id = p.player_id.clone();
        let pos = p.pos;
        self.state.ball.pos = Vec3::ground(pos.x,pos.y).clamp_to_pitch();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(side);
        self.state.ball.owner_player_id = Some(id.clone());
        crate::v4::transition::verify_controlled_actor(&self.state,idx)?;
        self.possession_side = side;
        self.diagnostics.observe_controlled_ball(side,pos);
        let observed:Vec<_>=self.state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off)
            .map(|(idx,p)|crate::player_intelligence::ObservedPlayer{idx,side:p.side,pos:p.pos,is_gk:p.is_goalkeeper}).collect();
        let receiving_line=crate::v4::ball_play::offside_reference_progress_m(&self.state,side,pos);
        let open_outlets=crate::player_intelligence::count_open_outlets(side,idx,pos,&observed,receiving_line);
        self.diagnostics.record_open_outlets(open_outlets);
        let ctx = evaluate_local_context(&self.state,&self.input,side,pos);
        let mut plan = perceive_evaluate_and_decide_des(&self.state,&self.input,
            &self.input.timing_model,idx,ctx,&mut self.rng);
        self.diagnostics.record_outlet_choice(open_outlets,matches!(plan.action,PlannedActionV4::HoldAndRecycle));
        // A one-touch intention is a property of this receiver's action,
        // not of the passer who delivered the previous ball.
        if !fresh_receipt { plan.is_one_touch = false; }
        let settle = if fresh_receipt && !plan.is_one_touch {
            self.input.timing_model.first_touch_settle_ms.sample_triangular(&mut self.rng,1.0).max(150)
        } else { 0 };
        let now = self.state.des_clock.sim_time_ms;
        let release_at_ms = now.saturating_add(plan.decision_delay_ms.max(1)).saturating_add(settle);
        let (at,challenger)=self.preparation_contest(idx,release_at_ms);
        self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
            DesEventPayload::ActionExecute{player_id:id.clone(),side})
            .map_err(|e|V4Error::InvalidClock(e.into()))?;
        let observed_pass_risk=crate::player_intelligence::observe_pass_risk(&self.state,&self.input,idx,ctx,plan.action);
        self.pending_decision=Some(PendingDecisionV4{player_id:id,side,started_at_ms:now,executes_at_ms:at,
            plan,release_at_ms,challenger,observed_pass_risk,final_aim_committed:false});
        Ok(())
    }

    /// A defender finishing a tackle must recover before committing to
    /// another tackle. This does not prevent movement, screening or
    /// pass interception; it is a separate action readiness clock.
    fn tackle_candidates(&self,side:TeamSideV4,end:i64)->Vec<crate::v4::interaction::PassReachCandidate> {
        let now=self.state.des_clock.sim_time_ms;
        self.state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off
            && !p.is_goalkeeper && p.side!=side)
            .filter_map(|(i,p)|{
                let ready=self.tackle_ready_at.get(&p.player_id).copied().unwrap_or(now);
                let remaining=ready.saturating_sub(now).max(0);
                let mut candidate=pass_reach_candidate(&self.state,i,false);
                // A recovering player cannot actively chase/attempt a
                // second tackle until recovery plus the new reaction.
                candidate.reaction_ms=candidate.reaction_ms.saturating_add(remaining);
                if now.saturating_add(candidate.reaction_ms)>=end {None}else{Some(candidate)}
            }).collect()
    }

    fn mark_tackle_recovery(&mut self,idx:usize) {
        let p=&self.state.players[idx];
        let attrs=lookup_attrs(&self.input,p);
        let agility=normalize_attr(attrs.physical.agility);
        let balance=normalize_attr(attrs.physical.balance);
        // Explicit follow-through + regain balance, in milliseconds.
        // This kinematic prior is independent of score and population KPIs.
        let duration=(450.0+450.0*(1.0-agility)+250.0*(1.0-balance)).round() as i64;
        self.tackle_ready_at.insert(p.player_id.clone(),self.state.des_clock.sim_time_ms.saturating_add(duration));
    }

    /// Reachable candidates first, then the PI's tackle-vs-contain
    /// choice. Evaluate each defender independently; only committed
    /// candidates compete for the earliest physical event.
    fn choose_committed_tackle(&mut self,idx:usize,motion:&PassContactMotion,end:i64)
        ->Option<(usize,i64,PassContactMotion)> {
        let candidates=self.tackle_candidates(motion.candidate.side,end);
        let mut best:Option<(usize,i64,PassContactMotion)>=None;
        for candidate in &candidates {
            let Some((_,at,chase))=find_carry_contact(motion,std::slice::from_ref(candidate),end) else {continue;};
            let Some(def_idx)=self.state.players.iter().position(|p|p.player_id==candidate.player_id) else {continue;};
            let elapsed=at.saturating_sub(motion.started_at_ms) as f32/1000.0;
            let evaluation=crate::player_intelligence::evaluate_tackle(&self.state,idx,def_idx,elapsed);
            let commits=evaluation.commits();
            self.diagnostics.record_tackle_choice(commits,evaluation.access,
                evaluation.clean_win_probability,evaluation.foul_probability);
            if !commits {continue;}
            if best.as_ref().map_or(true,|(_,old_time,old_motion)|
                at<*old_time || (at==*old_time && candidate.player_id<old_motion.candidate.player_id)) {
                best=Some((def_idx,at,chase));
            }
        }
        best
    }

    fn preparation_contest(&mut self,idx:usize,release:i64)->(i64,Option<PassContactMotion>) {
        let now=self.state.des_clock.sim_time_ms;
        let mut carrier=pass_reach_candidate(&self.state,idx,false);
        carrier.speed_mps=0.0;
        carrier.reaction_ms=0;
        carrier.velocity=Vec2::default();
        let pos=carrier.pos;
        let motion=PassContactMotion{candidate:carrier,target:pos,started_at_ms:now};
        match self.choose_committed_tackle(idx,&motion,release) {
            // Exact tie goes to the already prepared release.
            Some((_,at,chase)) if at<release=>(at,Some(chase)),
            _=>(release,None),
        }
    }

    fn execute_player_decision(&mut self, player_id: &str, side: TeamSideV4) -> Result<(), V4Error> {
        let Some(mut pending) = self.pending_decision.take() else { return Ok(()); };
        let now_ms=self.state.des_clock.sim_time_ms;
        if pending.player_id!=player_id || pending.side!=side || pending.executes_at_ms!=now_ms {
            return Err(V4Error::InvalidClock("PI action execution does not match pending intention".into()));
        }
        self.ensure_valid_carrier();
        crate::v4::transition::verify_action_actor(&self.state,self.carrier_idx)?;
        if self.state.players[self.carrier_idx].player_id!=player_id || self.possession_side!=side {
            return self.begin_player_decision(false);
        }
        let idx=self.carrier_idx;
        let pos=self.state.players[idx].pos;
        self.state.ball.pos=Vec3::ground(pos.x,pos.y).clamp_to_pitch();
        if let Some(chase)=pending.challenger.take() {
            let contact=self.state.players.iter().position(|p|p.player_id==chase.candidate.player_id
                && p.on_pitch && !p.sent_off && p.side!=side
                && p.pos.distance(pos)<=chase.candidate.reach_m+0.01);
            if let Some(def_idx)=contact {
                self.mark_tackle_recovery(def_idx);
                let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
                let mut outcome=self.resolve_audited_carry_contact(idx,Some(def_idx),ctx,
                    pending.plan.carry.map_or(false,|c|c.shielding));
                let foul=outcome.events.iter().any(|e|e.kind==EventTypeV4::Foul);
                let card=outcome.events.iter().any(|e|matches!(e.kind,EventTypeV4::YellowCard|EventTypeV4::RedCard));
                let injury=outcome.events.iter().any(|e|e.kind==EventTypeV4::Injury);
                let lost=outcome.next_side!=side;
                self.diagnostics.record_preparation_contact(lost,foul);
                for e in &mut outcome.events {
                    e.metadata.extra.insert("contestPhase".into(),"PREPARATION".into());
                    // The carrier retained control, not necessarily beat
                    // a defender or completed a dribble.
                    if e.kind==EventTypeV4::DribbleWon {
                        e.kind=EventTypeV4::Carry;
                        e.metadata.escaped_pressure=None;
                        e.metadata.outcome=Some("RETAINED_UNDER_CONTACT".into());
                    }
                }
                self.flush_play_events(outcome.events,side);
                if foul {
                    let kind=if is_in_attacking_penalty_box(side,pos){DeadBallKind::PenaltyKick}else{DeadBallKind::DirectFreeKick};
                    return self.open_dead_ball(kind,side,Vec3::ground(pos.x,pos.y),injury,card);
                }
                if self.state.ball.control==BallControlState::Loose {
                    return self.start_pass_recovery(side,Vec::new(),"TACKLE_CONTACT_LOOSE");
                }
                if lost {
                    self.last_passer_idx=None;
                    let avg_wr=self.team_avg_work_rate(outcome.next_side);
                    let ball=self.state.ball.pos;
                    let phase_events=update_tactical_phases(&mut self.state,side,outcome.next_side,ball,0,avg_wr);
                    self.flush_play_events(phase_events,outcome.next_side);
                    self.carrier_idx=outcome.next_carrier_idx;
                    self.possession_side=outcome.next_side;
                    return self.begin_player_decision(false);
                }
            }
            // A committed challenge was resolved or became invalid. If
            // the carrier retains control, keep the ORIGINAL planned
            // release time without rolling another tackle in this plan.
            pending.executes_at_ms=pending.release_at_ms;
            self.scheduler.schedule(pending.release_at_ms,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
                DesEventPayload::ActionExecute{player_id:player_id.into(),side})
                .map_err(|e|V4Error::InvalidClock(e.into()))?;
            self.pending_decision=Some(pending);
            return Ok(());
        }
        let mut plan=pending.plan;
        // Revalidate the intended target, and measure the physical situation
        // at execution. Do not re-roll the whole decision after every event.
        match &mut plan.action {
            PlannedActionV4::Pass{target_idx,kind,distance_m,progress_gain_m,lane_risk,interceptor_idx}=>{
                let Some(p)=self.state.players.get(*target_idx) else {return self.begin_player_decision(false);};
                if !p.on_pitch || p.sent_off || p.side!=side || *target_idx==idx {
                    return self.begin_player_decision(false);
                }
                *distance_m=pos.distance(p.pos);
                if *distance_m<0.5 {return self.begin_player_decision(false);}
                *progress_gain_m=attacking_progress_m(side,p.pos)-attacking_progress_m(side,pos);
                (*lane_risk,*interceptor_idx)=evaluate_passing_lane(&self.state,&self.input,side,pos,p.pos,kind.is_aerial());
            }
            PlannedActionV4::Shoot{distance_m,in_box}=>{
                *distance_m=distance_to_attacking_goal(side,pos);
                *in_box=is_in_attacking_penalty_box(side,pos);
            }
            _=>{}
        }
        // Audit the original intended release before deciding whether
        // there is a better compatible ground kick. No reroll of the
        // original action-family decision or the original timing draw.
        let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
        let release_risk=if pending.observed_pass_risk.is_some() {
            crate::player_intelligence::observe_pass_risk(&self.state,&self.input,idx,ctx,plan.action)
        }else{None};
        if let (Some(initial),Some(release))=(pending.observed_pass_risk,release_risk) {
            self.diagnostics.record_pass_risk_drift(initial,release);
        }
        if !plan.is_one_touch && !pending.final_aim_committed &&
            matches!(plan.action,PlannedActionV4::Pass{kind:PassKindV4::Short,..}) {
            self.diagnostics.record_reaim_review();
            if let Some(reaim)=crate::player_intelligence::reconsider_short_pass(
                &self.state,&self.input,idx,ctx,plan.action) {
                let updated_risk=crate::player_intelligence::observe_pass_risk(
                    &self.state,&self.input,idx,ctx,reaim.action);
                self.diagnostics.record_reaim_choice(reaim.delay_ms,release_risk,updated_risk);
                plan.action=reaim.action;
                let release_at_ms=now_ms.saturating_add(reaim.delay_ms);
                let (at,challenger)=self.preparation_contest(idx,release_at_ms);
                self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
                    DesEventPayload::ActionExecute{player_id:player_id.into(),side})
                    .map_err(|e|V4Error::InvalidClock(e.into()))?;
                self.pending_decision=Some(PendingDecisionV4{
                    player_id:player_id.into(),side,started_at_ms:now_ms,executes_at_ms:at,
                    plan,release_at_ms,challenger,observed_pass_risk:updated_risk,final_aim_committed:true,
                });
                return Ok(());
            }
        }
        if pending.final_aim_committed {self.diagnostics.record_reaim_release();}
        let prev_possession = self.possession_side;
        let (next_side, next_carrier, entered_dead_ball, next_delay_ms) =
            self.step_live_action_des(self.carrier_idx, plan)?;

        self.possession_side = next_side;
        self.carrier_idx = next_carrier;

        if prev_possession != self.possession_side {
            let avg_wr = self.team_avg_work_rate(self.possession_side);
            let ball_pos_now = self.state.ball.pos;
            let phase_events = update_tactical_phases(
                &mut self.state,
                prev_possession,
                self.possession_side,
                ball_pos_now,
                0,
                avg_wr,
            );
            if !phase_events.is_empty() {
                let poss = self.possession_side;
                self.flush_play_events(phase_events, poss);
            }
        }

        if !entered_dead_ball && self.state.dead_ball.is_none() && self.pending_flight.is_none()
            && self.pending_carry.is_none() {
            let cid = self.state.players[self.carrier_idx].player_id.clone();
            self.scheduler
                .schedule(
                    now_ms.saturating_add(next_delay_ms.max(1)),
                    EventPriority::ActionDecision,
                    CancellationScope::LivePlayOnly,
                    DesEventPayload::DecisionReady {
                        player_id: cid,
                        side: self.possession_side,
                    },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }

        Ok(())
    }

    /// CRITICAL-01: Resolves an early mid-flight interaction (interception, out-of-bounds, or
    /// shot block) at its exact `hit_time_ms` AFTER the clock and world have advanced to `hit_time_ms`.
    fn handle_early_interaction_resolved(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn handle_ball_in_flight_arrival(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn commit_pending_flight(&mut self) -> Result<(), V4Error> {
        let Some(mut flight) = self.pending_flight.take() else { return Ok(()); };
        let now_ms = self.state.des_clock.sim_time_ms;
        if now_ms != flight.resolves_at_ms {
            return Err(V4Error::InvalidClock("flight resolved at incorrect timestamp".into()));
        }
        let launch_ms = flight.trajectory.start_time_ms;
        for ev in &mut flight.outcome.events {
            ev.metadata.extra.insert("launchSimTimeMs".into(), launch_ms.to_string());
            ev.metadata.extra.insert("plannedArrivalSimTimeMs".into(), flight.planned_arrival_ms.to_string());
            if matches!(&flight.action, FlightActionV4::Shot { .. }) {
                ev.metadata.extra.insert("shotLaunchSimTimeMs".into(), launch_ms.to_string());
            }
        }
        // Decide control at actual contact, using live fatigue and pressure,
        // not the cloned launch-state. A failed control leaves a real loose ball.
        let pass_contact = matches!(&flight.action, FlightActionV4::Pass { .. })
            && flight.early_interaction.as_ref().map_or(false, |h| h.kind == InteractionKind::PassInterception);
        if pass_contact {
            let h = flight.early_interaction.as_ref().expect("contact");
            let idx = flight.outcome.next_carrier_idx;
            let p = &self.state.players[idx];
            let attrs = *lookup_attrs(&self.input, p);
            let profile = pass_reach_candidate(&self.state, idx, h.ball_pos.z > 0.8);
            let incoming = flight.trajectory.velocity_at_ms(now_ms);
            let speed = (incoming.x * incoming.x + incoming.y * incoming.y + incoming.z * incoming.z).sqrt();
            let skill = normalize_attr(attrs.technical.first_touch) * 0.45
                + normalize_attr(attrs.technical.technique) * 0.30
                + normalize_attr(attrs.mental.composure) * 0.25;
            let fatigue = (p.fatigue.aerobic * 0.6 + p.fatigue.anaerobic * 0.4) / 100.0;
            let pressure = evaluate_local_context(&self.state, &self.input, p.side, h.ball_pos.xy()).pressure / 100.0;
            let chance = contact_control_probability(skill, normalize_attr(attrs.physical.balance),
                fatigue, speed, h.ball_pos.z, h.distance_m / profile.reach_m.max(0.1), pressure);
            let controlled = (self.rng.next_double() as f32) < chance;
            if !controlled {
                let origin_side = p.side.opposite();
                let defender_id = p.player_id.clone();
                let defender_side = p.side;
                let spot = h.ball_pos.clamp_to_pitch();
                let rebound = blocked_ball_velocity(incoming, p.pos, spot);
                flight.counter_delta.apply(&mut self.state)?;
                self.state.ball.pos = spot;
                self.state.ball.vel = rebound;
                self.state.ball.control = BallControlState::Loose;
                self.state.ball.owner_side = None;
                self.state.ball.owner_player_id = None;
                self.state.ball.last_touch_side = Some(defender_side);
                self.state.ball.last_touch_player_id = Some(defender_id.clone());
                for ev in &mut flight.outcome.events {
                    if ev.kind == EventTypeV4::PassFailed {
                        ev.metadata.extra.insert("pendingPassRecovery".into(), "true".into());
                        ev.metadata.extra.insert("contactOutcome".into(), "BLOCKED_LOOSE".into());
                    }
                }
                // Interception means controlled interception here, not merely contact.
                flight.outcome.events.retain(|ev| ev.kind != EventTypeV4::Interception);
                self.flush_play_events(flight.outcome.events, origin_side);
                self.emit(EventTypeV4::LooseBall, HighlightTier::None, None, Some(defender_side),
                    Some(defender_id), None, EventMetadataV4 {
                        cause: Some("PASS_CONTACT_LOOSE".into()),
                        ..EventMetadataV4::default()
                    });
                return self.start_pass_recovery(origin_side, flight.offside_at_launch, "PASS_CONTACT_LOOSE");
            }
            for ev in &mut flight.outcome.events {
                ev.metadata.extra.insert("contactOutcome".into(), "CONTROLLED".into());
            }
            if h.time_ms < flight.planned_arrival_ms {
                self.diagnostics.record_early_interception();
            }
        }
        if let FlightActionV4::LooseRecovery { origin_side, cause } = &flight.action {
            let origin_side = *origin_side;
            let cause = cause.clone();
            if let Some(hit) = flight.early_interaction.as_ref() {
                return self.finish_loose_ball_boundary(origin_side, &cause, hit);
            }
            let idx = flight.outcome.next_carrier_idx;
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                return self.start_pass_recovery(origin_side, flight.offside_at_launch, &cause);
            }
            return self.finish_pass_recovery(origin_side, idx, &flight.offside_at_launch, &cause);
        }
        if matches!(&flight.action, FlightActionV4::Pass { .. }) && flight.ball_after.control == BallControlState::Loose {
            let origin_side = flight.outcome.next_side;
            let heavy = flight.outcome.events.iter().any(|ev| ev.kind == EventTypeV4::FirstTouch
                && ev.metadata.cause.as_deref() == Some("FIRST_TOUCH_ERROR"));
            let cause = if heavy { "FIRST_TOUCH_LOOSE" } else { "PASS_MISS_LOOSE" };
            let offside = if heavy {
                let actor = flight.outcome.next_carrier_idx;
                self.state.players.iter().enumerate().filter(|(i,p)| *i != actor
                    && p.on_pitch && !p.sent_off && p.side == origin_side
                    && is_receiver_offside(&self.state, origin_side, flight.ball_after.pos.xy(), p.pos))
                    .map(|(_,p)|p.player_id.clone()).collect()
            } else { flight.offside_at_launch };
            flight.counter_delta.apply(&mut self.state)?;
            self.state.ball = flight.ball_after;
            self.flush_play_events(flight.outcome.events, origin_side);
            self.emit(EventTypeV4::LooseBall, HighlightTier::None, None,
                self.state.ball.last_touch_side, self.state.ball.last_touch_player_id.clone(), None,
                EventMetadataV4 { cause: Some(cause.into()), ..EventMetadataV4::default() });
            return self.start_pass_recovery(origin_side, offside, cause);
        }
        flight.counter_delta.apply(&mut self.state)?;
        self.state.ball = flight.ball_after;
        match flight.action {
            FlightActionV4::Pass { passer_idx, kind, distance_m, is_one_touch } => {
                let side = self.state.players[passer_idx].side;
                self.complete_pass_outcome(side, passer_idx, kind, distance_m, is_one_touch, flight.outcome)
            }
            FlightActionV4::LooseRecovery { .. } => unreachable!("handled before normal flight commit"),
            FlightActionV4::Shot { shooter_idx } => {
                let side = self.state.players[shooter_idx].side;
                let (next_side, next_carrier, dead, delay) = self.finalize_shot_outcome_des(side, flight.outcome, flight.won_corner)?;
                self.possession_side = next_side;
                self.carrier_idx = next_carrier;
                if !dead && self.state.dead_ball.is_none() {
                    let cid = self.state.players[next_carrier].player_id.clone();
                    self.scheduler.schedule(now_ms.saturating_add(delay.max(150)), EventPriority::ActionDecision,
                        CancellationScope::LivePlayOnly, DesEventPayload::DecisionReady { player_id: cid, side: next_side })
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                }
                Ok(())
            }
        }
    }


    /// Both teams pursue the actual rolling/descending ball.
    fn start_pass_recovery(&mut self, origin_side: TeamSideV4, offside_at_launch: Vec<String>, cause: &str) -> Result<(), V4Error> {
        let now = self.state.des_clock.sim_time_ms;
        let indices: Vec<_> = self.state.players.iter().enumerate()
            .filter(|(_,p)| p.on_pitch && !p.sent_off).map(|(i,_)| i).collect();
        let candidates: Vec<_> = indices.iter().map(|&i| pass_reach_candidate(&self.state,i,true)).collect();
        let trajectory = BallTrajectory { start_time_ms:now, start_pos:self.state.ball.pos,
            initial_vel:self.state.ball.vel, gravity_z:9.81, drag:1.6, is_shot:false,is_carrier_dribble:false };
        let resolution = find_moving_ball_recovery(&trajectory,&candidates)
            .ok_or_else(|| V4Error::InvalidClock("no active player can reach loose ball".into()))?;
        let (idx,at,motion,boundary) = match resolution {
            LooseBallResolution::Player{candidate_idx,time_ms,ball_pos} => (indices[candidate_idx],time_ms,
                Some(PassContactMotion{candidate:candidates[candidate_idx].clone(),target:ball_pos.xy(),started_at_ms:now}),None),
            LooseBallResolution::Boundary(h) => (self.carrier_idx,h.time_ms.max(now+1),None,Some(h)),
        };
        let side=self.state.players[idx].side;
        let cid=self.state.players[idx].player_id.clone();
        self.last_passer_idx=None;
        self.possession_side=origin_side;
        self.state.ball.control=BallControlState::Loose;
        self.state.ball.owner_side=None;
        self.state.ball.owner_player_id=None;
        let flight=PendingFlightSnapshotV4 {
            trajectory,planned_arrival_ms:at,resolves_at_ms:at,
            action:FlightActionV4::LooseRecovery{origin_side,cause:cause.into()},
            outcome:crate::v4::ball_play::BallPlayOutcome{next_side:side,next_carrier_idx:idx,stoppage_seconds:0,events:Vec::new()},
            ball_after:self.state.ball.clone(),counter_delta:FlightCounterDeltaV4::between(&self.state,&self.state),
            won_corner:false,early_interaction:boundary,pass_contact_motion:motion,offside_at_launch,
        };
        self.scheduler.schedule(at,EventPriority::PhysicsAndBoundary,CancellationScope::LivePlayOnly,
            DesEventPayload::EarlyInteractionResolved{interaction_tag:"MOVING_PASS_RECOVERY".into(),actor_id:Some(cid)})
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        self.pending_flight=Some(flight);
        Ok(())
    }

    #[cfg(test)]
    fn start_grounded_pass_recovery(&mut self, origin_side: TeamSideV4, offside_at_launch: Vec<String>) -> Result<(), V4Error> {
        self.start_pass_recovery(origin_side,offside_at_launch,"PASS_CONTACT_LOOSE")
    }

    fn finish_pass_recovery(&mut self, origin_side: TeamSideV4, idx: usize, offside_at_launch: &[String], cause: &str) -> Result<(), V4Error> {
        let side = self.state.players[idx].side;
        let id = self.state.players[idx].player_id.clone();
        let spot = self.state.ball.pos;
        let now = self.state.des_clock.sim_time_ms;
        // An accidental block does not reset the original Law 11 snapshot.
        if side == origin_side && offside_at_launch.contains(&id) {
            self.state.team_state_mut(side).offsides += 1;
            let mut meta = EventMetadataV4 { outcome: Some("OFFSIDE_INVOLVEMENT".into()), cause: Some(cause.into()), ..EventMetadataV4::default() };
            meta.extra.insert("passAlreadyCounted".into(), "true".into());
            self.emit(EventTypeV4::Offside, HighlightTier::Extended, None, Some(side), Some(id), None, meta);
            return self.open_dead_ball(DeadBallKind::Offside, side.opposite(), spot, false, false);
        }
        self.possession_side = side;
        self.carrier_idx = idx;
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(side);
        self.state.ball.owner_player_id = Some(id.clone());
        self.state.ball.last_touch_side = Some(side);
        self.state.ball.last_touch_player_id = Some(id.clone());
        self.state.ball.vel = Vec3::default();
        let mut meta = EventMetadataV4 { cause: Some(cause.into()), ..EventMetadataV4::default() };
        meta.extra.insert("recoveryOutcome".into(), if side == origin_side { "ATTACK_REGAINED".into() } else { "DEFENCE_WON".into() });
        self.emit(EventTypeV4::SecondBallWon, HighlightTier::None, None, Some(side), Some(id.clone()), None, meta);
        if cause=="TACKLE_CONTACT_LOOSE" {
            self.diagnostics.record_tackle_loose_recovery(side==origin_side);
        }
        if cause=="TACKLE_CONTACT_LOOSE" && side!=origin_side {
            let avg_wr=self.team_avg_work_rate(side);
            let ball=self.state.ball.pos;
            let phase_events=update_tactical_phases(&mut self.state,origin_side,side,ball,0,avg_wr);
            self.flush_play_events(phase_events,side);
        }
        let settle = self.input.timing_model.first_touch_settle_ms.sample_triangular(&mut self.rng, 1.0).max(150);
        self.scheduler.schedule(now.saturating_add(settle), EventPriority::BallStateTransition, CancellationScope::LivePlayOnly,
            DesEventPayload::FirstTouchComplete { player_id: id, side })
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        Ok(())
    }

    fn finish_loose_ball_boundary(&mut self, origin_side: TeamSideV4, cause: &str, hit: &EarlyInteractionHit) -> Result<(), V4Error> {
        if cause=="TACKLE_CONTACT_LOOSE" {self.diagnostics.record_tackle_loose_boundary();}
        let spot=hit.ball_pos.clamp_to_pitch();
        let last_side=self.state.ball.last_touch_side.unwrap_or(origin_side);
        let last_id=self.state.ball.last_touch_player_id.clone();
        let line=hit.boundary.ok_or_else(|| V4Error::InvalidClock("missing loose ball boundary".into()))?;
        let defender=match line { BoundaryKind::HomeGoalLine=>Some(TeamSideV4::Home),
            BoundaryKind::AwayGoalLine=>Some(TeamSideV4::Away), _=>None };
        self.state.ball.pos=spot;
        self.state.ball.vel=Vec3::default();
        if hit.kind==InteractionKind::GoalScored {
            let defender=defender.ok_or_else(|| V4Error::InvalidClock("goal outside goal line".into()))?;
            let beneficiary=defender.opposite();
            let actor=last_id.as_deref().and_then(|id|self.state.players.iter().position(|p|p.player_id==id))
                .ok_or_else(||V4Error::InvalidClock("goal without last touch".into()))?;
            let seq=self.alloc_highlight_sequence();
            if last_side==defender {
                let event=crate::v4::own_goal::award_own_goal(&mut self.state,actor,beneficiary,
                    if cause=="TACKLE_CONTACT_LOOSE" {crate::v4::own_goal::DeflectionCause::TackleContact}
                    else {crate::v4::own_goal::DeflectionCause::PassContact},seq);
                self.flush_play_events(vec![event],beneficiary);
            } else {
                self.state.team_state_mut(beneficiary).goals+=1;
                self.state.players[actor].goals=self.state.players[actor].goals.saturating_add(1);
                self.emit(EventTypeV4::Goal,HighlightTier::GoalKey,Some(seq),Some(beneficiary),last_id,None,
                    EventMetadataV4{scored:Some(true),cause:Some("LOOSE_BALL_GOAL".into()),..EventMetadataV4::default()});
            }
            shift_momentum(&mut self.state,beneficiary,0.35);
            return self.open_dead_ball(DeadBallKind::GoalCelebration,defender,Vec3::center_spot(),false,false);
        }
        let (kind,ev,restart_side,restart_spot)=match defender {
            None=>(DeadBallKind::ThrowIn,EventTypeV4::ThrowIn,last_side.opposite(),spot),
            Some(d) if last_side==d=>(DeadBallKind::Corner,EventTypeV4::Corner,d.opposite(),
                Vec3::ground(spot.x,if spot.y<34.0 {0.5} else {67.5})),
            Some(d)=>(DeadBallKind::GoalKick,EventTypeV4::GoalKick,d,
                Vec3::ground(if d==TeamSideV4::Home {5.5} else {PITCH_LENGTH_M-5.5},34.0)),
        };
        match kind { DeadBallKind::ThrowIn=>self.state.team_state_mut(restart_side).throw_ins+=1,
            DeadBallKind::Corner=>self.state.team_state_mut(restart_side).corners+=1,_=>{} }
        let mut meta=EventMetadataV4{cause:Some(cause.into()),..EventMetadataV4::default()};
        meta.extra.insert("looseRecoveryBoundary".into(),"true".into());
        meta.extra.insert("recoveryOutcome".into(),if restart_side==origin_side {"ATTACK_RESTART".into()} else {"DEFENCE_RESTART".into()});
        self.emit(ev,HighlightTier::None,None,Some(restart_side),None,None,meta);
        self.open_dead_ball(kind,restart_side,restart_spot,false,false)
    }

    fn install_flight(&mut self, flight: PendingFlightSnapshotV4, payload: DesEventPayload, actor_idx: usize) -> Result<(), V4Error> {
        let time = flight.resolves_at_ms;
        self.state.ball.pos = flight.trajectory.start_pos;
        self.state.ball.vel = flight.trajectory.initial_vel;
        self.state.ball.control = BallControlState::InFlight;
        self.state.ball.owner_side = None;
        self.state.ball.owner_player_id = None;
        self.state.ball.last_touch_side = Some(self.state.players[actor_idx].side);
        self.state.ball.last_touch_player_id = Some(self.state.players[actor_idx].player_id.clone());
        let payload = if let Some(hit) = flight.early_interaction.as_ref() {
            DesEventPayload::EarlyInteractionResolved {
                interaction_tag: format!("{:?}", hit.kind), actor_id: hit.defender_id.clone(),
            }
        } else { payload };
        self.scheduler.schedule(time, EventPriority::PhysicsAndBoundary, CancellationScope::LivePlayOnly, payload)
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        self.pending_flight = Some(flight);
        Ok(())
    }

    fn complete_pass_outcome(
        &mut self,
        att_side: TeamSideV4,
        passer_idx: usize,
        kind: PassKindV4,
        distance_m: f32,
        is_one_touch: bool,
        outcome: crate::v4::ball_play::BallPlayOutcome,
    ) -> Result<(), V4Error> {
        let def_side = att_side.opposite();
        let now_ms = self.state.des_clock.sim_time_ms;
        let retained = outcome.next_side == att_side;
        self.last_passer_idx = if retained { Some(passer_idx) } else { None };
        self.possession_side = outcome.next_side;
        self.carrier_idx = outcome.next_carrier_idx;

        let had_offside = outcome.events.iter().any(|e| e.kind == EventTypeV4::Offside);
        let had_throw_in = outcome.events.iter().any(|e| e.kind == EventTypeV4::ThrowIn);
        let had_goal_kick = outcome.events.iter().any(|e| e.kind == EventTypeV4::GoalKick);
        let last_pos = outcome
            .events
            .last()
            .map(|e| e.ball_pos)
            .unwrap_or(self.state.ball.pos);

        self.flush_play_events(outcome.events, att_side);

        if had_offside {
            self.open_dead_ball(DeadBallKind::Offside, def_side, last_pos, false, false)?;
            return Ok(());
        }
        if had_throw_in {
            self.open_dead_ball(DeadBallKind::ThrowIn, outcome.next_side, last_pos, false, false)?;
            return Ok(());
        }
        if had_goal_kick {
            self.open_dead_ball(DeadBallKind::GoalKick, outcome.next_side, last_pos, false, false)?;
            return Ok(());
        }

        if retained
            && kind == PassKindV4::Cross
            && is_in_attacking_penalty_box(att_side, self.state.players[outcome.next_carrier_idx].pos)
        {
            self.schedule_shot_flight(outcome.next_carrier_idx, Some(passer_idx), Some(ShotKindV4::Header))?;
            return Ok(());
        }

        let _ = (distance_m, is_one_touch, now_ms);
        // Receiver chooses its own intent immediately after the contact.
        // A controlling touch is included only if the new plan requires it.
        self.begin_player_decision(true)
    }

    /// CRITICAL-01: Player finishes first touch and schedules the next decision.
    fn handle_first_touch_complete(&mut self) -> Result<(), V4Error> {
        self.begin_player_decision(false)
    }

    /// CRITICAL-01: Schedules a shot in-flight so `Goal`/`Save`/`ShotOffTarget` occurs at
    /// `t_shot + shot_flight_ms` (or early `ShotBlocked` at `t_block`), never at `t_shot`.
    fn schedule_shot_flight(&mut self, shooter_idx: usize, assist_idx: Option<usize>, shot_kind: Option<ShotKindV4>) -> Result<(), V4Error> {
        let now = self.state.des_clock.sim_time_ms;
        let side = self.state.players[shooter_idx].side;
        let mut resolved = self.state.clone();
        let shot = resolve_shot_v4_des(&mut resolved, &self.input, &self.input.timing_model,
            shooter_idx, assist_idx, shot_kind, false, &mut self.rng);
        let delta = FlightCounterDeltaV4::between(&self.state, &resolved);
        self.state.next_highlight_sequence_id = resolved.next_highlight_sequence_id;
        let flight = PendingFlightSnapshotV4 {
            trajectory: shot.trajectory,
            planned_arrival_ms: now.saturating_add(shot.planned_flight_ms.max(1)),
            resolves_at_ms: now.saturating_add(shot.flight_ms.max(1)),
            action: FlightActionV4::Shot { shooter_idx },
            outcome: shot.outcome, ball_after: resolved.ball, counter_delta: delta,
            won_corner: shot.won_corner, early_interaction: shot.early_interaction,
            pass_contact_motion: None,
            offside_at_launch: Vec::new(),
        };
        let payload = DesEventPayload::ShotArrival { shooter_id: self.state.players[shooter_idx].player_id.clone(), side,
            target_y_mm: 34_000, target_z_mm: 1_100 };
        self.install_flight(flight, payload, shooter_idx)
    }

    fn handle_shot_arrival(&mut self) -> Result<(), V4Error> {
        self.commit_pending_flight()
    }

    fn step_live_action_des(
        &mut self,
        carrier_idx: usize,
        plan: DecisionPlanV4,
    ) -> Result<(TeamSideV4, usize, bool, i64), V4Error> {
        let att_side = self.state.players[carrier_idx].side;
        let def_side = att_side.opposite();
        let carrier_pos = self.state.players[carrier_idx].pos;
        let carrier_ctx = evaluate_local_context(&self.state, &self.input, att_side, carrier_pos);

        // Fouls during preparation now arise from a scheduled, reachable
        // contact. Merely being within 7.5m is not a tackle or a foul.

        match plan.action {
            PlannedActionV4::Pass {
                target_idx,
                kind,
                distance_m,
                progress_gain_m,
                lane_risk,
                interceptor_idx,
            } => {
                let now = self.state.des_clock.sim_time_ms;
                let is_one_touch = plan.is_one_touch;
                if is_one_touch { self.diagnostics.record_one_touch_pass(); }
                let mut resolved = self.state.clone();
                let pass = execute_pass_and_first_touch_des(&mut resolved, &self.input, &self.input.timing_model,
                    carrier_idx, target_idx, kind, distance_m, progress_gain_m, lane_risk, interceptor_idx,
                    carrier_ctx, is_one_touch, &mut self.rng);
                let delta = FlightCounterDeltaV4::between(&self.state, &resolved);
                self.state.next_highlight_sequence_id = resolved.next_highlight_sequence_id;
                let trajectory = pass.trajectory.ok_or_else(|| V4Error::InvalidClock("missing pass trajectory".into()))?;
                let contact_idx = if let Some(hit) = pass.early_interaction.as_ref() {
                    if hit.kind == InteractionKind::PassInterception {
                        hit.defender_id.as_deref().and_then(|id|
                            self.state.players.iter().position(|p| p.player_id == id))
                    } else { None }
                } else if pass.outcome.events.iter().any(|e| matches!(e.kind,
                    EventTypeV4::PassCompleted | EventTypeV4::ThroughBall | EventTypeV4::Cross | EventTypeV4::Offside)) {
                    Some(target_idx)
                } else { None };
                let pass_contact_motion = contact_idx.map(|idx| PassContactMotion {
                    candidate: pass_reach_candidate(&self.state, idx, kind.is_aerial()),
                    target: trajectory.position_at_ms(now.saturating_add(pass.elapsed_ms.max(1))).xy(),
                    started_at_ms: now,
                });
                let offside_at_launch = self.state.players.iter().filter(|p|
                    p.on_pitch && !p.sent_off && p.side == att_side
                    && is_receiver_offside(&self.state, att_side, self.state.players[carrier_idx].pos, p.pos))
                    .map(|p| p.player_id.clone()).collect();
                let flight = PendingFlightSnapshotV4 {
                    trajectory,
                    planned_arrival_ms: now.saturating_add(pass.planned_flight_ms.max(1)),
                    resolves_at_ms: now.saturating_add(pass.elapsed_ms.max(1)),
                    action: FlightActionV4::Pass { passer_idx: carrier_idx, kind, distance_m, is_one_touch },
                    outcome: pass.outcome, ball_after: resolved.ball, counter_delta: delta,
                    won_corner: false, early_interaction: pass.early_interaction,
                    pass_contact_motion, offside_at_launch,
                };
                let target = self.state.players[target_idx].pos;
                let payload = DesEventPayload::BallInFlightArrival {
                    passer_id: Some(self.state.players[carrier_idx].player_id.clone()),
                    receiver_id: Some(self.state.players[target_idx].player_id.clone()),
                    target_x_mm: (target.x * 1000.0).round() as i32, target_y_mm: (target.y * 1000.0).round() as i32,
                };
                self.install_flight(flight, payload, carrier_idx)?;

                Ok((att_side, carrier_idx, false, 0))
            }
            PlannedActionV4::Dribble { .. } => {
                let intent=plan.carry.ok_or_else(||V4Error::InvalidClock("PI dribble lacks carry intention".into()))?;
                self.start_carry_intent(carrier_idx,intent,false)?;
                Ok((att_side,carrier_idx,false,0))
            }
            PlannedActionV4::Shoot { .. } => {
                let assist = self.last_passer_idx;
                self.last_passer_idx = None;
                self.schedule_shot_flight(carrier_idx, assist, None)?;
                Ok((att_side, carrier_idx, false, 0))
            }
            PlannedActionV4::Clearance => {
                self.last_passer_idx = None;
                let carrier_id = self.state.players[carrier_idx].player_id.clone();
                let dir = match att_side {
                    TeamSideV4::Home => 1.0f32,
                    TeamSideV4::Away => -1.0f32,
                };
                let clear_pos = Vec3::new(
                    (carrier_pos.x + dir * 32.0).clamp(5.0, PITCH_LENGTH_M - 5.0),
                    (carrier_pos.y + (self.rng.next_double() as f32 - 0.5) * 18.0)
                        .clamp(0.0, 68.0),
                    4.5,
                );
                let flight_ms = estimate_ball_flight_ms(
                    &self.input.timing_model,
                    carrier_pos.distance(clear_pos.xy()),
                    BallFlightKind::Clearance,
                    self.input.match_environment.weather,
                    self.input.match_environment.pitch_quality,
                    &mut self.rng,
                );
                self.state.ball.pos = clear_pos;
                let g_sec = self.state.des_clock.game_second();
                let mut meta = EventMetadataV4 {
                    pressure: Some(carrier_ctx.pressure.round() as u8),
                    ..EventMetadataV4::default()
                };
                attach_tactical_flags(&mut meta, compute_tactical_flags(&self.state, att_side, g_sec));
                self.emit(
                    EventTypeV4::Clearance,
                    HighlightTier::None,
                    None,
                    Some(att_side),
                    Some(carrier_id),
                    None,
                    meta,
                );
                let next_idx = self.pick_outfield_index(def_side);
                let next_id = self.state.players[next_idx].player_id.clone();
                self.state.ball.control = BallControlState::Controlled;
                self.state.ball.owner_side = Some(def_side);
                self.state.ball.owner_player_id = Some(next_id);
                Ok((def_side, next_idx, false, flight_ms))
            }
            PlannedActionV4::HoldAndRecycle => {
                let intent=plan.carry.ok_or_else(||V4Error::InvalidClock("PI hold lacks carry intention".into()))?;
                self.start_carry_intent(carrier_idx,intent,true)?;
                Ok((att_side,carrier_idx,false,0))
            }
        }
    }

    #[cfg(test)]
    fn start_timed_dribble(&mut self,idx:usize)->Result<(),V4Error>{
        let dir=if self.state.players[idx].side==TeamSideV4::Home{1.0}else{-1.0};
        self.start_carry_intent(idx,CarryIntentV4{direction:Vec2::new(dir,0.0),duration_ms:1600,shielding:false},false)
    }

    fn start_carry_intent(&mut self,idx:usize,intent:CarryIntentV4,is_hold:bool)->Result<(),V4Error>{
        let now=self.state.des_clock.sim_time_ms;
        let mut candidate=pass_reach_candidate(&self.state,idx,false);
        candidate.speed_mps*=if intent.shielding{0.42}else{0.88};
        candidate.reaction_ms=0;
        let target=carry_target(candidate.pos,candidate.speed_mps,intent);
        let duration=intent.duration_ms.max(150);
        let end=now.saturating_add(duration);
        let side=candidate.side;
        let id=candidate.player_id.clone();
        let start_pressure=evaluate_local_context(&self.state,&self.input,side,candidate.pos).pressure;
        let motion=PassContactMotion{candidate,target,started_at_ms:now};
        let (at,challenger)=match self.choose_committed_tackle(idx,&motion,end) {
            Some((_,at,chase))=>(at,Some(chase)),None=>(end,None),
        };
        self.scheduler.schedule(at,EventPriority::ActionDecision,CancellationScope::LivePlayOnly,
            DesEventPayload::DribbleStep{carrier_id:id,side}).map_err(|e|V4Error::InvalidClock(e.into()))?;
        self.diagnostics.record_carry_start(is_hold,intent.shielding,intent.direction.y.abs()>0.1);
        self.pending_carry=Some(PendingCarryV4{carrier_idx:idx,resolves_at_ms:at,motion,shielding:intent.shielding,
            is_hold,challenger,start_pressure});
        Ok(())
    }

    fn resolve_audited_carry_contact(&mut self,idx:usize,contact:Option<usize>,
        ctx:crate::v4::movement::LocalSpatialContext,shielding:bool)->crate::v4::ball_play::BallPlayOutcome {
        // Evaluate against the SAME physical snapshot and same actual
        // contact, not all candidate commits or hypothetical reaches.
        // No forecast is used to change the resolved outcome.
        let audit=contact.map(|def_idx|{
            let pi=crate::player_intelligence::evaluate_tackle(&self.state,idx,def_idx,0.0);
            let resolver=crate::v4::ball_play::duel_contact_probabilities(&self.state,&self.input,idx,def_idx,ctx,shielding);
            (pi,resolver)
        });
        let out=resolve_carry_contact(&mut self.state,&self.input,idx,contact,ctx,shielding,&mut self.rng);
        if let Some((pi,resolver))=audit {
            let has=|kind|out.events.iter().any(|e|e.kind==kind);
            let missed_ball_body=out.events.iter().any(|e|matches!(e.kind,EventTypeV4::Foul|EventTypeV4::AdvantagePlayed)
                && e.metadata.extra.get("contactBranch").map(String::as_str)==Some("MISSED_BALL_BODY"));
            let loose=out.events.iter().any(|e|e.kind==EventTypeV4::LooseBall
                && e.metadata.cause.as_deref()==Some("TACKLE_CONTACT_LOOSE"));
            self.diagnostics.record_tackle_contact_audit(pi.clean_win_probability,pi.foul_probability,
                resolver,has(EventTypeV4::TackleWon)||loose,has(EventTypeV4::Foul),has(EventTypeV4::AdvantagePlayed),missed_ball_body);
            self.diagnostics.record_tackle_control_split(loose,has(EventTypeV4::TackleWon));
            self.diagnostics.record_tackle_second_ball_prior(pi.controlled_probability,pi.loose_probability,
                pi.recovery_given_loose,loose);
            if loose {
                let def_idx=contact.expect("loose tackle has a defender");
                let spot=self.state.ball.pos.xy();
                let vel=self.state.ball.vel;
                let speed=(vel.x*vel.x+vel.y*vel.y).sqrt();
                self.diagnostics.record_tackle_loose_kinematics(speed,
                    spot.distance(self.state.players[idx].pos),spot.distance(self.state.players[def_idx].pos));
            }
        }
        out
    }

    fn finish_timed_dribble(&mut self, player_id:&str, side:TeamSideV4) -> Result<(),V4Error> {
        let Some(carry)=self.pending_carry.take() else {return Ok(());};
        let idx=carry.carrier_idx;
        let now=self.state.des_clock.sim_time_ms;
        if carry.resolves_at_ms!=now || carry.motion.candidate.player_id!=player_id || carry.motion.candidate.side!=side {
            return Err(V4Error::InvalidClock("carry completion does not match pending motion".into()));
        }
        if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
            return self.begin_player_decision(false);
        }
        let pos=self.state.players[idx].pos;
        let moved=carry.motion.candidate.pos.distance(pos);
        let gain=if side==TeamSideV4::Home{pos.x-carry.motion.candidate.pos.x}else{carry.motion.candidate.pos.x-pos.x};
        let ctx=evaluate_local_context(&self.state,&self.input,side,pos);
        // World integration has already accounted for the entire carry.
        // The existing duel resolver is used only for its contact outcome
        // with zero displacement/time, not a second movement interval.
        let contact=carry.challenger.as_ref().and_then(|chase|
            self.state.players.iter().position(|p|p.player_id==chase.candidate.player_id
                && p.on_pitch && !p.sent_off && p.pos.distance(pos)<=chase.candidate.reach_m+0.01));
        if let Some(def_idx)=contact{
            self.mark_tackle_recovery(def_idx);
            self.diagnostics.record_carry_contact();
        }
        let mut outcome=self.resolve_audited_carry_contact(idx,contact,ctx,carry.shielding);
        self.diagnostics.record_carry_result(outcome.next_side!=side);
        let had_foul=outcome.events.iter().any(|e|e.kind==EventTypeV4::Foul);
        if had_foul {self.diagnostics.record_carry_foul();}
        let had_card=outcome.events.iter().any(|e|matches!(e.kind,EventTypeV4::YellowCard|EventTypeV4::RedCard));
        let had_injury=outcome.events.iter().any(|e|e.kind==EventTypeV4::Injury);
        for ev in &mut outcome.events {
            if ev.kind==EventTypeV4::DribbleWon {
                ev.metadata.progress_gain=Some(gain.round() as i32);
                ev.metadata.escaped_pressure=if carry.start_pressure>=45.0 && ctx.pressure<carry.start_pressure-15.0 {Some(true)}else{None};
            }
            ev.metadata.extra.insert("carryMode".into(),if carry.shielding{"SHIELD"}else if carry.is_hold{"HOLD"}else{"DRIBBLE"}.into());
            ev.metadata.extra.insert("carryDistanceM".into(),format!("{moved:.3}"));
        }
        if outcome.events.is_empty() {
            self.emit(EventTypeV4::Carry,HighlightTier::None,None,Some(side),Some(player_id.into()),None,
                EventMetadataV4{progress_gain:Some(gain.round() as i32),pressure:Some(ctx.pressure.round() as u8),
                    escaped_pressure:if carry.start_pressure>=45.0 && ctx.pressure<carry.start_pressure-15.0{Some(true)}else{None},
                    extra: [("carryMode".into(),if carry.shielding{"SHIELD"}else if carry.is_hold{"HOLD"}else{"DRIBBLE"}.into()),
                        ("carryDistanceM".into(),format!("{moved:.3}"))].into_iter().collect(),
                    ..EventMetadataV4::default()});
        }
        self.flush_play_events(outcome.events,side);
        if had_foul {
            let kind=if is_in_attacking_penalty_box(side,pos){DeadBallKind::PenaltyKick}else{DeadBallKind::DirectFreeKick};
            return self.open_dead_ball(kind,side,Vec3::ground(pos.x,pos.y),had_injury,had_card);
        }
        if self.state.ball.control==BallControlState::Loose {
            return self.start_pass_recovery(side,Vec::new(),"TACKLE_CONTACT_LOOSE");
        }
        if outcome.next_side!=side {
            self.last_passer_idx=None;
            let avg_wr=self.team_avg_work_rate(outcome.next_side);
            let ball=self.state.ball.pos;
            let phase_events=update_tactical_phases(&mut self.state,side,outcome.next_side,ball,0,avg_wr);
            self.flush_play_events(phase_events,outcome.next_side);
        }
        self.carrier_idx=outcome.next_carrier_idx;
        self.possession_side=outcome.next_side;
        self.begin_player_decision(false)
    }

    fn finalize_shot_outcome_des(
        &mut self,
        att_side: TeamSideV4,
        shot_out: crate::v4::ball_play::BallPlayOutcome,
        won_corner: bool,
    ) -> Result<(TeamSideV4, usize, bool, i64), V4Error> {
        let def_side = att_side.opposite();
        let scored = shot_out
            .events
            .iter()
            .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
        let had_goal_kick = shot_out.events.iter().any(|e| e.kind == EventTypeV4::GoalKick);
        let last_pos = shot_out
            .events
            .last()
            .map(|e| e.ball_pos)
            .unwrap_or(self.state.ball.pos);

        self.flush_play_events(shot_out.events, att_side);

        if scored {
            shift_momentum(&mut self.state, att_side, 0.35);
            // Strictly enter GoalCelebration lifecycle (Celebrating -> ReturningToHalf -> KickoffSetup -> ExecutingRestart).
            self.open_dead_ball(
                DeadBallKind::GoalCelebration,
                def_side,
                Vec3::center_spot(),
                false,
                false,
            )?;
            return Ok((def_side, shot_out.next_carrier_idx, true, 0));
        }

        if won_corner {
            let corner_x = match att_side {
                TeamSideV4::Home => PITCH_LENGTH_M,
                TeamSideV4::Away => 0.0,
            };
            let corner_spot = Vec3::new(corner_x, if self.rng.next_boolean() { 0.5 } else { 67.5 }, 0.0);
            self.open_dead_ball(DeadBallKind::Corner, att_side, corner_spot, false, false)?;
            return Ok((att_side, shot_out.next_carrier_idx, true, 0));
        }

        if had_goal_kick {
            self.open_dead_ball(DeadBallKind::GoalKick, def_side, last_pos, false, false)?;
            return Ok((def_side, shot_out.next_carrier_idx, true, 0));
        }

        Ok((
            shot_out.next_side,
            shot_out.next_carrier_idx,
            false,
            900,
        ))
    }
}
