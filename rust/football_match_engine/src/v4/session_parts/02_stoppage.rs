// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    /// Proportionally credits elapsed compensable stoppage time if a dead-ball phase is active
    /// when added time is announced or period-end is checked, preventing lost stoppage without double counting.
    fn credit_ongoing_dead_ball_compensable_ms(&mut self) {
        let now_ms = self.state.des_clock.sim_time_ms;
        let credit = if let Some(db) = self.state.dead_ball.as_mut() {
            if db.phase_compensable_ms > 0 && db.phase_ends_at_ms > db.phase_started_at_ms {
                let total = (db.phase_ends_at_ms - db.phase_started_at_ms).max(1);
                let elapsed = now_ms.saturating_sub(db.phase_started_at_ms).clamp(0, total);
                let accrued = ((db.phase_compensable_ms as i128 * elapsed as i128) / total as i128) as i64;
                let take = accrued.saturating_sub(db.phase_compensable_credited_ms).max(0);
                db.phase_compensable_credited_ms = db.phase_compensable_credited_ms.saturating_add(take);
                take
            } else {
                0
            }
        } else {
            0
        };
        if credit > 0 {
            self.state.des_clock.record_compensable_lost_ms(credit);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();
        }
    }

    fn handle_announce_added_time(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        self.credit_ongoing_dead_ball_compensable_ms();
        if !self.state.des_clock.added_time_locked {
            let rounded_ms = self
                .ruleset
                .round_announced_added_time_ms(self.state.des_clock.compensable_lost_ms);
            let announced_ms = self.state.des_clock.lock_announced_added_ms(rounded_ms);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();

            if announced_ms > 0 {
                let mut meta = EventMetadataV4::default();
                meta.extra
                    .insert("addedSeconds".into(), (announced_ms / 1000).to_string());
                meta.extra
                    .insert("addedMs".into(), announced_ms.to_string());
                self.emit(
                    EventTypeV4::AddedTimeAnnounced,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    meta,
                );
            }
        }

        let target_ms = self
            .state
            .des_clock
            .period_started_at_ms
            .saturating_add(self.state.des_clock.period_target_ms());
        let now_ms = self.state.des_clock.sim_time_ms;
        if target_ms <= now_ms {
            self.handle_check_period_end(period, auto_advance_breaks)?;
        } else {
            self.scheduler
                .schedule(
                    target_ms,
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }
        Ok(())
    }

    fn handle_check_period_end(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        self.credit_ongoing_dead_ball_compensable_ms();
        // Dynamically read `effective_added_ms()` (no static added_target lock!).
        let dynamic_target_elapsed = self.state.des_clock.period_target_ms();
        let elapsed_now = self.state.des_clock.period_elapsed_ms;
        let now_ms = self.state.des_clock.sim_time_ms;

        if elapsed_now < dynamic_target_elapsed {
            let next_check_ms = self
                .state
                .des_clock
                .period_started_at_ms
                .saturating_add(dynamic_target_elapsed);
            self.scheduler
                .schedule(
                    next_check_ms.max(now_ms.saturating_add(1)),
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
            return Ok(());
        }

        // Law 7: never blow whistle during an awarded penalty kick, or an active dangerous attack
        // under `HalfEndPolicy::AllowAttackCompletion` (bounded by watchdog).
        let penalty_active = matches!(
            self.state.dead_ball.as_ref().map(|d| d.kind),
            Some(DeadBallKind::PenaltyKick)
        );
        let dangerous_attack = self.ruleset.half_end_policy == HalfEndPolicy::AllowAttackCompletion
            && self.state.dead_ball.is_none()
            && is_in_attacking_penalty_box(self.possession_side, self.state.ball.pos.xy())
            && elapsed_now < dynamic_target_elapsed.saturating_add(15_000);

        if penalty_active || dangerous_attack {
            let defer_ms = now_ms.saturating_add(3_000);
            self.scheduler
                .schedule(
                    defer_ms,
                    EventPriority::PeriodBoundary,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::CheckPeriodEnd { period },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
            return Ok(());
        }

        self.complete_current_half(period, auto_advance_breaks)
    }

    fn handle_tactical_command_effective(&mut self, command_index: usize, side: TeamSideV4) {
        let Some(entry) = self.pending_tactical_commands.get_mut(command_index) else {
            return;
        };
        if entry.status != CommandLifecycleStatus::Scheduled || entry.side != side {
            return;
        }
        let now_ms = self.state.des_clock.sim_time_ms;
        let cmd_id = entry.command_id;
        let instructions = entry.instructions.clone();
        entry.status = CommandLifecycleStatus::Applied;
        entry.actual_effective_sim_time_ms = Some(now_ms);

        for rec in &mut self.command_timeline {
            if rec.command_id == cmd_id {
                rec.status = CommandLifecycleStatus::Applied;
                rec.actual_effective_sim_time_ms = Some(now_ms);
            }
        }

        self.apply_tactical_instructions_now(side, instructions, "USER_TACTICS_UPDATE");
    }

    fn apply_tactical_instructions_now(
        &mut self,
        side: TeamSideV4,
        instructions: TeamInstructionsV4,
        cause: &str,
    ) {
        let high_line = instructions.line_height >= 68;
        let wasting = instructions.time_wasting >= 55;
        *self.state.active_instructions_mut(side) = instructions;
        self.tactical_intents=None;
        let ball_pos = self.state.ball.pos;
        self.emit(
            EventTypeV4::TacticalPhaseChanged,
            HighlightTier::None,
            None,
            Some(side),
            None,
            None,
            EventMetadataV4 {
                high_defensive_line: if high_line { Some(true) } else { None },
                time_wasting: if wasting { Some(true) } else { None },
                cause: Some(cause.into()),
                ..EventMetadataV4::default()
            },
        );
        self.state.ball.pos = ball_pos;
    }

    fn handle_ai_manager_check(&mut self, period: Period) -> Result<(), V4Error> {
        let now_ms = self.state.des_clock.sim_time_ms;
        self.state.last_ai_eval_second = (now_ms / 1000) as i32;

        // Only apply AI tactical/substitution decisions when not mid-celebration;
        // AI substitutions during live play are routed cleanly through dead-ball or immediate AI hook.
        let ai_events = evaluate_ai_managers(&mut self.state, &self.input);
        if !ai_events.is_empty() {
            let poss = self.possession_side;
            self.flush_play_events(ai_events, poss);
            self.ensure_valid_carrier();
        }

        // Schedule next periodic AI manager evaluation (+300_000 ms) if still within period.
        let period_end_est = self
            .state
            .des_clock
            .period_started_at_ms
            .saturating_add(self.state.des_clock.period_target_ms());
        let next_ai_ms = now_ms.saturating_add(300_000);
        if next_ai_ms < period_end_est {
            let _ = self.scheduler.schedule(
                next_ai_ms,
                EventPriority::TacticalAndCommand,
                CancellationScope::PeriodScoped,
                DesEventPayload::AiManagerCheck {
                    side: TeamSideV4::Home,
                },
            );
        }
        let _ = period;
        Ok(())
    }

    fn open_dead_ball(
        &mut self,
        kind: DeadBallKind,
        restart_side: TeamSideV4,
        restart_spot: Vec3,
        has_injury: bool,
        has_discipline_review: bool,
    ) -> Result<(), V4Error> {
        self.scheduler.invalidate_live_play();

        let now_ms = self.state.des_clock.sim_time_ms;
        let has_pending_subs = self.has_pending_substitutions();
        let initial_phase = initial_dead_ball_phase(
            kind,
            has_injury,
            has_discipline_review,
            has_pending_subs,
        );
        let pending_count = self.count_pending_substitutions();
        let goal_diff = self.goal_diff_for(restart_side);
        let tw = self.state.active_instructions(restart_side).time_wasting;
        let ref_strict = self.input.referee_profile.strictness;

        let (phase_dur_ms, comp_ms) = estimate_dead_ball_phase_ms(
            &self.input.timing_model,
            kind,
            initial_phase,
            goal_diff,
            now_ms,
            tw,
            ref_strict,
            pending_count,
            &mut self.rng,
        );

        let restart_spot=if self.uses_locomotion_experiment() && kind==DeadBallKind::PenaltyKick {
            let p=crate::v4::spatial::penalty_spot(restart_side);
            Vec3::ground(p.x,p.y)
        }else{restart_spot};
        let db = DeadBallState::new(
            kind,
            initial_phase,
            restart_side,
            restart_spot.clamp_to_pitch(),
            now_ms,
            phase_dur_ms,
            comp_ms,
        );
        let phase_end_ms = db.phase_ends_at_ms;
        self.state.ball.pos = db.restart_spot;
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::DeadBall;
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.tactical_intents=None;
        self.state.dead_ball = Some(db);

        self.scheduler
            .schedule(
                phase_end_ms,
                EventPriority::DeadBallAndSubstitution,
                CancellationScope::PeriodScoped,
                DesEventPayload::DeadBallPhaseComplete {
                    phase_tag: format!("{:?}::{:?}", kind, initial_phase),
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        Ok(())
    }

    /// Deterministic eligible taker for ordinary restarts; kickoff retains
    /// the existing designated striker. Do not select a random distant actor
    /// after setup. Specialist restarts share the legacy designated-taker selector.
    fn experimental_restart_actor(&self, db: &DeadBallState) -> Option<usize> {
        match db.kind {
            DeadBallKind::GoalCelebration | DeadBallKind::Kickoff =>
                Some(self.kickoff_carrier_index(db.restart_side)),
            DeadBallKind::GoalKick => Some(self.find_goalkeeper_index(db.restart_side)),
            DeadBallKind::ThrowIn | DeadBallKind::Offside | DeadBallKind::InjuryStoppage
                | DeadBallKind::SubstitutionWindow =>
                self.state.players.iter().enumerate().filter(|(_,p)|
                    p.side==db.restart_side && p.on_pitch && !p.sent_off && !p.is_goalkeeper)
                    .min_by(|(_,a),(_,b)|a.pos.distance(db.restart_spot.xy())
                        .total_cmp(&b.pos.distance(db.restart_spot.xy()))
                        .then_with(||a.player_id.cmp(&b.player_id))).map(|(idx,_)|idx),
            DeadBallKind::Corner | DeadBallKind::DirectFreeKick | DeadBallKind::IndirectFreeKick
                | DeadBallKind::QuickFreeKick | DeadBallKind::PenaltyKick => {
                let side=db.restart_side;
                let plan=match side {
                    TeamSideV4::Home=>&self.input.home_team.set_piece_plan,
                    TeamSideV4::Away=>&self.input.away_team.set_piece_plan,
                };
                let (slots,metric):(&[String],fn(&crate::v4::attributes::PlayerAttributes54)->u8)=match db.kind {
                    DeadBallKind::Corner=>(&plan.corner_taker_slot_ids,|a|a.technical.corners),
                    DeadBallKind::PenaltyKick=>(&plan.penalty_taker_slot_ids,|a|a.technical.penalties),
                    _=>(
                        if distance_to_attacking_goal(side,db.restart_spot.xy())<=28.0 {
                            &plan.close_free_kick_taker_slot_ids
                        }else{&plan.far_free_kick_taker_slot_ids},
                        |a|a.technical.free_kicks),
                };
                Some(crate::v4::set_pieces::pick_planned_or_best_specialist(
                    &self.state,&self.input,side,slots,metric))
            }
        }
    }

    /// Return explicit legal-side setup targets, not replacement positions.
    /// Margins are provisional player-center/ball-control tolerances, not
    /// a complete Law 8/15/16 body geometry implementation.
    fn experimental_restart_targets(&self,db:&DeadBallState)->Vec<Option<(Vec2,f32)>> {
        let mut targets=vec![None;self.state.players.len()];
        let Some(taker)=self.experimental_restart_actor(db) else{return targets;};
        let kickoff=matches!(db.kind,DeadBallKind::GoalCelebration|DeadBallKind::Kickoff);
        if kickoff && db.phase==DeadBallPhase::Celebrating {return targets;}
        let center=Vec3::center_spot().xy();
        for (idx,p) in self.state.players.iter().enumerate(){
            if !p.on_pitch || p.sent_off {continue;}
            if idx==taker {
                let target=if kickoff {
                    Vec2::new(center.x+if p.side==TeamSideV4::Home{-0.3}else{0.3},center.y)
                }else{
                    let spot=db.restart_spot.xy();
                    Vec2::new(spot.x.clamp(0.3,PITCH_LENGTH_M-0.3),spot.y.clamp(0.3,67.7))
                };
                targets[idx]=Some((target,3.5));
            }else if kickoff {
                // Move only as far as necessary to the own half, leaving
                // opposing players outside the center circle. No forced
                // return to a formation slot or post-goal velocity reset.
                let own_x=if p.side==TeamSideV4::Home {p.pos.x.min(center.x-1.0)}
                    else {p.pos.x.max(center.x+1.0)};
                let mut target=Vec2::new(own_x,p.pos.y.clamp(1.0,67.0));
                if p.side!=db.restart_side && target.distance(center)<10.0 {
                    target.x=if p.side==TeamSideV4::Home {center.x-10.0}else{center.x+10.0};
                }
                targets[idx]=Some((target,3.5));
            }
        }
        targets
    }

    fn experimental_restart_ready(&self,db:&DeadBallState)->Result<bool,V4Error>{
        let Some(taker)=self.experimental_restart_actor(db) else {
            return Ok(!matches!(db.kind,DeadBallKind::ThrowIn|DeadBallKind::Offside
                |DeadBallKind::InjuryStoppage|DeadBallKind::SubstitutionWindow));
        };
        let p=self.state.players.get(taker).ok_or_else(||V4Error::InvalidInput("restart taker missing".into()))?;
        if p.side!=db.restart_side || !p.on_pitch || p.sent_off {
            return Err(V4Error::InvalidInput("restart taker inactive".into()));
        }
        let kickoff=matches!(db.kind,DeadBallKind::GoalCelebration|DeadBallKind::Kickoff);
        let spot=if kickoff{Vec3::center_spot().xy()}else{db.restart_spot.xy()};
        if p.pos.distance(spot)>0.75 || p.vel.distance(Vec2::default())>0.8 {return Ok(false);}
        if kickoff {
            for (idx,p) in self.state.players.iter().enumerate(){
                if idx==taker || !p.on_pitch || p.sent_off {continue;}
                let own=if p.side==TeamSideV4::Home{p.pos.x<=spot.x}else{p.pos.x>=spot.x};
                if !own || (p.side!=db.restart_side && p.pos.distance(spot)<9.15){return Ok(false);}
            }
        }
        Ok(true)
    }

    /// Extend the existing dead-ball phase by a distance-derived approach
    /// interval, then re-check actual positions. Never snap at the deadline.
    fn defer_experimental_restart(&mut self,db:&DeadBallState)->Result<(),V4Error>{
        let now=self.state.des_clock.sim_time_ms;
        if now-db.started_at_ms>120_000 {
            return Err(V4Error::InvalidClock("restart positioning exceeded 120s watchdog".into()));
        }
        let targets=self.experimental_restart_targets(db);
        let distance=targets.iter().zip(&self.state.players)
            .filter_map(|(t,p)|t.map(|(target,_)|p.pos.distance(target)))
            .fold(0.0f32,f32::max);
        let duration=((distance/3.5+2.0)*1000.0).ceil() as i64;
        let duration=duration.clamp(1000,30_000);
        let active=self.state.dead_ball.as_mut().ok_or_else(||V4Error::InvalidClock("missing restart phase".into()))?;
        active.advance_to(DeadBallPhase::ExecutingRestart,now,duration,duration);
        self.scheduler.schedule(active.phase_ends_at_ms,EventPriority::DeadBallAndSubstitution,
            CancellationScope::PeriodScoped,DesEventPayload::DeadBallPhaseComplete{
                phase_tag:format!("{:?}::ExecutingRestart",db.kind)})
            .map_err(|e|V4Error::InvalidClock(e.into()))?;
        Ok(())
    }

    fn handle_dead_ball_phase_complete(&mut self, dt_ms: i64) -> Result<(), V4Error> {
        let Some(db_snapshot) = self.state.dead_ball.clone() else {
            return Ok(());
        };
        let _ = dt_ms;
        let now_ms = self.state.des_clock.sim_time_ms;

        // Record compensable lost time accrued in this completed phase now that
        // `advance_ms(dt_ms, false)` has added the phase duration to `ball_out_of_play_ms`.
        let prev_comp_ms = db_snapshot.phase_compensable_ms.saturating_sub(db_snapshot.phase_compensable_credited_ms);
        if prev_comp_ms > 0 {
            self.state.des_clock.record_compensable_lost_ms(prev_comp_ms);
            self.state.sync_legacy_clock();
            self.sync_stage_from_clock();
        }

        // Execute queued substitutions upon completing `ProcessingSubstitutions`.
        if db_snapshot.phase == DeadBallPhase::ProcessingSubstitutions {
            self.flush_pending_substitutions();
        }

        let has_pending_subs = self.has_pending_substitutions();
        let next_phase = next_dead_ball_phase(&db_snapshot, has_pending_subs, false, false);

        if let Some(next_p) = next_phase {
            let pending_count = self.count_pending_substitutions();
            let goal_diff = self.goal_diff_for(db_snapshot.restart_side);
            let tw = self
                .state
                .active_instructions(db_snapshot.restart_side)
                .time_wasting;
            let ref_strict = self.input.referee_profile.strictness;

            let (next_dur_ms, next_comp_ms) = estimate_dead_ball_phase_ms(
                &self.input.timing_model,
                db_snapshot.kind,
                next_p,
                goal_diff,
                now_ms,
                tw,
                ref_strict,
                pending_count,
                &mut self.rng,
            );

            if let Some(db) = self.state.dead_ball.as_mut() {
                db.advance_to(next_p, now_ms, next_dur_ms, next_comp_ms);
                let phase_end_ms = db.phase_ends_at_ms;
                let kind = db.kind;
                self.scheduler
                    .schedule(
                        phase_end_ms,
                        EventPriority::DeadBallAndSubstitution,
                        CancellationScope::PeriodScoped,
                        DesEventPayload::DeadBallPhaseComplete {
                            phase_tag: format!("{:?}::{:?}", kind, next_p),
                        },
                    )
                    .map_err(|e| V4Error::InvalidClock(e.into()))?;
            }
            return Ok(());
        }

        if self.uses_locomotion_experiment() && !self.experimental_restart_ready(&db_snapshot)? {
            return self.defer_experimental_restart(&db_snapshot);
        }

        // Dead ball completed! Record total duration diagnostics & compensable lost time
        // for the initial phase if not yet counted.
        let total_db_ms = now_ms.saturating_sub(db_snapshot.started_at_ms).max(0);
        if let Some(diag_kind) = map_dead_ball_diag_kind(db_snapshot.kind) {
            self.diagnostics
                .record_dead_ball_duration(diag_kind, total_db_ms);
        }
        // Clear dead ball and execute the actual restart.
        self.state.dead_ball = None;
        self.execute_dead_ball_restart(db_snapshot)?;
        Ok(())
    }

    fn execute_dead_ball_restart(&mut self, db: DeadBallState) -> Result<(), V4Error> {
        let now_ms = self.state.des_clock.sim_time_ms;
        let restart_side = db.restart_side;
        let def_side = restart_side.opposite();

        if self.uses_locomotion_experiment() &&
            (db.kind==DeadBallKind::Corner ||
                (matches!(db.kind,DeadBallKind::DirectFreeKick|DeadBallKind::IndirectFreeKick|DeadBallKind::QuickFreeKick)
                && (db.kind==DeadBallKind::IndirectFreeKick || distance_to_attacking_goal(restart_side,db.restart_spot.xy())>28.0))) {
            let taker=self.experimental_restart_actor(&db)
                .ok_or_else(||V4Error::InvalidInput("missing delivery taker".into()))?;
            return self.schedule_experimental_set_piece_delivery(&db,taker);
        }
        match db.kind {
            DeadBallKind::GoalCelebration | DeadBallKind::Kickoff => {
                let kicker_idx = self.restart_after_goal_kickoff(restart_side);
                self.schedule_next_decision(kicker_idx, restart_side, now_ms, false)?;
            }
            DeadBallKind::Corner => {
                let corner_out =
                    execute_corner_v4(&mut self.state, &self.input, restart_side, &mut self.rng);
                let scored = corner_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(corner_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = corner_out.next_side;
                    self.carrier_idx = corner_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    self.schedule_next_decision(self.carrier_idx, self.possession_side, now_ms, false)?;
                }
            }
            DeadBallKind::PenaltyKick => {
                let sp_out = execute_penalty_kick_v4_with_rules(
                    &mut self.state,
                    &self.input,
                    restart_side,
                    self.ruleset.allow_penalty_rebound,
                    &mut self.rng,
                );
                let scored = sp_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(sp_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = sp_out.next_side;
                    self.carrier_idx = sp_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    let one_touch = self.state.ball.control == BallControlState::Loose;
                    if self.uses_locomotion_experiment() && one_touch {
                        self.state.ball.vel=Vec3::default();
                        return self.start_pass_recovery(restart_side,Vec::new(),"PENALTY_REBOUND_LOOSE");
                    }
                    self.schedule_next_decision(
                        self.carrier_idx,
                        self.possession_side,
                        now_ms,
                        one_touch,
                    )?;
                }
            }
            DeadBallKind::DirectFreeKick
            | DeadBallKind::IndirectFreeKick
            | DeadBallKind::QuickFreeKick => {
                let sp_out = execute_free_kick_v4(
                    &mut self.state,
                    &self.input,
                    restart_side,
                    db.restart_spot.xy(),
                    &mut self.rng,
                );
                let scored = sp_out
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                self.flush_play_events(sp_out.events, restart_side);
                if scored {
                    shift_momentum(&mut self.state, restart_side, 0.35);
                    self.open_dead_ball(
                        DeadBallKind::GoalCelebration,
                        def_side,
                        Vec3::center_spot(),
                        false,
                        false,
                    )?;
                } else {
                    self.possession_side = sp_out.next_side;
                    self.carrier_idx = sp_out.next_carrier_idx;
                    self.ensure_valid_carrier();
                    self.schedule_next_decision(self.carrier_idx, self.possession_side, now_ms, false)?;
                }
            }
            DeadBallKind::ThrowIn
            | DeadBallKind::GoalKick
            | DeadBallKind::Offside
            | DeadBallKind::InjuryStoppage
            | DeadBallKind::SubstitutionWindow => {
                self.possession_side = restart_side;
                self.carrier_idx = if self.uses_locomotion_experiment() {
                    self.experimental_restart_actor(&db)
                        .ok_or_else(||V4Error::InvalidInput("no eligible ordinary restart taker".into()))?
                } else if db.kind == DeadBallKind::GoalKick {
                    self.find_goalkeeper_index(restart_side)
                } else {
                    self.pick_outfield_index(restart_side)
                };
                self.ensure_valid_carrier();
                let cid = self.state.players[self.carrier_idx].player_id.clone();
                self.state.ball.pos = db.restart_spot.clamp_to_pitch();
                self.state.ball.control = BallControlState::Controlled;
                self.state.ball.owner_side = Some(restart_side);
                self.state.ball.owner_player_id = Some(cid);
                self.schedule_next_decision(self.carrier_idx, restart_side, now_ms, false)?;
            }
        }
        Ok(())
    }

    fn flush_pending_substitutions(&mut self) {
        let now_ms = self.state.des_clock.sim_time_ms;
        for side in [TeamSideV4::Home, TeamSideV4::Away] {
            let mut pairs: Vec<(String, String)> = Vec::new();
            let mut cmd_ids: Vec<u64> = Vec::new();
            for item in &self.pending_substitutions {
                if item.side == side && item.status == CommandLifecycleStatus::QueuedForDeadBall {
                    pairs.push((item.player_out_id.clone(), item.player_in_id.clone()));
                    cmd_ids.push(item.command_id);
                }
            }
            if pairs.is_empty() {
                continue;
            }
            match apply_manual_substitution_group(&mut self.state, &self.input, side, &pairs, false)
            {
                Ok(events) => {
                    let poss = self.possession_side;
                    self.flush_play_events(events, poss);
                    self.ensure_valid_carrier();
                    for item in &mut self.pending_substitutions {
                        if item.side == side
                            && item.status == CommandLifecycleStatus::QueuedForDeadBall
                        {
                            item.status = CommandLifecycleStatus::Applied;
                        }
                    }
                    for rec in &mut self.command_timeline {
                        if cmd_ids.contains(&rec.command_id) {
                            rec.status = CommandLifecycleStatus::Applied;
                            rec.actual_effective_sim_time_ms = Some(now_ms);
                        }
                    }
                }
                Err(_) => {
                    for item in &mut self.pending_substitutions {
                        if item.side == side
                            && item.status == CommandLifecycleStatus::QueuedForDeadBall
                        {
                            item.status = CommandLifecycleStatus::Rejected;
                        }
                    }
                    for rec in &mut self.command_timeline {
                        if cmd_ids.contains(&rec.command_id) {
                            rec.status = CommandLifecycleStatus::Rejected;
                        }
                    }
                }
            }
        }
    }
}
