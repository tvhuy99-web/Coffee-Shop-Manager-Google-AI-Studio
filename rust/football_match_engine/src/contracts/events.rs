use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fingerprints;
use crate::v4::input::SimulationMode;
use crate::v4::rules::CommandLifecycleStatus;
use crate::v4::spatial::{quantize_mm, TeamSideV4, Vec3};
use crate::v4::state::InjuryKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HighlightTier {
    #[default]
    None,
    Extended,
    GoalKey,
}

impl HighlightTier {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            HighlightTier::None => 0,
            HighlightTier::Extended => 1,
            HighlightTier::GoalKey => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(i32)]
pub enum EventTypeV4 {
    MatchStarted = 0,
    Kickoff = 1,
    PossessionStarted = 2,
    TacticalPhaseChanged = 3,
    TransitionStarted = 4,
    TransitionEnded = 5,
    PassCompleted = 6,
    PassFailed = 7,
    ThroughBall = 8,
    FirstTouch = 9,
    Carry = 10,
    DribbleWon = 11,
    DribbleLost = 12,
    Cross = 13,
    TackleWon = 14,
    Interception = 15,
    Block = 16,
    Clearance = 17,
    AerialDuel = 18,
    LooseBall = 19,
    SecondBallWon = 20,
    Turnover = 21,
    Shot = 22,
    ShotOnTarget = 23,
    ShotOffTarget = 24,
    Header = 25,
    Goal = 26,
    OwnGoal = 27,
    Save = 28,
    GoalkeeperClaim = 29,
    GoalkeeperPunch = 30,
    GoalkeeperRush = 31,
    GoalkeeperDistribution = 32,
    Rebound = 33,
    Foul = 34,
    AdvantagePlayed = 35,
    YellowCard = 36,
    RedCard = 37,
    Offside = 38,
    ThrowIn = 39,
    GoalKick = 40,
    Corner = 41,
    FreeKick = 42,
    PenaltyAwarded = 43,
    PenaltyKick = 44,
    Injury = 45,
    Substitution = 46,
    AddedTimeAnnounced = 47,
    HalfTime = 48,
    ExtraTimeStarted = 49,
    ExtraTimeHalfTime = 50,
    PenaltyShootoutStarted = 51,
    PenaltyShootoutKick = 52,
    PenaltyShootoutEnded = 53,
    FullTime = 54,
}

/// Typed Event Metadata Contract aligned 1-to-1 with `LabCommentaryTemplatePolicy.kt`:
/// - `distance_m` serializes as `"distance"` (exact key checked by Kotlin commentary policy)
/// - `progress_gain` serializes as integer meters (`i32`) matching Kotlin's `toIntOrNull()`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMetadataV4 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escaped_pressure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sustained_pressure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_wasting: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_defensive_line: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ten_men: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_kick_executed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finesse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shot_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebound_shot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_box: Option<bool>,
    #[serde(rename = "distance", skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_vs_one: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xg: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_gain: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goalkeeper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injury_kind: Option<InjuryKind>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl EventMetadataV4 {
    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        let mix_opt_bool = |h: u64, v: Option<bool>| match v {
            None => fingerprints::mix_i32(h, -1),
            Some(b) => fingerprints::mix_bool(h, b),
        };
        let mix_opt_mm = |h: u64, v: Option<f32>| match v {
            None => fingerprints::mix_i32(h, i32::MIN),
            Some(f) => fingerprints::mix_i32(h, quantize_mm(f)),
        };
        h = mix_opt_bool(h, self.escaped_pressure);
        h = mix_opt_bool(h, self.overload);
        h = mix_opt_bool(h, self.sustained_pressure);
        h = mix_opt_bool(h, self.time_wasting);
        h = mix_opt_bool(h, self.high_defensive_line);
        h = mix_opt_bool(h, self.ten_men);
        h = mix_opt_bool(h, self.free_kick_executed);
        h = mix_opt_bool(h, self.finesse);
        h = fingerprints::mix_string(h, self.shot_type.as_deref());
        h = mix_opt_bool(h, self.rebound_shot);
        h = fingerprints::mix_string(h, self.pass_type.as_deref());
        h = mix_opt_bool(h, self.in_box);
        h = mix_opt_mm(h, self.distance_m);
        h = mix_opt_bool(h, self.one_vs_one);
        h = mix_opt_mm(h, self.xg);
        h = fingerprints::mix_i32(h, self.pressure.map(|p| p as i32).unwrap_or(-1));
        h = fingerprints::mix_i32(h, self.progress_gain.unwrap_or(i32::MIN));
        h = mix_opt_bool(h, self.scored);
        h = fingerprints::mix_string(h, self.outcome.as_deref());
        h = fingerprints::mix_string(h, self.cause.as_deref());
        h = fingerprints::mix_string(h, self.goalkeeper.as_deref());
        h = fingerprints::mix_i32(
            h,
            match self.injury_kind {
                None => -1,
                Some(InjuryKind::Impact) => 1,
                Some(InjuryKind::MuscleNonContact) => 2,
            },
        );
        for (k, v) in &self.extra {
            h = fingerprints::mix_string(h, Some(k));
            h = fingerprints::mix_string(h, Some(v));
        }
        h
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchEventV4 {
    pub sequence: u32,
    pub game_second: i32,
    pub period: i32,
    /// Seconds elapsed in this specific period, excluding preceding halves' added time.
    #[serde(default)]
    pub period_elapsed_second: i32,
    /// Monotonic DES simulation timestamp in milliseconds.
    #[serde(default)]
    pub sim_time_ms: i64,
    /// Milliseconds elapsed in this specific period, excluding preceding halves' added time.
    #[serde(default)]
    pub period_elapsed_ms: i64,
    /// DES interval duration in milliseconds that led to this event.
    #[serde(default)]
    pub dt_ms: i64,
    pub kind: EventTypeV4,
    pub highlight_tier: HighlightTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight_sequence_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<TeamSideV4>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub ball_pos: Vec3,
    #[serde(default)]
    pub metadata: EventMetadataV4,
}

impl MatchEventV4 {
    pub fn fingerprint(&self) -> u64 {
        fingerprint_event_parts(
            self.sequence,
            self.game_second,
            self.sim_time_ms,
            self.period,
            self.kind,
            self.highlight_tier,
            self.highlight_sequence_id,
            self.side,
            self.actor.as_deref(),
            self.target.as_deref(),
            self.ball_pos,
            &self.metadata,
        )
    }
}

/// Zero-allocation fingerprint helper so `FastBatch` computes identical `event_timeline_fingerprint`
/// without allocating a `MatchEventV4` struct when `retain_events == false`.
#[allow(clippy::too_many_arguments)]
pub fn fingerprint_event_parts(
    sequence: u32,
    game_second: i32,
    sim_time_ms: i64,
    period: i32,
    kind: EventTypeV4,
    highlight_tier: HighlightTier,
    highlight_sequence_id: Option<u32>,
    side: Option<TeamSideV4>,
    actor: Option<&str>,
    target: Option<&str>,
    ball_pos: Vec3,
    metadata: &EventMetadataV4,
) -> u64 {
    let mut h = fingerprints::start();
    h = fingerprints::mix_i32(h, sequence as i32);
    h = fingerprints::mix_i32(h, game_second);
    h = fingerprints::mix_i64(h, sim_time_ms);
    h = fingerprints::mix_i32(h, period);
    h = fingerprints::mix_i32(h, kind as i32);
    h = fingerprints::mix_i32(h, highlight_tier.code());
    h = fingerprints::mix_i32(h, highlight_sequence_id.map(|v| v as i32).unwrap_or(-1));
    h = fingerprints::mix_i32(h, side.map(|s| s.code()).unwrap_or(-1));
    h = fingerprints::mix_string(h, actor);
    h = fingerprints::mix_string(h, target);
    h = fingerprints::mix_i32(h, quantize_mm(ball_pos.x));
    h = fingerprints::mix_i32(h, quantize_mm(ball_pos.y));
    h = fingerprints::mix_i32(h, quantize_mm(ball_pos.z));
    h = fingerprints::mix_i64(h, metadata.fingerprint() as i64);
    h
}

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
