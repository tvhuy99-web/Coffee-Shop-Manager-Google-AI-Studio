// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    fn complete_current_half(
        &mut self,
        period: Period,
        auto_advance_breaks: bool,
    ) -> Result<(), V4Error> {
        let elapsed_ms = self.state.des_clock.period_elapsed_ms;
        self.scheduler.invalidate_period();
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.state.dead_ball = None;

        match period {
            Period::FirstHalf => {
                self.first_half_elapsed_ms = elapsed_ms;
                self.emit(
                    EventTypeV4::HalfTime,
                    HighlightTier::Extended,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.stage = HalfStageV4::HalfTimeBreak;
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
            }
            Period::SecondHalf => {
                self.second_half_elapsed_ms = elapsed_ms;
                if self.ruleset.extra_time_enabled
                    && self.state.home.goals == self.state.away.goals
                {
                    self.stage = HalfStageV4::ExtraTimeBreak {
                        next_period: Period::ExtraTimeFirst,
                    };
                    if auto_advance_breaks {
                        self.begin_next_period_internal()?;
                    }
                } else {
                    self.finish_full_time();
                }
            }
            Period::ExtraTimeFirst => {
                self.et1_elapsed_ms = elapsed_ms;
                self.emit(
                    EventTypeV4::ExtraTimeHalfTime,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.stage = HalfStageV4::ExtraTimeBreak {
                    next_period: Period::ExtraTimeSecond,
                };
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
            }
            Period::ExtraTimeSecond => {
                self.et2_elapsed_ms = elapsed_ms;
                if self.ruleset.penalty_shootout_enabled
                    && self.state.home.goals == self.state.away.goals
                {
                    self.stage = HalfStageV4::PenaltyShootout;
                    self.run_penalty_shootout()?;
                }
                self.finish_full_time();
            }
            _ => {
                self.finish_full_time();
            }
        }
        Ok(())
    }

    fn finish_full_time(&mut self) {
        self.state.des_clock.period = Period::FullTime;
        self.state.sync_legacy_clock();
        self.emit(
            EventTypeV4::FullTime,
            HighlightTier::Extended,
            None,
            None,
            None,
            None,
            EventMetadataV4::default(),
        );
        self.stage = HalfStageV4::FullTime;
    }

    fn schedule_next_decision(
        &mut self,
        carrier_idx: usize,
        side: TeamSideV4,
        now_ms: i64,
        one_touch: bool,
    ) -> Result<(), V4Error> {
        let cid = self.state.players[carrier_idx].player_id.clone();
        let delay_ms: i64 = 1;
        let _ = one_touch; // actual intent belongs to the upcoming PI observation
        self.scheduler
            .schedule(
                now_ms.saturating_add(delay_ms),
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DecisionReady {
                    player_id: cid,
                    side,
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;
        Ok(())
    }

    fn flush_play_events(&mut self, events: Vec<EmittedPlayEvent>, att_side: TeamSideV4) {
        let g_sec = self.state.des_clock.game_second();
        let flags = compute_tactical_flags(&self.state, att_side, g_sec);
        let actual_ball_pos = self.state.ball.pos;
        for mut ev in events {
            attach_tactical_flags(&mut ev.metadata, flags);
            self.state.ball.pos = ev.ball_pos.clamp_to_pitch();
            self.emit(
                ev.kind,
                ev.tier,
                ev.seq_id,
                ev.side,
                ev.actor,
                ev.target,
                ev.metadata,
            );
        }
        self.state.ball.pos = actual_ball_pos;
    }

    /// Law 10: snapshot eligible on-pitch players, equalize team counts, and run shootout rounds.
    fn run_penalty_shootout(&mut self) -> Result<(), V4Error> {
        self.went_to_shootout = true;
        self.state
            .des_clock
            .begin_period(Period::Shootout, 0);
        self.state.sync_legacy_clock();
        let seq_id = self.alloc_highlight_sequence();
        self.emit(
            EventTypeV4::PenaltyShootoutStarted,
            HighlightTier::GoalKey,
            Some(seq_id),
            None,
            None,
            None,
            EventMetadataV4::default(),
        );

        let mut home_order = self.shootout_eligible_order(TeamSideV4::Home);
        let mut away_order = self.shootout_eligible_order(TeamSideV4::Away);
        let equal_count = home_order.len().min(away_order.len());
        if equal_count == 0 {
            return Err(V4Error::InvalidInput("no eligible penalty kickers".into()));
        }
        home_order.truncate(equal_count);
        away_order.truncate(equal_count);

        let mut home_kicks: u32 = 0;
        let mut away_kicks: u32 = 0;
        let mut round: u32 = 0;
        const MAX_ROUNDS: u32 = 200;
        'shootout: loop {
            if round >= MAX_ROUNDS {
                return Err(V4Error::InvalidInput(
                    "penalty shootout exceeded simulation safety limit without a winner".into(),
                ));
            }
            round += 1;
            for side in [TeamSideV4::Home, TeamSideV4::Away] {
                let order = if side == TeamSideV4::Home {
                    &home_order
                } else {
                    &away_order
                };
                let kick_number = if side == TeamSideV4::Home {
                    home_kicks
                } else {
                    away_kicks
                };
                let taker_idx = order[(kick_number as usize) % order.len()];
                let gk_idx = self.find_goalkeeper_index(side.opposite());
                let taker_id = self.state.players[taker_idx].player_id.clone();
                let gk_id = self.state.players[gk_idx].player_id.clone();
                let taker_attrs = *lookup_attrs(&self.input, &self.state.players[taker_idx]);
                let gk_attrs = *lookup_attrs(&self.input, &self.state.players[gk_idx]);
                let pen_skill = normalize_attr(taker_attrs.technical.penalties) * 0.50
                    + normalize_attr(taker_attrs.mental.composure) * 0.30
                    + normalize_attr(taker_attrs.hidden.pressure) * 0.20;
                let gk_skill = normalize_attr(gk_attrs.goalkeeping.reflexes) * 0.65
                    + normalize_attr(gk_attrs.goalkeeping.one_on_ones) * 0.35;
                let score_prob = (0.76 + (pen_skill - gk_skill) * 0.18).clamp(0.55, 0.92);
                let scored = (self.rng.next_double() as f32) < score_prob;
                match side {
                    TeamSideV4::Home => {
                        home_kicks += 1;
                        if scored {
                            self.state.shootout_home_goals += 1;
                        }
                    }
                    TeamSideV4::Away => {
                        away_kicks += 1;
                        if scored {
                            self.state.shootout_away_goals += 1;
                        }
                    }
                }
                let mut meta = EventMetadataV4 {
                    scored: Some(scored),
                    goalkeeper: Some(gk_id),
                    ..EventMetadataV4::default()
                };
                meta.extra.insert("round".into(), round.to_string());
                meta.extra.insert(
                    "kickNumber".into(),
                    kick_number.saturating_add(1).to_string(),
                );
                self.emit(
                    EventTypeV4::PenaltyShootoutKick,
                    HighlightTier::GoalKey,
                    Some(seq_id),
                    Some(side),
                    Some(taker_id),
                    None,
                    meta,
                );

                if round <= 5 {
                    let home_remaining = 5u32.saturating_sub(home_kicks);
                    let away_remaining = 5u32.saturating_sub(away_kicks);
                    if self.state.shootout_home_goals as u32
                        > self.state.shootout_away_goals as u32 + away_remaining
                        || self.state.shootout_away_goals as u32
                            > self.state.shootout_home_goals as u32 + home_remaining
                    {
                        break 'shootout;
                    }
                }
            }
            if round >= 5 && self.state.shootout_home_goals != self.state.shootout_away_goals {
                break;
            }
        }
        self.emit(
            EventTypeV4::PenaltyShootoutEnded,
            HighlightTier::GoalKey,
            Some(seq_id),
            None,
            None,
            None,
            EventMetadataV4::default(),
        );
        Ok(())
    }

    fn shootout_eligible_order(&self, side: TeamSideV4) -> Vec<usize> {
        let mut eligible: Vec<usize> = self
            .state
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == side && p.on_pitch && !p.sent_off)
            .map(|(i, _)| i)
            .collect();
        eligible.sort_by(|&a, &b| {
            let ar = lookup_attrs(&self.input, &self.state.players[a]);
            let br = lookup_attrs(&self.input, &self.state.players[b]);
            let ascore = ar.technical.penalties as u16 * 3
                + ar.mental.composure as u16 * 2
                + ar.hidden.pressure as u16;
            let bscore = br.technical.penalties as u16 * 3
                + br.mental.composure as u16 * 2
                + br.hidden.pressure as u16;
            bscore
                .cmp(&ascore)
                .then_with(|| self.state.players[a].player_id.cmp(&self.state.players[b].player_id))
        });
        eligible
    }

    /// Executes the post-celebration Kickoff once `DeadBallKind::GoalCelebration` finishes
    /// `ExecutingRestart` (strictly later in `sim_time_ms` than the `Goal` event).
    fn restart_after_goal_kickoff(&mut self, kickoff_side: TeamSideV4) -> usize {
        self.reset_kickoff_positions(kickoff_side);
        let kicker = self.kickoff_carrier_index(kickoff_side);
        let kicker_id = self.state.players[kicker].player_id.clone();
        self.state.ball.pos = Vec3::center_spot();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(kickoff_side);
        self.state.ball.owner_player_id = Some(kicker_id.clone());
        self.state.ball.last_touch_side = None;
        self.state.ball.last_touch_player_id = None;
        self.possession_side = kickoff_side;
        self.carrier_idx = kicker;
        self.last_passer_idx = None;
        self.emit(
            EventTypeV4::Kickoff,
            HighlightTier::None,
            None,
            Some(kickoff_side),
            Some(kicker_id),
            None,
            EventMetadataV4::default(),
        );
        kicker
    }

    fn reset_kickoff_positions(&mut self, _kickoff_side: TeamSideV4) {
        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                continue;
            }
            let side = self.state.players[idx].side;
            let slot_id = self.state.players[idx].slot_id.clone();
            let team = match side {
                TeamSideV4::Home => &self.input.home_team,
                TeamSideV4::Away => &self.input.away_team,
            };
            if let Some(slot) = team.tactical_slots.iter().find(|s| s.slot_id == slot_id) {
                let mut pos = Vec2::from_normalized_slot(side, slot.out_x as f32, slot.out_y as f32);
                match side {
                    TeamSideV4::Home => pos.x = pos.x.min(PITCH_LENGTH_M * 0.49),
                    TeamSideV4::Away => pos.x = pos.x.max(PITCH_LENGTH_M * 0.51),
                }
                self.state.players[idx].pos = pos;
                self.state.players[idx].target_pos = pos;
                self.state.players[idx].vel = Vec2::default();
                self.state.players[idx].last_moved_m = 0.0;
            }
        }
        self.state.ball.pos = Vec3::center_spot();
    }

    fn kickoff_carrier_index(&self, side: TeamSideV4) -> usize {
        self.state
            .players
            .iter()
            .rposition(|p| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .unwrap_or(0)
    }

    fn ensure_valid_carrier(&mut self) {
        if self.carrier_idx >= self.state.players.len()
            || !self.state.players[self.carrier_idx].on_pitch
            || self.state.players[self.carrier_idx].sent_off
        {
            self.carrier_idx = self.kickoff_carrier_index(self.possession_side);
        }
    }
}
