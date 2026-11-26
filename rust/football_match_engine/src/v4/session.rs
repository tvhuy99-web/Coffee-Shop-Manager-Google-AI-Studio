use std::collections::BTreeMap;

use crate::clock::Period;
use crate::fingerprints;
use crate::rng::DeterministicRng;
use crate::v4::coach_adapter::evaluate_ai_managers;
use crate::v4::substitution::apply_manual_substitution_group;
use crate::v4::attributes::normalize_attr;
use crate::v4::ball_play::{resolve_carry_contact, execute_pass_and_first_touch_des, is_receiver_offside, pass_reach_candidate, EmittedPlayEvent};
use crate::v4::checkpoint::{
    PendingFlightSnapshotV4, FlightActionV4, FlightCounterDeltaV4, PendingDecisionV4, PendingCarryV4,
    compute_checkpoint_checksum, verify_checkpoint, MatchCheckpointV4, MatchSessionSnapshotV4,
    RngSnapshotV4, SessionStatsSnapshotV4, CHECKPOINT_SCHEMA_VERSION_V4,
};
use crate::v4::diagnostics::{DeadBallDiagKind, MatchDiagnosticsCollectorV4};
use crate::v4::error::V4Error;
use crate::v4::events::{
    fingerprint_event_parts, EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4,
    MatchResultV4, MatchSummaryV4, PendingCommandSummaryV4, PlayerMatchSummaryV4, SessionStageV4,
    SessionStepResultV4, TeamMatchSummaryV4, TeamRosterStateV4,
};
use crate::v4::input::{MatchInputV4, TeamInstructionsV4};
use crate::v4::interaction::{
    InteractionKind, PassContactMotion, BallTrajectory,
    contact_control_probability, blocked_ball_velocity, find_moving_ball_recovery,
    LooseBallResolution, BoundaryKind, EarlyInteractionHit, find_carry_contact,
};
use crate::v4::movement::{advance_off_ball_movement_with_report, evaluate_local_context, evaluate_passing_lane, lookup_attrs};
use crate::player_intelligence::{perceive_evaluate_and_decide_des, DecisionPlanV4, CarryIntentV4, carry_target, PassKindV4, PlannedActionV4, PI_POLICY_ID};
use crate::v4::psychology::{
    check_muscle_injury, compute_post_match_morale_delta,
    sample_prematch_form,
    shift_momentum, update_in_match_psychology,
};
use crate::v4::rules::{
    estimate_ball_flight_ms, estimate_dead_ball_phase_ms,
    initial_dead_ball_phase, next_dead_ball_phase, BallFlightKind, CommandLifecycleStatus,
    CommandOrigin, DeadBallKind, DeadBallPhase, DeadBallState, HalfEndPolicy, HalfStageV4,
    PendingSubstitutionItem, PendingTacticalCommand, RulesetConfig,
};
use crate::v4::scheduler::{CancellationScope, DesEventPayload, EventPriority, EventScheduler};
use crate::v4::set_pieces::{
    execute_corner_v4, execute_free_kick_v4, execute_penalty_kick_v4_with_rules,
};
use crate::v4::shooting::{resolve_shot_v4_des, ShotKindV4};
use crate::v4::spatial::{
    attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box, TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};
use crate::v4::tactics::{
    attach_tactical_flags, compute_tactical_delay_for_context_ms, compute_tactical_flags,
    update_tactical_phases,
};
use crate::v4::{
    ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, EVENT_SCHEMA_VERSION_V4,
};

pub use crate::v4::session_commands::{AppliedCommandV4, MatchCommandV4, SubstitutionChangeV4};
pub use crate::v4::session_replay::{replay_v4_with_timeline, simulate_batch_fast_v4, simulate_v4};

/// Discrete-Event Simulation (DES v1.4) Match Engine V4 session supporting:
/// - **Interactive time-targeted & step-by-step control** (`advance_until_ms`, `advance_steps`,
///   `start_next_period`, `submit_command`) with explicit `HalfTimeBreak` / `ExtraTimeBreak` pauses
/// - **Self-verifying checkpoint export/import** (`export_checkpoint`, `from_checkpoint`)
/// - **FastBatch / Seed-Replay** (`run_to_full_time`) with zero-allocation event fingerprinting
#[derive(Clone)]
pub struct MatchSessionV4 {
    input: MatchInputV4,
    ruleset: RulesetConfig,
    pub(super) state: MatchStateV4,
    scheduler: EventScheduler,
    rng: DeterministicRng,
    retain_events: bool,
    events: Vec<MatchEventV4>,
    diagnostics: MatchDiagnosticsCollectorV4,
    last_delivered_event_index: usize,
    event_count: u32,
    timeline_fingerprint: u64,
    went_to_extra_time: bool,
    went_to_shootout: bool,
    pub(super) finished: bool,
    last_passer_idx: Option<usize>,
    pub(super) stage: HalfStageV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    first_half_elapsed_ms: i64,
    second_half_elapsed_ms: i64,
    et1_elapsed_ms: i64,
    et2_elapsed_ms: i64,
    home_possession_ms: i64,
    away_possession_ms: i64,
    last_dt_ms: i64,
    pub(super) completed_beats: u64,
    next_command_id: u64,
    pending_substitutions: Vec<PendingSubstitutionItem>,
    pending_tactical_commands: Vec<PendingTacticalCommand>,
    command_timeline: Vec<AppliedCommandV4>,
    pending_flight: Option<PendingFlightSnapshotV4>,
    pending_decision: Option<PendingDecisionV4>,
    pending_carry: Option<PendingCarryV4>,
    tackle_ready_at: BTreeMap<String,i64>,
    goal_audit: Option<crate::v4::transition::GoalDeltaAudit>,
    #[cfg(test)]
    locomotion_experiment: bool,
}

impl MatchSessionV4 {
    pub fn new(input: MatchInputV4) -> Result<Self, V4Error> {
        input.validate()?;
        let retain_events = input.simulation_mode.retain_events();
        let ruleset = input.effective_ruleset();
        let mut state = MatchStateV4::from_input(&input);
        let mut rng = DeterministicRng::new(input.seed);

        let importance = input.match_environment.match_importance;
        for idx in 0..state.players.len() {
            let attrs = *lookup_attrs(&input, &state.players[idx]);
            let (match_form,form_mult)=sample_prematch_form(&attrs,importance,&mut rng);
            state.players[idx].psychology.match_form_multiplier=match_form;
            state.players[idx].psychology.confidence =
                (state.players[idx].psychology.confidence * form_mult).clamp(0.15, 1.0);
        }

        Ok(Self {
            input,
            ruleset,
            state,
            scheduler: EventScheduler::new(),
            rng,
            retain_events,
            events: if retain_events {
                Vec::with_capacity(1024)
            } else {
                Vec::new()
            },
            diagnostics: MatchDiagnosticsCollectorV4::default(),
            last_delivered_event_index: 0,
            event_count: 0,
            timeline_fingerprint: fingerprints::start(),
            went_to_extra_time: false,
            went_to_shootout: false,
            finished: false,
            last_passer_idx: None,
            stage: HalfStageV4::PreMatch,
            possession_side: TeamSideV4::Home,
            carrier_idx: 10,
            first_half_elapsed_ms: 0,
            second_half_elapsed_ms: 0,
            et1_elapsed_ms: 0,
            et2_elapsed_ms: 0,
            home_possession_ms: 0,
            away_possession_ms: 0,
            last_dt_ms: 0,
            completed_beats: 0,
            next_command_id: 1,
            pending_substitutions: Vec::new(),
            pending_tactical_commands: Vec::new(),
            command_timeline: Vec::new(),
            pending_flight: None,
            pending_decision: None,
            pending_carry: None,
            tackle_ready_at: BTreeMap::new(),
            goal_audit: None,
            #[cfg(test)]
            locomotion_experiment: false,
        })
    }

    /// Internal experiment only; not selectable by JNI, MatchInput or shipping binaries.
    #[cfg(test)]
    fn new_locomotion_experiment(input: MatchInputV4) -> Result<Self, V4Error> {
        let mut session = Self::new(input)?;
        session.locomotion_experiment = true;
        Ok(session)
    }

    fn uses_locomotion_experiment(&self) -> bool {
        #[cfg(test)]
        { self.locomotion_experiment }
        #[cfg(not(test))]
        { false }
    }

    pub fn is_full_time(&self) -> bool {
        self.finished || self.stage.is_full_time()
    }

    pub fn stage(&self) -> HalfStageV4 {
        self.stage
    }

    pub fn sim_time_ms(&self) -> i64 {
        self.state.des_clock.sim_time_ms
    }

    /// Exports a self-verifying DES v1.4 checkpoint containing full engine state, scheduler,
    /// RNG state, pending commands, and deterministic payload checksum.
    pub fn export_checkpoint(&self) -> Result<MatchCheckpointV4, V4Error> {
        let mut diag_clone = self.diagnostics.clone();
        let diagnostics_summary = diag_clone.finish();
        let diagnostics_collector = self.diagnostics.clone();

        let payload = MatchSessionSnapshotV4 {
            input: self.input.clone(),
            state: self.state.clone(),
            scheduler: self.scheduler.clone(),
            rng: RngSnapshotV4 {
                root_seed: self.rng.root_seed(),
                state: self.rng.state(),
            },
            half_stage: self.stage,
            pending_substitutions: self.pending_substitutions.clone(),
            pending_tactical_commands: self.pending_tactical_commands.clone(),
            applied_commands: self.command_timeline.clone(),
            next_event_sequence: self.event_count,
            stats: SessionStatsSnapshotV4 {
                timeline_fingerprint: self.timeline_fingerprint,
                event_count: self.event_count,
                last_delivered_event_index: self.last_delivered_event_index,
                completed_beats: self.completed_beats,
                next_command_id: self.next_command_id,
                went_to_extra_time: self.went_to_extra_time,
                went_to_shootout: self.went_to_shootout,
                finished: self.finished,
                possession_side: self.possession_side,
                carrier_idx: self.carrier_idx,
                last_passer_idx: self.last_passer_idx,
                first_half_elapsed_ms: self.first_half_elapsed_ms,
                second_half_elapsed_ms: self.second_half_elapsed_ms,
                et1_elapsed_ms: self.et1_elapsed_ms,
                et2_elapsed_ms: self.et2_elapsed_ms,
                home_possession_ms: self.home_possession_ms,
                away_possession_ms: self.away_possession_ms,
                retained_events: if self.retain_events {
                    self.events.clone()
                } else {
                    Vec::new()
                },
            },
            diagnostics_collector,
            diagnostics_summary,
            pending_flight: self.pending_flight.clone(),
            pi_policy_id: PI_POLICY_ID.into(),
            pending_decision: self.pending_decision.clone(),
            pending_carry: self.pending_carry.clone(),
            tackle_ready_at: self.tackle_ready_at.clone(),
            locomotion_experiment_id: self.uses_locomotion_experiment().then(|| crate::v4::checkpoint::LOCOMOTION_EXPERIMENT_ID.to_string()),
        };

        let payload_json = serde_json::to_string(&payload)?;
        let checksum_hex = compute_checkpoint_checksum(&payload_json);

        Ok(MatchCheckpointV4 {
            checkpoint_schema_version: CHECKPOINT_SCHEMA_VERSION_V4,
            engine_version: ENGINE_VERSION_V4.to_string(),
            protocol_version: ENGINE_PROTOCOL_VERSION_V4,
            event_schema_version: EVENT_SCHEMA_VERSION_V4,
            ruleset_identity: self.ruleset.identity.clone(),
            match_id: self.input.match_id.clone(),
            seed: self.input.seed,
            input_fingerprint: format!("{:016x}", self.input.input_fingerprint()),
            sim_time_ms: self.state.des_clock.sim_time_ms,
            checksum_hex,
            payload,
        })
    }

    /// Verifies and restores a `MatchSessionV4` from a saved `MatchCheckpointV4`.
    pub fn from_checkpoint(ckpt: MatchCheckpointV4) -> Result<Self, V4Error> {
        verify_checkpoint(&ckpt)?;
        let payload = ckpt.payload;
        payload.input.validate()?;
        let ruleset = payload.input.effective_ruleset();
        let retain_events = payload.input.simulation_mode.retain_events();
        let rng = DeterministicRng::from_seed_and_state(payload.rng.root_seed, payload.rng.state);
        let diagnostics = payload.diagnostics_collector;

        Ok(Self {
            input: payload.input,
            ruleset,
            state: payload.state,
            scheduler: payload.scheduler,
            rng,
            retain_events,
            events: payload.stats.retained_events,
            diagnostics,
            last_delivered_event_index: payload.stats.last_delivered_event_index,
            event_count: payload.stats.event_count,
            timeline_fingerprint: payload.stats.timeline_fingerprint,
            went_to_extra_time: payload.stats.went_to_extra_time,
            went_to_shootout: payload.stats.went_to_shootout,
            finished: payload.stats.finished,
            last_passer_idx: payload.stats.last_passer_idx,
            stage: payload.half_stage,
            possession_side: payload.stats.possession_side,
            carrier_idx: payload.stats.carrier_idx,
            first_half_elapsed_ms: payload.stats.first_half_elapsed_ms,
            second_half_elapsed_ms: payload.stats.second_half_elapsed_ms,
            et1_elapsed_ms: payload.stats.et1_elapsed_ms,
            et2_elapsed_ms: payload.stats.et2_elapsed_ms,
            home_possession_ms: payload.stats.home_possession_ms,
            away_possession_ms: payload.stats.away_possession_ms,
            last_dt_ms: 0,
            completed_beats: payload.stats.completed_beats,
            next_command_id: payload.stats.next_command_id,
            pending_substitutions: payload.pending_substitutions,
            pending_tactical_commands: payload.pending_tactical_commands,
            command_timeline: payload.applied_commands,
            pending_flight: payload.pending_flight,
            pending_decision: payload.pending_decision,
            pending_carry: payload.pending_carry,
            tackle_ready_at: payload.tackle_ready_at,
            goal_audit: None,
            #[cfg(test)]
            locomotion_experiment: payload.locomotion_experiment_id.is_some(),
        })
    }

    /// Submits a live tactical update, manual substitution, or period-start command.
    ///
    /// - `UpdateTactics`: never applies instantaneously in live play; computes natural
    ///   propagation delay via `compute_tactical_delay_for_context_ms` and schedules
    ///   `TacticalCommandEffective` (immediate only during `HalfTimeBreak`/`ExtraTimeBreak`).
    /// - `Substitute` / `SubstituteBatch`: immediate without window cost during interval breaks;
    ///   queued in `pending_substitutions` during live play and executed on the next
    ///   `DeadBallPhase::ProcessingSubstitutions`.
    /// - `StartNextPeriod`: transitions out of `HalfTimeBreak` / `ExtraTimeBreak` / `PreMatch`.
    pub fn submit_command(&mut self, cmd: MatchCommandV4) -> Result<(), V4Error> {
        self.transact(|candidate| candidate.submit_command_inner(cmd))
    }

    /// Execute a public operation against a private candidate. The live session,
    /// including queue, RNG, ledger, diagnostics and event-delivery cursor,
    /// changes only after the whole operation succeeds. Batch size is bounded
    /// by the existing public API; do not clone the event history per DES event.
    fn transact<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T,V4Error>) -> Result<T,V4Error> {
        let mut candidate=self.clone();
        let result=operation(&mut candidate)?;
        crate::v4::transition::verify_commit_progress(
            self.state.des_clock.sim_time_ms,candidate.state.des_clock.sim_time_ms,
            self.event_count,candidate.event_count,
            self.completed_beats,candidate.completed_beats,
        )?;
        crate::v4::transition::verify_commit_structure(&candidate.state)?;
        crate::v4::transition::verify_roster_profiles(&candidate.state,&candidate.input)?;
        crate::v4::transition::verify_uncontrolled_ball(&candidate.state)?;
        crate::v4::transition::verify_clock_accounting(&candidate.state,candidate.home_possession_ms,candidate.away_possession_ms)?;
        crate::v4::transition::verify_scheduler_clock(&candidate.state, &candidate.scheduler)?;
        crate::v4::transition::verify_command_journal(&candidate.command_timeline,candidate.next_command_id,candidate.sim_time_ms())?;
        crate::v4::transition::verify_event_journal(&candidate.events,candidate.event_count,candidate.last_delivered_event_index,
            candidate.retain_events,candidate.finished,candidate.sim_time_ms())?;
        if candidate.finished && !matches!(candidate.stage,HalfStageV4::FullTime) {
            return Err(V4Error::InvalidInput("INV-12: result closed before full time".into()));
        }
        *self=candidate;
        Ok(result)
    }

    fn submit_command_inner(&mut self, cmd: MatchCommandV4) -> Result<(), V4Error> {
        if self.is_full_time() {
            return Err(V4Error::SessionFinished);
        }
        let now_ms = self.state.des_clock.sim_time_ms;
        let command_id = self.alloc_command_id();
        let to_record = cmd.clone();

        match cmd {
            MatchCommandV4::StartNextPeriod => {
                self.begin_next_period_internal()?;
                self.command_timeline.push(AppliedCommandV4 {
                    command_id,
                    accepted_sim_time_ms: now_ms,
                    scheduled_effective_sim_time_ms: now_ms,
                    actual_effective_sim_time_ms: Some(now_ms),
                    applied_step_index: self.completed_beats,
                    status: CommandLifecycleStatus::Applied,
                    completed_beats: self.completed_beats,
                    game_second: (now_ms / 1000) as i32,
                    command: to_record,
                });
                Ok(())
            }
            MatchCommandV4::UpdateTactics { side, instructions } => {
                let is_break = self.stage.is_break() || matches!(self.stage, HalfStageV4::PreMatch);
                let is_dead_ball = self.state.dead_ball.is_some();
                let (avg_tw, avg_dec) = self.team_tactical_readiness(side);
                let old_instr = self.state.active_instructions(side).clone();
                let (start_delay_ms, _ramp_ms) = compute_tactical_delay_for_context_ms(
                    &old_instr,
                    &instructions,
                    is_dead_ball,
                    is_break,
                    avg_tw,
                    avg_dec,
                    &mut self.rng,
                );

                // Supersede any previously queued tactical command for the same side.
                for pending in &mut self.pending_tactical_commands {
                    if pending.side == side && pending.status == CommandLifecycleStatus::Scheduled {
                        pending.status = CommandLifecycleStatus::Superseded;
                        if let Some(entry) = self
                            .command_timeline
                            .iter_mut()
                            .rev()
                            .find(|c| c.command_id == pending.command_id)
                        {
                            entry.status = CommandLifecycleStatus::Superseded;
                        }
                    }
                }

                if start_delay_ms <= 0 {
                    self.apply_tactical_instructions_now(side, instructions, "INTERVAL_TACTICS_UPDATE");
                    self.command_timeline.push(AppliedCommandV4 {
                        command_id,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: now_ms,
                        actual_effective_sim_time_ms: Some(now_ms),
                        applied_step_index: self.completed_beats,
                        status: CommandLifecycleStatus::Applied,
                        completed_beats: self.completed_beats,
                        game_second: (now_ms / 1000) as i32,
                        command: to_record,
                    });
                } else {
                    let effective_ms = now_ms.saturating_add(start_delay_ms);
                    let cmd_idx = self.pending_tactical_commands.len() as u32;
                    self.pending_tactical_commands.push(PendingTacticalCommand {
                        command_id,
                        side,
                        origin: CommandOrigin::User,
                        instructions,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: effective_ms,
                        actual_effective_sim_time_ms: None,
                        status: CommandLifecycleStatus::Scheduled,
                    });
                    self.scheduler
                        .schedule(
                            effective_ms,
                            EventPriority::TacticalAndCommand,
                            CancellationScope::Persistent,
                            DesEventPayload::TacticalCommandEffective {
                                command_index: cmd_idx,
                                side,
                            },
                        )
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                    self.diagnostics.record_tactical_command_delayed();
                    self.command_timeline.push(AppliedCommandV4 {
                        command_id,
                        accepted_sim_time_ms: now_ms,
                        scheduled_effective_sim_time_ms: effective_ms,
                        actual_effective_sim_time_ms: None,
                        applied_step_index: self.completed_beats,
                        status: CommandLifecycleStatus::Scheduled,
                        completed_beats: self.completed_beats,
                        game_second: (now_ms / 1000) as i32,
                        command: to_record,
                    });
                }
                Ok(())
            }
            MatchCommandV4::Substitute {
                side,
                player_out_id,
                player_in_id,
            } => self.handle_substitution_command(
                command_id,
                side,
                &[(player_out_id, player_in_id)],
                to_record,
            ),
            MatchCommandV4::SubstituteBatch { side, changes } => {
                let pairs = changes
                    .iter()
                    .map(|c| (c.player_out_id.clone(), c.player_in_id.clone()))
                    .collect::<Vec<_>>();
                self.handle_substitution_command(command_id, side, &pairs, to_record)
            }
        }
    }

    fn handle_substitution_command(
        &mut self,
        command_id: u64,
        side: TeamSideV4,
        changes: &[(String, String)],
        recorded_command: MatchCommandV4,
    ) -> Result<(), V4Error> {
        if changes.is_empty() {
            return Err(V4Error::InvalidInput("substitution group cannot be empty".into()));
        }
        let now_ms = self.state.des_clock.sim_time_ms;

        // Validate basic eligibility and quota upfront so invalid requests fail immediately.
        let period = self.state.des_clock.period;
        let max_subs = self.ruleset.max_substitutions_for_period(period);
        let max_windows = self.ruleset.max_in_play_sub_windows_for_period(period);
        let pending_for_side = self
            .pending_substitutions
            .iter()
            .filter(|s| s.side == side && s.status == CommandLifecycleStatus::QueuedForDeadBall)
            .count();
        let used_subs = self.state.team_state(side).substitutions_used as usize;
        if used_subs + pending_for_side + changes.len() > max_subs as usize {
            return Err(V4Error::InvalidInput("maximum substitutions reached".into()));
        }

        let at_interval = self.is_interval_substitution();
        if !at_interval
            && self.state.team_state(side).substitution_windows_used >= max_windows
            && pending_for_side == 0
        {
            return Err(V4Error::InvalidInput("maximum substitution windows reached".into()));
        }

        if at_interval {
            // Interval substitutions execute immediately without consuming an in-play window.
            let events = apply_manual_substitution_group(
                &mut self.state,
                &self.input,
                side,
                changes,
                true,
            )
            .map_err(|e| V4Error::InvalidInput(e.into()))?;
            let poss = self.possession_side;
            self.flush_play_events(events, poss);
            self.ensure_valid_carrier();
            self.command_timeline.push(AppliedCommandV4 {
                command_id,
                accepted_sim_time_ms: now_ms,
                scheduled_effective_sim_time_ms: now_ms,
                actual_effective_sim_time_ms: Some(now_ms),
                applied_step_index: self.completed_beats,
                status: CommandLifecycleStatus::Applied,
                completed_beats: self.completed_beats,
                game_second: (now_ms / 1000) as i32,
                command: recorded_command,
            });
            return Ok(());
        }

        // Validate players exist on pitch / bench and have no duplicates before queuing for next dead ball.
        let team_snap = match side {
            TeamSideV4::Home => &self.input.home_team,
            TeamSideV4::Away => &self.input.away_team,
        };
        let mut seen_out: Vec<&str> = Vec::with_capacity(changes.len());
        let mut seen_in: Vec<&str> = Vec::with_capacity(changes.len());
        for (out_id, in_id) in changes {
            if seen_out.contains(&out_id.as_str()) {
                return Err(V4Error::InvalidInput(format!(
                    "duplicate outgoing player '{out_id}' in substitution batch"
                )));
            }
            if seen_in.contains(&in_id.as_str()) {
                return Err(V4Error::InvalidInput(format!(
                    "duplicate incoming player '{in_id}' in substitution batch"
                )));
            }
            if self.pending_substitutions.iter().any(|s| {
                s.side == side
                    && s.status == CommandLifecycleStatus::QueuedForDeadBall
                    && s.player_out_id == *out_id
            }) {
                return Err(V4Error::InvalidInput(format!(
                    "outgoing player '{out_id}' is already queued for substitution"
                )));
            }
            if self.pending_substitutions.iter().any(|s| {
                s.side == side
                    && s.status == CommandLifecycleStatus::QueuedForDeadBall
                    && s.player_in_id == *in_id
            }) {
                return Err(V4Error::InvalidInput(format!(
                    "incoming player '{in_id}' is already queued for substitution"
                )));
            }
            seen_out.push(out_id.as_str());
            seen_in.push(in_id.as_str());

            let on_pitch = self
                .state
                .players
                .iter()
                .any(|p| p.side == side && p.player_id == *out_id && p.on_pitch && !p.sent_off);
            if !on_pitch {
                return Err(V4Error::InvalidInput(format!(
                    "outgoing player '{out_id}' is not active on pitch"
                )));
            }
            let on_bench = team_snap
                .bench
                .iter()
                .any(|p| p.player_id == *in_id && p.eligible && !p.injured)
                && !self.state.players.iter().any(|p| p.player_id == *in_id);
            if !on_bench {
                return Err(V4Error::InvalidInput(format!(
                    "incoming player '{in_id}' is not available on bench"
                )));
            }
        }

        for (out_id, in_id) in changes {
            self.pending_substitutions.push(PendingSubstitutionItem {
                command_id,
                side,
                origin: CommandOrigin::User,
                player_out_id: out_id.clone(),
                player_in_id: in_id.clone(),
                accepted_sim_time_ms: now_ms,
                status: CommandLifecycleStatus::QueuedForDeadBall,
            });
        }

        self.command_timeline.push(AppliedCommandV4 {
            command_id,
            accepted_sim_time_ms: now_ms,
            scheduled_effective_sim_time_ms: now_ms,
            actual_effective_sim_time_ms: None,
            applied_step_index: self.completed_beats,
            status: CommandLifecycleStatus::QueuedForDeadBall,
            completed_beats: self.completed_beats,
            game_second: (now_ms / 1000) as i32,
            command: recorded_command,
        });
        Ok(())
    }

    fn is_interval_substitution(&self) -> bool {
        if self.stage.is_break() {
            return self.ruleset.interval_subs_exempt_from_windows;
        }
        matches!(
            self.stage,
            HalfStageV4::InHalf {
                period: Period::SecondHalf | Period::ExtraTimeFirst | Period::ExtraTimeSecond,
                elapsed_in_half_ms: 0,
                ..
            }
        )
    }

    fn roster_snapshot(&self, side: TeamSideV4) -> TeamRosterStateV4 {
        let team = match side {
            TeamSideV4::Home => &self.input.home_team,
            TeamSideV4::Away => &self.input.away_team,
        };
        let on_pitch = self
            .state
            .players
            .iter()
            .filter(|p| p.side == side && p.on_pitch && !p.sent_off)
            .map(|p| p.player_id.clone())
            .collect::<Vec<_>>();
        let available_bench = team
            .bench
            .iter()
            .filter(|p| {
                p.eligible
                    && !p.injured
                    && !self
                        .state
                        .players
                        .iter()
                        .any(|runtime| runtime.player_id == p.player_id)
            })
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

    fn session_stage_summary(&self) -> SessionStageV4 {
        match self.stage {
            HalfStageV4::PreMatch => SessionStageV4::PreMatch,
            HalfStageV4::InHalf { .. } => SessionStageV4::InHalf,
            HalfStageV4::HalfTimeBreak => SessionStageV4::HalfTimeBreak,
            HalfStageV4::ExtraTimeBreak { .. } => SessionStageV4::ExtraTimeBreak,
            HalfStageV4::PenaltyShootout => SessionStageV4::PenaltyShootout,
            HalfStageV4::FullTime => SessionStageV4::FullTime,
        }
    }

    fn dead_ball_state_label(&self) -> String {
        match &self.state.dead_ball {
            None => "LIVE_PLAY".to_string(),
            Some(db) => format!("{:?}::{:?}", db.kind, db.phase),
        }
    }

    fn pending_command_summaries(&self) -> Vec<PendingCommandSummaryV4> {
        let mut out = Vec::new();
        for tac in &self.pending_tactical_commands {
            if tac.status == CommandLifecycleStatus::Scheduled {
                out.push(PendingCommandSummaryV4 {
                    command_id: tac.command_id,
                    command_type: "UPDATE_TACTICS".to_string(),
                    side: tac.side,
                    accepted_sim_time_ms: tac.accepted_sim_time_ms,
                    scheduled_effective_sim_time_ms: Some(tac.scheduled_effective_sim_time_ms),
                    status: tac.status,
                });
            }
        }
        for sub in &self.pending_substitutions {
            if sub.status == CommandLifecycleStatus::QueuedForDeadBall {
                out.push(PendingCommandSummaryV4 {
                    command_id: sub.command_id,
                    command_type: "SUBSTITUTE".to_string(),
                    side: sub.side,
                    accepted_sim_time_ms: sub.accepted_sim_time_ms,
                    scheduled_effective_sim_time_ms: None,
                    status: sub.status,
                });
            }
        }
        out
    }

    fn build_step_result(&mut self, steps_executed: u32, stop_reason: &str) -> SessionStepResultV4 {
        let prev_len = self.last_delivered_event_index;
        let new_events = if self.retain_events && self.events.len() >= prev_len {
            self.events[prev_len..].to_vec()
        } else {
            Vec::new()
        };
        self.last_delivered_event_index = self.events.len();

        let dc = &self.state.des_clock;
        SessionStepResultV4 {
            ok: true,
            period: dc.period as i32,
            game_second: dc.game_second(),
            period_elapsed_second: dc.period_elapsed_second(),
            sim_time_ms: dc.sim_time_ms,
            period_elapsed_ms: dc.period_elapsed_ms,
            stage: self.session_stage_summary(),
            dead_ball_state: self.dead_ball_state_label(),
            stop_reason: stop_reason.to_string(),
            ball_in_play_ms: dc.ball_in_play_ms,
            ball_out_of_play_ms: dc.ball_out_of_play_ms,
            compensable_lost_ms: dc.compensable_lost_ms,
            announced_added_ms: dc.announced_added_ms,
            additional_added_ms: dc.additional_added_ms,
            effective_added_ms: dc.effective_added_ms(),
            steps_executed,
            state_generation: self.scheduler.live_play_generation(),
            completed_beats: self.completed_beats,
            home_goals: self.state.home.goals,
            away_goals: self.state.away.goals,
            is_full_time: self.is_full_time(),
            total_events: self.event_count,
            new_events,
            pending_commands: self.pending_command_summaries(),
            home_roster: self.roster_snapshot(TeamSideV4::Home),
            away_roster: self.roster_snapshot(TeamSideV4::Away),
        }
    }

    /// Starts the next match period from `PreMatch`, `HalfTimeBreak`, or `ExtraTimeBreak`.
    pub fn start_next_period(&mut self) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.start_next_period_inner())
    }

    fn start_next_period_inner(&mut self) -> Result<SessionStepResultV4, V4Error> {
        if self.is_full_time() {
            return Err(V4Error::SessionFinished);
        }
        self.begin_next_period_internal()?;
        Ok(self.build_step_result(1, "PERIOD_STARTED"))
    }

    /// Advances an interactive match by up to `max_steps` DES events, stopping early on
    /// `HalfTimeBreak`, `ExtraTimeBreak`, or `FullTime`.
    pub fn advance_steps(&mut self, max_steps: usize) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.advance_steps_inner(max_steps))
    }

    fn advance_steps_inner(&mut self, max_steps: usize) -> Result<SessionStepResultV4, V4Error> {
        let steps_limit = max_steps.clamp(1, 500);
        let mut executed = 0u32;
        let mut stop_reason = "MAX_STEPS_REACHED";

        for _ in 0..steps_limit {
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            self.step_one_des_event(false)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
            executed = executed.saturating_add(1);
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
        }

        Ok(self.build_step_result(executed, stop_reason))
    }

    /// Processes scheduled DES events up to the requested time, stopping
    /// at a period break, full time, or the step guard. This is an
    /// event-aligned API: the returned sim_time_ms can be less than the
    /// requested horizon if the next event lies beyond it.
    /// Polling must not invent a movement interval or consume policy RNG.
    /// TARGET_TIME_REACHED denotes an exhausted horizon, not necessarily
    /// equality of the actual event clock and the requested horizon.
    /// A stale (past) target is a no-op during an active period.
    pub fn advance_until_ms(
        &mut self,
        target_sim_time_ms: i64,
        max_steps_guard: usize,
    ) -> Result<SessionStepResultV4, V4Error> {
        self.transact(|candidate| candidate.advance_until_ms_inner(target_sim_time_ms,max_steps_guard))
    }

    fn advance_until_ms_inner(
        &mut self,
        target_sim_time_ms: i64,
        max_steps_guard: usize,
    ) -> Result<SessionStepResultV4, V4Error> {
        if matches!(self.stage,HalfStageV4::InHalf{..})
            && target_sim_time_ms<self.state.des_clock.sim_time_ms {
            return Ok(self.build_step_result(0,"TARGET_TIME_REACHED"));
        }
        let guard = max_steps_guard.clamp(1, 10_000);
        let mut executed = 0u32;
        let mut stop_reason = "TARGET_TIME_REACHED";

        for _ in 0..guard {
            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            if matches!(self.stage, HalfStageV4::PreMatch) {
                self.step_one_des_event(false)?;
                self.completed_beats = self.completed_beats.saturating_add(1);
                executed = executed.saturating_add(1);
                continue;
            }
            if let Some(next_ms) = self.scheduler.peek_next_valid_time_ms() {
                if next_ms > target_sim_time_ms {
                    stop_reason = "TARGET_TIME_REACHED";
                    break;
                }
            }
            self.step_one_des_event(false)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
            executed = executed.saturating_add(1);

            if self.is_full_time() {
                stop_reason = "FULL_TIME";
                break;
            }
            if self.stage.is_break() {
                stop_reason = match self.stage {
                    HalfStageV4::HalfTimeBreak => "HALF_TIME_BREAK",
                    _ => "EXTRA_TIME_BREAK",
                };
                break;
            }
            if self.state.des_clock.sim_time_ms >= target_sim_time_ms {
                stop_reason = "TARGET_TIME_REACHED";
                break;
            }
        }

        if executed as usize >= guard && stop_reason == "TARGET_TIME_REACHED" && self.state.des_clock.sim_time_ms < target_sim_time_ms {
            stop_reason = "MAX_STEPS_GUARD_REACHED";
        }

        Ok(self.build_step_result(executed, stop_reason))
    }

    /// Test-only explicit integration boundary. The dispatcher ignores
    /// a tactical command with no matching entry. This exercises the same
    /// clock and movement path as a real mid-interval tactical event,
    /// without changing instructions or injecting an event into production.
    #[cfg(test)]
    pub(super) fn schedule_test_clock_probe(&mut self,at:i64)->Result<(),V4Error> {
        self.scheduler.schedule(at,EventPriority::PeriodBoundary,CancellationScope::PeriodScoped,
            DesEventPayload::TacticalCommandEffective{command_index:u32::MAX,side:TeamSideV4::Home})
            .map(|_|()).map_err(|e|V4Error::InvalidClock(e.into()))
    }

    /// Runs the match to Full Time (used by `simulate_v4`, `FastBatch`, and interactive `finish`).
    /// Automatically advances through `HalfTimeBreak` / `ExtraTimeBreak` with half-time recovery.
    pub fn run_to_full_time(&mut self) -> Result<MatchResultV4, V4Error> {
        self.transact(|candidate| candidate.run_to_full_time_inner())
    }

    fn run_to_full_time_inner(&mut self) -> Result<MatchResultV4, V4Error> {
        if self.finished {
            return Err(V4Error::SessionFinished);
        }
        while !self.is_full_time() {
            if self.stage.is_break() {
                self.begin_next_period_internal()?;
                continue;
            }
            self.step_one_des_event(true)?;
            self.completed_beats = self.completed_beats.saturating_add(1);
        }
        self.finished = true;
        Ok(self.build_result())
    }

    pub(super) fn begin_next_period_internal(&mut self) -> Result<(), V4Error> {
        match self.stage {
            HalfStageV4::PreMatch => {
                self.emit(
                    EventTypeV4::MatchStarted,
                    HighlightTier::None,
                    None,
                    None,
                    None,
                    None,
                    EventMetadataV4::default(),
                );
                self.begin_half(
                    Period::FirstHalf,
                    self.ruleset.half_duration_ms,
                    TeamSideV4::Home,
                )?;
                Ok(())
            }
            HalfStageV4::HalfTimeBreak => {
                self.apply_half_time_recovery();
                self.begin_half(
                    Period::SecondHalf,
                    self.ruleset.half_duration_ms,
                    TeamSideV4::Away,
                )?;
                Ok(())
            }
            HalfStageV4::ExtraTimeBreak { next_period } => {
                if next_period == Period::ExtraTimeFirst {
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
                        self.ruleset.extra_time_half_duration_ms,
                        TeamSideV4::Home,
                    )?;
                } else {
                    self.begin_half(
                        Period::ExtraTimeSecond,
                        self.ruleset.extra_time_half_duration_ms,
                        TeamSideV4::Away,
                    )?;
                }
                Ok(())
            }
            HalfStageV4::InHalf { .. } | HalfStageV4::PenaltyShootout => Ok(()),
            HalfStageV4::FullTime => Err(V4Error::SessionFinished),
        }
    }

    fn begin_half(
        &mut self,
        period: Period,
        regulation_ms: i64,
        kickoff_side: TeamSideV4,
    ) -> Result<(), V4Error> {
        self.scheduler.invalidate_period();
        self.pending_flight = None;
        self.pending_decision = None;
        self.pending_carry = None;
        self.state.dead_ball = None;
        self.state.des_clock.begin_period(period, regulation_ms);
        self.state.sync_legacy_clock();
        self.reset_kickoff_positions(kickoff_side);

        self.carrier_idx = self.kickoff_carrier_index(kickoff_side);
        self.possession_side = kickoff_side;
        self.last_passer_idx = None;
        self.last_dt_ms = 0;

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
            Some(kicker_id.clone()),
            None,
            EventMetadataV4::default(),
        );

        self.stage = HalfStageV4::InHalf {
            period,
            regulation_duration_ms: regulation_ms,
            elapsed_in_half_ms: 0,
            added_time_announced: false,
            announced_added_ms: 0,
            additional_added_ms: 0,
        };

        let now_ms = self.state.des_clock.sim_time_ms;
        self.scheduler
            .advance_cursor_to(now_ms)
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        // Schedule period regulation boundary for fourth-official added-time announcement.
        let announce_ms = now_ms.saturating_add(regulation_ms);
        self.scheduler
            .schedule(
                announce_ms,
                EventPriority::PeriodBoundary,
                CancellationScope::PeriodScoped,
                DesEventPayload::AnnounceAddedTime { period },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        // Schedule periodic AI manager checks every 5 simulated minutes (300_000 ms).
        let ai_check_ms = now_ms.saturating_add(300_000);
        if ai_check_ms < announce_ms {
            self.scheduler
                .schedule(
                    ai_check_ms,
                    EventPriority::TacticalAndCommand,
                    CancellationScope::PeriodScoped,
                    DesEventPayload::AiManagerCheck {
                        side: TeamSideV4::Home,
                    },
                )
                .map_err(|e| V4Error::InvalidClock(e.into()))?;
        }

        // Schedule first live-play decision from kickoff.
        let first_dt = 1; // observation, not an extra decision delay
        self.scheduler
            .schedule(
                now_ms.saturating_add(first_dt),
                EventPriority::ActionDecision,
                CancellationScope::LivePlayOnly,
                DesEventPayload::DecisionReady {
                    player_id: kicker_id,
                    side: kickoff_side,
                },
            )
            .map_err(|e| V4Error::InvalidClock(e.into()))?;

        Ok(())
    }

    pub(super) fn step_one_des_event(&mut self, auto_advance_breaks: bool) -> Result<(), V4Error> {
        // The retained journal is append-only inside a DES step. Exclude it
        // from the candidate clone to avoid quadratic copying in FullEvent.
        // The live session retains the old journal throughout execution.
        let history=std::mem::take(&mut self.events);
        let mut candidate=self.clone();
        self.events=history;
        let before_count=self.event_count;
        candidate.step_one_des_event_checked(auto_advance_breaks)?;
        let new_count=candidate.event_count.checked_sub(before_count)
            .ok_or_else(||V4Error::InvalidInput("INV-11: event sequence regressed".into()))?;
        if candidate.retain_events {
            if candidate.events.len()!=new_count as usize ||
                candidate.events.iter().enumerate().any(|(i,event)|event.sequence as u64!=before_count as u64+i as u64+1) ||
                candidate.events.windows(2).any(|w|w[0].sim_time_ms>w[1].sim_time_ms) ||
                self.events.last().zip(candidate.events.first()).is_some_and(|(old,new)|old.sim_time_ms>new.sim_time_ms) {
                return Err(V4Error::InvalidInput("INV-11: invalid event-local journal".into()));
            }
        } else if !candidate.events.is_empty(){
            return Err(V4Error::InvalidInput("INV-11: unexpected FastBatch journal".into()));
        }
        let mut history=std::mem::take(&mut self.events);
        history.append(&mut candidate.events);
        candidate.events=history;
        *self=candidate;
        Ok(())
    }

    fn step_one_des_event_checked(&mut self, auto_advance_breaks: bool) -> Result<(), V4Error> {
        let before=crate::v4::transition::goal_counters(&self.state);
        let before_ms=self.sim_time_ms();
        let before_events=self.event_count;
        let before_steps=self.completed_beats;
        self.goal_audit=Some(crate::v4::transition::GoalDeltaAudit::default());
        let result=self.step_one_des_event_inner(auto_advance_breaks);
        let audit=self.goal_audit.take().expect("event audit initialized");
        result?;
        audit.verify(before,&self.state)?;
        crate::v4::transition::verify_commit_progress(before_ms,self.sim_time_ms(),before_events,self.event_count,before_steps,self.completed_beats)?;
        crate::v4::transition::verify_clock_accounting(&self.state,self.home_possession_ms,self.away_possession_ms)?;
        crate::v4::transition::verify_scheduler_clock(&self.state, &self.scheduler)?;
        crate::v4::transition::verify_uncontrolled_ball(&self.state)?;
        Ok(())
    }

    fn step_one_des_event_inner(&mut self, auto_advance_breaks: bool) -> Result<(), V4Error> {
        match self.stage {
            HalfStageV4::PreMatch => {
                self.begin_next_period_internal()?;
                Ok(())
            }
            HalfStageV4::HalfTimeBreak | HalfStageV4::ExtraTimeBreak { .. } => {
                if auto_advance_breaks {
                    self.begin_next_period_internal()?;
                }
                Ok(())
            }
            HalfStageV4::FullTime => Ok(()),
            HalfStageV4::PenaltyShootout => {
                self.run_penalty_shootout()?;
                self.finish_full_time();
                Ok(())
            }
            HalfStageV4::InHalf { period, .. } => {
                let next_ev = self
                    .scheduler
                    .pop_next_valid()
                    .map_err(|e| V4Error::InvalidClock(e.into()))?;

                let Some(ev) = next_ev else {
                    // Queue unexpectedly empty during half: schedule immediate period-end check.
                    self.complete_current_half(period, auto_advance_breaks)?;
                    return Ok(());
                };

                self.diagnostics.record_des_step();
                let prev_ms = self.state.des_clock.sim_time_ms;
                let dt_ms = ev.event_time_ms.checked_sub(prev_ms)
                    .filter(|dt|*dt>=0)
                    .ok_or_else(||V4Error::InvalidClock("INV-03: scheduled event precedes authoritative clock".into()))?;
                self.last_dt_ms = dt_ms;
                let is_ball_in_play = self.state.dead_ball.is_none();

                if dt_ms > 0 {
                    self.state
                        .des_clock
                        .advance_ms(dt_ms, is_ball_in_play)
                        .map_err(|e| V4Error::InvalidClock(e.into()))?;
                    self.state.sync_legacy_clock();
                    self.sync_stage_from_clock();

                    // CRITICAL-08: Centralized world integration BEFORE dispatching any event.
                    let interrupted_by_injury = self.advance_world_for_dt(dt_ms, is_ball_in_play)?;
                    // Injury invalidates live-play actions, but must never discard a
                    // period boundary already popped from the scheduler. Losing the
                    // announcement/end check can leave a half running until watchdog.
                    if interrupted_by_injury
                        && !matches!(
                            &ev.payload,
                            DesEventPayload::AnnounceAddedTime { .. }
                                | DesEventPayload::CheckPeriodEnd { .. }
                        )
                    {
                        return Ok(());
                    }
                }

                match ev.payload {
                    DesEventPayload::AnnounceAddedTime { period: p } => {
                        self.handle_announce_added_time(p, auto_advance_breaks)?;
                    }
                    DesEventPayload::CheckPeriodEnd { period: p } => {
                        self.handle_check_period_end(p, auto_advance_breaks)?;
                    }
                    DesEventPayload::TacticalCommandEffective {
                        command_index,
                        side,
                    } => {
                        self.handle_tactical_command_effective(command_index as usize, side);
                    }
                    DesEventPayload::AiManagerCheck { .. } => {
                        self.handle_ai_manager_check(period)?;
                    }
                    DesEventPayload::DeadBallPhaseComplete { .. } => {
                        self.handle_dead_ball_phase_complete(dt_ms)?;
                    }
                    DesEventPayload::EarlyInteractionResolved { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_early_interaction_resolved()?;
                    }
                    DesEventPayload::BallInFlightArrival { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_ball_in_flight_arrival()?;
                    }
                    DesEventPayload::FirstTouchComplete { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_first_touch_complete()?;
                    }
                    DesEventPayload::ShotArrival { .. } => {
                        if self.state.dead_ball.is_some() {
                            return Ok(());
                        }
                        self.handle_shot_arrival()?;
                    }
                    DesEventPayload::DecisionReady { .. } => {
                        if self.state.dead_ball.is_none() {
                            self.begin_player_decision(false)?;
                        }
                    }
                    DesEventPayload::ActionExecute { player_id, side } => {
                        if self.state.dead_ball.is_none() {
                            self.execute_player_decision(&player_id, side)?;
                        }
                    }
                    DesEventPayload::DribbleStep { carrier_id, side } => {
                        if self.state.dead_ball.is_none() {
                            self.finish_timed_dribble(&carrier_id, side)?;
                        }
                    }
                }

                Ok(())
            }
        }
    }

    fn sync_stage_from_clock(&mut self) {
        if let HalfStageV4::InHalf {
            period,
            regulation_duration_ms,
            ..
        } = self.stage
        {
            self.stage = HalfStageV4::InHalf {
                period,
                regulation_duration_ms,
                elapsed_in_half_ms: self.state.des_clock.period_elapsed_ms,
                added_time_announced: self.state.des_clock.added_time_locked,
                announced_added_ms: self.state.des_clock.announced_added_ms,
                additional_added_ms: self.state.des_clock.additional_added_ms,
            };
        }
    }

}

// Preserve the session module's private state and method namespace.
include!("session_parts/01_world.rs");
include!("session_parts/02_stoppage.rs");
include!("session_parts/03_actions.rs");
include!("session_parts/04_period.rs");
include!("session_parts/05_condition.rs");
include!("session_parts/06_telemetry.rs");

fn map_dead_ball_diag_kind(kind: DeadBallKind) -> Option<DeadBallDiagKind> {
    match kind {
        DeadBallKind::GoalCelebration => Some(DeadBallDiagKind::GoalCelebration),
        DeadBallKind::ThrowIn => Some(DeadBallDiagKind::ThrowIn),
        DeadBallKind::Corner => Some(DeadBallDiagKind::Corner),
        DeadBallKind::GoalKick => Some(DeadBallDiagKind::GoalKick),
        DeadBallKind::QuickFreeKick
        | DeadBallKind::DirectFreeKick
        | DeadBallKind::IndirectFreeKick
        | DeadBallKind::Offside => Some(DeadBallDiagKind::FreeKick),
        DeadBallKind::PenaltyKick => Some(DeadBallDiagKind::Penalty),
        DeadBallKind::InjuryStoppage => Some(DeadBallDiagKind::Injury),
        DeadBallKind::SubstitutionWindow => Some(DeadBallDiagKind::Substitution),
        DeadBallKind::Kickoff => None,
    }
}

#[cfg(test)]
include!("session_tests.rs");
