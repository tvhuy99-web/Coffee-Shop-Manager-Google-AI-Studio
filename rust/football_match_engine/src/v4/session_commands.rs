//! Interactive command DTOs and command journal records.

use serde::{Deserialize, Serialize};

use crate::v4::input::TeamInstructionsV4;
use crate::v4::rules::CommandLifecycleStatus;
use crate::v4::spatial::TeamSideV4;

/// One player exchange within a grouped, simultaneous substitution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionChangeV4 {
    pub player_out_id: String,
    pub player_in_id: String,
}

/// Live interactive commands submitted by the player during a V4 DES match session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MatchCommandV4 {
    UpdateTactics {
        side: TeamSideV4,
        instructions: TeamInstructionsV4,
    },
    Substitute {
        side: TeamSideV4,
        #[serde(rename = "playerOutId")]
        player_out_id: String,
        #[serde(rename = "playerInId")]
        player_in_id: String,
    },
    SubstituteBatch {
        side: TeamSideV4,
        changes: Vec<SubstitutionChangeV4>,
    },
    StartNextPeriod,
}

/// Command journal entry recording exact DES acceptance, scheduled effectiveness,
/// and actual effectiveness timestamps for deterministic replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedCommandV4 {
    #[serde(default)]
    pub command_id: u64,
    #[serde(default)]
    pub accepted_sim_time_ms: i64,
    #[serde(default)]
    pub scheduled_effective_sim_time_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_effective_sim_time_ms: Option<i64>,
    #[serde(default)]
    pub applied_step_index: u64,
    #[serde(default = "default_applied_status")]
    pub status: CommandLifecycleStatus,
    /// Legacy compatibility alias mirrored from `applied_step_index`.
    #[serde(default)]
    pub completed_beats: u64,
    /// Legacy compatibility alias mirrored from `accepted_sim_time_ms / 1000`.
    #[serde(default)]
    pub game_second: i32,
    pub command: MatchCommandV4,
}

fn default_applied_status() -> CommandLifecycleStatus {
    CommandLifecycleStatus::Accepted
}

