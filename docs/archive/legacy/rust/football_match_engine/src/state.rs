use crate::{
    clock::{ClockState, Period},
    fingerprints as fp,
    geometry::{self, TeamSide},
    input::{
        MatchInput, SetPiecePlanSnapshot, SlotInstructionsSnapshot, TacticalSlotSnapshot,
        TeamInstructionsSnapshot, TeamMatchSnapshot,
    },
    match_day,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TacticalPhase {
    DeepBuildup = 0,
    MidBuildup = 1,
    Progression = 2,
    FinalThird = 3,
    BoxAttack = 4,
}

impl TacticalPhase {
    pub fn from_ball(attacking_side: TeamSide, ball_y: i32) -> Self {
        let progress = geometry::attacking_progress(attacking_side, ball_y);
        if progress < 20 {
            Self::DeepBuildup
        } else if progress < 42 {
            Self::MidBuildup
        } else if progress < 66 {
            Self::Progression
        } else if progress < 84 {
            Self::FinalThird
        } else {
            Self::BoxAttack
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeepBuildup => "DEEP_BUILDUP",
            Self::MidBuildup => "MID_BUILDUP",
            Self::Progression => "PROGRESSION",
            Self::FinalThird => "FINAL_THIRD",
            Self::BoxAttack => "BOX_ATTACK",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum BallControl {
    Controlled = 0,
    Loose = 1,
    Airborne = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BallState {
    pub x: i32,
    pub y: i32,
    pub owner_player_id: Option<String>,
    pub owner_team: Option<TeamSide>,
    pub control: BallControl,
    pub last_touch_player_id: Option<String>,
    pub last_touch_team: Option<TeamSide>,
    pub movement_intent: String,
}

impl Default for BallState {
    fn default() -> Self {
        Self {
            x: 50,
            y: 50,
            owner_player_id: None,
            owner_team: None,
            control: BallControl::Loose,
            last_touch_player_id: None,
            last_touch_team: None,
            movement_intent: "NONE".into(),
        }
    }
}

impl BallState {
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x.clamp(0, 100);
        self.y = y.clamp(0, 100);
    }

    pub fn set_controlled(&mut self, team: TeamSide, player_id: impl Into<String>) {
        let pid = player_id.into();
        self.owner_team = Some(team);
        self.owner_player_id = Some(pid.clone());
        self.control = BallControl::Controlled;
        self.last_touch_team = Some(team);
        self.last_touch_player_id = Some(pid);
        self.movement_intent = "CONTROLLED".into();
    }

    pub fn set_loose(
        &mut self,
        x: i32,
        y: i32,
        last_team: Option<TeamSide>,
        last_player: Option<String>,
        intent: Option<&str>,
    ) {
        self.set_position(x, y);
        self.owner_team = None;
        self.owner_player_id = None;
        self.control = BallControl::Loose;
        if let Some(t) = last_team {
            self.last_touch_team = Some(t);
        }
        if let Some(p) = last_player {
            self.last_touch_player_id = Some(p);
        }
        self.movement_intent = intent.unwrap_or("LOOSE").into();
    }

    pub fn set_airborne(
        &mut self,
        x: i32,
        y: i32,
        last_team: Option<TeamSide>,
        last_player: Option<String>,
        intent: Option<&str>,
    ) {
        self.set_position(x, y);
        self.owner_team = None;
        self.owner_player_id = None;
        self.control = BallControl::Airborne;
        if let Some(t) = last_team {
            self.last_touch_team = Some(t);
        }
        if let Some(p) = last_player {
            self.last_touch_player_id = Some(p);
        }
        self.movement_intent = intent.unwrap_or("AIRBORNE").into();
    }

    pub fn set_movement_intent(&mut self, value: &str) {
        self.movement_intent = value.into();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PossessionState {
    pub controlling_team: Option<TeamSide>,
    pub previous_controlling_team: Option<TeamSide>,
    pub sequence_id: i64,
    pub started_at_second: i32,
    pub phase: TacticalPhase,
    pub transition_active: bool,
    pub transition_started_at_second: i32,
    pub completed_passes_in_sequence: i32,
}

impl Default for PossessionState {
    fn default() -> Self {
        Self {
            controlling_team: None,
            previous_controlling_team: None,
            sequence_id: 0,
            started_at_second: 0,
            phase: TacticalPhase::Progression,
            transition_active: false,
            transition_started_at_second: -1,
            completed_passes_in_sequence: 0,
        }
    }
}

impl PossessionState {
    pub fn begin(&mut self, side: TeamSide, second: i32) {
        self.previous_controlling_team = self.controlling_team;
        self.controlling_team = Some(side);
        self.sequence_id += 1;
        self.started_at_second = second;
        self.completed_passes_in_sequence = 0;
        self.transition_active = false;
        self.transition_started_at_second = -1;
    }

    pub fn turnover_to(&mut self, side: TeamSide, second: i32) -> Result<(), &'static str> {
        if self.controlling_team == Some(side) {
            return Err("turnover must change team");
        }
        self.previous_controlling_team = self.controlling_team;
        self.controlling_team = Some(side);
        self.sequence_id += 1;
        self.started_at_second = second;
        self.completed_passes_in_sequence = 0;
        self.transition_active = true;
        self.transition_started_at_second = second;
        Ok(())
    }

    pub fn record_completed_pass(&mut self) {
        self.completed_passes_in_sequence += 1;
    }

    pub fn settle_transition(&mut self) {
        self.transition_active = false;
        self.transition_started_at_second = -1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RestartType {
    None = 0,
    Kickoff = 1,
    ThrowIn = 2,
    GoalKick = 3,
    Corner = 4,
    FreeKick = 5,
    Penalty = 6,
}

impl RestartType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Kickoff => "KICKOFF",
            Self::ThrowIn => "THROW_IN",
            Self::GoalKick => "GOAL_KICK",
            Self::Corner => "CORNER",
            Self::FreeKick => "FREE_KICK",
            Self::Penalty => "PENALTY",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartState {
    pub kind: RestartType,
    pub team: Option<TeamSide>,
    pub reason: String,
    pub x: i32,
    pub y: i32,
}

impl Default for RestartState {
    fn default() -> Self {
        Self {
            kind: RestartType::None,
            team: None,
            reason: String::new(),
            x: 50,
            y: 50,
        }
    }
}

impl RestartState {
    pub fn set(&mut self, kind: RestartType, team: Option<TeamSide>, reason: &str, x: i32, y: i32) {
        self.kind = kind;
        self.team = team;
        self.reason = reason.into();
        self.x = x.clamp(0, 100);
        self.y = y.clamp(0, 100);
    }

    pub fn clear(&mut self) {
        self.set(RestartType::None, None, "", 50, 50);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScoreState {
    pub home_goals: i32,
    pub away_goals: i32,
    pub home_shootout_goals: i32,
    pub away_shootout_goals: i32,
}

impl ScoreState {
    pub fn is_draw(&self) -> bool { self.home_goals == self.away_goals }

    pub fn add_goal(&mut self, side: TeamSide) {
        match side {
            TeamSide::Home => self.home_goals += 1,
            TeamSide::Away => self.away_goals += 1,
        }
    }

    pub fn add_shootout_goal(&mut self, side: TeamSide) {
        match side {
            TeamSide::Home => self.home_shootout_goals += 1,
            TeamSide::Away => self.away_shootout_goals += 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRuntimeState {
    pub player_id: String,
    pub side: TeamSide,
    pub slot_id: String,
    pub x: i32,
    pub y: i32,
    pub target_x: i32,
    pub target_y: i32,
    pub energy: i32,
    pub acute_fatigue: i32,
    pub workload: i32,
    pub distance_covered_tenths: i32,
    pub fatigue_accumulator: i64,
    pub current_task: String,
    pub pressure_level: i32,
    pub morale_override: i32,
    pub marking_player_id: Option<String>,
    pub pressing_player_id: Option<String>,
    pub defensive_role: String,
    pub on_pitch: bool,
    pub yellow_cards: i32,
    pub red_card: bool,
    pub injured: bool,
}

impl PlayerRuntimeState {
    pub fn new(player_id: impl Into<String>, side: TeamSide, slot_id: impl Into<String>, x: i32, y: i32) -> Self {
        let cx = x.clamp(0, 100);
        let cy = y.clamp(0, 100);
        Self {
            player_id: player_id.into(),
            side,
            slot_id: slot_id.into(),
            x: cx,
            y: cy,
            target_x: cx,
            target_y: cy,
            energy: 100,
            acute_fatigue: 0,
            workload: 0,
            distance_covered_tenths: 0,
            fatigue_accumulator: 0,
            current_task: "SHAPE".into(),
            pressure_level: 0,
            morale_override: -1,
            marking_player_id: None,
            pressing_player_id: None,
            defensive_role: "NONE".into(),
            on_pitch: true,
            yellow_cards: 0,
            red_card: false,
            injured: false,
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x.clamp(0, 100);
        self.y = y.clamp(0, 100);
    }

    pub fn set_target(&mut self, x: i32, y: i32) {
        self.target_x = x.clamp(0, 100);
        self.target_y = y.clamp(0, 100);
    }

    pub fn set_pressure_level(&mut self, value: i32) {
        self.pressure_level = value.clamp(0, 100);
    }

    pub fn set_morale_override(&mut self, value: i32) {
        self.morale_override = value.clamp(0, 100);
    }

    pub fn set_energy(&mut self, value: i32) {
        self.energy = value.clamp(0, 100);
    }

    pub fn decrease_energy(&mut self, value: i32) {
        self.set_energy(self.energy - value.max(0));
    }

    pub fn add_workload(&mut self, value: i32) {
        self.workload = self.workload.saturating_add(value.max(0)).max(0);
    }

    pub fn add_distance_covered_tenths(&mut self, value: i32) {
        self.distance_covered_tenths = self.distance_covered_tenths.saturating_add(value.max(0)).max(0);
    }

    pub fn remove_from_pitch_injured(&mut self) {
        self.injured = true;
        self.on_pitch = false;
        self.current_task = "INJURED_OFF".into();
        self.defensive_role = "NONE".into();
    }

    pub fn set_red_card(&mut self, value: bool) {
        self.red_card = value;
        if value {
            self.on_pitch = false;
            self.defensive_role = "NONE".into();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TeamRuntimeState {
    pub side: TeamSide,
    pub match_day_performance: f64,
    pub designated_goalkeeper_id: Option<String>,
    pub requires_goalkeeper_assignment: bool,
    pub shape_id: String,
    pub tactical_slots: Vec<(String, TacticalSlotSnapshot)>,
    pub team_instructions: TeamInstructionsSnapshot,
    pub slot_instructions: Vec<(String, SlotInstructionsSnapshot)>,
    pub set_piece_plan: SetPiecePlanSnapshot,
    pub last_substitution_window_second: i32,

    pub possession_sequences: i32,
    pub attempted_passes: i32,
    pub completed_passes: i32,
    pub failed_passes: i32,
    pub turnovers_won: i32,
    pub turnovers_lost: i32,
    pub pressing_actions: i32,
    pub cumulative_pressure_faced: i64,
    pub cumulative_pressure_applied: i64,
    pub phase_entries_deep_buildup: i32,
    pub phase_entries_mid_buildup: i32,
    pub phase_entries_progression: i32,
    pub phase_entries_final_third: i32,
    pub phase_entries_box_attack: i32,
    pub carries: i32,
    pub dribbles_attempted: i32,
    pub dribbles_won: i32,
    pub crosses: i32,
    pub shots: i32,
    pub shots_on_target: i32,
    pub goals: i32,
    pub saves: i32,
    pub tackles_attempted: i32,
    pub tackles_won: i32,
    pub interceptions: i32,
    pub blocks: i32,
    pub clearances: i32,
    pub aerial_duels: i32,
    pub aerial_duels_won: i32,
    pub fouls: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub offsides: i32,
    pub corners: i32,
    pub free_kicks: i32,
    pub penalties: i32,
    pub injuries: i32,
    pub substitutions_used: i32,
    pub substitution_windows_used: i32,
}

impl TeamRuntimeState {
    pub fn new(side: TeamSide, snapshot: &TeamMatchSnapshot, match_day_performance: f64) -> Self {
        let tactical_slots = snapshot
            .tactical_slots
            .iter()
            .map(|s| (s.slot_id.clone(), s.clone()))
            .collect();
        let slot_instructions = snapshot
            .slot_instructions
            .iter()
            .map(|s| (s.slot_id.clone(), s.clone()))
            .collect();
        Self {
            side,
            match_day_performance,
            designated_goalkeeper_id: Some(snapshot.designated_goalkeeper_id.clone()),
            requires_goalkeeper_assignment: false,
            shape_id: snapshot.shape_id.clone(),
            tactical_slots,
            team_instructions: snapshot.team_instructions.clone(),
            slot_instructions,
            set_piece_plan: snapshot.set_piece_plan.clone(),
            last_substitution_window_second: i32::MIN,
            possession_sequences: 0,
            attempted_passes: 0,
            completed_passes: 0,
            failed_passes: 0,
            turnovers_won: 0,
            turnovers_lost: 0,
            pressing_actions: 0,
            cumulative_pressure_faced: 0,
            cumulative_pressure_applied: 0,
            phase_entries_deep_buildup: 0,
            phase_entries_mid_buildup: 0,
            phase_entries_progression: 0,
            phase_entries_final_third: 0,
            phase_entries_box_attack: 0,
            carries: 0,
            dribbles_attempted: 0,
            dribbles_won: 0,
            crosses: 0,
            shots: 0,
            shots_on_target: 0,
            goals: 0,
            saves: 0,
            tackles_attempted: 0,
            tackles_won: 0,
            interceptions: 0,
            blocks: 0,
            clearances: 0,
            aerial_duels: 0,
            aerial_duels_won: 0,
            fouls: 0,
            yellow_cards: 0,
            red_cards: 0,
            offsides: 0,
            corners: 0,
            free_kicks: 0,
            penalties: 0,
            injuries: 0,
            substitutions_used: 0,
            substitution_windows_used: 0,
        }
    }

    pub fn tactical_slot(&self, slot_id: &str) -> Option<&TacticalSlotSnapshot> {
        self.tactical_slots.iter().find(|(k, _)| k == slot_id).map(|(_, v)| v)
    }

    pub fn slot_instructions(&self, slot_id: &str) -> Option<&SlotInstructionsSnapshot> {
        self.slot_instructions.iter().find(|(k, _)| k == slot_id).map(|(_, v)| v)
    }

    pub fn require_goalkeeper_assignment(&mut self) {
        self.requires_goalkeeper_assignment = true;
        self.designated_goalkeeper_id = None;
    }

    pub fn clear_goalkeeper_assignment_requirement(&mut self) {
        self.requires_goalkeeper_assignment = false;
    }

    pub fn set_shape(&mut self, new_shape_id: Option<&str>, slots: &[TacticalSlotSnapshot]) -> Result<(), &'static str> {
        if slots.len() != 11 {
            return Err("shape requires 11 tactical slots");
        }
        let mut next: Vec<(String, TacticalSlotSnapshot)> = Vec::with_capacity(11);
        for slot in slots {
            if slot.slot_id.is_empty() || next.iter().any(|(k, _)| k == &slot.slot_id) {
                return Err("invalid/duplicate tactical slot");
            }
            next.push((slot.slot_id.clone(), slot.clone()));
        }
        for (slot_id, _) in &self.slot_instructions {
            if !next.iter().any(|(k, _)| k == slot_id) {
                return Err("shape missing active slot");
            }
        }
        self.tactical_slots = next;
        self.shape_id = new_shape_id.unwrap_or("runtime-shape").into();
        Ok(())
    }

    pub fn set_team_instruction(&mut self, key: &str, value: i32) -> Result<(), &'static str> {
        let v = value.clamp(0, 100);
        let t = &mut self.team_instructions;
        match key {
            "PRESSING" | "PRESSING_INTENSITY" => t.pressing_intensity = v,
            "TEMPO" => t.tempo = v,
            "LINE_HEIGHT" => t.line_height = v,
            "WIDTH" => t.width = v,
            "TIME_WASTING" => t.time_wasting = v,
            "AGGRESSION" => t.aggression = v,
            "TRANSITION_RISK" => t.transition_risk = v,
            "BUILD_UP_RISK" => t.build_up_risk = v,
            "CONCENTRATION" => t.concentration = v,
            "OFFSIDE_TRAP" => t.offside_trap = v >= 50,
            _ => return Err("unknown team instruction"),
        }
        Ok(())
    }

    pub fn set_slot_instruction(&mut self, slot_id: &str, key: &str, value: i32) -> Result<(), &'static str> {
        let v = value.clamp(0, 100);
        let entry = self
            .slot_instructions
            .iter_mut()
            .find(|(k, _)| k == slot_id)
            .ok_or("unknown slot")?;
        let s = &mut entry.1;
        match key {
            "FORWARD_MOVEMENT" => s.forward_movement = v,
            "PASSING_RISK" => s.passing_risk = v,
            "CROSSING_TENDENCY" => s.crossing_tendency = v,
            "DRIBBLING_TENDENCY" => s.dribbling_tendency = v,
            "DEFENSIVE_AGGRESSION" => s.defensive_aggression = v,
            "SUPPORT_DISTANCE" => s.support_distance = v,
            "ROAMING" => s.roaming = v,
            "SHOOTING_TENDENCY" => s.shooting_tendency = v,
            _ => return Err("unknown slot instruction"),
        }
        Ok(())
    }

    pub fn substitution_consumes_window(&self, game_second: i32, period: Period, period_started_at_second: i32) -> bool {
        let interval_exempt = game_second == period_started_at_second
            && matches!(period, Period::SecondHalf | Period::ExtraTimeFirst | Period::ExtraTimeSecond);
        !interval_exempt && self.last_substitution_window_second != game_second
    }

    pub fn add_substitution(&mut self, game_second: i32, consume_window: bool) {
        self.substitutions_used += 1;
        if consume_window {
            self.substitution_windows_used += 1;
            self.last_substitution_window_second = game_second;
        }
    }

    pub fn add_pass_attempt(&mut self, pressure: i32) {
        self.attempted_passes += 1;
        self.cumulative_pressure_faced += pressure.max(0) as i64;
    }

    pub fn add_pressing_action(&mut self, pressure: i32) {
        self.pressing_actions += 1;
        self.cumulative_pressure_applied += pressure.max(0) as i64;
    }

    pub fn add_shot(&mut self, on_target: bool) {
        self.shots += 1;
        if on_target {
            self.shots_on_target += 1;
        }
    }

    pub fn add_aerial_duel(&mut self, won: bool) {
        self.aerial_duels += 1;
        if won {
            self.aerial_duels_won += 1;
        }
    }

    pub fn record_phase_entry(&mut self, phase: TacticalPhase) {
        match phase {
            TacticalPhase::DeepBuildup => self.phase_entries_deep_buildup += 1,
            TacticalPhase::MidBuildup => self.phase_entries_mid_buildup += 1,
            TacticalPhase::Progression => self.phase_entries_progression += 1,
            TacticalPhase::FinalThird => self.phase_entries_final_third += 1,
            TacticalPhase::BoxAttack => self.phase_entries_box_attack += 1,
        }
    }

    pub fn average_pressure_faced(&self) -> f64 {
        if self.attempted_passes == 0 { 0.0 } else { self.cumulative_pressure_faced as f64 / self.attempted_passes as f64 }
    }

    pub fn average_pressure_applied(&self) -> f64 {
        if self.pressing_actions == 0 { 0.0 } else { self.cumulative_pressure_applied as f64 / self.pressing_actions as f64 }
    }

    pub fn tactical_fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.shape_id));
        for (k, v) in &self.tactical_slots {
            h = fp::mix_string(h, Some(k));
            h = fp::mix_i64(h, v.fingerprint() as i64);
        }
        h = fp::mix_i64(h, self.team_instructions.fingerprint() as i64);
        for (k, v) in &self.slot_instructions {
            h = fp::mix_string(h, Some(k));
            h = fp::mix_i64(h, v.fingerprint() as i64);
        }
        fp::mix_i64(h, self.set_piece_plan.fingerprint() as i64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchState {
    pub clock: ClockState,
    pub score: ScoreState,
    pub ball: BallState,
    pub home_team: TeamRuntimeState,
    pub away_team: TeamRuntimeState,

    pub players: Vec<(String, PlayerRuntimeState)>,
    pub restart: RestartState,
    pub possession: PossessionState,
}

impl MatchState {
    pub fn new(input: &MatchInput) -> Self {
        let home_perf = match_day::adjustment(input.seed, TeamSide::Home, &input.home_team.team_id);
        let away_perf = match_day::adjustment(input.seed, TeamSide::Away, &input.away_team.team_id);
        let edge = (home_perf - away_perf) * 0.50;
        let mut state = Self {
            clock: ClockState::default(),
            score: ScoreState::default(),
            ball: BallState::default(),
            home_team: TeamRuntimeState::new(TeamSide::Home, &input.home_team, edge),
            away_team: TeamRuntimeState::new(TeamSide::Away, &input.away_team, -edge),
            players: Vec::with_capacity(28),
            restart: RestartState::default(),
            possession: PossessionState::default(),
        };
        state.add_players(&input.home_team, TeamSide::Home);
        state.add_players(&input.away_team, TeamSide::Away);
        state
    }

    fn add_players(&mut self, team: &TeamMatchSnapshot, side: TeamSide) {
        for a in &team.assignments {
            let slot = team.slot(&a.slot_id).expect("assignment without tactical slot");
            let x = if side == TeamSide::Home { slot.out_x } else { 100 - slot.out_x };
            let y = if side == TeamSide::Home { slot.out_y } else { 100 - slot.out_y };
            let mut runtime = PlayerRuntimeState::new(a.player_id.clone(), side, a.slot_id.clone(), x, y);
            let p = team.player(&a.player_id).expect("assignment without player snapshot");
            runtime.set_energy(p.condition);
            self.players.push((a.player_id.clone(), runtime));
        }
    }

    pub fn team(&self, side: TeamSide) -> &TeamRuntimeState {
        match side {
            TeamSide::Home => &self.home_team,
            TeamSide::Away => &self.away_team,
        }
    }

    pub fn team_mut(&mut self, side: TeamSide) -> &mut TeamRuntimeState {
        match side {
            TeamSide::Home => &mut self.home_team,
            TeamSide::Away => &mut self.away_team,
        }
    }

    pub fn player(&self, player_id: &str) -> Option<&PlayerRuntimeState> {
        self.players.iter().find(|(k, _)| k == player_id).map(|(_, v)| v)
    }

    pub fn player_mut(&mut self, player_id: &str) -> Option<&mut PlayerRuntimeState> {
        self.players.iter_mut().find(|(k, _)| k == player_id).map(|(_, v)| v)
    }

    pub fn add_substitute_runtime(
        &mut self,
        input: &MatchInput,
        side: TeamSide,
        player_id: &str,
        slot_id: &str,
        x: i32,
        y: i32,
    ) -> Result<(), &'static str> {
        if self.players.iter().any(|(k, _)| k == player_id) {
            return Err("player already has runtime state");
        }
        let team = input.team(side);
        let player = team.player(player_id).ok_or("invalid substitute")?;
        if !player.eligible || player.injured {
            return Err("invalid substitute");
        }
        let mut runtime = PlayerRuntimeState::new(player_id, side, slot_id, x, y);
        runtime.set_energy(player.condition);
        self.players.push((player_id.into(), runtime));
        Ok(())
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.clock.game_second);
        h = fp::mix_i32(h, self.clock.period as i32);
        h = fp::mix_i32(h, self.clock.period_started_at_second);
        h = fp::mix_i32(h, self.clock.stoppage_accumulator_seconds);
        h = fp::mix_i32(h, self.clock.added_time_seconds);
        h = fp::mix_i32(h, self.clock.added_time_extension_seconds);
        h = fp::mix_i32(h, self.clock.effective_added_time());
        h = fp::mix_bool(h, self.clock.added_time_locked);
        h = fp::mix_i32(h, self.score.home_goals);
        h = fp::mix_i32(h, self.score.away_goals);
        h = fp::mix_i32(h, self.score.home_shootout_goals);
        h = fp::mix_i32(h, self.score.away_shootout_goals);
        h = fp::mix_i32(h, self.ball.x);
        h = fp::mix_i32(h, self.ball.y);
        h = fp::mix_string(h, self.ball.owner_player_id.as_deref());
        h = fp::mix_i32(h, self.ball.owner_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_i32(h, self.ball.control as i32);
        h = fp::mix_string(h, self.ball.last_touch_player_id.as_deref());
        h = fp::mix_i32(h, self.ball.last_touch_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_string(h, Some(&self.ball.movement_intent));
        h = Self::mix_team(h, &self.home_team);
        h = Self::mix_team(h, &self.away_team);
        for (_, p) in &self.players {
            h = fp::mix_string(h, Some(&p.player_id));
            h = fp::mix_i32(h, p.side as i32);
            h = fp::mix_string(h, Some(&p.slot_id));
            h = fp::mix_i32(h, p.x);
            h = fp::mix_i32(h, p.y);
            h = fp::mix_i32(h, p.target_x);
            h = fp::mix_i32(h, p.target_y);
            h = fp::mix_i32(h, p.energy);
            h = fp::mix_i32(h, p.acute_fatigue);
            h = fp::mix_i32(h, p.workload);
            h = fp::mix_i32(h, p.distance_covered_tenths);
            h = fp::mix_string(h, Some(&p.current_task));
            h = fp::mix_i32(h, p.pressure_level);
            if p.morale_override >= 0 {
                h = fp::mix_i32(h, 1000 + p.morale_override);
            }
            h = fp::mix_string(h, p.marking_player_id.as_deref());
            h = fp::mix_string(h, p.pressing_player_id.as_deref());
            h = fp::mix_string(h, Some(&p.defensive_role));
            h = fp::mix_bool(h, p.on_pitch);
            h = fp::mix_i32(h, p.yellow_cards);
            h = fp::mix_bool(h, p.red_card);
            h = fp::mix_bool(h, p.injured);
        }
        h = fp::mix_i32(h, self.restart.kind as i32);
        h = fp::mix_i32(h, self.restart.team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_string(h, Some(&self.restart.reason));
        h = fp::mix_i32(h, self.restart.x);
        h = fp::mix_i32(h, self.restart.y);
        h = fp::mix_i32(h, self.possession.controlling_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_i32(h, self.possession.previous_controlling_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_i64(h, self.possession.sequence_id);
        h = fp::mix_i32(h, self.possession.started_at_second);
        h = fp::mix_i32(h, self.possession.phase as i32);
        h = fp::mix_bool(h, self.possession.transition_active);
        h = fp::mix_i32(h, self.possession.transition_started_at_second);
        fp::mix_i32(h, self.possession.completed_passes_in_sequence)
    }

    fn mix_team(mut h: u64, t: &TeamRuntimeState) -> u64 {
        h = fp::mix_i32(h, t.side as i32);
        h = fp::mix_i64(h, t.match_day_performance.to_bits() as i64);
        h = fp::mix_string(h, t.designated_goalkeeper_id.as_deref());
        h = fp::mix_bool(h, t.requires_goalkeeper_assignment);
        h = fp::mix_i64(h, t.tactical_fingerprint() as i64);
        h = fp::mix_i32(h, t.possession_sequences);
        h = fp::mix_i32(h, t.attempted_passes);
        h = fp::mix_i32(h, t.completed_passes);
        h = fp::mix_i32(h, t.failed_passes);
        h = fp::mix_i32(h, t.turnovers_won);
        h = fp::mix_i32(h, t.turnovers_lost);
        h = fp::mix_i32(h, t.pressing_actions);
        h = fp::mix_i64(h, t.cumulative_pressure_faced);
        h = fp::mix_i64(h, t.cumulative_pressure_applied);
        h = fp::mix_i32(h, t.phase_entries_deep_buildup);
        h = fp::mix_i32(h, t.phase_entries_mid_buildup);
        h = fp::mix_i32(h, t.phase_entries_progression);
        h = fp::mix_i32(h, t.phase_entries_final_third);
        h = fp::mix_i32(h, t.phase_entries_box_attack);
        h = fp::mix_i32(h, t.carries);
        h = fp::mix_i32(h, t.dribbles_attempted);
        h = fp::mix_i32(h, t.dribbles_won);
        h = fp::mix_i32(h, t.crosses);
        h = fp::mix_i32(h, t.shots);
        h = fp::mix_i32(h, t.shots_on_target);
        h = fp::mix_i32(h, t.goals);
        h = fp::mix_i32(h, t.saves);
        h = fp::mix_i32(h, t.tackles_attempted);
        h = fp::mix_i32(h, t.tackles_won);
        h = fp::mix_i32(h, t.interceptions);
        h = fp::mix_i32(h, t.blocks);
        h = fp::mix_i32(h, t.clearances);
        h = fp::mix_i32(h, t.aerial_duels);
        h = fp::mix_i32(h, t.aerial_duels_won);
        h = fp::mix_i32(h, t.fouls);
        h = fp::mix_i32(h, t.yellow_cards);
        h = fp::mix_i32(h, t.red_cards);
        h = fp::mix_i32(h, t.offsides);
        h = fp::mix_i32(h, t.corners);
        h = fp::mix_i32(h, t.free_kicks);
        h = fp::mix_i32(h, t.penalties);
        h = fp::mix_i32(h, t.injuries);
        h = fp::mix_i32(h, t.substitutions_used);
        fp::mix_i32(h, t.substitution_windows_used)
    }
}
