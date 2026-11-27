// Engine-owned transition implementation; included in session.rs.
impl MatchSessionV4 {
    /// Emits an event: in `FastBatch` (`retain_events == false`), computes the exact same
    /// `timeline_fingerprint` inline without allocating a `MatchEventV4` entry in `self.events`.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        kind: EventTypeV4,
        highlight_tier: HighlightTier,
        highlight_sequence_id: Option<u32>,
        side: Option<TeamSideV4>,
        actor: Option<String>,
        target: Option<String>,
        metadata: EventMetadataV4,
    ) {
        if let Some(audit)=&mut self.goal_audit {
            audit.record(&self.state,kind,side,actor.as_deref());
        }
        self.event_count = self.event_count.saturating_add(1);
        let ball_pos = self.state.ball.pos.clamp_to_pitch();
        let sim_time_ms = self.state.des_clock.sim_time_ms;
        let period_elapsed_ms = self.state.des_clock.period_elapsed_ms;
        let g_sec = self.state.des_clock.game_second();
        let p_sec = self.state.des_clock.period_elapsed_second();
        let period = self.state.des_clock.period as i32;

        let ev_fp = fingerprint_event_parts(
            self.event_count,
            g_sec,
            sim_time_ms,
            period,
            kind,
            highlight_tier,
            highlight_sequence_id,
            side,
            actor.as_deref(),
            target.as_deref(),
            ball_pos,
            &metadata,
        );
        self.timeline_fingerprint = fingerprints::mix_i64(self.timeline_fingerprint, ev_fp as i64);

        self.diagnostics.on_event(kind, side, ball_pos, &metadata);

        // Use the authoritative emission boundary, not retained commentary.
        // Shootout kicks are a separate phase and do not feed open-play PI.
        if self.state.des_clock.period!=Period::Shootout {
            crate::v4::psychology::observe_personal_outcome(&mut self.state,kind,side,actor.as_deref(),&metadata);
        }

        if self.retain_events {
            self.events.push(MatchEventV4 {
                sequence: self.event_count,
                game_second: g_sec,
                period,
                period_elapsed_second: p_sec,
                sim_time_ms,
                period_elapsed_ms,
                dt_ms: self.last_dt_ms,
                kind,
                highlight_tier,
                highlight_sequence_id,
                side,
                actor,
                target,
                ball_pos,
                metadata,
            });
        }
    }

    pub(super) fn build_result(&mut self) -> MatchResultV4 {
        let total_poss_ms = (self.home_possession_ms + self.away_possession_ms).max(1);
        let home_poss = (self.home_possession_ms as f64 / total_poss_ms as f64 * 100.0) as f32;
        let away_poss = 100.0 - home_poss;
        let home_goals = self.state.home.goals;
        let away_goals = self.state.away.goals;

        // Populate DES clock totals and average outfield distance into diagnostics.
        let dc = &self.state.des_clock;
        self.diagnostics.record_des_clock_totals(
            dc.ball_in_play_ms,
            dc.ball_out_of_play_ms,
            dc.compensable_lost_ms,
            dc.announced_added_ms,
            dc.additional_added_ms,
        );
        let mut outfield_dist_sum = 0.0f32;
        let mut outfield_count = 0u32;
        for p in &self.state.players {
            if !p.is_goalkeeper {
                outfield_dist_sum += p.fatigue.distance_m;
                outfield_count += 1;
            }
        }
        if outfield_count > 0 {
            self.diagnostics
                .set_avg_player_distance_m(outfield_dist_sum / outfield_count as f32);
        }

        let summary = MatchSummaryV4 {
            home: TeamMatchSummaryV4 {
                team_id: self.input.home_team.team_id.clone(),
                goals: home_goals,
                shots: self.state.home.shots,
                shots_on_target: self.state.home.shots_on_target,
                shots_off_target: self.state.home.shots_off_target,
                shots_blocked: self.state.home.shots_blocked,
                saves: self.state.home.saves,
                own_goals_for: self.state.home.own_goals_for,
                own_goals_conceded: self.state.home.own_goals_conceded,
                passes_attempted: self.state.home.passes_attempted,
                passes_completed: self.state.home.passes_completed,
                possession_pct: (home_poss * 10.0).round() / 10.0,
                xg: (self.state.home.xg_millis as f32) / 1000.0,
                fouls: self.state.home.fouls,
                yellow_cards: self.state.home.yellow_cards,
                red_cards: self.state.home.red_cards,
                corners: self.state.home.corners,
                offsides: self.state.home.offsides,
                throw_ins: self.state.home.throw_ins,
            },
            away: TeamMatchSummaryV4 {
                team_id: self.input.away_team.team_id.clone(),
                goals: away_goals,
                shots: self.state.away.shots,
                shots_on_target: self.state.away.shots_on_target,
                shots_off_target: self.state.away.shots_off_target,
                shots_blocked: self.state.away.shots_blocked,
                saves: self.state.away.saves,
                own_goals_for: self.state.away.own_goals_for,
                own_goals_conceded: self.state.away.own_goals_conceded,
                passes_attempted: self.state.away.passes_attempted,
                passes_completed: self.state.away.passes_completed,
                possession_pct: (away_poss * 10.0).round() / 10.0,
                xg: (self.state.away.xg_millis as f32) / 1000.0,
                fouls: self.state.away.fouls,
                yellow_cards: self.state.away.yellow_cards,
                red_cards: self.state.away.red_cards,
                corners: self.state.away.corners,
                offsides: self.state.away.offsides,
                throw_ins: self.state.away.throw_ins,
            },
            final_game_second: self.state.des_clock.game_second(),
            first_half_elapsed_seconds: (self.first_half_elapsed_ms / 1000) as i32,
            second_half_elapsed_seconds: (self.second_half_elapsed_ms / 1000) as i32,
            extra_time_first_elapsed_seconds: (self.et1_elapsed_ms / 1000) as i32,
            extra_time_second_elapsed_seconds: (self.et2_elapsed_ms / 1000) as i32,
            went_to_extra_time: self.went_to_extra_time,
            went_to_shootout: self.went_to_shootout,
            shootout_home_goals: if self.went_to_shootout {
                Some(self.state.shootout_home_goals)
            } else {
                None
            },
            shootout_away_goals: if self.went_to_shootout {
                Some(self.state.shootout_away_goals)
            } else {
                None
            },
            players: self
                .state
                .players
                .iter()
                .map(|p| {
                    let attrs = lookup_attrs(&self.input, p);
                    let rating = (p.rating_tenths as f32) / 10.0;
                    let (won, drew) = match p.side {
                        TeamSideV4::Home => (home_goals > away_goals, home_goals == away_goals),
                        TeamSideV4::Away => (away_goals > home_goals, home_goals == away_goals),
                    };
                    PlayerMatchSummaryV4 {
                        player_id: p.player_id.clone(),
                        side: p.side,
                        goals: p.goals,
                        own_goals: p.own_goals,
                        assists: p.assists,
                        yellow_cards: p.yellow_cards,
                        sent_off: p.sent_off,
                        suspended_next_match: p.sent_off,
                        injury: p.injury,
                        final_aerobic: (p.fatigue.aerobic * 10.0).round() / 10.0,
                        final_anaerobic: (p.fatigue.anaerobic * 10.0).round() / 10.0,
                        post_match_condition: p.fatigue.aerobic.round().clamp(5.0, 100.0) as u8,
                        morale_delta: compute_post_match_morale_delta(attrs, won, drew, rating),
                        distance_m: p.fatigue.distance_m.round(),
                        rating,
                    }
                })
                .collect(),
        };

        MatchResultV4 {
            ok: true,
            protocol_version: ENGINE_PROTOCOL_VERSION_V4 as i32,
            engine_version: self.runtime_behavior_label(),
            event_schema_version: EVENT_SCHEMA_VERSION_V4 as i32,
            match_id: self.input.match_id.clone(),
            seed: self.input.seed,
            simulation_mode: self.input.simulation_mode,
            input_fingerprint: format!("{:016x}", self.input.input_fingerprint()),
            event_timeline_fingerprint: format!("{:016x}", self.timeline_fingerprint),
            event_count: self.event_count,
            command_timeline: self.command_timeline.clone(),
            summary,
            diagnostics: Some(self.diagnostics.finish()),
            events: std::mem::take(&mut self.events),
        }
    }
}
