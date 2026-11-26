//! Engine result and session envelopes; canonical event DTOs and fingerprints are shared.
use serde::{Deserialize, Serialize};
use crate::v4::input::SimulationMode;
use crate::v4::rules::CommandLifecycleStatus;
use crate::v4::spatial::TeamSideV4;
pub use match_contracts::event_types::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMatchSummaryV4 {
    pub player_id: String,
    pub side: TeamSideV4,
    pub goals: u8,
    #[serde(default)]
    pub own_goals: u8,
    pub assists: u8,
    pub yellow_cards: u8,
    pub sent_off: bool,
    pub suspended_next_match: bool,
    pub injury: Option<InjuryKind>,
    pub final_aerobic: f32,
    pub final_anaerobic: f32,
    pub post_match_condition: u8,
    pub morale_delta: i8,
    pub distance_m: f32,
    pub rating: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMatchSummaryV4 {
    pub team_id: String,
    pub goals: u16,
    pub shots: u16,
    pub shots_on_target: u16,
    #[serde(default)]
    pub shots_off_target: u16,
    #[serde(default)]
    pub shots_blocked: u16,
    #[serde(default)]
    pub saves: u16,
    #[serde(default)]
    pub own_goals_for: u16,
    #[serde(default)]
    pub own_goals_conceded: u16,
    pub passes_attempted: u16,
    pub passes_completed: u16,
    pub possession_pct: f32,
    pub xg: f32,
    pub fouls: u16,
    pub yellow_cards: u16,
    pub red_cards: u16,
    pub corners: u16,
    pub offsides: u16,
    pub throw_ins: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummaryV4 {
    pub home: TeamMatchSummaryV4,
    pub away: TeamMatchSummaryV4,
    pub final_game_second: i32,
    pub first_half_elapsed_seconds: i32,
    pub second_half_elapsed_seconds: i32,
    pub extra_time_first_elapsed_seconds: i32,
    pub extra_time_second_elapsed_seconds: i32,
    pub went_to_extra_time: bool,
    pub went_to_shootout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shootout_home_goals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shootout_away_goals: Option<u8>,
    pub players: Vec<PlayerMatchSummaryV4>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResultV4 {
    pub ok: bool,
    pub protocol_version: i32,
    pub engine_version: String,
    pub event_schema_version: i32,
    pub match_id: String,
    pub seed: i64,
    pub simulation_mode: SimulationMode,
    pub input_fingerprint: String,
    pub event_timeline_fingerprint: String,
    pub event_count: u32,
    /// Accepted player commands with precise DES timeline ordering for deterministic replay.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_timeline: Vec<crate::v4::session::AppliedCommandV4>,
    pub summary: MatchSummaryV4,
    /// Optional for backward-compatible JSON readers; always populated by V4.1 DES.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<crate::v4::diagnostics::MatchDiagnosticsV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<MatchEventV4>,
}

/// Authoritative, read-only live squad state for the Android substitution UI.
/// This reflects AI-initiated as well as user-initiated substitutions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRosterStateV4 {
    pub on_pitch_player_ids: Vec<String>,
    pub available_bench_player_ids: Vec<String>,
    pub substitutions_used: u8,
    pub substitution_windows_used: u8,
    pub interval_substitution_allowed: bool,
}

/// Coarse session lifecycle stage surfaced on `SessionStepResultV4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStageV4 {
    #[default]
    PreMatch,
    InHalf,
    HalfTimeBreak,
    ExtraTimeBreak,
    PenaltyShootout,
    FullTime,
}

/// Lightweight status summary for pending tactical or substitution commands in the DES queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCommandSummaryV4 {
    pub command_id: u64,
    pub command_type: String,
    pub side: TeamSideV4,
    pub accepted_sim_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_effective_sim_time_ms: Option<i64>,
    pub status: CommandLifecycleStatus,
}

/// Interactive step response returned by `MatchSessionV4::advance_steps` /
/// `advance_until_ms` / `start_next_period` / JNI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStepResultV4 {
    pub ok: bool,
    pub period: i32,
    pub game_second: i32,
    /// Independent playing time for the active half/extra-time period.
    pub period_elapsed_second: i32,
    pub sim_time_ms: i64,
    pub period_elapsed_ms: i64,
    pub stage: SessionStageV4,
    pub dead_ball_state: String,
    pub stop_reason: String,
    pub ball_in_play_ms: i64,
    pub ball_out_of_play_ms: i64,
    pub compensable_lost_ms: i64,
    pub announced_added_ms: i64,
    pub additional_added_ms: i64,
    pub effective_added_ms: i64,
    pub steps_executed: u32,
    pub state_generation: u64,
    /// Number of simulation steps already applied (command replay boundary).
    pub completed_beats: u64,
    pub home_goals: u16,
    pub away_goals: u16,
    pub is_full_time: bool,
    pub total_events: u32,
    pub new_events: Vec<MatchEventV4>,
    #[serde(default)]
    pub pending_commands: Vec<PendingCommandSummaryV4>,
    pub home_roster: TeamRosterStateV4,
    pub away_roster: TeamRosterStateV4,
}
