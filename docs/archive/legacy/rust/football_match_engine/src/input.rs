use std::collections::HashSet;
use crate::{fingerprints as fp, geometry::TeamSide, rules::MatchRules, snapshot::PlayerAttributesSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum LeagueSeasonObjective {
    TitleChallenge = 0,
    Qualification = 1,
    TopHalf = 2,
    Survival = 3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeagueObjectiveSnapshot {
    pub available: bool,
    pub team_count: i32,
    pub current_position: i32,
    pub opponent_position: i32,
    pub current_points: i32,
    pub opponent_points: i32,
    pub matches_played: i32,
    pub matches_remaining_after_this_match: i32,
    pub projected_win_position: i32,
    pub projected_draw_position: i32,
    pub projected_loss_position: i32,
    pub season_progress: f64,
    pub direct_rival_intensity: f64,
    pub win_utility: f64,
    pub draw_utility: f64,
    pub loss_utility: f64,
    pub match_importance: f64,
    pub goal_difference_importance: f64,
    pub draw_acceptable: bool,
    pub season_objective: LeagueSeasonObjective,
    pub objective_boundary_position: i32,
    pub objective_pressure: f64,
    pub win_meets_objective: bool,
    pub draw_meets_objective: bool,
    pub loss_meets_objective: bool,
    pub must_win_now: bool,
}

impl LeagueObjectiveSnapshot {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            team_count: 0,
            current_position: 0,
            opponent_position: 0,
            current_points: 0,
            opponent_points: 0,
            matches_played: 0,
            matches_remaining_after_this_match: 0,
            projected_win_position: 0,
            projected_draw_position: 0,
            projected_loss_position: 0,
            season_progress: 0.0,
            direct_rival_intensity: 0.0,
            win_utility: 3.0,
            draw_utility: 1.0,
            loss_utility: 0.0,
            match_importance: 0.90,
            goal_difference_importance: 0.35,
            draw_acceptable: false,
            season_objective: LeagueSeasonObjective::TopHalf,
            objective_boundary_position: 0,
            objective_pressure: 0.0,
            win_meets_objective: false,
            draw_meets_objective: false,
            loss_meets_objective: false,
            must_win_now: false,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.available {
            return Ok(());
        }
        let ratio = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        if self.team_count < 2
            || self.current_position < 1 || self.current_position > self.team_count
            || self.opponent_position < 1 || self.opponent_position > self.team_count
            || self.current_points < 0 || self.opponent_points < 0
            || self.matches_played < 0 || self.matches_remaining_after_this_match < 0
            || self.projected_win_position < 1 || self.projected_win_position > self.team_count
            || self.projected_draw_position < 1 || self.projected_draw_position > self.team_count
            || self.projected_loss_position < 1 || self.projected_loss_position > self.team_count
            || !ratio(self.season_progress) || !ratio(self.direct_rival_intensity)
            || !self.win_utility.is_finite() || !self.draw_utility.is_finite() || !self.loss_utility.is_finite()
            || !ratio(self.match_importance) || !ratio(self.goal_difference_importance)
            || self.objective_boundary_position < 1 || self.objective_boundary_position > self.team_count
            || !ratio(self.objective_pressure)
        {
            return Err("invalid league objective snapshot");
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_bool(h, self.available);
        h = fp::mix_i32(h, self.team_count);
        h = fp::mix_i32(h, self.current_position);
        h = fp::mix_i32(h, self.opponent_position);
        h = fp::mix_i32(h, self.current_points);
        h = fp::mix_i32(h, self.opponent_points);
        h = fp::mix_i32(h, self.matches_played);
        h = fp::mix_i32(h, self.matches_remaining_after_this_match);
        h = fp::mix_i32(h, self.projected_win_position);
        h = fp::mix_i32(h, self.projected_draw_position);
        h = fp::mix_i32(h, self.projected_loss_position);
        h = fp::mix_i64(h, self.season_progress.to_bits() as i64);
        h = fp::mix_i64(h, self.direct_rival_intensity.to_bits() as i64);
        h = fp::mix_i64(h, self.win_utility.to_bits() as i64);
        h = fp::mix_i64(h, self.draw_utility.to_bits() as i64);
        h = fp::mix_i64(h, self.loss_utility.to_bits() as i64);
        h = fp::mix_i64(h, self.match_importance.to_bits() as i64);
        h = fp::mix_i64(h, self.goal_difference_importance.to_bits() as i64);
        h = fp::mix_bool(h, self.draw_acceptable);
        if !self.available {
            return h;
        }
        h = fp::mix_i32(h, self.season_objective as i32);
        h = fp::mix_i32(h, self.objective_boundary_position);
        h = fp::mix_i64(h, self.objective_pressure.to_bits() as i64);
        h = fp::mix_bool(h, self.win_meets_objective);
        h = fp::mix_bool(h, self.draw_meets_objective);
        h = fp::mix_bool(h, self.loss_meets_objective);
        fp::mix_bool(h, self.must_win_now)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineupAssignmentSnapshot {
    pub player_id: String,
    pub slot_id: String,
}

impl LineupAssignmentSnapshot {
    pub fn new(player_id: impl Into<String>, slot_id: impl Into<String>) -> Result<Self, &'static str> {
        let player_id = player_id.into();
        let slot_id = slot_id.into();
        if player_id.is_empty() { return Err("playerId required"); }
        if slot_id.is_empty() { return Err("slotId required"); }
        Ok(Self { player_id, slot_id })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.player_id));
        fp::mix_string(h, Some(&self.slot_id))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TacticalSlotSnapshot {
    pub slot_id: String,
    pub out_x: i32,
    pub out_y: i32,
    pub in_x: i32,
    pub in_y: i32,
}

impl TacticalSlotSnapshot {
    pub fn new(slot_id: impl Into<String>, out_x: i32, out_y: i32, in_x: i32, in_y: i32) -> Result<Self, &'static str> {
        let slot_id = slot_id.into();
        if slot_id.is_empty() { return Err("slotId required"); }
        for v in [out_x, out_y, in_x, in_y] {
            if !(0..=100).contains(&v) { return Err("coordinate out of range"); }
        }
        Ok(Self { slot_id, out_x, out_y, in_x, in_y })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.slot_id));
        h = fp::mix_i32(h, self.out_x);
        h = fp::mix_i32(h, self.out_y);
        h = fp::mix_i32(h, self.in_x);
        fp::mix_i32(h, self.in_y)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotInstructionsSnapshot {
    pub slot_id: String,
    pub forward_movement: i32,
    pub passing_risk: i32,
    pub crossing_tendency: i32,
    pub dribbling_tendency: i32,
    pub defensive_aggression: i32,
    pub support_distance: i32,
    pub roaming: i32,
    pub shooting_tendency: i32,
}

impl SlotInstructionsSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        slot_id: impl Into<String>,
        forward_movement: i32,
        passing_risk: i32,
        crossing_tendency: i32,
        dribbling_tendency: i32,
        defensive_aggression: i32,
        support_distance: i32,
        roaming: i32,
        shooting_tendency: i32,
    ) -> Result<Self, &'static str> {
        let slot_id = slot_id.into();
        if slot_id.is_empty() { return Err("slotId required"); }
        for v in [
            forward_movement, passing_risk, crossing_tendency, dribbling_tendency,
            defensive_aggression, support_distance, roaming, shooting_tendency,
        ] {
            if !(0..=100).contains(&v) { return Err("slot instruction out of range"); }
        }
        Ok(Self {
            slot_id,
            forward_movement,
            passing_risk,
            crossing_tendency,
            dribbling_tendency,
            defensive_aggression,
            support_distance,
            roaming,
            shooting_tendency,
        })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.slot_id));
        h = fp::mix_i32(h, self.forward_movement);
        h = fp::mix_i32(h, self.passing_risk);
        h = fp::mix_i32(h, self.crossing_tendency);
        h = fp::mix_i32(h, self.dribbling_tendency);
        h = fp::mix_i32(h, self.defensive_aggression);
        h = fp::mix_i32(h, self.support_distance);
        h = fp::mix_i32(h, self.roaming);
        fp::mix_i32(h, self.shooting_tendency)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamInstructionsSnapshot {
    pub pressing_intensity: i32,
    pub tempo: i32,
    pub line_height: i32,
    pub width: i32,
    pub time_wasting: i32,
    pub offside_trap: bool,
    pub aggression: i32,
    pub transition_risk: i32,
    pub build_up_risk: i32,
    pub concentration: i32,
}

impl TeamInstructionsSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pressing_intensity: i32,
        tempo: i32,
        line_height: i32,
        width: i32,
        time_wasting: i32,
        offside_trap: bool,
        aggression: i32,
        transition_risk: i32,
        build_up_risk: i32,
        concentration: i32,
    ) -> Result<Self, &'static str> {
        for v in [
            pressing_intensity, tempo, line_height, width, time_wasting,
            aggression, transition_risk, build_up_risk, concentration,
        ] {
            if !(0..=100).contains(&v) { return Err("instruction out of range"); }
        }
        Ok(Self {
            pressing_intensity,
            tempo,
            line_height,
            width,
            time_wasting,
            offside_trap,
            aggression,
            transition_risk,
            build_up_risk,
            concentration,
        })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.pressing_intensity);
        h = fp::mix_i32(h, self.tempo);
        h = fp::mix_i32(h, self.line_height);
        h = fp::mix_i32(h, self.width);
        h = fp::mix_i32(h, self.time_wasting);
        h = fp::mix_bool(h, self.offside_trap);
        h = fp::mix_i32(h, self.aggression);
        h = fp::mix_i32(h, self.transition_risk);
        h = fp::mix_i32(h, self.build_up_risk);
        fp::mix_i32(h, self.concentration)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamCollectiveSnapshot {
    pub cohesion: i32,
    pub system_familiarity: i32,
    pub confidence: i32,
}

impl TeamCollectiveSnapshot {
    pub fn new(cohesion: i32, system_familiarity: i32, confidence: i32) -> Result<Self, &'static str> {
        for v in [cohesion, system_familiarity, confidence] {
            if !(1..=99).contains(&v) { return Err("collective value out of range"); }
        }
        Ok(Self { cohesion, system_familiarity, confidence })
    }

    pub fn neutral() -> Self {
        Self { cohesion: 50, system_familiarity: 50, confidence: 50 }
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.cohesion);
        h = fp::mix_i32(h, self.system_familiarity);
        fp::mix_i32(h, self.confidence)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetPiecePlanSnapshot {
    pub penalty_taker_slot_ids: Vec<String>,
    pub corner_taker_slot_ids: Vec<String>,
    pub close_free_kick_taker_slot_ids: Vec<String>,
    pub far_free_kick_taker_slot_ids: Vec<String>,
    pub captain_slot_ids: Vec<String>,
    pub corner_box_slot_ids: Vec<String>,
    pub close_free_kick_box_slot_ids: Vec<String>,
    pub far_free_kick_box_slot_ids: Vec<String>,
}

impl SetPiecePlanSnapshot {
    pub fn empty() -> Self { Self::default() }

    fn mix_list(mut h: u64, values: &[String]) -> u64 {
        h = fp::mix_i32(h, values.len() as i32);
        for v in values {
            h = fp::mix_string(h, Some(v));
        }
        h
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = Self::mix_list(h, &self.penalty_taker_slot_ids);
        h = Self::mix_list(h, &self.corner_taker_slot_ids);
        h = Self::mix_list(h, &self.close_free_kick_taker_slot_ids);
        h = Self::mix_list(h, &self.far_free_kick_taker_slot_ids);
        h = Self::mix_list(h, &self.captain_slot_ids);
        h = Self::mix_list(h, &self.corner_box_slot_ids);
        h = Self::mix_list(h, &self.close_free_kick_box_slot_ids);
        Self::mix_list(h, &self.far_free_kick_box_slot_ids)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerMatchSnapshot {
    pub player_id: String,
    pub display_name: String,
    pub attributes: PlayerAttributesSnapshot,
    pub condition: i32,
    pub morale: i32,
    pub form: i32,
    pub age: i32,
    pub injured: bool,
    pub eligible: bool,
    pub primary_position: String,
    pub secondary_position: String,
}

impl PlayerMatchSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        player_id: impl Into<String>,
        display_name: impl Into<String>,
        attributes: PlayerAttributesSnapshot,
        condition: i32,
        morale: i32,
        form: i32,
        age: i32,
        injured: bool,
        eligible: bool,
        primary_position: impl Into<String>,
        secondary_position: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let player_id = player_id.into();
        if player_id.is_empty() { return Err("playerId required"); }
        if !(0..=100).contains(&condition) { return Err("condition out of range"); }
        if !(0..=100).contains(&morale) { return Err("morale out of range"); }
        if !(0..=100).contains(&form) { return Err("form out of range"); }
        if !(1..=80).contains(&age) { return Err("age out of range"); }
        Ok(Self {
            player_id,
            display_name: display_name.into(),
            attributes,
            condition,
            morale,
            form,
            age,
            injured,
            eligible,
            primary_position: primary_position.into(),
            secondary_position: secondary_position.into(),
        })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.player_id));
        h = fp::mix_string(h, Some(&self.display_name));
        h = fp::mix_i64(h, self.attributes.fingerprint() as i64);
        h = fp::mix_i32(h, self.condition);
        h = fp::mix_i32(h, self.morale);
        h = fp::mix_i32(h, self.form);
        h = fp::mix_i32(h, self.age);
        h = fp::mix_bool(h, self.injured);
        fp::mix_bool(h, self.eligible)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamMatchSnapshot {
    pub team_id: String,
    pub display_name: String,
    pub lineup: Vec<PlayerMatchSnapshot>,
    pub bench: Vec<PlayerMatchSnapshot>,
    pub assignments: Vec<LineupAssignmentSnapshot>,
    pub designated_goalkeeper_id: String,
    pub shape_id: String,
    pub tactical_slots: Vec<TacticalSlotSnapshot>,
    pub team_instructions: TeamInstructionsSnapshot,
    pub slot_instructions: Vec<SlotInstructionsSnapshot>,
    pub set_piece_plan: SetPiecePlanSnapshot,
    pub collective_state: TeamCollectiveSnapshot,
}

impl TeamMatchSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        team_id: impl Into<String>,
        display_name: impl Into<String>,
        mut lineup: Vec<PlayerMatchSnapshot>,
        mut bench: Vec<PlayerMatchSnapshot>,
        mut assignments: Vec<LineupAssignmentSnapshot>,
        designated_goalkeeper_id: impl Into<String>,
        shape_id: impl Into<String>,
        mut tactical_slots: Vec<TacticalSlotSnapshot>,
        team_instructions: TeamInstructionsSnapshot,
        mut slot_instructions: Vec<SlotInstructionsSnapshot>,
        set_piece_plan: SetPiecePlanSnapshot,
        collective_state: TeamCollectiveSnapshot,
    ) -> Result<Self, &'static str> {
        let team_id = team_id.into();
        let shape_id = shape_id.into();
        let designated_goalkeeper_id = designated_goalkeeper_id.into();
        if team_id.is_empty() { return Err("teamId required"); }
        if shape_id.is_empty() { return Err("shapeId required"); }
        lineup.sort_by(|a, b| a.player_id.cmp(&b.player_id));
        bench.sort_by(|a, b| a.player_id.cmp(&b.player_id));
        assignments.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));
        tactical_slots.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));
        slot_instructions.sort_by(|a, b| a.slot_id.cmp(&b.slot_id));

        let team = Self {
            team_id,
            display_name: display_name.into(),
            lineup,
            bench,
            assignments,
            designated_goalkeeper_id,
            shape_id,
            tactical_slots,
            team_instructions,
            slot_instructions,
            set_piece_plan,
            collective_state,
        };
        team.validate()?;
        Ok(team)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.lineup.len() != 11 { return Err("lineup must contain 11 players"); }
        if self.assignments.len() != 11 { return Err("assignments must contain 11 entries"); }
        if self.tactical_slots.len() != 11 { return Err("shape must contain 11 tactical slots"); }
        if self.slot_instructions.len() != 11 { return Err("slotInstructions must contain 11 entries"); }

        let mut player_ids = HashSet::new();
        let mut lineup_ids = HashSet::new();
        for p in &self.lineup {
            if !player_ids.insert(p.player_id.as_str()) { return Err("duplicate lineup playerId"); }
            if p.injured || !p.eligible { return Err("ineligible player in starting lineup"); }
            lineup_ids.insert(p.player_id.as_str());
        }
        for p in &self.bench {
            if !player_ids.insert(p.player_id.as_str()) { return Err("duplicate player across lineup/bench"); }
        }

        let mut slot_ids = HashSet::new();
        for s in &self.tactical_slots {
            if !slot_ids.insert(s.slot_id.as_str()) { return Err("duplicate tactical slot"); }
        }
        let mut assigned_players = HashSet::new();
        let mut assigned_slots = HashSet::new();
        for a in &self.assignments {
            if !assigned_players.insert(a.player_id.as_str()) { return Err("duplicate assigned player"); }
            if !assigned_slots.insert(a.slot_id.as_str()) { return Err("duplicate assigned slot"); }
        }
        if assigned_players != lineup_ids { return Err("assignments must match lineup exactly"); }
        if assigned_slots != slot_ids { return Err("assignments must cover shape exactly"); }
        if !assigned_players.contains(self.designated_goalkeeper_id.as_str()) {
            return Err("designated goalkeeper must be in lineup");
        }
        let instr_slots: HashSet<&str> = self.slot_instructions.iter().map(|s| s.slot_id.as_str()).collect();
        if instr_slots != slot_ids { return Err("slot instructions must cover shape exactly"); }

        for list in [
            &self.set_piece_plan.penalty_taker_slot_ids,
            &self.set_piece_plan.corner_taker_slot_ids,
            &self.set_piece_plan.close_free_kick_taker_slot_ids,
            &self.set_piece_plan.far_free_kick_taker_slot_ids,
            &self.set_piece_plan.captain_slot_ids,
            &self.set_piece_plan.corner_box_slot_ids,
            &self.set_piece_plan.close_free_kick_box_slot_ids,
            &self.set_piece_plan.far_free_kick_box_slot_ids,
        ] {
            for s in list {
                if !slot_ids.contains(s.as_str()) { return Err("unknown set piece slot"); }
            }
        }
        Ok(())
    }

    pub fn player(&self, player_id: &str) -> Option<&PlayerMatchSnapshot> {
        self.lineup.iter().find(|p| p.player_id == player_id)
            .or_else(|| self.bench.iter().find(|p| p.player_id == player_id))
    }

    pub fn player_id_for_slot(&self, slot_id: &str) -> Option<&str> {
        self.assignments.iter().find(|a| a.slot_id == slot_id).map(|a| a.player_id.as_str())
    }

    pub fn slot(&self, slot_id: &str) -> Option<&TacticalSlotSnapshot> {
        self.tactical_slots.iter().find(|s| s.slot_id == slot_id)
    }

    pub fn slot_for_player(&self, player_id: &str) -> Option<&TacticalSlotSnapshot> {
        let a = self.assignments.iter().find(|a| a.player_id == player_id)?;
        self.slot(&a.slot_id)
    }

    pub fn slot_instructions(&self, slot_id: &str) -> Option<&SlotInstructionsSnapshot> {
        self.slot_instructions.iter().find(|s| s.slot_id == slot_id)
    }

    pub fn slot_instructions_for_player(&self, player_id: &str) -> Option<&SlotInstructionsSnapshot> {
        let slot = self.slot_for_player(player_id)?;
        self.slot_instructions(&slot.slot_id)
    }

    pub fn lineup_player_ids_in_slot_order(&self) -> Vec<String> {
        self.assignments.iter().map(|a| a.player_id.clone()).collect()
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.team_id));
        h = fp::mix_string(h, Some(&self.display_name));
        h = fp::mix_string(h, Some(&self.shape_id));
        h = fp::mix_string(h, Some(&self.designated_goalkeeper_id));
        for p in &self.lineup { h = fp::mix_i64(h, p.fingerprint() as i64); }
        for p in &self.bench { h = fp::mix_i64(h, p.fingerprint() as i64); }
        for a in &self.assignments { h = fp::mix_i64(h, a.fingerprint() as i64); }
        for s in &self.tactical_slots { h = fp::mix_i64(h, s.fingerprint() as i64); }
        h = fp::mix_i64(h, self.team_instructions.fingerprint() as i64);
        for i in &self.slot_instructions { h = fp::mix_i64(h, i.fingerprint() as i64); }
        h = fp::mix_i64(h, self.set_piece_plan.fingerprint() as i64);
        fp::mix_i64(h, self.collective_state.fingerprint() as i64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchInput {
    pub match_id: String,
    pub seed: i64,
    pub engine_version: String,
    pub home_team: TeamMatchSnapshot,
    pub away_team: TeamMatchSnapshot,
    pub match_rules: MatchRules,
    pub neutral_ground: bool,
    pub competition_context: String,
    pub home_league_context: LeagueObjectiveSnapshot,
    pub away_league_context: LeagueObjectiveSnapshot,
    pub simulated_action_minutes: i32,
}

impl MatchInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        match_id: impl Into<String>,
        seed: i64,
        engine_version: impl Into<String>,
        home_team: TeamMatchSnapshot,
        away_team: TeamMatchSnapshot,
        match_rules: MatchRules,
        neutral_ground: bool,
        competition_context: impl Into<String>,
        home_league_context: LeagueObjectiveSnapshot,
        away_league_context: LeagueObjectiveSnapshot,
        simulated_action_minutes: i32,
    ) -> Result<Self, &'static str> {
        let match_id = match_id.into();
        let engine_version = engine_version.into();
        if !(10..=90).contains(&simulated_action_minutes) { return Err("simulatedActionMinutes out of range"); }
        if match_id.is_empty() { return Err("matchId required"); }
        if engine_version.is_empty() { return Err("engineVersion required"); }
        if seed == 0 { return Err("non-zero deterministic seed required"); }
        if home_team.team_id == away_team.team_id { return Err("team ids must be different"); }
        home_league_context.validate()?;
        away_league_context.validate()?;

        let mut ids = HashSet::new();
        for p in home_team.lineup.iter().chain(home_team.bench.iter()) {
            ids.insert(p.player_id.as_str());
        }
        for p in away_team.lineup.iter().chain(away_team.bench.iter()) {
            if !ids.insert(p.player_id.as_str()) {
                return Err("playerId appears for both teams");
            }
        }
        Ok(Self {
            match_id,
            seed,
            engine_version,
            home_team,
            away_team,
            match_rules,
            neutral_ground,
            competition_context: competition_context.into(),
            home_league_context,
            away_league_context,
            simulated_action_minutes,
        })
    }

    pub fn simulated_action_minutes(&self) -> i32 {
        if self.simulated_action_minutes == 0 { 90 } else { self.simulated_action_minutes }
    }

    pub fn with_simulated_action_minutes(&self, minutes: i32) -> Result<Self, &'static str> {
        if !(10..=90).contains(&minutes) { return Err("simulatedActionMinutes out of range"); }
        let mut copy = self.clone();
        copy.simulated_action_minutes = minutes;
        Ok(copy)
    }

    pub fn team(&self, side: TeamSide) -> &TeamMatchSnapshot {
        match side {
            TeamSide::Home => &self.home_team,
            TeamSide::Away => &self.away_team,
        }
    }

    pub fn league_context(&self, side: TeamSide) -> &LeagueObjectiveSnapshot {
        match side {
            TeamSide::Home => &self.home_league_context,
            TeamSide::Away => &self.away_league_context,
        }
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.match_id));
        h = fp::mix_i64(h, self.seed);
        h = fp::mix_string(h, Some(&self.engine_version));
        h = fp::mix_i64(h, self.home_team.fingerprint() as i64);
        h = fp::mix_i64(h, self.away_team.fingerprint() as i64);
        h = fp::mix_i64(h, self.match_rules.fingerprint() as i64);
        h = fp::mix_bool(h, self.neutral_ground);
        h = fp::mix_string(h, Some(&self.competition_context));
        h = fp::mix_i64(h, self.home_league_context.fingerprint() as i64);
        h = fp::mix_i64(h, self.away_league_context.fingerprint() as i64);
        if self.simulated_action_minutes() != 90 {
            h = fp::mix_string(h, Some("SIMULATED_ACTION_MINUTES"));
            h = fp::mix_i32(h, self.simulated_action_minutes());
        }
        h
    }
}
