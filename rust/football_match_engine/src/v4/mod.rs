pub mod ai_manager;
pub mod attributes;
pub mod ball_play;
pub mod calibration;
pub mod error;
pub mod events;
pub mod goalkeeper;
pub mod input;
pub mod movement;
pub mod own_goal;
pub mod perception;
pub mod psychology;
pub mod session;
pub mod set_pieces;
pub mod shooting;
pub mod spatial;
pub mod stats;
pub mod state;
pub mod tactics;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod attribute_impact_tests;

pub const ENGINE_VERSION_V4: &str = "v4.0.0";
pub const JNI_BRIDGE_VERSION: i32 = 7;
pub const ENGINE_PROTOCOL_VERSION_V4: i32 = 4;
pub const EVENT_SCHEMA_VERSION_V4: i32 = 4;
pub const REGULATION_MINUTES_V4: i32 = 90;
pub const REGULATION_SECONDS_V4: i32 = 5400;
pub const HALF_SECONDS_V4: i32 = 2700;

pub use attributes::{
    normalize_attr, normalize_attr_elite, GoalkeepingAttributes, HiddenAttributes,
    MentalAttributes, PhysicalAttributes, PlayerAttributes54, TechnicalAttributes,
};
pub use calibration::{run_monte_carlo_v4, verify_seed_replay_parity, MonteCarloReportV4};
pub use error::V4Error;
pub use events::{
    EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4, MatchResultV4, MatchSummaryV4,
    PlayerMatchSummaryV4, SessionStepResultV4, TeamMatchSummaryV4, TeamRosterStateV4,
};
pub use goalkeeper::{GkAerialOutcome, GkShotOutcome};
pub use input::{
    LineupAssignmentV4, MatchEnvironment, MatchInputV4, PitchQuality, PlayerMatchSnapshotV4,
    PreferredFoot, RefereeProfile, RulesModeV4, SetPiecePlanV4, SimulationMode,
    SlotInstructionsV4, TacticalSlotV4, TeamCollectiveV4, TeamInstructionsV4, TeamMatchSnapshotV4,
    WeatherCondition,
};
pub use perception::{PassKindV4, PlannedActionV4};
pub use session::{simulate_batch_fast_v4, simulate_v4, replay_v4_with_timeline, AppliedCommandV4, MatchCommandV4, MatchSessionV4, SubstitutionChangeV4};
pub use shooting::ShotKindV4;
pub use spatial::{TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M, PITCH_WIDTH_M};
pub use state::{
    BallControlState, BallStateV4, InjuryKind, MatchStateV4, PlayerPsychologyState,
    PlayerRuntimeStateV4, TacticalPhaseV4, TeamRuntimeStateV4, TwoPoolFatigueState,
};
pub use tactics::TacticalMetadataFlags;
