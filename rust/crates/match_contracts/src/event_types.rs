use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fingerprints;
use crate::spatial::{quantize_mm, TeamSideV4, Vec3};

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


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InjuryKind {
    Impact,
    MuscleNonContact,
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum DefensiveRole { Press, Screen, Cover, Mark }

