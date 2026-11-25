pub mod ai_manager;
pub mod attributes;
pub mod ball_play;
pub mod calibration;
pub mod checkpoint;
pub mod diagnostics;
pub mod error;
pub mod events;
pub mod goalkeeper;
pub mod input;
pub mod interaction;
pub mod movement;
pub mod coach_adapter;
pub mod own_goal;
pub mod perception;
pub mod psychology;
pub mod rules;
pub mod scheduler;
pub mod session;
pub mod session_commands;
pub mod session_replay;
pub mod set_pieces;
pub mod shooting;
pub mod spatial;
pub mod state;
pub mod stats;
pub mod substitution;
pub mod tactics;
pub mod transition;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod attribute_impact_tests;

pub const ENGINE_VERSION_V4: &str = "v4.1.1-des";
pub const JNI_BRIDGE_VERSION: i32 = 8;
pub const ENGINE_PROTOCOL_VERSION_V4: u32 = 5;
pub const EVENT_SCHEMA_VERSION_V4: u32 = 5;
pub const REGULATION_MINUTES_V4: i32 = 90;
pub const REGULATION_SECONDS_V4: i32 = 5400;
pub const HALF_SECONDS_V4: i32 = 2700;
pub const HALF_MS_V4: i64 = 2_700_000;
pub const EXTRA_TIME_HALF_MS_V4: i64 = 900_000;

pub use attributes::{
    normalize_attr, normalize_attr_elite, GoalkeepingAttributes, HiddenAttributes,
    MentalAttributes, PhysicalAttributes, PlayerAttributes54, TechnicalAttributes,
};
pub use calibration::{run_monte_carlo_v4, verify_seed_replay_parity, MonteCarloReportV4};
pub use checkpoint::{
    compute_checkpoint_checksum, verify_checkpoint, MatchCheckpointV4, MatchSessionSnapshotV4,
    RngSnapshotV4, SessionStatsSnapshotV4, CHECKPOINT_SCHEMA_VERSION_V4,
};
pub use diagnostics::{
    DeadBallDiagKind, DeadBallDistributionsV4, DesMatchDiagnosticsV4, DurationDistributionMs,
    MatchDiagnosticsV4, PassBreakdownV4, TeamAttackDiagnosticsV4, TurnoverBreakdownV4,
    DIAGNOSTICS_SCHEMA_VERSION_V4,
};
pub use error::V4Error;
pub use events::{
    EventMetadataV4, EventTypeV4, HighlightTier, MatchEventV4, MatchResultV4, MatchSummaryV4,
    PendingCommandSummaryV4, PlayerMatchSummaryV4, SessionStageV4, SessionStepResultV4,
    TeamMatchSummaryV4, TeamRosterStateV4,
};
pub use goalkeeper::{GkAerialOutcome, GkShotOutcome};
pub use input::{
    LineupAssignmentV4, MatchEnvironment, MatchInputV4, PitchQuality, PlayerMatchSnapshotV4,
    PreferredFoot, RefereeProfile, RulesModeV4, SetPiecePlanV4, SimulationMode,
    SlotInstructionsV4, TacticalSlotV4, TeamCollectiveV4, TeamInstructionsV4, TeamMatchSnapshotV4,
    WeatherCondition,
};
pub use interaction::{
    find_earliest_ball_interaction, BallTrajectory, BoundaryKind, DefenderCandidate,
    EarlyInteractionHit, InteractionKind,
};
pub use crate::player_intelligence::{PassKindV4, PlannedActionV4};
pub use rules::{
    check_ruleset_compatibility, estimate_ball_flight_ms, estimate_dead_ball_phase_ms,
    estimate_reaction_decision_ms, initial_dead_ball_phase, next_dead_ball_phase, BallFlightKind,
    BoundedDurationRangeMs, CommandLifecycleStatus, CommandOrigin, DeadBallKind, DeadBallPhase,
    DeadBallState, HalfEndPolicy, HalfStageV4, PendingSubstitutionItem, PendingTacticalCommand,
    RulesetConfig, RulesetIdentity, TimingModelConfig,
};
pub use scheduler::{
    CancellationScope, DesEventPayload, EventPriority, EventScheduler, ScheduledEvent,
};
pub use session::{
    replay_v4_with_timeline, simulate_batch_fast_v4, simulate_v4, AppliedCommandV4, MatchCommandV4,
    MatchSessionV4, SubstitutionChangeV4,
};
pub use shooting::ShotKindV4;
pub use spatial::{TeamSideV4, Vec2, Vec3, PITCH_LENGTH_M, PITCH_WIDTH_M};
pub use state::{
    BallControlState, BallStateV4, InjuryKind, MatchStateV4, PlayerPsychologyState,
    PlayerRuntimeStateV4, PlayerStaticProfile, SlotAnchorPair, TacticalPhaseV4,
    TeamRuntimeStateV4, TwoPoolFatigueState,
};
pub use tactics::TacticalMetadataFlags;
