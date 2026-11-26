// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    /// CRITICAL-08: Single centralized continuous-time world integrator called strictly once per
    /// positive `dt_ms` advance before dispatching any scheduled DES event.
    fn advance_world_for_dt(&mut self, dt_ms: i64, is_ball_in_play: bool) -> Result<bool, V4Error> {
        if dt_ms <= 0 {
            return Ok(false);
        }
        let dt_sec = (dt_ms as f32) / 1000.0;
        let game_sec = self.state.des_clock.game_second();

        if is_ball_in_play {
            match self.possession_side {
                TeamSideV4::Home => {
                    self.home_possession_ms = self.home_possession_ms.saturating_add(dt_ms);
                    self.state.home.possession_seconds = (self.home_possession_ms / 1000) as u32;
                }
                TeamSideV4::Away => {
                    self.away_possession_ms = self.away_possession_ms.saturating_add(dt_ms);
                    self.state.away.possession_seconds = (self.away_possession_ms / 1000) as u32;
                }
            }
        }

        update_in_match_psychology(&mut self.state, &self.input, game_sec, dt_sec);

        if is_ball_in_play {
            let prev_positions: Vec<Vec2> = self.state.players.iter().map(|p| p.pos).collect();
            let mut budgets:Vec<f32>=self.state.players.iter().map(|p| {
                if p.on_pitch&&!p.sent_off {
                    crate::v4::movement::player_max_speed_mps(crate::v4::movement::player_attrs(&self.state,p),p)*dt_sec
                } else {0.0}
            }).collect();
            let defense=advance_off_ball_movement_with_report(
                &mut self.state,
                &self.input,
                self.possession_side,
                self.carrier_idx,
                dt_sec,
                &mut self.rng,
            );
            // Keep the predicted contact participant on the same motion
            // path used by the resolver, including across unrelated DES events.
            if let Some(motion) = self.pending_flight.as_ref().and_then(|f| f.pass_contact_motion.as_ref()) {
                let now = self.state.des_clock.sim_time_ms;
                if let Some((idx, p)) = self.state.players.iter_mut().enumerate()
                    .find(|(_, p)| p.player_id == motion.candidate.player_id && p.on_pitch && !p.sent_off) {
                    let old = prev_positions[idx];
                    let next = motion.position_at(now);
                    budgets[idx]=motion.candidate.speed_mps.max(0.0)*dt_sec;
                    p.pos = next;
                    p.target_pos = motion.target;
                    p.vel = Vec2::new((next.x - old.x) / dt_sec, (next.y - old.y) / dt_sec);
                    if old.distance(next) > 0.05 {
                        p.facing_rad = (next.y - old.y).atan2(next.x - old.x);
                    }
                }
            }
            // A carry is a future, real movement interval, never an
            // additional displacement for an already integrated interval.
            if let Some(carry) = self.pending_carry.as_ref() {
                self.diagnostics.record_carry_interval(dt_ms,carry.is_hold);
                let idx = carry.carrier_idx;
                let p = &mut self.state.players[idx];
                if p.on_pitch && !p.sent_off && p.player_id == carry.motion.candidate.player_id {
                    let old = prev_positions[idx];
                    let next = carry.motion.position_at(self.state.des_clock.sim_time_ms.min(carry.resolves_at_ms));
                    budgets[idx]=carry.motion.candidate.speed_mps.max(0.0)*dt_sec;
                    p.pos = next;
                    p.target_pos = carry.motion.target;
                    p.vel = Vec2::new((next.x-old.x)/dt_sec, (next.y-old.y)/dt_sec);
                    if old.distance(next)>0.05 { p.facing_rad=(next.y-old.y).atan2(next.x-old.x); }
                    self.state.ball.pos=Vec3::ground(next.x,next.y);
                    self.state.ball.vel=Vec3::new(p.vel.x,p.vel.y,0.0);
                }
                if let Some(chase)=carry.challenger.as_ref() {
                    if let Some((di,def))=self.state.players.iter_mut().enumerate().find(|(_,p)|
                        p.player_id==chase.candidate.player_id && p.on_pitch && !p.sent_off) {
                        let old=prev_positions[di];
                        let next=chase.position_at(self.state.des_clock.sim_time_ms.min(carry.resolves_at_ms));
                        budgets[di]=chase.candidate.speed_mps.max(0.0)*dt_sec;
                        def.pos=next; def.target_pos=chase.target;
                        def.vel=Vec2::new((next.x-old.x)/dt_sec,(next.y-old.y)/dt_sec);
                        if old.distance(next)>0.05 {def.facing_rad=(next.y-old.y).atan2(next.x-old.x);}
                    }
                }
                if carry.shielding {
                    // Face away from the actual challenger so the body can
                    // protect the ball; do not change the ball position here.
                    if let Some(chase)=carry.challenger.as_ref() {
                        if let Some(def)=self.state.players.iter().find(|p|p.player_id==chase.candidate.player_id) {
                            let pos=self.state.players[idx].pos;
                            let angle=(pos.y-def.pos.y).atan2(pos.x-def.pos.x);
                            self.state.players[idx].facing_rad=angle;
                        }
                    }
                }
            }
            // Preparation contests use the same committed contact path
            // as carries. No extra movement or fatigue at resolution.
            if let Some(chase)=self.pending_decision.as_ref().and_then(|p|p.challenger.as_ref()) {
                if let Some((di,def))=self.state.players.iter_mut().enumerate()
                    .find(|(_,p)|p.player_id==chase.candidate.player_id && p.on_pitch && !p.sent_off) {
                    let old=prev_positions[di];
                    let next=chase.position_at(self.state.des_clock.sim_time_ms);
                    budgets[di]=chase.candidate.speed_mps.max(0.0)*dt_sec;
                    def.pos=next;
                    def.target_pos=chase.target;
                    def.vel=Vec2::new((next.x-old.x)/dt_sec,(next.y-old.y)/dt_sec);
                    if old.distance(next)>0.05 {def.facing_rad=(next.y-old.y).atan2(next.x-old.x);}
                }
            }
            for (idx, p) in self.state.players.iter_mut().enumerate() {
                if let Some(prev) = prev_positions.get(idx) {
                    crate::v4::transition::verify_motion_budget(*prev,p.pos,budgets[idx],&p.player_id)?;
                    p.last_moved_m = prev.distance(p.pos);
                }
            }
            // Measure the realized movement after physical contact path
            // overrides, rather than the preliminary tactical displacement.
            // A selected carry challenger is an independent physical race;
            // exclude that player's interval from tactical-role distance.
            for a in &defense {
                let p=&self.state.players[a.idx];
                let overridden=self.pending_carry.as_ref().and_then(|c|c.challenger.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id)
                    || self.pending_flight.as_ref().and_then(|f|f.pass_contact_motion.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id)
                    || self.pending_decision.as_ref().and_then(|p|p.challenger.as_ref())
                    .map_or(false,|c|c.candidate.player_id==p.player_id);
                if !overridden {
                    self.diagnostics.record_defensive_interval(a.role,dt_ms,p.last_moved_m);
                }
            }
        } else {
            // NHIỆM VỤ 6: Simulate purposeful low-intensity dead-ball movement (celebration,
            // returning to own half for kickoff, entering box for corner/FK, forming wall).
            let db_ctx = self.state.dead_ball.as_ref().map(|db| (db.kind, db.phase, db.restart_side, db.restart_spot));
            for p in &mut self.state.players {
                if !p.on_pitch || p.sent_off {
                    p.last_moved_m = 0.0;
                    continue;
                }
                let prev = p.pos;
                let (target, walk_speed_mps) = if let Some((kind, phase, r_side, spot)) = db_ctx {
                    match (kind, phase) {
                        (DeadBallKind::GoalCelebration, DeadBallPhase::Celebrating) => {
                            if p.side == r_side.opposite() {
                                // Scoring team jogs toward corner/celebration zone
                                (Vec2::new(spot.x.clamp(12.0, 93.0), if spot.y < 34.0 { 6.0 } else { 62.0 }), 2.4f32)
                            } else {
                                // Conceding team walks back toward own half
                                let own_x = if p.side == TeamSideV4::Home { 36.0 } else { 69.0 };
                                (Vec2::new(own_x, p.pos.y.clamp(12.0, 56.0)), 1.4f32)
                            }
                        }
                        (_, DeadBallPhase::ReturningToHalf | DeadBallPhase::KickoffSetup) => {
                            let own_x = if p.side == TeamSideV4::Home {
                                p.pos.x.min(50.0).max(8.0)
                            } else {
                                p.pos.x.max(55.0).min(97.0)
                            };
                            (Vec2::new(own_x, p.pos.y.clamp(8.0, 60.0)), 1.8f32)
                        }
                        (DeadBallKind::Corner | DeadBallKind::DirectFreeKick | DeadBallKind::IndirectFreeKick, DeadBallPhase::SettingBall) => {
                            let box_x = if r_side == TeamSideV4::Home { 93.0 } else { 12.0 };
                            (Vec2::new((p.pos.x * 0.6 + box_x * 0.4).clamp(6.0, 99.0), (p.pos.y * 0.7 + 34.0 * 0.3).clamp(8.0, 60.0)), 2.1f32)
                        }
                        _ => (Vec2::new((p.pos.x * 0.85 + spot.x * 0.15).clamp(4.0, 101.0), (p.pos.y * 0.85 + spot.y * 0.15).clamp(4.0, 64.0)), 1.3f32),
                    }
                } else {
                    (prev, 0.0f32)
                };
                let max_step = walk_speed_mps * dt_sec;
                let delta = Vec2::new(target.x - prev.x, target.y - prev.y);
                let dist = (delta.x * delta.x + delta.y * delta.y).sqrt();
                if dist > 1e-3 && max_step > 0.0 {
                    let frac = (max_step / dist).min(1.0);
                    p.pos = Vec2::new(
                        (prev.x + delta.x * frac).clamp(1.0, PITCH_LENGTH_M - 1.0),
                        (prev.y + delta.y * frac).clamp(1.0, 67.0),
                    );
                }
                p.last_moved_m = prev.distance(p.pos);
            }
        }

        if let Some(flight) = &self.pending_flight {
            let now = self.state.des_clock.sim_time_ms;
            self.state.ball.pos = flight.trajectory.position_at_ms(now).clamp_to_pitch();
            self.state.ball.vel = flight.trajectory.velocity_at_ms(now);
            if matches!(&flight.action, FlightActionV4::LooseRecovery { .. }) && self.state.ball.pos.z <= 0.0 {
                self.state.ball.vel.z=0.0;
            }
        }
        self.update_interval_fatigue(dt_sec, self.possession_side, !is_ball_in_play);

        if is_ball_in_play {
            let avg_wr = self.team_avg_work_rate(self.possession_side);
            let ball_pos_now = self.state.ball.pos;
            let phase_events = update_tactical_phases(
                &mut self.state,
                self.possession_side,
                self.possession_side,
                ball_pos_now,
                dt_ms,
                avg_wr,
            );
            if !phase_events.is_empty() {
                let poss = self.possession_side;
                self.flush_play_events(phase_events, poss);
            }

            if let Some(muscle_inj) =
                check_muscle_injury(&mut self.state, &self.input, dt_sec, &mut self.rng)
            {
                let inj_pos = muscle_inj.ball_pos;
                let poss = self.possession_side;
                self.flush_play_events(vec![muscle_inj], poss);
                let inj_ai = evaluate_ai_managers(&mut self.state, &self.input);
                if !inj_ai.is_empty() {
                    self.flush_play_events(inj_ai, poss);
                    self.ensure_valid_carrier();
                }
                self.open_dead_ball(DeadBallKind::InjuryStoppage, poss, inj_pos, true, false)?;
                return Ok(true);
            }
        }

        Ok(false)
    }
}
