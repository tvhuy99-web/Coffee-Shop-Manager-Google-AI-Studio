use serde::{Deserialize, Serialize};

use crate::v4::diagnostics::{MatchDiagnosticsCollectorV4, MatchDiagnosticsV4};
use crate::v4::perception::PassKindV4;
use crate::v4::interaction::EarlyInteractionHit;
use crate::v4::error::V4Error;
use crate::v4::events::MatchEventV4;
use crate::v4::input::MatchInputV4;
use crate::v4::rules::{
    check_ruleset_compatibility, HalfStageV4, PendingSubstitutionItem, PendingTacticalCommand,
    RulesetIdentity,
};
use crate::v4::scheduler::EventScheduler;
use crate::v4::session::AppliedCommandV4;
use crate::v4::spatial::TeamSideV4;
use crate::v4::state::MatchStateV4;
use crate::v4::{ENGINE_PROTOCOL_VERSION_V4, EVENT_SCHEMA_VERSION_V4};

pub const CHECKPOINT_SCHEMA_VERSION_V4: u32 = 2;

/// Serializable snapshot of `DeterministicRng` state for exact post-resume replay parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RngSnapshotV4 {
    pub root_seed: i64,
    pub state: i64,
}

/// Serializable aggregate session counters preserved across checkpoint export/import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatsSnapshotV4 {
    pub timeline_fingerprint: u64,
    pub event_count: u32,
    pub last_delivered_event_index: usize,
    pub completed_beats: u64,
    pub next_command_id: u64,
    pub went_to_extra_time: bool,
    pub went_to_shootout: bool,
    pub finished: bool,
    pub possession_side: TeamSideV4,
    pub carrier_idx: usize,
    pub last_passer_idx: Option<usize>,
    pub first_half_elapsed_ms: i64,
    pub second_half_elapsed_ms: i64,
    pub et1_elapsed_ms: i64,
    pub et2_elapsed_ms: i64,
    pub home_possession_ms: i64,
    pub away_possession_ms: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained_events: Vec<MatchEventV4>,
}

/// A single sampled flight resolution. Do not apply its counters or events until the
/// scheduled interaction time. No second pass/shot RNG draw is permitted at arrival.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFlightSnapshotV4 {
    pub trajectory: crate::v4::interaction::BallTrajectory,
    pub planned_arrival_ms: i64,
    pub resolves_at_ms: i64,
    pub action: FlightActionV4,
    pub outcome: crate::v4::ball_play::BallPlayOutcome,
    pub ball_after: crate::v4::state::BallStateV4,
    pub counter_delta: FlightCounterDeltaV4,
    pub won_corner: bool,
    pub early_interaction: Option<EarlyInteractionHit>,
    // Optional for existing development snapshots. Final release must version
    // the changed engine behaviour before promising replay compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pass_contact_motion: Option<crate::v4::interaction::PassContactMotion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) offside_at_launch: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlightActionV4 {
    Pass { passer_idx: usize, kind: PassKindV4, distance_m: f32, is_one_touch: bool },
    Shot { shooter_idx: usize },
    LooseRecovery { origin_side: TeamSideV4,
        #[serde(default = "default_loose_pass_cause")] cause: String },

}

fn default_loose_pass_cause() -> String { "PASS_CONTACT_LOOSE".into() }

/// Only copy additive outcome deltas, never the entire cloned world: intervening
/// fatigue, tactical orders, positions and disciplinary state must survive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightCounterDeltaV4 {
    pub home: TeamFlightDeltaV4,
    pub away: TeamFlightDeltaV4,
    pub players: Vec<PlayerFlightDeltaV4>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamFlightDeltaV4 {
    pub goals: u16, pub shots: u16, pub shots_on_target: u16, pub shots_off_target: u16,
    pub shots_blocked: u16, pub saves: u16, pub own_goals_for: u16, pub own_goals_conceded: u16,
    pub passes_attempted: u16, pub passes_completed: u16, pub xg_millis: u32,
    pub corners: u16, pub offsides: u16, pub throw_ins: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerFlightDeltaV4 {
    pub player_id: String, pub goals: u8, pub own_goals: u8, pub assists: u8, pub rating_delta: i16,
}
impl TeamFlightDeltaV4 {
    fn between(before: &crate::v4::state::TeamRuntimeStateV4, after: &crate::v4::state::TeamRuntimeStateV4) -> Self {
        Self {
            goals: after.goals.saturating_sub(before.goals),
            shots: after.shots.saturating_sub(before.shots),
            shots_on_target: after.shots_on_target.saturating_sub(before.shots_on_target),
            shots_off_target: after.shots_off_target.saturating_sub(before.shots_off_target),
            shots_blocked: after.shots_blocked.saturating_sub(before.shots_blocked),
            saves: after.saves.saturating_sub(before.saves),
            own_goals_for: after.own_goals_for.saturating_sub(before.own_goals_for),
            own_goals_conceded: after.own_goals_conceded.saturating_sub(before.own_goals_conceded),
            passes_attempted: after.passes_attempted.saturating_sub(before.passes_attempted),
            passes_completed: after.passes_completed.saturating_sub(before.passes_completed),
            xg_millis: after.xg_millis.saturating_sub(before.xg_millis),
            corners: after.corners.saturating_sub(before.corners),
            offsides: after.offsides.saturating_sub(before.offsides),
            throw_ins: after.throw_ins.saturating_sub(before.throw_ins),
        }
    }
    fn apply(&self, team: &mut crate::v4::state::TeamRuntimeStateV4) {
        team.goals = team.goals.saturating_add(self.goals);
        team.shots = team.shots.saturating_add(self.shots);
        team.shots_on_target = team.shots_on_target.saturating_add(self.shots_on_target);
        team.shots_off_target = team.shots_off_target.saturating_add(self.shots_off_target);
        team.shots_blocked = team.shots_blocked.saturating_add(self.shots_blocked);
        team.saves = team.saves.saturating_add(self.saves);
        team.own_goals_for = team.own_goals_for.saturating_add(self.own_goals_for);
        team.own_goals_conceded = team.own_goals_conceded.saturating_add(self.own_goals_conceded);
        team.passes_attempted = team.passes_attempted.saturating_add(self.passes_attempted);
        team.passes_completed = team.passes_completed.saturating_add(self.passes_completed);
        team.xg_millis = team.xg_millis.saturating_add(self.xg_millis);
        team.corners = team.corners.saturating_add(self.corners);
        team.offsides = team.offsides.saturating_add(self.offsides);
        team.throw_ins = team.throw_ins.saturating_add(self.throw_ins);
    }
}
impl FlightCounterDeltaV4 {
    pub fn between(before: &MatchStateV4, after: &MatchStateV4) -> Self {
        let players = before.players.iter().zip(after.players.iter()).filter_map(|(a,b)| {
            let goals = b.goals.saturating_sub(a.goals);
            let own_goals = b.own_goals.saturating_sub(a.own_goals);
            let assists = b.assists.saturating_sub(a.assists);
            let rating_delta = i16::from(b.rating_tenths) - i16::from(a.rating_tenths);
            if goals == 0 && own_goals == 0 && assists == 0 && rating_delta == 0 { return None; }
            Some(PlayerFlightDeltaV4 { player_id: a.player_id.clone(), goals, own_goals, assists, rating_delta })
        }).collect();
        Self { home: TeamFlightDeltaV4::between(&before.home, &after.home), away: TeamFlightDeltaV4::between(&before.away, &after.away), players }
    }
    pub fn apply(&self, state: &mut MatchStateV4) {
        self.home.apply(&mut state.home); self.away.apply(&mut state.away);
        for delta in &self.players {
            if let Some(player) = state.players.iter_mut().find(|p| p.player_id == delta.player_id) {
                player.goals = player.goals.saturating_add(delta.goals);
                player.own_goals = player.own_goals.saturating_add(delta.own_goals);
                player.assists = player.assists.saturating_add(delta.assists);
                player.rating_tenths = (i16::from(player.rating_tenths) + delta.rating_delta).clamp(0, 100) as u8;
            }
        }
    }
}

/// Complete deterministic session payload stored inside `MatchCheckpointV4`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDecisionV4 {
    pub player_id: String,
    pub side: TeamSideV4,
    pub started_at_ms: i64,
    pub executes_at_ms: i64,
    pub plan: crate::player_intelligence::DecisionPlanV4,
    /// Original planned release; the first event may be an earlier contest.
    pub release_at_ms: i64,
    #[serde(default)]
    pub observed_pass_risk: Option<f32>,
    /// Final aiming preparation is committed; do not repeatedly restart it.
    #[serde(default)]
    pub final_aim_committed: bool,
    #[serde(default)]
    pub challenger: Option<crate::v4::interaction::PassContactMotion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCarryV4 {
    pub carrier_idx: usize,
    pub resolves_at_ms: i64,
    pub motion: crate::v4::interaction::PassContactMotion,
    #[serde(default)]
    pub shielding: bool,
    #[serde(default)]
    pub is_hold: bool,
    #[serde(default)]
    pub challenger: Option<crate::v4::interaction::PassContactMotion>,
    #[serde(default)]
    pub start_pressure: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSessionSnapshotV4 {
    pub input: MatchInputV4,
    pub state: MatchStateV4,
    pub scheduler: EventScheduler,
    pub rng: RngSnapshotV4,
    pub half_stage: HalfStageV4,
    pub pending_substitutions: Vec<PendingSubstitutionItem>,
    pub pending_tactical_commands: Vec<PendingTacticalCommand>,
    pub applied_commands: Vec<AppliedCommandV4>,
    pub next_event_sequence: u32,
    pub stats: SessionStatsSnapshotV4,
    /// Full live diagnostics collector (preserves DES counters, dead-ball samples & FastBatch state).
    pub diagnostics_collector: MatchDiagnosticsCollectorV4,
    /// Summary snapshot for external inspection/backward compatibility.
    #[serde(default)]
    pub diagnostics_summary: MatchDiagnosticsV4,
    /// In-flight pass/shot awaiting `BallInFlightArrival`, `ShotArrival`, or `EarlyInteractionResolved`.
    #[serde(default)]
    pub pending_flight: Option<PendingFlightSnapshotV4>,
    #[serde(default)]
    pub pi_policy_id: String,
    #[serde(default)]
    pub pending_decision: Option<PendingDecisionV4>,
    #[serde(default)]
    pub pending_carry: Option<PendingCarryV4>,
    /// Physical follow-through/recovery after a committed tackle.
    #[serde(default)]
    pub tackle_ready_at: std::collections::BTreeMap<String,i64>,
}

/// Self-verifying versioned DES v1.4 match checkpoint envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCheckpointV4 {
    pub checkpoint_schema_version: u32,
    pub engine_version: String,
    pub protocol_version: u32,
    pub event_schema_version: u32,
    pub ruleset_identity: RulesetIdentity,
    pub match_id: String,
    pub seed: i64,
    pub input_fingerprint: String,
    pub sim_time_ms: i64,
    pub checksum_hex: String,
    pub payload: MatchSessionSnapshotV4,
}

/// Computes a deterministic 128-bit (32 hex chars) dual-lane FNV-1a checksum over the canonical
/// JSON serialization of `MatchCheckpointV4::payload` with zero external crate dependencies.
pub fn compute_checkpoint_checksum(payload_json: &str) -> String {
    const FNV_OFFSET_A: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME_A: u64 = 0x0000_0100_0000_01b3;
    const FNV_OFFSET_B: u64 = 0x8422_2325_cbf2_9ce4;
    const FNV_PRIME_B: u64 = 0x0000_0100_0000_01b3;

    let mut ha = FNV_OFFSET_A;
    let mut hb = FNV_OFFSET_B;
    for (idx, &b) in payload_json.as_bytes().iter().enumerate() {
        ha ^= b as u64;
        ha = ha.wrapping_mul(FNV_PRIME_A);

        hb ^= (b as u64).wrapping_add((idx as u64) & 0xff);
        hb = hb.wrapping_mul(FNV_PRIME_B).rotate_left(7);
    }
    format!("{ha:016x}{hb:016x}")
}

/// Validates checkpoint integrity, schema/protocol compatibility, ruleset identity, and checksum.
pub fn verify_checkpoint(ckpt: &MatchCheckpointV4) -> Result<(), V4Error> {
    if ckpt.checkpoint_schema_version != CHECKPOINT_SCHEMA_VERSION_V4 {
        return Err(V4Error::InvalidInput(format!(
            "unsupported checkpoint schema version: expected {}, got {}",
            CHECKPOINT_SCHEMA_VERSION_V4, ckpt.checkpoint_schema_version
        )));
    }
    if ckpt.protocol_version != ENGINE_PROTOCOL_VERSION_V4 {
        return Err(V4Error::InvalidProtocolVersion {
            expected: ENGINE_PROTOCOL_VERSION_V4 as i32,
            actual: ckpt.protocol_version as i32,
        });
    }
    if ckpt.event_schema_version != EVENT_SCHEMA_VERSION_V4 {
        return Err(V4Error::InvalidInput(format!(
            "unsupported checkpoint event schema version: expected {}, got {}",
            EVENT_SCHEMA_VERSION_V4, ckpt.event_schema_version
        )));
    }

    if ckpt.payload.pi_policy_id != crate::player_intelligence::PI_POLICY_ID {
        return Err(V4Error::InvalidInput(format!("checkpoint PI policy mismatch: expected {}, got {:?}",
            crate::player_intelligence::PI_POLICY_ID, ckpt.payload.pi_policy_id)));
    }
    let current_ruleset = ckpt.payload.input.effective_ruleset();
    check_ruleset_compatibility(&ckpt.ruleset_identity, &current_ruleset.identity)
        .map_err(V4Error::InvalidInput)?;

    if ckpt.match_id != ckpt.payload.input.match_id {
        return Err(V4Error::InvalidInput(format!(
            "checkpoint match_id mismatch: envelope='{}', payload='{}'",
            ckpt.match_id, ckpt.payload.input.match_id
        )));
    }
    if ckpt.seed != ckpt.payload.input.seed {
        return Err(V4Error::InvalidInput(format!(
            "checkpoint seed mismatch: envelope={}, payload={}",
            ckpt.seed, ckpt.payload.input.seed
        )));
    }

    let expected_input_fp = format!("{:016x}", ckpt.payload.input.input_fingerprint());
    if ckpt.input_fingerprint != expected_input_fp {
        return Err(V4Error::InvalidInput(format!(
            "checkpoint input_fingerprint mismatch: expected {}, got {}",
            expected_input_fp, ckpt.input_fingerprint
        )));
    }

    let payload_json = serde_json::to_string(&ckpt.payload)?;
    let actual_checksum = compute_checkpoint_checksum(&payload_json);
    if actual_checksum != ckpt.checksum_hex {
        return Err(V4Error::InvalidInput(format!(
            "checkpoint checksum mismatch: expected {}, computed {}",
            ckpt.checksum_hex, actual_checksum
        )));
    }

    Ok(())
}
