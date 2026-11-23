use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::fingerprints;
use crate::v4::attributes::PlayerAttributes54;
use crate::v4::error::V4Error;
use crate::v4::{ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, REGULATION_MINUTES_V4};

fn default_protocol_version() -> i32 {
    ENGINE_PROTOCOL_VERSION_V4
}

fn default_engine_version() -> String {
    ENGINE_VERSION_V4.to_string()
}

fn default_weak_foot() -> u8 {
    3
}

fn default_height_cm() -> u16 {
    180
}

fn default_fifty() -> u8 {
    50
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SimulationMode {
    #[default]
    FullEvent,
    FastBatch,
}

impl SimulationMode {
    #[inline]
    pub fn retain_events(self) -> bool {
        matches!(self, SimulationMode::FullEvent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RulesModeV4 {
    #[default]
    Standard,
    Knockout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreferredFoot {
    #[default]
    Right,
    Left,
    Either,
}

impl PreferredFoot {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            PreferredFoot::Right => 0,
            PreferredFoot::Left => 1,
            PreferredFoot::Either => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WeatherCondition {
    #[default]
    Clear,
    LightRain,
    HeavyRain,
    Windy,
    Hot,
}

impl WeatherCondition {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            WeatherCondition::Clear => 0,
            WeatherCondition::LightRain => 1,
            WeatherCondition::HeavyRain => 2,
            WeatherCondition::Windy => 3,
            WeatherCondition::Hot => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PitchQuality {
    Excellent,
    #[default]
    Good,
    Heavy,
    Poor,
}

impl PitchQuality {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            PitchQuality::Excellent => 0,
            PitchQuality::Good => 1,
            PitchQuality::Heavy => 2,
            PitchQuality::Poor => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchEnvironment {
    #[serde(default)]
    pub weather: WeatherCondition,
    #[serde(default)]
    pub pitch_quality: PitchQuality,
    #[serde(default = "default_fifty")]
    pub wind_strength: u8,
    #[serde(default = "default_fifty")]
    pub home_crowd_intensity: u8,
    #[serde(default = "default_fifty")]
    pub match_importance: u8,
}

impl Default for MatchEnvironment {
    fn default() -> Self {
        Self {
            weather: WeatherCondition::Clear,
            pitch_quality: PitchQuality::Good,
            wind_strength: 15,
            home_crowd_intensity: 60,
            match_importance: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefereeProfile {
    #[serde(default = "default_fifty")]
    pub strictness: u8,
    #[serde(default = "default_fifty")]
    pub card_threshold: u8,
    #[serde(default = "default_fifty")]
    pub advantage_tendency: u8,
}

impl Default for RefereeProfile {
    fn default() -> Self {
        Self {
            strictness: 50,
            card_threshold: 50,
            advantage_tendency: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMatchSnapshotV4 {
    pub player_id: String,
    pub display_name: String,
    pub attributes: PlayerAttributes54,
    pub condition: u8,
    pub morale: u8,
    pub form: u8,
    pub age: u8,
    #[serde(default)]
    pub injured: bool,
    #[serde(default = "default_true")]
    pub eligible: bool,
    pub primary_position: String,
    #[serde(default)]
    pub secondary_position: String,
    #[serde(default)]
    pub preferred_foot: PreferredFoot,
    #[serde(default = "default_weak_foot")]
    pub weak_foot: u8,
    #[serde(default = "default_height_cm")]
    pub height_cm: u16,
}

impl PlayerMatchSnapshotV4 {
    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        h = fingerprints::mix_string(h, Some(&self.player_id));
        h = fingerprints::mix_i64(h, self.attributes.fingerprint() as i64);
        h = fingerprints::mix_i32(h, self.condition as i32);
        h = fingerprints::mix_i32(h, self.morale as i32);
        h = fingerprints::mix_i32(h, self.form as i32);
        h = fingerprints::mix_i32(h, self.age as i32);
        h = fingerprints::mix_bool(h, self.injured);
        h = fingerprints::mix_bool(h, self.eligible);
        h = fingerprints::mix_string(h, Some(&self.primary_position));
        h = fingerprints::mix_string(h, Some(&self.secondary_position));
        h = fingerprints::mix_i32(h, self.preferred_foot.code());
        h = fingerprints::mix_i32(h, self.weak_foot as i32);
        h = fingerprints::mix_i32(h, self.height_cm as i32);
        h
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupAssignmentV4 {
    pub player_id: String,
    pub slot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalSlotV4 {
    pub slot_id: String,
    pub out_x: i32,
    pub out_y: i32,
    pub in_x: i32,
    pub in_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotInstructionsV4 {
    pub slot_id: String,
    pub forward_movement: u8,
    pub passing_risk: u8,
    pub crossing_tendency: u8,
    pub dribbling_tendency: u8,
    pub defensive_aggression: u8,
    pub support_distance: u8,
    pub roaming: u8,
    pub shooting_tendency: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInstructionsV4 {
    pub pressing_intensity: u8,
    pub tempo: u8,
    pub line_height: u8,
    pub width: u8,
    pub time_wasting: u8,
    pub offside_trap: bool,
    pub aggression: u8,
    pub transition_risk: u8,
    pub build_up_risk: u8,
    pub concentration: u8,
}

impl Default for TeamInstructionsV4 {
    fn default() -> Self {
        Self {
            pressing_intensity: 50,
            tempo: 50,
            line_height: 50,
            width: 50,
            time_wasting: 10,
            offside_trap: false,
            aggression: 50,
            transition_risk: 50,
            build_up_risk: 50,
            concentration: 50,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPiecePlanV4 {
    #[serde(default)]
    pub penalty_taker_slot_ids: Vec<String>,
    #[serde(default)]
    pub corner_taker_slot_ids: Vec<String>,
    #[serde(default)]
    pub close_free_kick_taker_slot_ids: Vec<String>,
    #[serde(default)]
    pub far_free_kick_taker_slot_ids: Vec<String>,
    #[serde(default)]
    pub captain_slot_ids: Vec<String>,
    #[serde(default)]
    pub corner_box_slot_ids: Vec<String>,
    #[serde(default)]
    pub close_free_kick_box_slot_ids: Vec<String>,
    #[serde(default)]
    pub far_free_kick_box_slot_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamCollectiveV4 {
    #[serde(default = "default_fifty")]
    pub cohesion: u8,
    #[serde(default = "default_fifty")]
    pub system_familiarity: u8,
    #[serde(default = "default_fifty")]
    pub confidence: u8,
}

impl Default for TeamCollectiveV4 {
    fn default() -> Self {
        Self {
            cohesion: 50,
            system_familiarity: 50,
            confidence: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMatchSnapshotV4 {
    pub team_id: String,
    pub display_name: String,
    pub shape_id: String,
    pub designated_goalkeeper_id: String,
    pub lineup: Vec<PlayerMatchSnapshotV4>,
    #[serde(default)]
    pub bench: Vec<PlayerMatchSnapshotV4>,
    pub assignments: Vec<LineupAssignmentV4>,
    pub tactical_slots: Vec<TacticalSlotV4>,
    #[serde(default)]
    pub team_instructions: TeamInstructionsV4,
    #[serde(default)]
    pub slot_instructions: Vec<SlotInstructionsV4>,
    #[serde(default)]
    pub set_piece_plan: SetPiecePlanV4,
    #[serde(default)]
    pub collective_state: TeamCollectiveV4,
}

impl TeamMatchSnapshotV4 {
    pub fn validate(&self) -> Result<(), V4Error> {
        if self.team_id.trim().is_empty() {
            return Err(V4Error::InvalidRoster {
                team_id: self.team_id.clone(),
                reason: "team_id cannot be empty".into(),
            });
        }
        if self.lineup.len() != 11 {
            return Err(V4Error::InvalidRoster {
                team_id: self.team_id.clone(),
                reason: format!("starting lineup must have 11 players, got {}", self.lineup.len()),
            });
        }
        if self.assignments.len() != 11 {
            return Err(V4Error::InvalidRoster {
                team_id: self.team_id.clone(),
                reason: format!("assignments must have 11 entries, got {}", self.assignments.len()),
            });
        }
        if self.tactical_slots.len() != 11 {
            return Err(V4Error::InvalidRoster {
                team_id: self.team_id.clone(),
                reason: format!("tactical_slots must have 11 entries, got {}", self.tactical_slots.len()),
            });
        }

        // Strict uniqueness & eligibility checks across lineup and bench
        let mut lineup_ids: HashSet<&str> = HashSet::with_capacity(11);
        let mut all_ids: HashSet<&str> = HashSet::with_capacity(11 + self.bench.len());
        for p in &self.lineup {
            if p.player_id.trim().is_empty() {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: "player_id in lineup cannot be empty".into(),
                });
            }
            if !lineup_ids.insert(p.player_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("duplicate player_id '{}' in starting lineup", p.player_id),
                });
            }
            all_ids.insert(p.player_id.as_str());
            if p.injured || !p.eligible {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("starting player '{}' is injured or ineligible", p.player_id),
                });
            }
        }
        for p in &self.bench {
            if p.player_id.trim().is_empty() {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: "player_id in bench cannot be empty".into(),
                });
            }
            if !all_ids.insert(p.player_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("duplicate player_id '{}' across lineup/bench", p.player_id),
                });
            }
        }

        if !lineup_ids.contains(self.designated_goalkeeper_id.as_str()) {
            return Err(V4Error::InvalidRoster {
                team_id: self.team_id.clone(),
                reason: "designated_goalkeeper_id must be present in starting lineup".into(),
            });
        }

        // Validate 1-to-1 slot & assignment linkage
        let mut slot_ids: HashSet<&str> = HashSet::with_capacity(11);
        for s in &self.tactical_slots {
            if !slot_ids.insert(s.slot_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("duplicate tactical slot_id '{}'", s.slot_id),
                });
            }
            if !(0..=100).contains(&s.out_x)
                || !(0..=100).contains(&s.out_y)
                || !(0..=100).contains(&s.in_x)
                || !(0..=100).contains(&s.in_y)
            {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("tactical slot '{}' coordinates must be in 0..=100", s.slot_id),
                });
            }
        }
        let mut assigned_players: HashSet<&str> = HashSet::with_capacity(11);
        let mut assigned_slots: HashSet<&str> = HashSet::with_capacity(11);
        for a in &self.assignments {
            if !lineup_ids.contains(a.player_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("assignment references unknown lineup player '{}'", a.player_id),
                });
            }
            if !slot_ids.contains(a.slot_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("assignment references unknown tactical slot '{}'", a.slot_id),
                });
            }
            if !assigned_players.insert(a.player_id.as_str()) || !assigned_slots.insert(a.slot_id.as_str()) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("duplicate player or slot in assignments ('{}' -> '{}')", a.player_id, a.slot_id),
                });
            }
        }

        for p in self.lineup.iter().chain(self.bench.iter()) {
            p.attributes.validate()?;
            if p.condition > 100 || p.morale > 100 || p.form > 100 {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("player '{}' dynamic state must be in 0..=100", p.player_id),
                });
            }
            if !(1..=5).contains(&p.weak_foot) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("player '{}' weak_foot must be in 1..=5", p.player_id),
                });
            }
            if !(150..=220).contains(&p.height_cm) {
                return Err(V4Error::InvalidRoster {
                    team_id: self.team_id.clone(),
                    reason: format!("player '{}' height_cm must be in 150..=220", p.player_id),
                });
            }
        }
        Ok(())
    }

    /// Comprehensive fingerprint covering lineup, bench, assignments, slots, instructions,
    /// set-piece plans, and collective state for immutable Seed-Replay verification.
    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        h = fingerprints::mix_string(h, Some(&self.team_id));
        h = fingerprints::mix_string(h, Some(&self.shape_id));
        h = fingerprints::mix_string(h, Some(&self.designated_goalkeeper_id));
        for p in &self.lineup {
            h = fingerprints::mix_i64(h, p.fingerprint() as i64);
        }
        h = fingerprints::mix_i32(h, self.bench.len() as i32);
        for p in &self.bench {
            h = fingerprints::mix_i64(h, p.fingerprint() as i64);
        }
        for a in &self.assignments {
            h = fingerprints::mix_string(h, Some(&a.player_id));
            h = fingerprints::mix_string(h, Some(&a.slot_id));
        }
        for s in &self.tactical_slots {
            h = fingerprints::mix_string(h, Some(&s.slot_id));
            h = fingerprints::mix_i32(h, s.out_x);
            h = fingerprints::mix_i32(h, s.out_y);
            h = fingerprints::mix_i32(h, s.in_x);
            h = fingerprints::mix_i32(h, s.in_y);
        }
        let ti = &self.team_instructions;
        h = fingerprints::mix_i32(h, ti.pressing_intensity as i32);
        h = fingerprints::mix_i32(h, ti.tempo as i32);
        h = fingerprints::mix_i32(h, ti.line_height as i32);
        h = fingerprints::mix_i32(h, ti.width as i32);
        h = fingerprints::mix_i32(h, ti.time_wasting as i32);
        h = fingerprints::mix_bool(h, ti.offside_trap);
        h = fingerprints::mix_i32(h, ti.aggression as i32);
        h = fingerprints::mix_i32(h, ti.transition_risk as i32);
        h = fingerprints::mix_i32(h, ti.build_up_risk as i32);
        h = fingerprints::mix_i32(h, ti.concentration as i32);
        for si in &self.slot_instructions {
            h = fingerprints::mix_string(h, Some(&si.slot_id));
            h = fingerprints::mix_i32(h, si.forward_movement as i32);
            h = fingerprints::mix_i32(h, si.passing_risk as i32);
            h = fingerprints::mix_i32(h, si.crossing_tendency as i32);
            h = fingerprints::mix_i32(h, si.dribbling_tendency as i32);
            h = fingerprints::mix_i32(h, si.defensive_aggression as i32);
            h = fingerprints::mix_i32(h, si.support_distance as i32);
            h = fingerprints::mix_i32(h, si.roaming as i32);
            h = fingerprints::mix_i32(h, si.shooting_tendency as i32);
        }
        h = fingerprints::mix_i32(h, self.collective_state.cohesion as i32);
        h = fingerprints::mix_i32(h, self.collective_state.system_familiarity as i32);
        h = fingerprints::mix_i32(h, self.collective_state.confidence as i32);
        h
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInputV4 {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: i32,
    pub match_id: String,
    pub seed: i64,
    #[serde(default = "default_engine_version")]
    pub engine_version: String,
    #[serde(default)]
    pub neutral_ground: bool,
    #[serde(default)]
    pub competition_context: String,
    #[serde(default)]
    pub rules_mode: RulesModeV4,
    #[serde(default)]
    pub simulation_mode: SimulationMode,
    #[serde(default)]
    pub simulated_action_minutes: Option<i32>,
    #[serde(default)]
    pub ai_home_manager: bool,
    #[serde(default = "default_true")]
    pub ai_away_manager: bool,
    #[serde(default)]
    pub match_environment: MatchEnvironment,
    #[serde(default)]
    pub referee_profile: RefereeProfile,
    pub home_team: TeamMatchSnapshotV4,
    pub away_team: TeamMatchSnapshotV4,
}

impl MatchInputV4 {
    pub fn validate(&self) -> Result<(), V4Error> {
        if self.protocol_version != ENGINE_PROTOCOL_VERSION_V4 {
            return Err(V4Error::InvalidProtocolVersion {
                expected: ENGINE_PROTOCOL_VERSION_V4,
                actual: self.protocol_version,
            });
        }
        if let Some(mins) = self.simulated_action_minutes {
            if mins != REGULATION_MINUTES_V4 {
                return Err(V4Error::UnsupportedCompressionMode);
            }
        }
        if self.match_id.trim().is_empty() {
            return Err(V4Error::InvalidInput("match_id cannot be empty".into()));
        }
        if self.home_team.team_id == self.away_team.team_id {
            return Err(V4Error::InvalidInput(
                "home_team and away_team cannot share the same team_id".into(),
            ));
        }
        self.home_team.validate()?;
        self.away_team.validate()?;

        // Cross-team duplicate player_id guard
        let home_ids: HashSet<&str> = self
            .home_team
            .lineup
            .iter()
            .chain(self.home_team.bench.iter())
            .map(|p| p.player_id.as_str())
            .collect();
        for p in self.away_team.lineup.iter().chain(self.away_team.bench.iter()) {
            if home_ids.contains(p.player_id.as_str()) {
                return Err(V4Error::InvalidInput(format!(
                    "player_id '{}' appears in both home_team and away_team",
                    p.player_id
                )));
            }
        }
        Ok(())
    }

    /// Complete deterministic input fingerprint over every parameter affecting simulation.
    pub fn input_fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        h = fingerprints::mix_i32(h, self.protocol_version);
        h = fingerprints::mix_string(h, Some(&self.match_id));
        h = fingerprints::mix_i64(h, self.seed);
        h = fingerprints::mix_bool(h, self.neutral_ground);
        h = fingerprints::mix_string(h, Some(&self.competition_context));
        h = fingerprints::mix_i32(h, match self.rules_mode {
            RulesModeV4::Standard => 0,
            RulesModeV4::Knockout => 1,
        });
        h = fingerprints::mix_bool(h, self.ai_home_manager);
        h = fingerprints::mix_bool(h, self.ai_away_manager);
        h = fingerprints::mix_i32(h, self.match_environment.weather.code());
        h = fingerprints::mix_i32(h, self.match_environment.pitch_quality.code());
        h = fingerprints::mix_i32(h, self.match_environment.wind_strength as i32);
        h = fingerprints::mix_i32(h, self.match_environment.home_crowd_intensity as i32);
        h = fingerprints::mix_i32(h, self.match_environment.match_importance as i32);
        h = fingerprints::mix_i32(h, self.referee_profile.strictness as i32);
        h = fingerprints::mix_i32(h, self.referee_profile.card_threshold as i32);
        h = fingerprints::mix_i32(h, self.referee_profile.advantage_tendency as i32);
        h = fingerprints::mix_i64(h, self.home_team.fingerprint() as i64);
        h = fingerprints::mix_i64(h, self.away_team.fingerprint() as i64);
        h
    }
}
