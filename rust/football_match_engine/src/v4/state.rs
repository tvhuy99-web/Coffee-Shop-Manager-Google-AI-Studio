use serde::{Deserialize, Serialize};

use crate::clock::ClockState;
use crate::v4::attributes::normalize_attr;
use crate::v4::input::{
    MatchInputV4, PlayerMatchSnapshotV4, TeamInstructionsV4, TeamMatchSnapshotV4,
};
use crate::v4::spatial::{TeamSideV4, Vec2, Vec3};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BallControlState {
    DeadBall,
    Controlled,
    InFlight,
    Loose,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BallStateV4 {
    pub pos: Vec3,
    pub vel: Vec3,
    pub control: BallControlState,
    pub owner_side: Option<TeamSideV4>,
    pub owner_player_id: Option<String>,
    pub last_touch_side: Option<TeamSideV4>,
    pub last_touch_player_id: Option<String>,
}

impl Default for BallStateV4 {
    fn default() -> Self {
        Self {
            pos: Vec3::center_spot(),
            vel: Vec3::default(),
            control: BallControlState::DeadBall,
            owner_side: None,
            owner_player_id: None,
            last_touch_side: None,
            last_touch_player_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwoPoolFatigueState {
    pub aerobic: f32,
    pub anaerobic: f32,
    pub distance_m: f32,
    pub sprint_count: u32,
    pub high_intensity_actions: u32,
}

impl TwoPoolFatigueState {
    pub fn from_condition(condition: u8) -> Self {
        Self {
            aerobic: (condition as f32).clamp(10.0, 100.0),
            anaerobic: 100.0,
            distance_m: 0.0,
            sprint_count: 0,
            high_intensity_actions: 0,
        }
    }

    pub fn apply_interval(
        &mut self,
        duration_sec: f32,
        distance_m: f32,
        is_high_intensity: bool,
        stamina: u8,
        natural_fitness: u8,
    ) {
        let stam = normalize_attr(stamina);
        let nat = normalize_attr(natural_fitness);
        let aerobic_efficiency = 1.24 - (stam * 0.52);
        let base_aerobic_drain = (distance_m * 0.0021 + duration_sec * 0.0038) * aerobic_efficiency;
        let intensity_aerobic_drain = if is_high_intensity {
            0.08 * aerobic_efficiency
        } else {
            0.0
        };
        self.aerobic = (self.aerobic - base_aerobic_drain - intensity_aerobic_drain).clamp(5.0, 100.0);

        if is_high_intensity {
            let ana_drain = (4.8 + distance_m * 0.14) * (1.20 - stam * 0.35);
            self.anaerobic = (self.anaerobic - ana_drain).clamp(0.0, 100.0);
            self.sprint_count = self.sprint_count.saturating_add(1);
            self.high_intensity_actions = self.high_intensity_actions.saturating_add(1);
        } else {
            let recovery = duration_sec * (0.24 + nat * 0.38);
            self.anaerobic = (self.anaerobic + recovery).clamp(0.0, 100.0);
        }
        self.distance_m += distance_m.max(0.0);
    }

    pub fn apply_half_time_recovery(&mut self, natural_fitness: u8) {
        let nat = normalize_attr(natural_fitness);
        self.aerobic = (self.aerobic + 5.0 + nat * 7.0).clamp(5.0, 100.0);
        self.anaerobic = (self.anaerobic + 45.0 + nat * 35.0).clamp(0.0, 100.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InjuryKind {
    Impact,
    MuscleNonContact,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPsychologyState {
    pub composure_delta: f32,
    pub concentration_level: f32,
    pub pressure_load: f32,
    pub confidence: f32,
}

impl PlayerPsychologyState {
    pub fn from_snapshot(player: &PlayerMatchSnapshotV4) -> Self {
        let readiness = player.attributes.career_readiness_hook();
        Self {
            composure_delta: 0.0,
            concentration_level: (0.78 + normalize_attr(player.attributes.mental.concentration) * 0.22)
                .clamp(0.4, 1.0),
            pressure_load: 0.0,
            confidence: ((player.morale as f32 * 0.5 + player.form as f32 * 0.5) / 100.0
                * (0.90 + readiness * 0.10))
                .clamp(0.15, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TacticalPhaseV4 {
    InPossession,
    AttackingTransition,
    DefensiveTransition,
    OutOfPossession,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRuntimeStateV4 {
    pub player_id: String,
    pub side: TeamSideV4,
    pub slot_id: String,
    pub pos: Vec2,
    pub target_pos: Vec2,
    pub vel: Vec2,
    pub facing_rad: f32,
    pub on_pitch: bool,
    pub is_goalkeeper: bool,
    pub sent_off: bool,
    pub yellow_cards: u8,
    pub injury: Option<InjuryKind>,
    pub position_familiarity: f32,
    pub fatigue: TwoPoolFatigueState,
    pub psychology: PlayerPsychologyState,
    pub goals: u8,
    pub own_goals: u8,
    pub assists: u8,
    pub rating_tenths: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRuntimeStateV4 {
    pub side: TeamSideV4,
    pub goals: u16,
    pub shots: u16,
    pub shots_on_target: u16,
    pub shots_off_target: u16,
    pub shots_blocked: u16,
    pub saves: u16,
    pub own_goals_for: u16,
    pub own_goals_conceded: u16,
    pub passes_attempted: u16,
    pub passes_completed: u16,
    pub possession_seconds: u32,
    pub xg_millis: u32,
    pub fouls: u16,
    pub yellow_cards: u16,
    pub red_cards: u16,
    pub corners: u16,
    pub offsides: u16,
    pub throw_ins: u16,
    pub substitutions_used: u8,
    pub substitution_windows_used: u8,
    pub momentum: f32,
    pub phase: TacticalPhaseV4,
    pub sustained_pressure_streak: u8,
}

impl TeamRuntimeStateV4 {
    pub fn new(side: TeamSideV4) -> Self {
        Self {
            side,
            goals: 0,
            shots: 0,
            shots_on_target: 0,
            shots_off_target: 0,
            shots_blocked: 0,
            saves: 0,
            own_goals_for: 0,
            own_goals_conceded: 0,
            passes_attempted: 0,
            passes_completed: 0,
            possession_seconds: 0,
            xg_millis: 0,
            fouls: 0,
            yellow_cards: 0,
            red_cards: 0,
            corners: 0,
            offsides: 0,
            throw_ins: 0,
            substitutions_used: 0,
            substitution_windows_used: 0,
            momentum: 0.0,
            phase: TacticalPhaseV4::OutOfPossession,
            sustained_pressure_streak: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchStateV4 {
    pub clock: ClockState,
    pub ball: BallStateV4,
    pub home: TeamRuntimeStateV4,
    pub away: TeamRuntimeStateV4,
    pub active_home_ti: TeamInstructionsV4,
    pub active_away_ti: TeamInstructionsV4,
    pub transition_beats_remaining: u8,
    pub last_ai_eval_second: i32,
    pub players: Vec<PlayerRuntimeStateV4>,
    pub next_highlight_sequence_id: u32,
    pub shootout_home_goals: u8,
    pub shootout_away_goals: u8,
}

impl MatchStateV4 {
    pub fn from_input(input: &MatchInputV4) -> Self {
        let mut players = Vec::with_capacity(32);
        init_team_players(TeamSideV4::Home, &input.home_team, &mut players);
        init_team_players(TeamSideV4::Away, &input.away_team, &mut players);
        Self {
            clock: ClockState::default(),
            ball: BallStateV4::default(),
            home: TeamRuntimeStateV4::new(TeamSideV4::Home),
            away: TeamRuntimeStateV4::new(TeamSideV4::Away),
            active_home_ti: input.home_team.team_instructions.clone(),
            active_away_ti: input.away_team.team_instructions.clone(),
            transition_beats_remaining: 0,
            last_ai_eval_second: 0,
            players,
            next_highlight_sequence_id: 1,
            shootout_home_goals: 0,
            shootout_away_goals: 0,
        }
    }

    pub fn team_state(&self, side: TeamSideV4) -> &TeamRuntimeStateV4 {
        match side {
            TeamSideV4::Home => &self.home,
            TeamSideV4::Away => &self.away,
        }
    }

    pub fn team_state_mut(&mut self, side: TeamSideV4) -> &mut TeamRuntimeStateV4 {
        match side {
            TeamSideV4::Home => &mut self.home,
            TeamSideV4::Away => &mut self.away,
        }
    }

    pub fn active_instructions(&self, side: TeamSideV4) -> &TeamInstructionsV4 {
        match side {
            TeamSideV4::Home => &self.active_home_ti,
            TeamSideV4::Away => &self.active_away_ti,
        }
    }

    pub fn active_instructions_mut(&mut self, side: TeamSideV4) -> &mut TeamInstructionsV4 {
        match side {
            TeamSideV4::Home => &mut self.active_home_ti,
            TeamSideV4::Away => &mut self.active_away_ti,
        }
    }
}

fn init_team_players(
    side: TeamSideV4,
    team: &TeamMatchSnapshotV4,
    out: &mut Vec<PlayerRuntimeStateV4>,
) {
    let facing_rad = match side {
        TeamSideV4::Home => 0.0,
        TeamSideV4::Away => std::f32::consts::PI,
    };
    for player in &team.lineup {
        let slot_id = team
            .assignments
            .iter()
            .find(|a| a.player_id == player.player_id)
            .map(|a| a.slot_id.as_str())
            .unwrap_or("CM");
        let slot = team.tactical_slots.iter().find(|s| s.slot_id == slot_id);
        let pos = match slot {
            Some(s) => Vec2::from_normalized_slot(side, s.out_x as f32, s.out_y as f32),
            None => Vec2::center_spot(),
        };
        let familiarity = compute_position_familiarity(
            &player.primary_position,
            &player.secondary_position,
            slot_id,
        );
        out.push(PlayerRuntimeStateV4 {
            player_id: player.player_id.clone(),
            side,
            slot_id: slot_id.to_string(),
            pos,
            target_pos: pos,
            vel: Vec2::default(),
            facing_rad,
            on_pitch: true,
            is_goalkeeper: player.player_id == team.designated_goalkeeper_id,
            sent_off: false,
            yellow_cards: 0,
            injury: None,
            position_familiarity: familiarity,
            fatigue: TwoPoolFatigueState::from_condition(player.condition),
            psychology: PlayerPsychologyState::from_snapshot(player),
            goals: 0,
            own_goals: 0,
            assists: 0,
            rating_tenths: 60,
        });
    }
}

pub fn compute_position_familiarity(primary: &str, secondary: &str, slot_id: &str) -> f32 {
    let p = canonical_role(primary);
    let s = canonical_role(secondary);
    let slot = canonical_role(slot_id);
    if p == slot {
        1.0
    } else if !s.is_empty() && s == slot {
        0.92
    } else if slot == "GK" || p == "GK" {
        0.55
    } else if role_band(p) == role_band(slot) {
        0.86
    } else {
        0.72
    }
}

fn canonical_role(raw: &str) -> &'static str {
    let u = raw.trim().to_ascii_uppercase();
    match u.as_str() {
        "GK" => "GK",
        "CB" | "LCB" | "RCB" | "SW" | "B" => "CB",
        "LB" | "LWB" => "LB",
        "RB" | "RWB" => "RB",
        "DM" | "CDM" | "LDM" | "RDM" => "DM",
        "CM" | "LCM" | "RCM" | "M" => "CM",
        "AM" | "CAM" | "LAM" | "RAM" => "AM",
        "LW" | "LM" => "LW",
        "RW" | "RM" => "RW",
        "CF" | "ST" | "FW" | "F" | "LS" | "RS" => "CF",
        _ => "",
    }
}

fn role_band(role: &str) -> u8 {
    match role {
        "GK" => 0,
        "CB" | "LB" | "RB" => 1,
        "DM" | "CM" | "AM" => 2,
        "LW" | "RW" | "CF" => 3,
        _ => 4,
    }
}
