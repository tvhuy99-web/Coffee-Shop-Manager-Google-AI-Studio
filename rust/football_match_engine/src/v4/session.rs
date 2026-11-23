use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::clock::Period;
use crate::fingerprints;
use crate::rng::DeterministicRng;
use crate::v4::ai_manager::{apply_manual_substitution_group, evaluate_ai_managers};
use crate::v4::attributes::normalize_attr;
use crate::v4::ball_play::{execute_dribble_duel, execute_pass_and_first_touch, EmittedPlayEvent};
use crate::v4::diagnostics::MatchDiagnosticsCollectorV4;
use crate::v4::error::V4Error;
use crate::v4::events::{
    fingerprint_event_parts, EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4,
    MatchResultV4, MatchSummaryV4, PlayerMatchSummaryV4, SessionStepResultV4, TeamMatchSummaryV4,
    TeamRosterStateV4,
};
use crate::v4::input::{MatchInputV4, RulesModeV4, TeamInstructionsV4};
use crate::v4::movement::{advance_off_ball_movement, evaluate_local_context, lookup_attrs};
use crate::v4::perception::{perceive_evaluate_and_decide, PassKindV4, PlannedActionV4};
use crate::v4::psychology::{
    check_impact_injury, check_muscle_injury, compute_post_match_morale_delta,
    compute_prematch_form_multiplier, resolve_foul_discipline, shift_momentum,
    update_in_match_psychology,
};
use crate::v4::set_pieces::{execute_corner_v4, execute_free_kick_v4, execute_penalty_kick_v4};
use crate::v4::shooting::{resolve_shot_v4, ShotKindV4};
use crate::v4::spatial::{
    is_in_attacking_penalty_box, TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};
use crate::v4::tactics::{
    attach_tactical_flags, compute_tactical_flags, update_tactical_phases,
};
use crate::v4::{
    ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, EVENT_SCHEMA_VERSION_V4, HALF_SECONDS_V4,
};

const EXTRA_TIME_HALF_SECONDS: i32 = 900;

/// One player exchange within a grouped, simultaneous substitution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionChangeV4 {
    pub player_out_id: String,
    pub player_in_id: String,
}

/// Live interactive commands submitted by the player during a V4 match session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchCommandV4 {
    UpdateTactics {
        side: TeamSideV4,
        instructions: TeamInstructionsV4,
    },
    Substitute {
        side: TeamSideV4,
        #[serde(rename = "playerOutId")]
        player_out_id: String,
        #[serde(rename = "playerInId")]
        player_in_id: String,
    },
    SubstituteBatch {
        side: TeamSideV4,
        changes: Vec<SubstitutionChangeV4>,
    },
}

/// Command accepted after `completed_beats` simulation beats. It must be
/// replayed before beat `completed_beats + 1` (not by wall-clock time or TTS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedCommandV4 {
    pub completed_beats: u64,
    pub game_second: i32,
    pub command: MatchCommandV4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HalfStage {
    PreMatch,
    InHalf {
        period: Period,
        regulation_duration: i32,
        added_cap: i32,
        elapsed_in_half: i32,
        added_locked: bool,
        added_target: i32,
    },
    FullTime,
}

/// Event-driven 90-minute Match Engine V4.0 session supporting BOTH:
/// - **Interactive step-by-step control** (`advance_steps`, `submit_command`, `finish`) for player matches
/// - **FastBatch / Seed-Replay** (`run_to_full_time`) with zero-allocation event fingerprinting
///
/// Clock invariant: each half (H1 2700s, H2 2700s, ET1 900s, ET2 900s) tracks its own independent
/// `elapsed_in_half` and `added_target`, so stoppage time in H1 NEVER shortens H2 or Extra Time!
pub struct MatchSessionV4 {
    input: MatchInputV4,
    state: MatchStateV4,
    rng: DeterministicRng,
    retain_events: bool,
    events: Vec<MatchEventV4>,
    diagnostics: MatchDiagnosticsCollectorV4,
    /// Last event delivered to an interactive client (commands also emit events).
    last_delivered_event_index: usize,
    event_count: u32,
    timeline_fingerprint: u64,
    went_to_extra_time: bool,
    went_to_shootout: bool,
    finished: bool,
    last_passer_idx: Option<usize>,
    stage: HalfStage,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    first_half_elapsed: i32,
    second_half_elapsed: i32,
    et1_elapsed: i32,
    et2_elapsed: i32,
    completed_beats: u64,
    command_timeline: Vec<AppliedCommandV4>,
}

impl MatchSessionV4 {
    pub fn new(input: MatchInputV4) -> Result<Self, V4Error> {
        input.validate()?;
        let retain_events = input.simulation_mode.retain_events();
        let mut state = MatchStateV4::from_input(&input);
        let mut rng = DeterministicRng::new(input.seed);

        let importance = input.match_environment.match_importance;
        for idx in 0..state.players.len() {
            let attrs = *lookup_attrs(&input, &state.players[idx]);
            let form_mult = compute_prematch_form_multiplier(&attrs, importance, &mut rng);
            state.players[idx].psychology.confidence =
                (state.players[idx].psychology.confidence * form_mult).clamp(0.15, 1.0);
        }

        Ok(Self {
            input,
            state,
            rng,
            retain_events,
            events: if retain_events {
                Vec::with_capacity(1024)
            } else {
                Vec::new()
            },
            event_count: 0,
            diagnostics: MatchDiagnosticsCollectorV4::default(),
            last_delivered_event_index: 0,
            timeline_fingerprint: fingerprints::start(),
            went_to_extra_time: false,
            went_to_shootout: false,
            finished: false,
            last_passer_idx: None,
            stage: HalfStage::PreMatch,
            possession_side: TeamSideV4::Home,
            carrier_idx: 10,
            first_half_elapsed: 0,
            second_half_elapsed: 0,
            et1_elapsed: 0,
            et2_elapsed: 0,
            completed_beats: 0,
            command_timeline: Vec::new(),
        })
    }

    pub fn is_full_time(&self) -> bool {
        self.finished || self.stage == HalfStage::FullTime
    }

    /// Submits a live tactical update or manual substitution during an interactive match.
    pub fn submit_command(&mut self, cmd: MatchCommandV4) -> Result<(), V4Error> {
        if self.is_full_time() {
            return Err(V4Error::SessionFinished);
        }
        let to_record = cmd.clone();
        match cmd {
            MatchCommandV4::UpdateTactics { side, instructions } => {
                let high_line = instructions.line_height >= 68;
                let wasting = instructions.time_wasting >= 55;
                *self.state.active_instructions_mut(side) = instructions;
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
                        cause: Some("USER_TACTICS_UPDATE".into()),
                        ..EventMetadataV4::default()
                    },
                );
                self.state.ball.pos = ball_pos;
                self.command_timeline.push(AppliedCommandV4 {
                    completed_beats: self.completed_beats,
                    game_second: self.state.clock.game_second,
                    command: to_record.clone(),
                });
                Ok(())
            }
            MatchCommandV4::Substitute {
                side, player_out_id, player_in_id,
            } => self.execute_substitution_group(
                side,
                &[(player_out_id, player_in_id)],
                to_record,
            ),
            MatchCommandV4::SubstituteBatch { side, changes } => {
                let pairs = changes.iter()
                    .map(|c| (c.player_out_id.clone(), c.player_in_id.clone()))
                    .collect::<Vec<_>>();
                self.execute_substitution_group(side, &pairs, to_record)
            }
        }
    }

    fn is_interval_substitution(&self) -> bool {
        // The previous half has ended and the next one has been initialized,
        // but no in-play beat of that new half has been simulated yet.
        matches!(self.stage,
            HalfStage::InHalf {
                period: Period::SecondHalf | Period::ExtraTimeFirst | Period::ExtraTimeSecond,
                elapsed_in_half: 0, ..
            }
        )
    }

    fn roster_snapshot(&self, side: TeamSideV4) -> TeamRosterStateV4 {
        let team = match side {
            TeamSideV4::Home => &self.input.home_team,
            TeamSideV4::Away => &self.input.away_team,
        };
        let on_pitch = self.state.players.iter()
            .filter(|p| p.side == side && p.on_pitch && !p.sent_off)
            .map(|p| p.player_id.clone())
            .collect::<Vec<_>>();
        let available_bench = team.bench.iter()
            .filter(|p| p.eligible && !p.injured &&
                !self.state.players.iter().any(|runtime| runtime.player_id == p.player_id))
            .map(|p| p.player_id.clone())
            .collect::<Vec<_>>();
        let counters = self.state.team_state(side);
        TeamRosterStateV4 {
            on_pitch_player_ids: on_pitch,
            available_bench_player_ids: available_bench,
            substitutions_used: counters.substitutions_used,
            substitution_windows_used: counters.substitution_windows_used,
            interval_substitution_allowed: self.is_interval_substitution(),
        }
    }

    fn execute_substitution_group(
        &mut self,
        side: TeamSideV4,
        changes: &[(String, String)],
        recorded_command: MatchCommandV4,
    ) -> Result<(), V4Error> {
        let at_interval = self.is_interval_substitution();
        let events = apply_manual_substitution_group(
            &mut self.state, &self.input, side, changes, at_interval,
        ).map_err(|e| V4Error::InvalidInput(e.into()))?;
        let poss = self.possession_side;
        self.flush_play_events(events, poss);
        if !self.state.players[self.carrier_idx].on_pitch
            || self.state.players[self.carrier_idx].sent_off {
            self.carrier_idx = self.kickoff_carrier_index(self.possession_side);
        }
        self.command_timeline.push(AppliedCommandV4 {
            completed_beats: self.completed_beats,
            game_second: self.state.clock.game_second,
            command: recorded_command,
        });
        Ok(())
    }

    /// Advances an interactive match by up to `max_steps` action beats and returns
    /// newly emitted events + live score/clock snapshot.
    pub fn advance_steps(&mut self, max_steps: usize) -> Result<SessionStepResultV4, V4Error> {
        let prev_len = self.last_delivered_event_index;
        let steps = max_steps.clamp(1, 500);
        for _ in 0..steps {
            if self.is_full_time() {
                break;
            }
            self.step_one_beat()?;
            self.completed_beats += 1;
        }
        let new_events = if self.retain_events && self.events.len() >= prev_len {
            self.events[prev_len..].to_vec()
        } else {
            Vec::new()
        };
        self.last_delivered_event_index = self.events.len();
        Ok(SessionStepResultV4 {
            ok: true,
            period: self.state.clock.period as i32,
            game_second: self.state.clock.game_second,
            period_elapsed_second: self.state.clock.game_second - self.state.clock.period_started_at_second,
            completed_beats: self.completed_beats,
            home_goals: self.state.home.goals,
            away_goals: self.state.away.goals,
            is_full_time: self.is_full_time(),
            total_events: self.event_count,
            new_events,
            home_roster: self.roster_snapshot(TeamSideV4::Home),
            away_roster: self.roster_snapshot(TeamSideV4::Away),
        })
    }

    /// Runs the match to Full Time (used by both `simulate_v4` and interactive `finish`).
    pub fn run_to_full_time(&mut self) -> Result<MatchResultV4, V4Error> {
        if self.finished {
            return Err(V4Error::SessionFinished);
        }
        while !self.is_full_time() {
            self.step_one_beat()?;
            self.completed_beats += 1;
        }
        self.finished = true;
        Ok(self.build_result())
    }

    fn begin_half(
        &mut self,
        period: Period,
        regulation_duration: i32,
        added_cap: i32,
        kickoff_side: TeamSideV4,
    ) {
        self.state.clock.start_period(period);
        self.reset_kickoff_positions(kickoff_side);
        self.carrier_idx = self.kickoff_carrier_index(kickoff_side);
        self.possession_side = kickoff_side;
        self.last_passer_idx = None;

        let kicker_id = self.state.players[self.carrier_idx].player_id.clone();
        self.state.ball.pos = Vec3::center_spot();
        self.state.ball.vel = Vec3::default();
        self.state.ball.control = BallControlState::Controlled;
        self.state.ball.owner_side = Some(kickoff_side);
        self.state.ball.owner_player_id = Some(kicker_id.clone());
        self.state.ball.last_touch_side = None;
        self.state.ball.last_touch_player_id = None;
        self.emit(
            EventTypeV4::Kickoff,
            HighlightTier::None,
            None,
            Some(kickoff_side),
            Some(kicker_id),
            None,
            EventMetadataV4::default(),
        );
        self.stage = HalfStage::InHalf {
            period,
            regulation_duration,
            added_cap,
            elapsed_in_half: 0,
            added_locked: false,
            added_target: 0,
        };
    }

    fn step_one_beat(&mut self) -> Result<(), V4Error> {
        match self.stage {
            HalfStage::PreMatch => {
                self.emit(
                    EventTypeV4::MatchStarted,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.begin_half(Period::FirstHalf, HALF_SECONDS_V4, 240, TeamSideV4::Home);
                Ok(())
            }
            HalfStage::FullTime => Ok(()),
            HalfStage::InHalf {
                period,
                regulation_duration,
                added_cap,
                mut elapsed_in_half,
                mut added_locked,
                mut added_target,
            } => {
                let target_total = regulation_duration + added_target;
                if elapsed_in_half >= target_total {
                    self.complete_current_half(period, elapsed_in_half)?;
                    return Ok(());
                }

                let att_tempo = self.state.active_instructions(self.possession_side).tempo as i32;
                let tempo_offset = if att_tempo >= 70 { -1 } else if att_tempo <= 35 { 2 } else { 0 };
                let raw_step = (6 + self.rng.next_int(11) + tempo_offset).clamp(4, 18);
                let dt = raw_step.min((target_total - elapsed_in_half).max(1));

                elapsed_in_half += dt;
                let new_game_sec = self.state.clock.game_second + dt;
                self.state
                    .clock
                    .set_game_second(new_game_sec)
                    .map_err(|e| V4Error::InvalidClock(e.into()))?;

                self.state.team_state_mut(self.possession_side).possession_seconds += dt as u32;

                // Borrow-safe: extract `game_sec` into local variable before `&mut self.state`
                let game_sec = self.state.clock.game_second;
                update_in_match_psychology(&mut self.state, &self.input, game_sec);

                if game_sec - self.state.last_ai_eval_second >= 300 {
                    self.state.last_ai_eval_second = game_sec;
                    let ai_events = evaluate_ai_managers(&mut self.state, &self.input);
                    if !ai_events.is_empty() {
                        let poss = self.possession_side;
                        self.flush_play_events(ai_events, poss);
                        if !self.state.players[self.carrier_idx].on_pitch
                            || self.state.players[self.carrier_idx].sent_off
                        {
                            self.carrier_idx = self.kickoff_carrier_index(self.possession_side);
                        }
                    }
                }

                self.update_interval_fatigue(dt as f32, self.possession_side);

                if let Some(muscle_inj) = check_muscle_injury(&mut self.state, &self.input, &mut self.rng) {
                    self.state.clock.add_stoppage_seconds(30);
                    let poss = self.possession_side;
                    self.flush_play_events(vec![muscle_inj], poss);
                    let inj_ai = evaluate_ai_managers(&mut self.state, &self.input);
                    if !inj_ai.is_empty() {
                        self.flush_play_events(inj_ai, poss);
                        if !self.state.players[self.carrier_idx].on_pitch
                            || self.state.players[self.carrier_idx].sent_off
                        {
                            self.carrier_idx = self.kickoff_carrier_index(self.possession_side);
                        }
                    }
                }

                advance_off_ball_movement(
                    &mut self.state,
                    &self.input,
                    self.possession_side,
                    self.carrier_idx,
                    dt as f32,
                    &mut self.rng,
                );

                let c_pos = self.state.players[self.carrier_idx].pos;
                self.state.ball.pos = Vec3::ground(c_pos.x, c_pos.y).clamp_to_pitch();

                // Observe the controlled carrier BEFORE the action; shot event positions
                // represent the goal or blocker and must not be treated as box entries.
                self.diagnostics.observe_controlled_ball(self.possession_side, c_pos);
                let prev_possession = self.possession_side;
                let (next_side, next_carrier) = self.step_five_phase_cycle(self.carrier_idx);
                self.possession_side = next_side;
                self.carrier_idx = next_carrier;

                // Borrow-safe: extract `ball_pos` before `&mut self.state`
                let ball_pos_now = self.state.ball.pos;
                let phase_events = update_tactical_phases(
                    &mut self.state,
                    prev_possession,
                    self.possession_side,
                    ball_pos_now,
                );
                if !phase_events.is_empty() {
                    let poss = self.possession_side;
                    self.flush_play_events(phase_events, poss);
                }

                // Lock independent added time once full regulation of THIS half is reached
                if !added_locked && elapsed_in_half >= regulation_duration {
                    let added = self.state.clock.lock_added_time(added_cap);
                    added_locked = true;
                    added_target = added;
                    if added > 0 {
                        let mut meta = EventMetadataV4::default();
                        meta.extra.insert("addedSeconds".into(), added.to_string());
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

                if elapsed_in_half >= regulation_duration + added_target {
                    self.complete_current_half(period, elapsed_in_half)?;
                } else {
                    self.stage = HalfStage::InHalf {
                        period,
                        regulation_duration,
                        added_cap,
                        elapsed_in_half,
                        added_locked,
                        added_target,
                    };
                }
                Ok(())
            }
        }
    }

    fn complete_current_half(&mut self, period: Period, elapsed: i32) -> Result<(), V4Error> {
        match period {
            Period::FirstHalf => {
                self.first_half_elapsed = elapsed;
                self.emit(
                    EventTypeV4::HalfTime,
                    HighlightTier::Extended,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.apply_half_time_recovery();
                self.begin_half(Period::SecondHalf, HALF_SECONDS_V4, 360, TeamSideV4::Away);
            }
            Period::SecondHalf => {
                self.second_half_elapsed = elapsed;
                if self.input.rules_mode == RulesModeV4::Knockout
                    && self.state.home.goals == self.state.away.goals
                {
                    self.went_to_extra_time = true;
                    self.emit(
                        EventTypeV4::ExtraTimeStarted,
                        HighlightTier::Extended,
                        None,
                        None,
                        None,
                        None,
                        EventMetadataV4::default(),
                    );
                    self.begin_half(
                        Period::ExtraTimeFirst,
                        EXTRA_TIME_HALF_SECONDS,
                        120,
                        TeamSideV4::Home,
                    );
                } else {
                    self.finish_full_time();
                }
            }
            Period::ExtraTimeFirst => {
                self.et1_elapsed = elapsed;
                self.emit(
                    EventTypeV4::ExtraTimeHalfTime,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.begin_half(
                    Period::ExtraTimeSecond,
                    EXTRA_TIME_HALF_SECONDS,
                    120,
                    TeamSideV4::Away,
                );
            }
            Period::ExtraTimeSecond => {
                self.et2_elapsed = elapsed;
                if self.state.home.goals == self.state.away.goals {
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
        self.state.clock.set_period(Period::FullTime);
        self.emit(
            EventTypeV4::FullTime,
            HighlightTier::Extended,
            None,
            None,
            None,
            None,
            EventMetadataV4::default(),
        );
        self.stage = HalfStage::FullTime;
    }

    fn step_five_phase_cycle(&mut self, carrier_idx: usize) -> (TeamSideV4, usize) {
        let att_side = self.state.players[carrier_idx].side;
        let def_side = att_side.opposite();
        let carrier_pos = self.state.players[carrier_idx].pos;
        let carrier_ctx = evaluate_local_context(&self.state, &self.input, att_side, carrier_pos);

        if let Some(def_idx) = carrier_ctx.nearest_defender_idx {
            if carrier_ctx.nearest_defender_dist_m <= 7.5 {
                let p_norm = (carrier_ctx.pressure / 100.0).clamp(0.0, 1.0);
                let ref_mod = (self.input.referee_profile.strictness as f32 / 100.0 - 0.5) * 0.015;
                if (self.rng.next_double() as f32) < (0.046 + p_norm * 0.052 + ref_mod).clamp(0.018, 0.12) {
                    let seq_id = self.alloc_highlight_sequence();
                    let foul_pos = Vec3::ground(carrier_pos.x, carrier_pos.y).clamp_to_pitch();
                    let def_id = self.state.players[def_idx].player_id.clone();
                    let car_id = self.state.players[carrier_idx].player_id.clone();
                    self.state.team_state_mut(def_side).fouls += 1;
                    self.state.clock.add_stoppage_seconds(14);
                    let mut evs = vec![EmittedPlayEvent {
                        kind: EventTypeV4::Foul,
                        tier: HighlightTier::None,
                        seq_id: Some(seq_id),
                        side: Some(def_side),
                        actor: Some(def_id),
                        target: Some(car_id),
                        ball_pos: foul_pos,
                        metadata: EventMetadataV4 {
                            cause: Some("CONTESTED_PRESS_FOUL".into()),
                            ..EventMetadataV4::default()
                        },
                    }];
                    evs.extend(resolve_foul_discipline(
                        &mut self.state,
                        &self.input,
                        def_idx,
                        seq_id,
                        foul_pos,
                        &mut self.rng,
                    ));
                    if let Some(inj) = check_impact_injury(
                        &mut self.state,
                        &self.input,
                        carrier_idx,
                        def_idx,
                        &mut self.rng,
                    ) {
                        evs.push(inj);
                    }
                    self.flush_play_events(evs, att_side);
                    let sp_out = if is_in_attacking_penalty_box(att_side, carrier_pos) {
                        execute_penalty_kick_v4(&mut self.state, &self.input, att_side, &mut self.rng)
                    } else {
                        execute_free_kick_v4(&mut self.state, &self.input, att_side, carrier_pos, &mut self.rng)
                    };
                    if sp_out.stoppage_seconds > 0 {
                        self.state.clock.add_stoppage_seconds(sp_out.stoppage_seconds);
                    }
                    let scored = sp_out.events.iter().any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                    self.flush_play_events(sp_out.events, att_side);
                    if scored {
                        shift_momentum(&mut self.state, att_side, 0.35);
                        let restart_idx = self.restart_after_goal(def_side);
                        return (def_side, restart_idx);
                    }
                    return (sp_out.next_side, sp_out.next_carrier_idx);
                }
            }
        }

        let planned = perceive_evaluate_and_decide(
            &self.state,
            &self.input,
            carrier_idx,
            carrier_ctx,
            &mut self.rng,
        );

        match planned {
            PlannedActionV4::Pass {
                target_idx,
                kind,
                distance_m,
                progress_gain_m,
                lane_risk,
                interceptor_idx,
            } => {
                let outcome = execute_pass_and_first_touch(
                    &mut self.state,
                    &self.input,
                    carrier_idx,
                    target_idx,
                    kind,
                    distance_m,
                    progress_gain_m,
                    lane_risk,
                    interceptor_idx,
                    carrier_ctx,
                    &mut self.rng,
                );
                let retained = outcome.next_side == att_side;
                self.last_passer_idx = if retained { Some(carrier_idx) } else { None };
                if outcome.stoppage_seconds > 0 {
                    self.state.clock.add_stoppage_seconds(outcome.stoppage_seconds);
                }
                self.flush_play_events(outcome.events, att_side);

                if retained
                    && kind == PassKindV4::Cross
                    && is_in_attacking_penalty_box(att_side, self.state.players[outcome.next_carrier_idx].pos)
                {
                    let (shot_out, won_corner) = resolve_shot_v4(
                        &mut self.state,
                        &self.input,
                        outcome.next_carrier_idx,
                        Some(carrier_idx),
                        Some(ShotKindV4::Header),
                        false,
                        &mut self.rng,
                    );
                    return self.finalize_shot_outcome(att_side, shot_out, won_corner);
                }
                (outcome.next_side, outcome.next_carrier_idx)
            }
            PlannedActionV4::Dribble { defender_idx } => {
                let outcome = execute_dribble_duel(
                    &mut self.state,
                    &self.input,
                    carrier_idx,
                    defender_idx,
                    carrier_ctx,
                    &mut self.rng,
                );
                let had_foul = outcome.events.iter().any(|e| e.kind == EventTypeV4::Foul);
                let foul_xy = self.state.players[carrier_idx].pos;
                if outcome.next_side != att_side {
                    self.last_passer_idx = None;
                }
                if outcome.stoppage_seconds > 0 {
                    self.state.clock.add_stoppage_seconds(outcome.stoppage_seconds);
                }
                self.flush_play_events(outcome.events, att_side);

                if had_foul {
                    let sp_out = if is_in_attacking_penalty_box(att_side, foul_xy) {
                        execute_penalty_kick_v4(&mut self.state, &self.input, att_side, &mut self.rng)
                    } else {
                        execute_free_kick_v4(&mut self.state, &self.input, att_side, foul_xy, &mut self.rng)
                    };
                    if sp_out.stoppage_seconds > 0 {
                        self.state.clock.add_stoppage_seconds(sp_out.stoppage_seconds);
                    }
                    let scored = sp_out.events.iter().any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
                    self.flush_play_events(sp_out.events, att_side);
                    if scored {
                        shift_momentum(&mut self.state, att_side, 0.35);
                        let restart_idx = self.restart_after_goal(def_side);
                        return (def_side, restart_idx);
                    }
                    return (sp_out.next_side, sp_out.next_carrier_idx);
                }
                (outcome.next_side, outcome.next_carrier_idx)
            }
            PlannedActionV4::Shoot { .. } => {
                let assist = self.last_passer_idx;
                self.last_passer_idx = None;
                let (shot_out, won_corner) = resolve_shot_v4(
                    &mut self.state,
                    &self.input,
                    carrier_idx,
                    assist,
                    None,
                    false,
                    &mut self.rng,
                );
                self.finalize_shot_outcome(att_side, shot_out, won_corner)
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
                    (carrier_pos.y + (self.rng.next_double() as f32 - 0.5) * 18.0).clamp(0.0, 68.0),
                    4.5,
                );
                self.state.ball.pos = clear_pos;
                let g_sec = self.state.clock.game_second;
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
                (def_side, next_idx)
            }
            PlannedActionV4::HoldAndRecycle => {
                let carrier_id = self.state.players[carrier_idx].player_id.clone();
                let g_sec = self.state.clock.game_second;
                let mut meta = EventMetadataV4 {
                    pressure: Some(carrier_ctx.pressure.round() as u8),
                    escaped_pressure: if carrier_ctx.pressure >= 45.0 {
                        Some(true)
                    } else {
                        None
                    },
                    ..EventMetadataV4::default()
                };
                attach_tactical_flags(&mut meta, compute_tactical_flags(&self.state, att_side, g_sec));
                self.emit(
                    EventTypeV4::Carry,
                    HighlightTier::None,
                    None,
                    Some(att_side),
                    Some(carrier_id),
                    None,
                    meta,
                );
                (att_side, carrier_idx)
            }
        }
    }

    fn finalize_shot_outcome(
        &mut self,
        att_side: TeamSideV4,
        shot_out: crate::v4::ball_play::BallPlayOutcome,
        won_corner: bool,
    ) -> (TeamSideV4, usize) {
        let def_side = att_side.opposite();
        if shot_out.stoppage_seconds > 0 {
            self.state.clock.add_stoppage_seconds(shot_out.stoppage_seconds);
        }
        let scored = shot_out.events.iter().any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
        self.flush_play_events(shot_out.events, att_side);

        if scored {
            shift_momentum(&mut self.state, att_side, 0.35);
            let restart_idx = self.restart_after_goal(def_side);
            return (def_side, restart_idx);
        }

        if won_corner {
            let corner_out = execute_corner_v4(&mut self.state, &self.input, att_side, &mut self.rng);
            if corner_out.stoppage_seconds > 0 {
                self.state.clock.add_stoppage_seconds(corner_out.stoppage_seconds);
            }
            let corner_scored = corner_out.events.iter().any(|e| matches!(e.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal));
            self.flush_play_events(corner_out.events, att_side);
            if corner_scored {
                shift_momentum(&mut self.state, att_side, 0.35);
                let restart_idx = self.restart_after_goal(def_side);
                return (def_side, restart_idx);
            }
            return (corner_out.next_side, corner_out.next_carrier_idx);
        }

        (shot_out.next_side, shot_out.next_carrier_idx)
    }

    fn flush_play_events(&mut self, events: Vec<EmittedPlayEvent>, att_side: TeamSideV4) {
        let g_sec = self.state.clock.game_second;
        let flags = compute_tactical_flags(&self.state, att_side, g_sec);
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
    }

    /// Law 10: snapshot the eligible on-pitch players, reduce the larger team
    /// to the same number, then require EVERY eligible player to take one kick
    /// before anyone may take a second. Sudden death ends only after both sides
    /// have had an equal number of kicks. No fabricated deciding goal.
    fn run_penalty_shootout(&mut self) -> Result<(), V4Error> {
        self.went_to_shootout = true;
        self.state.clock.start_period(Period::Shootout);
        let seq_id = self.alloc_highlight_sequence();
        self.emit(
            EventTypeV4::PenaltyShootoutStarted,
            HighlightTier::GoalKey,
            Some(seq_id),
            None, None, None,
            EventMetadataV4::default(),
        );

        let mut home_order = self.shootout_eligible_order(TeamSideV4::Home);
        let mut away_order = self.shootout_eligible_order(TeamSideV4::Away);
        let equal_count = home_order.len().min(away_order.len());
        if equal_count == 0 {
            return Err(V4Error::InvalidInput("no eligible penalty kickers".into()));
        }
        // Reduction is based on the *eligible* players at full time, not the
        // bench or the number of substituted players. Keep the strongest
        // candidates for simulation; the real squad selector can choose this
        // order in future when explicit shootout orders are supported.
        home_order.truncate(equal_count);
        away_order.truncate(equal_count);

        let mut home_kicks: u32 = 0;
        let mut away_kicks: u32 = 0;
        let mut round: u32 = 0;
        // Technical error guard, not a way to manufacture a winner. This has
        // astronomically low probability at realistic kick conversion rates.
        const MAX_ROUNDS: u32 = 200;
        'shootout: loop {
            if round >= MAX_ROUNDS {
                return Err(V4Error::InvalidInput(
                    "penalty shootout exceeded simulation safety limit without a winner".into()
                ));
            }
            round += 1;
            for side in [TeamSideV4::Home, TeamSideV4::Away] {
                let order = if side == TeamSideV4::Home { &home_order } else { &away_order };
                let kick_number = if side == TeamSideV4::Home { home_kicks } else { away_kicks };
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
                        if scored { self.state.shootout_home_goals += 1; }
                    }
                    TeamSideV4::Away => {
                        away_kicks += 1;
                        if scored { self.state.shootout_away_goals += 1; }
                    }
                }
                let mut meta = EventMetadataV4 {
                    scored: Some(scored),
                    goalkeeper: Some(gk_id),
                    ..EventMetadataV4::default()
                };
                meta.extra.insert("round".into(), round.to_string());
                meta.extra.insert("kickNumber".into(), kick_number.saturating_add(1).to_string());
                self.emit(
                    EventTypeV4::PenaltyShootoutKick,
                    HighlightTier::GoalKey,
                    Some(seq_id), Some(side), Some(taker_id), None, meta,
                );

                if round <= 5 {
                    // Decide as soon as the trailing side cannot mathematically
                    // catch up with its remaining kicks in the initial series.
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
            // Sudden death: NEVER decide after only one team's extra kick.
            if round >= 5 && self.state.shootout_home_goals != self.state.shootout_away_goals {
                break;
            }
        }
        self.emit(
            EventTypeV4::PenaltyShootoutEnded,
            HighlightTier::GoalKey,
            Some(seq_id), None, None, None,
            EventMetadataV4::default(),
        );
        Ok(())
    }

    fn shootout_eligible_order(&self, side: TeamSideV4) -> Vec<usize> {
        let mut eligible: Vec<usize> = self.state.players.iter().enumerate()
            .filter(|(_, p)| p.side == side && p.on_pitch && !p.sent_off)
            .map(|(i, _)| i).collect();
        eligible.sort_by(|&a, &b| {
            let ar = lookup_attrs(&self.input, &self.state.players[a]);
            let br = lookup_attrs(&self.input, &self.state.players[b]);
            let ascore = ar.technical.penalties as u16 * 3
                + ar.mental.composure as u16 * 2 + ar.hidden.pressure as u16;
            let bscore = br.technical.penalties as u16 * 3
                + br.mental.composure as u16 * 2 + br.hidden.pressure as u16;
            bscore.cmp(&ascore)
                .then_with(|| self.state.players[a].player_id.cmp(&self.state.players[b].player_id))
        });
        eligible
    }

    /// Single authoritative post-goal restart. Ball, owner, carrier, clock and
    /// Kickoff event now agree; no stale ownership from the scoring shot survives.
    fn restart_after_goal(&mut self, kickoff_side: TeamSideV4) -> usize {
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

    fn update_interval_fatigue(&mut self, dt_sec: f32, possession_side: TeamSideV4) {
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
            let wr = normalize_attr(attrs.mental.work_rate);
            let sid = self.state.players[idx].slot_id.as_str();
            let role_speed = if is_gk {
                0.84
            } else if matches!(sid, "LCB" | "RCB" | "CB") {
                1.66
            } else if sid == "CF" {
                1.76
            } else if matches!(sid, "LB" | "RB" | "LW" | "RW") {
                1.88
            } else {
                1.98
            };
            let sprint_chance = if is_gk {
                0.0
            } else {
                let role_burst = if matches!(sid, "LW" | "RW" | "LB" | "RB" | "LCM" | "RCM") {
                    0.26
                } else {
                    0.15
                };
                let press_burst = if side != possession_side { press * 0.14 } else { 0.0 };
                (role_burst + press_burst).clamp(0.0, 0.50)
            };
            let high_intensity = !is_gk && (self.rng.next_double() as f32) < sprint_chance;
            let dist = dt_sec * role_speed * (0.90 + wr * 0.20) * if high_intensity { 1.22 } else { 0.96 };
            self.state.players[idx].fatigue.apply_interval(
                dt_sec,
                dist,
                high_intensity,
                attrs.physical.stamina,
                attrs.physical.natural_fitness,
            );
        }
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

    fn find_goalkeeper_index(&self, side: TeamSideV4) -> usize {
        self.state
            .players
            .iter()
            .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
            .or_else(|| self.state.players.iter().position(|p| p.side == side && p.on_pitch && !p.sent_off))
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
        self.state.next_highlight_sequence_id = self.state.next_highlight_sequence_id.saturating_add(1);
        id
    }

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
        self.event_count = self.event_count.saturating_add(1);
        let ball_pos = self.state.ball.pos.clamp_to_pitch();
        let g_sec = self.state.clock.game_second;
        let period = self.state.clock.period as i32;

        let ev_fp = fingerprint_event_parts(
            self.event_count,
            g_sec,
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

        // Pure observation: identical diagnostics for FullEvent and FastBatch,
        // without allocating retained events or drawing additional RNG values.
        self.diagnostics.on_event(kind, side, ball_pos, &metadata);

        if self.retain_events {
            self.events.push(MatchEventV4 {
                sequence: self.event_count,
                game_second: g_sec,
                period,
                period_elapsed_second: g_sec - self.state.clock.period_started_at_second,
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

    fn build_result(&mut self) -> MatchResultV4 {
        let total_poss = (self.state.home.possession_seconds + self.state.away.possession_seconds).max(1);
        let home_poss = (self.state.home.possession_seconds as f32 / total_poss as f32) * 100.0;
        let away_poss = 100.0 - home_poss;
        let home_goals = self.state.home.goals;
        let away_goals = self.state.away.goals;

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
            final_game_second: self.state.clock.game_second,
            first_half_elapsed_seconds: self.first_half_elapsed,
            second_half_elapsed_seconds: self.second_half_elapsed,
            extra_time_first_elapsed_seconds: self.et1_elapsed,
            extra_time_second_elapsed_seconds: self.et2_elapsed,
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
            protocol_version: ENGINE_PROTOCOL_VERSION_V4,
            engine_version: ENGINE_VERSION_V4.to_string(),
            event_schema_version: EVENT_SCHEMA_VERSION_V4,
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

/// Re-run a previously controlled game from an immutable snapshot and the exact
/// accepted command journal. Timings are measured in simulation beats, never
/// TTS or wall-clock time. Every command must match the original game-second.
/// Missing, reordered, duplicated or post-full-time commands are rejected.
pub fn replay_v4_with_timeline(
    input: MatchInputV4,
    commands: &[AppliedCommandV4],
) -> Result<MatchResultV4, V4Error> {
    let mut session = MatchSessionV4::new(input)?;
    let mut next = 0usize;
    while !session.is_full_time() {
        while next < commands.len() && commands[next].completed_beats == session.completed_beats {
            let recorded = &commands[next];
            if recorded.game_second != session.state.clock.game_second {
                return Err(V4Error::InvalidInput(format!(
                    "replay command {} gameSecond mismatch: recorded {}, actual {}",
                    next, recorded.game_second, session.state.clock.game_second
                )));
            }
            session.submit_command(recorded.command.clone())?;
            next += 1;
        }
        if next < commands.len() && commands[next].completed_beats < session.completed_beats {
            return Err(V4Error::InvalidInput(format!(
                "replay command order mismatch at index {next}"
            )));
        }
        session.step_one_beat()?;
        session.completed_beats += 1;
    }
    if next != commands.len() {
        return Err(V4Error::InvalidInput(format!(
            "{} commands have a simulation timestamp after full time",
            commands.len() - next
        )));
    }
    session.finished = true;
    Ok(session.build_result())
}

pub fn simulate_v4(input: MatchInputV4) -> Result<MatchResultV4, V4Error> {
    let mut session = MatchSessionV4::new(input)?;
    session.run_to_full_time()
}

pub fn simulate_batch_fast_v4(inputs: Vec<MatchInputV4>) -> Vec<Result<MatchResultV4, V4Error>> {
    inputs
        .into_par_iter()
        .map(|mut input| {
            input.simulation_mode = crate::v4::input::SimulationMode::FastBatch;
            simulate_v4(input)
        })
        .collect()
}
