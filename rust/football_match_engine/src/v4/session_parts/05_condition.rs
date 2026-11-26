// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    fn update_interval_fatigue(
        &mut self,
        dt_sec: f32,
        possession_side: TeamSideV4,
        is_dead_ball: bool,
        motion_loads: &[Option<crate::v4::movement::ExperimentalMotionLoad>],
    ) -> Result<(),V4Error> {
        if dt_sec <= 0.0 {
            return Ok(());
        }
        let experimental=self.uses_locomotion_experiment();
        if experimental && motion_loads.len()!=self.state.players.len() {
            return Err(V4Error::InvalidInput("motion workload roster mismatch".into()));
        }
        let home_press = self.state.active_home_ti.pressing_intensity as f32 / 100.0;
        let away_press = self.state.active_away_ti.pressing_intensity as f32 / 100.0;

        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch || self.state.players[idx].sent_off {
                continue;
            }
            let side = self.state.players[idx].side;
            let is_gk = self.state.players[idx].is_goalkeeper;
            let press = match side {
                TeamSideV4::Home => home_press,
                TeamSideV4::Away => away_press,
            };
            let attrs = *lookup_attrs(&self.input, &self.state.players[idx]);
            let moved_m = if is_dead_ball && !experimental {
                // Light walking/repositioning during stoppages (~0.35 m/s).
                dt_sec * if is_gk { 0.15 } else { 0.35 }
            } else {
                self.state.players[idx].last_moved_m.max(0.0)
            };
            if experimental {
                let load=motion_loads[idx].ok_or_else(||V4Error::InvalidInput(
                    format!("missing motion workload for {}",self.state.players[idx].player_id)))?;
                if !load.sprint_s.is_finite() || load.sprint_s<0.0 || load.sprint_s>load.high_s+0.0001 {
                    return Err(V4Error::InvalidInput("invalid sprint workload".into()));
                }
                self.state.players[idx].fatigue.apply_measured_motion(
                    dt_sec,moved_m,load.high_s,load.high_distance_m,load.high_entries,
                    load.sprint_entries,is_dead_ball,attrs.physical.stamina,attrs.physical.natural_fitness)
                    .map_err(|e|V4Error::InvalidInput(e.into()))?;
                continue;
            }
            let speed_mps = moved_m / dt_sec.max(0.05);
            let high_intensity = !is_dead_ball
                && !is_gk
                && (speed_mps >= 4.2 || (side != possession_side && press >= 0.70 && speed_mps >= 3.2));

            self.state.players[idx].fatigue.apply_interval(
                dt_sec,
                moved_m,
                high_intensity,
                is_dead_ball,
                attrs.physical.stamina,
                attrs.physical.natural_fitness,
            );
        }
        Ok(())
    }

    fn apply_half_time_recovery(&mut self) {
        for idx in 0..self.state.players.len() {
            if !self.state.players[idx].on_pitch {
                continue;
            }
            let attrs = *lookup_attrs(&self.input, &self.state.players[idx]);
            self.state.players[idx]
                .fatigue
                .apply_half_time_recovery(attrs.physical.natural_fitness);
        }
    }

    fn team_tactical_readiness(&self, side: TeamSideV4) -> (f32, f32) {
        let mut tw_sum = 0.0f32;
        let mut dec_sum = 0.0f32;
        let mut count = 0u32;
        for p in &self.state.players {
            if p.side == side && p.on_pitch && !p.sent_off {
                let a = lookup_attrs(&self.input, p);
                tw_sum += normalize_attr(a.mental.teamwork);
                dec_sum += normalize_attr(a.mental.decisions);
                count += 1;
            }
        }
        if count == 0 {
            (0.5, 0.5)
        } else {
            (tw_sum / count as f32, dec_sum / count as f32)
        }
    }

    fn team_avg_work_rate(&self, side: TeamSideV4) -> f32 {
        let mut wr_sum = 0.0f32;
        let mut count = 0u32;
        for p in &self.state.players {
            if p.side == side && p.on_pitch && !p.sent_off && !p.is_goalkeeper {
                let a = lookup_attrs(&self.input, p);
                wr_sum += normalize_attr(a.mental.work_rate);
                count += 1;
            }
        }
        if count == 0 {
            0.5
        } else {
            wr_sum / count as f32
        }
    }

    fn goal_diff_for(&self, side: TeamSideV4) -> i32 {
        self.state.team_state(side).goals as i32
            - self.state.team_state(side.opposite()).goals as i32
    }

    fn has_pending_substitutions(&self) -> bool {
        self.pending_substitutions
            .iter()
            .any(|s| s.status == CommandLifecycleStatus::QueuedForDeadBall)
    }

    fn count_pending_substitutions(&self) -> usize {
        self.pending_substitutions
            .iter()
            .filter(|s| s.status == CommandLifecycleStatus::QueuedForDeadBall)
            .count()
    }

    fn find_goalkeeper_index(&self, side: TeamSideV4) -> usize {
        self.state
            .players
            .iter()
            .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .or_else(|| {
                self.state
                    .players
                    .iter()
                    .position(|p| p.side == side && p.on_pitch && !p.sent_off)
            })
            .unwrap_or(0)
    }

    fn pick_outfield_index(&mut self, side: TeamSideV4) -> usize {
        let candidates: Vec<usize> = self
            .state
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return 0;
        }
        let pick = self.rng.next_int(candidates.len() as i32) as usize;
        candidates[pick]
    }

    fn alloc_highlight_sequence(&mut self) -> u32 {
        let id = self.state.next_highlight_sequence_id;
        self.state.next_highlight_sequence_id =
            self.state.next_highlight_sequence_id.saturating_add(1);
        id
    }

    fn alloc_command_id(&mut self) -> u64 {
        let id = self.next_command_id;
        self.next_command_id = self.next_command_id.saturating_add(1);
        id
    }
}
