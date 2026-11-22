use crate::{
    ai_manager::{AiManagerConfig, AiManagerDirector},
    clock::Period,
    commands::{CommandType, MatchCommand},
    discipline,
    events::{EventType, MatchEvent, MatchEventFacts, MatchEventSink},
    fingerprints as fp,
    football_resolver,
    geometry::TeamSide,
    input::MatchInput,
    movement,
    resolver_support::{java_round, kv, MatchRuntime},
    rng::{self, DeterministicRng},
    roster,
    state::{MatchState, RestartType, TacticalPhase},
    stats::{MatchResult, StatsCollector},
};

pub const ENGINE_VERSION: &str = "v3.17.0-realism-1";
pub const MAX_RETAINED_EVENTS: usize = 16384;

#[derive(Clone, Debug)]
struct PendingCommand {
    sequence: i64,
    command: MatchCommand,
}

pub struct MatchSession {
    input: MatchInput,
    state: MatchState,
    rng: DeterministicRng,
    retain_events: bool,
    events: Vec<MatchEvent>,
    commands: Vec<PendingCommand>,
    event_sinks: Vec<Box<dyn MatchEventSink>>,
    stats_collector: StatsCollector,
    event_sequence: i64,
    command_sequence: i64,
    event_timeline_fingerprint: u64,
    started: bool,
    first_half_kickoff_side: TeamSide,
    extra_time_kickoff_side: TeamSide,
    event_timeline_truncated: bool,
    last_emitted_facts: MatchEventFacts,
    final_result: Option<MatchResult>,
    virtual_clock_remainder: i32,
    played_physical_seconds: i64,
    ai_manager: AiManagerDirector,
}

impl MatchSession {
    pub fn new(input: MatchInput, retain_events: bool) -> Self {
        let state = MatchState::new(&input);
        let rng = DeterministicRng::new(rng::derive_seed(input.seed, "football-core-v3"));
        let stats_collector = StatsCollector::new(&input);
        let last_emitted_facts = MatchEventFacts::capture(&input, &state);
        Self {
            input,
            state,
            rng,
            retain_events,
            events: Vec::new(),
            commands: Vec::new(),
            event_sinks: Vec::new(),
            stats_collector,
            event_sequence: 0,
            command_sequence: 0,
            event_timeline_fingerprint: fp::start(),
            started: false,
            first_half_kickoff_side: TeamSide::Home,
            extra_time_kickoff_side: TeamSide::Home,
            event_timeline_truncated: false,
            last_emitted_facts,
            final_result: None,
            virtual_clock_remainder: 0,
            played_physical_seconds: 0,
            ai_manager: AiManagerDirector::new(AiManagerConfig::none()),
        }
    }

    pub fn add_event_sink(&mut self, sink: Box<dyn MatchEventSink>) -> Result<(), &'static str> {
        if self.started || self.event_sequence > 0 {
            return Err("event sink must be attached before match starts");
        }
        self.event_sinks.push(sink);
        Ok(())
    }

    pub fn restore_event_sink(&mut self, sink: Box<dyn MatchEventSink>) -> Result<(), &'static str> {
        if !self.started || self.state.clock.is_full_time() {
            return Err("event sink restore requires a live resumed match");
        }
        self.event_sinks.push(sink);
        Ok(())
    }

    pub fn is_retaining_events(&self) -> bool { self.retain_events }

    pub fn set_ai_manager(&mut self, config: AiManagerConfig) {
        self.ai_manager = AiManagerDirector::new(config);
    }

    pub fn input(&self) -> &MatchInput {
        &self.input
    }

    pub fn submit_command(&mut self, command: MatchCommand) -> Result<(), &'static str> {
        if self.state.clock.is_full_time() {
            return Err("match already finished");
        }
        if command.game_second < self.state.clock.game_second {
            return Err("cannot submit command in the past");
        }
        let rules = &self.input.match_rules;
        let mut max = rules.regulation_duration_seconds + 2 * rules.regulation_added_time_cap_seconds;
        if rules.allow_extra_time {
            max += rules.extra_time_duration_seconds + 2 * rules.extra_time_added_time_cap_seconds;
        }
        if command.game_second > max {
            return Err("command after possible match time");
        }
        self.command_sequence += 1;
        self.commands.push(PendingCommand {
            sequence: self.command_sequence,
            command,
        });
        self.commands.sort_by(|a, b| {
            a.command
                .game_second
                .cmp(&b.command.game_second)
                .then_with(|| a.sequence.cmp(&b.sequence))
        });
        Ok(())
    }

    pub fn advance(&mut self) -> bool {
        if self.state.clock.is_full_time() {
            return false;
        }
        if !self.started {
            self.start_match();
            return true;
        }
        if self.state.clock.period == Period::Shootout {
            self.resolve_penalty_shootout();
            self.finish_match();
            return false;
        }
        if self.handle_period_boundary() {
            return !self.state.clock.is_full_time();
        }
        if self.apply_due_command() {
            return true;
        }
        if self.has_pending_goalkeeper_assignment() {
            return true;
        }
        football_resolver::step(self);
        if (self.ai_manager.config.control_home || self.ai_manager.config.control_away)
            && !self.state.clock.is_full_time()
        {
            let ai_cmds = self.ai_manager.evaluate(&self.input, &self.state);
            for cmd in ai_cmds {
                let _ = self.submit_command(cmd);
            }
        }
        !self.state.clock.is_full_time()
    }

    pub fn advance_until(&mut self, game_second: i32) -> Result<(), &'static str> {
        let target = game_second.max(0);
        let mut guard = 0;
        while (!self.started || self.state.clock.game_second < target) && !self.state.clock.is_full_time() {
            self.advance();
            guard += 1;
            if guard > 200_000 {
                return Err("advanceUntil guard exceeded");
            }
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<MatchResult, &'static str> {
        if let Some(ref res) = self.final_result {
            return Ok(res.clone());
        }
        let mut guard = 0;
        while !self.state.clock.is_full_time() {
            self.advance();
            guard += 1;
            if guard > 200_000 {
                return Err("finish guard exceeded");
            }
        }
        let statistics = self.stats_collector.finish(&self.input, &self.state);
        let result = MatchResult::new(
            &self.input,
            &self.state,
            self.events.clone(),
            statistics,
            self.event_sequence as i32,
            self.event_timeline_fingerprint,
            self.retain_events && !self.event_timeline_truncated,
        );
        self.final_result = Some(result.clone());
        Ok(result)
    }

    pub fn played_physical_seconds(&self) -> i64 { self.played_physical_seconds }
    pub fn game_second(&self) -> i32 { self.state.clock.game_second }
    pub fn is_full_time(&self) -> bool { self.state.clock.is_full_time() }
    pub fn snapshot(&self) -> MatchState { self.state.clone() }
    pub fn events(&self) -> &[MatchEvent] { &self.events }
    pub fn event_count(&self) -> i32 { self.event_sequence as i32 }
    pub fn is_event_timeline_truncated(&self) -> bool { self.event_timeline_truncated }

    fn start_match(&mut self) {
        self.started = true;
        self.state.clock.start_period(Period::FirstHalf);
        self.first_half_kickoff_side = if self.rng.next_boolean() { TeamSide::Home } else { TeamSide::Away };
        self.emit(EventType::MatchStarted, None, None, None, Vec::new());
        let side = self.first_half_kickoff_side;
        self.kickoff(side, "FIRST_HALF");
    }

    fn kickoff(&mut self, side: TeamSide, period: &str) {
        movement::reset_for_kickoff(&self.input, &mut self.state);
        let p = self.choose_kickoff_player(side);
        self.state.restart.set(RestartType::Kickoff, Some(side), period, 50, 50);
        if let Some(kp) = self.state.player_mut(&p) {
            kp.set_position(50, 50);
            kp.set_target(50, 50);
        }
        self.state.ball.set_position(50, 50);
        self.state.ball.set_controlled(side, p.clone());
        self.state.ball.set_movement_intent("KICKOFF_RESTART");
        let sec = self.state.clock.game_second;
        self.state.possession.begin(side, sec);
        self.state.team_mut(side).possession_sequences += 1;
        let phase = TacticalPhase::from_ball(side, 50);
        self.state.possession.phase = phase;
        self.state.team_mut(side).record_phase_entry(phase);
        self.emit(EventType::Kickoff, Some(side), Some(&p), None, vec![kv("period", period)]);
        self.emit(
            EventType::PossessionStarted,
            Some(side),
            Some(&p),
            None,
            vec![kv("phase", phase.as_str())],
        );
        self.state.restart.clear();
        movement::plan_targets(&self.input, &mut self.state);
    }

    fn handle_period_boundary(&mut self) -> bool {
        let p = self.state.clock.period;
        if matches!(p, Period::NotStarted | Period::FullTime | Period::Shootout) {
            return false;
        }
        let nominal_end = self.state.clock.period_started_at_second + self.period_nominal_duration(p);
        if !self.state.clock.added_time_locked && self.state.clock.game_second >= nominal_end {
            let cap = if matches!(p, Period::FirstHalf | Period::SecondHalf) {
                self.input.match_rules.regulation_added_time_cap_seconds
            } else {
                self.input.match_rules.extra_time_added_time_cap_seconds
            };
            let added = self.state.clock.lock_added_time(cap);
            self.emit(
                EventType::AddedTimeAnnounced,
                None,
                None,
                None,
                vec![kv("period", period_name(p)), kv("seconds", added)],
            );
            if added > 0 {
                return true;
            }
        }
        let end = nominal_end
            + if self.state.clock.added_time_locked {
                self.state.clock.effective_added_time()
            } else {
                0
            };
        if self.state.clock.game_second < end {
            return false;
        }

        match p {
            Period::FirstHalf => {
                let eff = self.state.clock.effective_added_time();
                let ann = self.state.clock.added_time_seconds;
                let ext = self.state.clock.added_time_extension_seconds;
                self.emit(
                    EventType::HalfTime,
                    None,
                    None,
                    None,
                    vec![
                        kv("addedSeconds", eff),
                        kv("announcedAddedSeconds", ann),
                        kv("extensionSeconds", ext),
                    ],
                );
                movement::reset_for_kickoff(&self.input, &mut self.state);
                self.state.clock.start_period(Period::SecondHalf);
                let side = self.first_half_kickoff_side.opposite();
                self.kickoff(side, "SECOND_HALF");
                true
            }
            Period::SecondHalf => {
                let (hg, ag) = (self.state.score.home_goals, self.state.score.away_goals);
                if self.input.match_rules.requires_extra_time(hg, ag) {
                    self.extra_time_kickoff_side = if self.rng.next_boolean() { TeamSide::Home } else { TeamSide::Away };
                    let score = self.score_text();
                    self.emit(EventType::ExtraTimeStarted, None, None, None, vec![kv("score", score)]);
                    movement::reset_for_kickoff(&self.input, &mut self.state);
                    self.state.clock.start_period(Period::ExtraTimeFirst);
                    let side = self.extra_time_kickoff_side;
                    self.kickoff(side, "EXTRA_TIME_FIRST");
                    return true;
                }
                if self.input.match_rules.requires_shootout(hg, ag) && !self.input.match_rules.allow_extra_time {
                    self.start_shootout();
                    return true;
                }
                self.finish_match();
                true
            }
            Period::ExtraTimeFirst => {
                let eff = self.state.clock.effective_added_time();
                let ann = self.state.clock.added_time_seconds;
                let ext = self.state.clock.added_time_extension_seconds;
                self.emit(
                    EventType::ExtraTimeHalfTime,
                    None,
                    None,
                    None,
                    vec![
                        kv("addedSeconds", eff),
                        kv("announcedAddedSeconds", ann),
                        kv("extensionSeconds", ext),
                    ],
                );
                movement::reset_for_kickoff(&self.input, &mut self.state);
                self.state.clock.start_period(Period::ExtraTimeSecond);
                let side = self.extra_time_kickoff_side.opposite();
                self.kickoff(side, "EXTRA_TIME_SECOND");
                true
            }
            Period::ExtraTimeSecond => {
                let (hg, ag) = (self.state.score.home_goals, self.state.score.away_goals);
                if self.input.match_rules.requires_shootout(hg, ag) {
                    self.start_shootout();
                    return true;
                }
                self.finish_match();
                true
            }
            _ => false,
        }
    }

    fn period_nominal_duration(&self, p: Period) -> i32 {
        let r = &self.input.match_rules;
        match p {
            Period::FirstHalf => r.half_duration_seconds,
            Period::SecondHalf => r.regulation_duration_seconds - r.half_duration_seconds,
            Period::ExtraTimeFirst => r.extra_time_duration_seconds / 2,
            Period::ExtraTimeSecond => r.extra_time_duration_seconds - r.extra_time_duration_seconds / 2,
            _ => 0,
        }
    }

    fn current_boundary(&self) -> i32 {
        let c = &self.state.clock;
        let nominal = c.period_started_at_second + self.period_nominal_duration(c.period);
        nominal + if c.added_time_locked { c.effective_added_time() } else { 0 }
    }

    fn start_shootout(&mut self) {
        self.state.clock.set_period(Period::Shootout);
        let score = self.score_text();
        self.emit(EventType::PenaltyShootoutStarted, None, None, None, vec![kv("score", score)]);
    }

    fn resolve_penalty_shootout(&mut self) {
        let first = if self.rng.next_boolean() { TeamSide::Home } else { TeamSide::Away };
        let home_active = roster::active_player_ids_in_slot_order(&self.input, &self.state, TeamSide::Home).len();
        let away_active = roster::active_player_ids_in_slot_order(&self.input, &self.state, TeamSide::Away).len();
        let equal_eligible = home_active.min(away_active);
        if equal_eligible == 0 {
            return;
        }
        let home_order = self.shootout_order(TeamSide::Home, equal_eligible);
        let away_order = self.shootout_order(TeamSide::Away, equal_eligible);
        let (mut h, mut a, mut taken_h, mut taken_a) = (0, 0, 0, 0);
        for kick in 0..400 {
            let side = if kick % 2 == 0 { first } else { first.opposite() };
            let round = kick / 2;
            let order = if side == TeamSide::Home { &home_order } else { &away_order };
            let taker = order[round % order.len()].clone();
            let scored = self.shootout_kick(side, &taker);
            if side == TeamSide::Home {
                taken_h += 1;
                if scored {
                    h += 1;
                    self.state.score.add_shootout_goal(side);
                }
            } else {
                taken_a += 1;
                if scored {
                    a += 1;
                    self.state.score.add_shootout_goal(side);
                }
            }
            let opp_gk = self.state.team(side.opposite()).designated_goalkeeper_id.clone();
            self.emit(
                EventType::PenaltyShootoutKick,
                Some(side),
                Some(&taker),
                opp_gk.as_deref(),
                vec![
                    kv("round", round + 1),
                    kv("scored", scored),
                    kv("shootout", format!("{}-{}", h, a)),
                    kv("eligiblePlayers", equal_eligible),
                ],
            );
            if taken_h <= 5 || taken_a <= 5 {
                let rem_h = (5 - taken_h).max(0);
                let rem_a = (5 - taken_a).max(0);
                if h > a + rem_a || a > h + rem_h {
                    self.emit(
                        EventType::PenaltyShootoutEnded,
                        None,
                        None,
                        None,
                        vec![kv("shootout", format!("{}-{}", h, a))],
                    );
                    return;
                }
            }
            if taken_h >= 5 && taken_a >= 5 && taken_h == taken_a && h != a {
                self.emit(
                    EventType::PenaltyShootoutEnded,
                    None,
                    None,
                    None,
                    vec![kv("shootout", format!("{}-{}", h, a))],
                );
                return;
            }
        }
    }

    fn shootout_kick(&mut self, side: TeamSide, taker: &str) -> bool {
        let g = self.state.team(side.opposite()).designated_goalkeeper_id.as_deref();
        let sp = self.input.team(side).player(taker);
        let gp = g.and_then(|id| self.input.team(side.opposite()).player(id));
        let skill = sp
            .map(|p| p.attributes.get(6) as i32 + p.attributes.get(14) as i32 + p.attributes.get(4) as i32)
            .unwrap_or(150);
        let keeper = gp
            .map(|p| p.attributes.get(0) as i32 + p.attributes.get(9) as i32 + p.attributes.get(13) as i32)
            .unwrap_or(30);
        let chance = (70 + (skill - keeper) / 12).clamp(48, 91);
        self.rng.next_int(100) < chance
    }

    fn shootout_order(&self, side: TeamSide, limit: usize) -> Vec<String> {
        let mut ids = roster::active_player_ids_in_slot_order(&self.input, &self.state, side);
        let team = self.input.team(side);
        ids.sort_by(|a, b| {
            let qa = team
                .player(a)
                .map(|p| p.attributes.get(6) as i32 + p.attributes.get(14) as i32 + p.attributes.get(4) as i32)
                .unwrap_or(0);
            let qb = team
                .player(b)
                .map(|p| p.attributes.get(6) as i32 + p.attributes.get(14) as i32 + p.attributes.get(4) as i32)
                .unwrap_or(0);
            qb.cmp(&qa).then_with(|| a.cmp(b))
        });
        ids.truncate(limit);
        ids
    }

    fn finish_match(&mut self) {
        if self.state.clock.is_full_time() {
            return;
        }
        self.state.clock.set_period(Period::FullTime);
        let score = self.score_text();
        let shootout = format!(
            "{}-{}",
            self.state.score.home_shootout_goals, self.state.score.away_shootout_goals
        );
        self.emit(
            EventType::FullTime,
            None,
            None,
            None,
            vec![kv("score", score), kv("shootout", shootout)],
        );
    }

    fn apply_due_command(&mut self) -> bool {
        let idx = match self.next_executable_due_command_index() {
            Some(i) => i,
            None => return false,
        };
        let cmd = self.commands.remove(idx).command;
        let res: Result<&'static str, &'static str> = match cmd.kind {
            CommandType::Substitute => self.apply_substitute(&cmd).map(|_| "SUBSTITUTION_APPLIED"),
            CommandType::AssignGoalkeeper => match cmd.player_id.as_deref() {
                Some(pid) => self.assign_goalkeeper(cmd.side, pid, "COMMAND").map(|_| "GOALKEEPER_ASSIGNED"),
                None => Err("goalkeeper must be active"),
            },
            CommandType::ReassignSlot => self.apply_reassign(&cmd).map(|_| "SLOT_REASSIGNED"),
            CommandType::ChangePressing => self.apply_team_instruction(&cmd, "PRESSING").map(|_| "TACTIC_APPLIED"),
            CommandType::ChangeTempo => self.apply_team_instruction(&cmd, "TEMPO").map(|_| "TACTIC_APPLIED"),
            CommandType::ChangeWidth => self.apply_team_instruction(&cmd, "WIDTH").map(|_| "TACTIC_APPLIED"),
            CommandType::ChangeLineHeight => self.apply_team_instruction(&cmd, "LINE_HEIGHT").map(|_| "TACTIC_APPLIED"),
            CommandType::ChangeTeamInstruction => match cmd.string_value.as_deref() {
                Some(k) => self.apply_team_instruction(&cmd, k).map(|_| "TACTIC_APPLIED"),
                None => Err("instruction key required"),
            },
            CommandType::ChangeSlotInstruction => self.apply_slot_instruction(&cmd).map(|_| "SLOT_TACTIC_APPLIED"),
            CommandType::ChangeShape => self.apply_shape(&cmd).map(|_| "SHAPE_APPLIED"),
            CommandType::ChangeSetPiecePlan => self.apply_set_piece_plan(&cmd).map(|_| "SET_PIECE_PLAN_APPLIED"),
            CommandType::ChangePlayerMorale => self.apply_player_morale(&cmd).map(|_| "PLAYER_MORALE_APPLIED"),
        };
        let (ok, reason) = match res {
            Ok(r) => (true, r),
            Err(e) => (false, e),
        };
        let applied_sec = self.state.clock.game_second;
        self.emit(
            if ok { EventType::CommandAccepted } else { EventType::CommandRejected },
            Some(cmd.side),
            cmd.player_id.as_deref(),
            cmd.secondary_player_id.as_deref(),
            vec![
                kv("commandType", cmd.kind.as_str()),
                kv("requestedSecond", cmd.game_second),
                kv("appliedSecond", applied_sec),
                kv("effect", reason),
                kv("slotId", cmd.slot_id.as_deref().unwrap_or("null")),
                kv("intValue", cmd.int_value),
                kv("stringValue", cmd.string_value.as_deref().unwrap_or("null")),
            ],
        );
        true
    }

    fn next_executable_due_command_index(&self) -> Option<usize> {
        let now = self.state.clock.game_second;
        for (i, p) in self.commands.iter().enumerate() {
            if p.command.game_second > now {
                break;
            }
            if p.command.kind == CommandType::Substitute && !self.substitution_can_apply_now() {
                continue;
            }
            return Some(i);
        }
        None
    }

    fn substitution_can_apply_now(&self) -> bool {
        if self.state.restart.kind != RestartType::None {
            return true;
        }
        let c = &self.state.clock;
        !matches!(c.period, Period::NotStarted | Period::FullTime | Period::Shootout)
            && c.game_second == c.period_started_at_second
    }

    fn apply_substitute(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        let rules = &self.input.match_rules;
        let sec = self.state.clock.game_second;
        let period = self.state.clock.period;
        let period_start = self.state.clock.period_started_at_second;
        {
            let tr = self.state.team(cmd.side);
            if tr.substitutions_used >= rules.substitution_limit {
                return Err("substitution limit reached");
            }
            let consume = tr.substitution_consumes_window(sec, period, period_start);
            if consume && tr.substitution_windows_used >= rules.substitution_windows {
                return Err("substitution windows reached");
            }
        }
        let out_id = cmd.player_id.as_deref().ok_or("out player not active/replacable")?;
        let in_id = cmd.secondary_player_id.as_deref().ok_or("incoming player not on bench")?;
        let (slot, x, y) = {
            let out = self.state.player(out_id).ok_or("out player not active/replacable")?;
            if out.side != cmd.side || (!out.on_pitch && !(out.injured && !out.red_card)) {
                return Err("out player not active/replacable");
            }
            (out.slot_id.clone(), out.x, out.y)
        };
        if !self.input.team(cmd.side).bench.iter().any(|p| p.player_id == in_id) {
            return Err("incoming player not on bench");
        }
        if self.state.player(in_id).is_some() {
            return Err("incoming player already used");
        }
        let owner = self.state.ball.owner_player_id.as_deref() == Some(out_id);
        let replaces_gk = self.state.team(cmd.side).designated_goalkeeper_id.as_deref() == Some(out_id);
        let consume_window = self.state.team(cmd.side).substitution_consumes_window(sec, period, period_start);

        if let Some(o) = self.state.player_mut(out_id) {
            o.on_pitch = false;
        }
        self.state.add_substitute_runtime(&self.input, cmd.side, in_id, &slot, x, y)?;
        self.state.team_mut(cmd.side).add_substitution(sec, consume_window);
        if owner {
            self.state.ball.set_controlled(cmd.side, in_id);
        }
        if replaces_gk {
            self.state.team_mut(cmd.side).require_goalkeeper_assignment();
        }
        self.add_stoppage(30);
        let subs_used = self.state.team(cmd.side).substitutions_used;
        let win_used = self.state.team(cmd.side).substitution_windows_used;
        self.emit(
            EventType::Substitution,
            Some(cmd.side),
            Some(out_id),
            Some(in_id),
            vec![
                kv("slotId", &slot),
                kv("substitutionsUsed", subs_used),
                kv("substitutionWindowsUsed", win_used),
                kv("newWindow", consume_window),
                kv("goalkeeperDuty", replaces_gk),
            ],
        );
        if replaces_gk {
            self.emit(
                EventType::GoalkeeperAssignmentRequired,
                Some(cmd.side),
                None,
                None,
                vec![kv("reason", "SUBSTITUTION")],
            );
        } else {
            self.ensure_goalkeeper(cmd.side, "SUBSTITUTION");
        }
        Ok(())
    }

    fn apply_reassign(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        let pid = cmd.player_id.as_deref().ok_or("player not active")?;
        let new_slot = cmd.slot_id.as_deref().ok_or("unknown slot")?;
        if self.input.team(cmd.side).slot(new_slot).is_none() {
            return Err("unknown slot");
        }
        let old_slot = {
            let p = self.state.player(pid).ok_or("player not active")?;
            if !p.on_pitch || p.side != cmd.side {
                return Err("player not active");
            }
            p.slot_id.clone()
        };
        let other_id = self
            .state
            .players
            .iter()
            .find(|(_, r)| r.on_pitch && r.side == cmd.side && r.slot_id == new_slot && r.player_id != pid)
            .map(|(_, r)| r.player_id.clone());
        if let Some(p) = self.state.player_mut(pid) {
            p.slot_id = new_slot.into();
        }
        if let Some(oid) = other_id {
            if let Some(o) = self.state.player_mut(&oid) {
                o.slot_id = old_slot;
            }
        }
        movement::plan_targets(&self.input, &mut self.state);
        Ok(())
    }

    fn apply_team_instruction(&mut self, cmd: &MatchCommand, key: &str) -> Result<(), &'static str> {
        if !(0..=100).contains(&cmd.int_value) {
            return Err("instruction value out of range");
        }
        self.state.team_mut(cmd.side).set_team_instruction(key, cmd.int_value)?;
        movement::plan_targets(&self.input, &mut self.state);
        Ok(())
    }

    fn apply_slot_instruction(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        let slot_id = cmd.slot_id.as_deref().ok_or("slotId required")?;
        let key = cmd.string_value.as_deref().ok_or("slot instruction key required")?;
        if !(0..=100).contains(&cmd.int_value) {
            return Err("slot instruction value out of range");
        }
        self.state.team_mut(cmd.side).set_slot_instruction(slot_id, key, cmd.int_value)?;
        movement::plan_targets(&self.input, &mut self.state);
        Ok(())
    }

    fn apply_shape(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        if cmd.tactical_slots.len() != 11 {
            return Err("shape command requires 11 tactical slots");
        }
        self.state
            .team_mut(cmd.side)
            .set_shape(cmd.string_value.as_deref(), &cmd.tactical_slots)?;
        movement::plan_targets(&self.input, &mut self.state);
        Ok(())
    }

    fn apply_set_piece_plan(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        let plan = cmd.set_piece_plan.clone().ok_or("set-piece plan required")?;
        self.state.team_mut(cmd.side).set_piece_plan = plan;
        Ok(())
    }

    fn apply_player_morale(&mut self, cmd: &MatchCommand) -> Result<(), &'static str> {
        let pid = cmd.player_id.as_deref().ok_or("playerId required")?;
        if !(0..=100).contains(&cmd.int_value) {
            return Err("morale out of range");
        }
        let r = self.state.player_mut(pid).ok_or("morale target must be active")?;
        if r.side != cmd.side || !r.on_pitch {
            return Err("morale target must be active");
        }
        r.set_morale_override(cmd.int_value);
        Ok(())
    }

    fn assign_goalkeeper(&mut self, side: TeamSide, id: &str, reason: &str) -> Result<(), &'static str> {
        let active = self.state.player(id).map(|r| r.on_pitch && r.side == side).unwrap_or(false);
        if !active {
            return Err("goalkeeper must be active");
        }
        let tr = self.state.team_mut(side);
        tr.designated_goalkeeper_id = Some(id.into());
        tr.clear_goalkeeper_assignment_requirement();
        let gk_stat = self.input.team(side).player(id).map(|p| p.attributes.get(0) as i32).unwrap_or(0);
        self.emit(
            EventType::GoalkeeperReassigned,
            Some(side),
            Some(id),
            None,
            vec![kv("reason", reason), kv("goalkeeping", gk_stat)],
        );
        Ok(())
    }

    fn has_pending_goalkeeper_assignment(&self) -> bool {
        self.state.home_team.requires_goalkeeper_assignment || self.state.away_team.requires_goalkeeper_assignment
    }

    fn choose_kickoff_player(&self, side: TeamSide) -> String {
        let team = self.input.team(side);
        let mut best: Option<String> = None;
        let mut best_y = -1;
        for id in roster::active_player_ids_in_slot_order(&self.input, &self.state, side) {
            if let Some(r) = self.state.player(&id) {
                if let Some(slot) = team.slot(&r.slot_id) {
                    if best.is_none() || slot.in_y > best_y || (slot.in_y == best_y && id < *best.as_ref().unwrap()) {
                        best = Some(id);
                        best_y = slot.in_y;
                    }
                }
            }
        }
        best.expect("team without active kickoff player")
    }

    fn score_text(&self) -> String {
        format!("{}-{}", self.state.score.home_goals, self.state.score.away_goals)
    }
}

impl MatchRuntime for MatchSession {
    fn input(&self) -> &MatchInput { &self.input }
    fn state(&self) -> &MatchState { &self.state }
    fn state_mut(&mut self) -> &mut MatchState { &mut self.state }
    fn rng_mut(&mut self) -> &mut DeterministicRng { &mut self.rng }

    fn emit(
        &mut self,
        kind: EventType,
        side: Option<TeamSide>,
        actor: Option<&str>,
        target: Option<&str>,
        metadata: Vec<(String, String)>,
    ) {
        self.event_sequence += 1;
        let after = MatchEventFacts::capture(&self.input, &self.state);
        let before = std::mem::replace(&mut self.last_emitted_facts, after.clone());
        let event = MatchEvent {
            sequence: self.event_sequence,
            game_second: self.state.clock.game_second,
            kind,
            side,
            actor: actor.map(|s| s.to_string()),
            target: target.map(|s| s.to_string()),
            ball_x: self.state.ball.x,
            ball_y: self.state.ball.y,
            metadata,
            facts_before: Some(before),
            facts_after: Some(after),
        };
        if self.retain_events {
            if self.events.len() < MAX_RETAINED_EVENTS {
                self.events.push(event.clone());
            } else {
                self.event_timeline_truncated = true;
            }
        }
        self.stats_collector.accept(&event);
        self.event_timeline_fingerprint = fp::mix_i64(self.event_timeline_fingerprint, event.fingerprint() as i64);
        for sink in &mut self.event_sinks {
            sink.on_event(&event);
        }
    }

    fn advance_playing_time(&mut self, seconds: i32) -> i32 {
        let physical_request = seconds.max(0);
        if physical_request == 0 {
            return 0;
        }
        let action_minutes = self.input.simulated_action_minutes();
        let scaled = physical_request as i64 * 90 + self.virtual_clock_remainder as i64;
        let requested_clock_seconds = (scaled / action_minutes as i64) as i32;
        let next_remainder = (scaled % action_minutes as i64) as i32;
        let now = self.state.clock.game_second;
        let actual_virtual = (self.current_boundary() - now).min(requested_clock_seconds).max(0);
        if actual_virtual > 0 {
            let actual_physical = if action_minutes == 90 {
                actual_virtual
            } else {
                java_round(actual_virtual as f64 * (action_minutes as f64 / 90.0))
                    .max(1)
                    .min(physical_request)
            };
            movement::move_players(&self.input, &mut self.state, actual_physical);
            self.played_physical_seconds += actual_physical as i64;
            let _ = self.state.clock.set_game_second(now + actual_virtual);
            self.virtual_clock_remainder = if actual_virtual == requested_clock_seconds {
                next_remainder
            } else {
                0
            };
        } else {
            self.virtual_clock_remainder = 0;
        }
        actual_virtual
    }

    fn add_stoppage(&mut self, seconds: i32) {
        let physical = seconds.max(0);
        let action_minutes = self.input.simulated_action_minutes();
        let virtual_seconds = if action_minutes < 90 {
            physical.clamp(4, 25)
        } else {
            java_round(physical as f64 * (90.0 / action_minutes as f64))
        };
        self.state.clock.add_stoppage_seconds(virtual_seconds);
    }

    fn schedule_restart(&mut self, kind: RestartType, side: TeamSide, reason: &str, x: i32, y: i32) {
        let last_team = self.state.ball.last_touch_team;
        let last_player = self.state.ball.last_touch_player_id.clone();
        let intent = format!("RESTART:{}", kind.as_str());
        self.state.ball.set_loose(x, y, last_team, last_player, Some(&intent));
        self.state.restart.set(kind, Some(side), reason, x, y);
        self.emit(
            EventType::RestartScheduled,
            Some(side),
            None,
            None,
            vec![kv("type", kind.as_str()), kv("reason", reason)],
        );
    }

    fn change_possession(&mut self, side: TeamSide, player_id: &str, cause: &str) {
        let (rx, ry) = match self.state.player(player_id) {
            Some(r) if r.on_pitch && r.side == side => (r.x, r.y),
            _ => return,
        };
        let old = self.state.possession.controlling_team;
        let sec = self.state.clock.game_second;
        self.state.ball.set_position(rx, ry);
        self.state.ball.set_controlled(side, player_id);
        match old {
            None => {
                self.state.possession.begin(side, sec);
                self.state.team_mut(side).possession_sequences += 1;
            }
            Some(o) if o != side => {
                self.state.team_mut(o).turnovers_lost += 1;
                self.state.team_mut(side).turnovers_won += 1;
                self.state.team_mut(side).possession_sequences += 1;
                let old_phase = self.state.possession.phase;
                let _ = self.state.possession.turnover_to(side, sec);
                self.state.possession.phase = TacticalPhase::from_ball(side, ry);
                self.emit(
                    EventType::Turnover,
                    Some(side),
                    Some(player_id),
                    None,
                    vec![
                        kv("cause", cause),
                        kv("from", o.as_str()),
                        kv("oldPhase", old_phase.as_str()),
                    ],
                );
                self.emit(EventType::TransitionStarted, Some(side), Some(player_id), None, vec![kv("cause", cause)]);
                self.emit(EventType::PossessionStarted, Some(side), Some(player_id), None, vec![kv("cause", cause)]);
            }
            _ => {}
        }
    }

    fn ensure_goalkeeper(&mut self, side: TeamSide, reason: &str) {
        let current_ok = self
            .state
            .team(side)
            .designated_goalkeeper_id
            .as_deref()
            .and_then(|gid| self.state.player(gid))
            .map(|r| r.on_pitch)
            .unwrap_or(false);
        if current_ok {
            self.state.team_mut(side).clear_goalkeeper_assignment_requirement();
            return;
        }
        let already_required = self.state.team(side).requires_goalkeeper_assignment;
        self.state.team_mut(side).require_goalkeeper_assignment();
        if !already_required {
            self.emit(
                EventType::GoalkeeperAssignmentRequired,
                Some(side),
                None,
                None,
                vec![kv("reason", reason)],
            );
        }
    }
}

fn period_name(p: Period) -> &'static str {
    match p {
        Period::NotStarted => "NOT_STARTED",
        Period::FirstHalf => "FIRST_HALF",
        Period::SecondHalf => "SECOND_HALF",
        Period::ExtraTimeFirst => "EXTRA_TIME_FIRST",
        Period::ExtraTimeSecond => "EXTRA_TIME_SECOND",
        Period::Shootout => "SHOOTOUT",
        Period::FullTime => "FULL_TIME",
    }
}

pub struct UnifiedMatchEngine;

impl UnifiedMatchEngine {
    pub fn validate_input(input: &MatchInput) -> Result<(), &'static str> {
        if input.engine_version != ENGINE_VERSION {
            return Err("unsupported engine version");
        }
        if input.match_rules.card_rules_version != "cards-standard-v1" {
            return Err("unsupported card rules");
        }
        Ok(())
    }

    pub fn new_session(input: MatchInput) -> Result<MatchSession, &'static str> {
        Self::validate_input(&input)?;
        Ok(MatchSession::new(input, true))
    }

    pub fn new_compact_session(
        input: MatchInput,
        sinks: Vec<Box<dyn MatchEventSink>>,
    ) -> Result<MatchSession, &'static str> {
        Self::validate_input(&input)?;
        let mut session = MatchSession::new(input, false);
        for s in sinks {
            session.add_event_sink(s)?;
        }
        Ok(session)
    }

    pub fn simulate(input: MatchInput) -> Result<MatchResult, &'static str> {
        let input_ref = input.clone();
        let mut session = Self::new_session(input)?;
        let mut guard = 0;
        while !session.is_full_time() {
            session.advance();
            if session.is_full_time() {
                break;
            }
            let st = session.snapshot();
            if let Some(cmd) = discipline::resolve_goalkeeper_continuity(&input_ref, &st, TeamSide::Home) {
                session.submit_command(cmd)?;
            }
            if let Some(cmd) = discipline::resolve_goalkeeper_continuity(&input_ref, &st, TeamSide::Away) {
                session.submit_command(cmd)?;
            }
            guard += 1;
            if guard > 200_000 {
                return Err("simulate guard exceeded");
            }
        }
        session.finish()
    }
}
