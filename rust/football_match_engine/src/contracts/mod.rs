//! Phase A2 internal `contracts` boundary (future `match_contracts` leaf crate).
//!
//! Exposes canonical units, typed identifiers, shared V4 DTOs, narrow trait ports,
//! and `VersionManifest4` without containing simulation formulas or authoritative world state.

pub mod attributes;
pub mod error;
pub mod input;
pub mod events;
pub mod spatial;
pub mod ports;
pub mod units;
pub mod version;

pub use ports::{
    BallFlightKindContract, LocalContextQuery, LocalContextView, PassReachForecast, PassReachQuery,
    RngSampleSource, SpatialForecastPort, TackleGeometryForecast, TackleGeometryQuery,
};
pub use units::{
    is_finite_f32, is_finite_vec2, is_finite_vec3, ms_to_seconds_f32, ms_to_seconds_f64,
    CommandId, DurationMs, EventId, MatchId, PlayerId, SimTimeMs, TeamId, TeamSideV4, Vec2, Vec3,
    ATTRIBUTE_MAX, ATTRIBUTE_MIN, EXTRA_TIME_HALF_MS, PITCH_LENGTH_M, PITCH_WIDTH_M,
    REGULATION_HALF_MS,
};
pub use version::{
    BuildIdentity, DataSchemas, GameplayIdentity, MatchConfigIdentity, PresentationIdentity,
    VersionManifest4, CANONICAL_FINGERPRINT_VERSION, NUMERIC_POLICY_ID, RNG_ALGORITHM_ID,
    VERSION_MANIFEST_SCHEMA,
};

// Re-export canonical V4 schema DTOs so downstream modules can migrate imports
// to `crate::contracts::*` before Phase A5 splits `match_contracts` into its own crate.
pub use crate::clock::Period;
pub use crate::contracts::attributes::{
    GoalkeepingAttributes, HiddenAttributes, MentalAttributes, PhysicalAttributes,
    PlayerAttributes54, TechnicalAttributes,
};
pub use crate::contracts::error::V4Error;
pub use crate::contracts::events::{
    EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4, MatchResultV4, MatchSummaryV4,
    PendingCommandSummaryV4, PlayerMatchSummaryV4, SessionStageV4, SessionStepResultV4,
    TeamMatchSummaryV4, TeamRosterStateV4,
};
pub use crate::contracts::input::{
    LineupAssignmentV4, MatchEnvironment, MatchInputV4, PitchQuality, PlayerMatchSnapshotV4,
    PreferredFoot, RefereeProfile, RulesModeV4, SetPiecePlanV4, SimulationMode,
    SlotInstructionsV4, TacticalSlotV4, TeamCollectiveV4, TeamInstructionsV4, TeamMatchSnapshotV4,
    WeatherCondition,
};
