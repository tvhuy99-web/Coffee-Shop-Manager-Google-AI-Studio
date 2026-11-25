use serde::{Deserialize, Deserializer, Serialize, Serializer};

use match_contracts::rule_types::Period;
use match_contracts::fingerprints;
use match_contracts::ports::RngSampleSource;
use match_contracts::attributes::normalize_attr;
use match_contracts::rule_types::{PitchQuality, RulesModeV4, WeatherCondition};
use match_contracts::team_instructions::TeamInstructionsV4;
use match_contracts::rule_types::PassKindV4;
use match_contracts::rule_types::ShotKindV4;
use match_contracts::spatial::{TeamSideV4, Vec3};

pub const DEFAULT_RULESET_ID_LEAGUE: &str = "ifab_league_standard";
pub const DEFAULT_RULESET_ID_KNOCKOUT: &str = "ifab_knockout_cup";
pub const DEFAULT_RULESET_VERSION: u32 = 1;
pub const DEFAULT_TIMING_MODEL_ID: &str = "des_timing_v1";
pub const DEFAULT_TIMING_MODEL_VERSION: u32 = 1;

pub const HALF_DURATION_MS_STANDARD: i64 = 2_700_000;
pub const EXTRA_TIME_HALF_DURATION_MS: i64 = 900_000;
pub const ADDED_TIME_ROUNDING_MS_STANDARD: i64 = 60_000;
pub const TECHNICAL_WATCHDOG_MAX_ADDED_MS: i64 = 1_800_000;
pub const MAX_EVENTS_PER_SAME_MS_DEFAULT: u32 = 64;

/// Immutable versioned identity for the active football ruleset and timing model.
/// Included in `input_fingerprint` and `MatchCheckpoint` to prevent silent drift on resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesetIdentity {
    pub ruleset_id: String,
    pub ruleset_version: u32,
    pub timing_model_id: String,
    pub timing_model_version: u32,
}

impl RulesetIdentity {
    pub fn league_standard() -> Self {
        Self {
            ruleset_id: DEFAULT_RULESET_ID_LEAGUE.to_string(),
            ruleset_version: DEFAULT_RULESET_VERSION,
            timing_model_id: DEFAULT_TIMING_MODEL_ID.to_string(),
            timing_model_version: DEFAULT_TIMING_MODEL_VERSION,
        }
    }

    pub fn knockout_cup() -> Self {
        Self {
            ruleset_id: DEFAULT_RULESET_ID_KNOCKOUT.to_string(),
            ruleset_version: DEFAULT_RULESET_VERSION,
            timing_model_id: DEFAULT_TIMING_MODEL_ID.to_string(),
            timing_model_version: DEFAULT_TIMING_MODEL_VERSION,
        }
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        h = fingerprints::mix_string(h, Some(&self.ruleset_id));
        h = fingerprints::mix_i32(h, self.ruleset_version as i32);
        h = fingerprints::mix_string(h, Some(&self.timing_model_id));
        h = fingerprints::mix_i32(h, self.timing_model_version as i32);
        h
    }
}

/// Verifies that a saved checkpoint or replay ruleset matches the active engine ruleset.
pub fn check_ruleset_compatibility(
    saved: &RulesetIdentity,
    current: &RulesetIdentity,
) -> Result<(), String> {
    if saved.ruleset_id != current.ruleset_id
        || saved.ruleset_version != current.ruleset_version
    {
        return Err(format!(
            "Incompatible ruleset: saved={}@v{}, current={}@v{}",
            saved.ruleset_id, saved.ruleset_version, current.ruleset_id, current.ruleset_version
        ));
    }
    if saved.timing_model_id != current.timing_model_id
        || saved.timing_model_version != current.timing_model_version
    {
        return Err(format!(
            "Incompatible timing model: saved={}@v{}, current={}@v{}",
            saved.timing_model_id,
            saved.timing_model_version,
            current.timing_model_id,
            current.timing_model_version
        ));
    }
    Ok(())
}

/// Governs how the referee ends a half once regulation + effective added time has elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HalfEndPolicy {
    /// Blow whistle immediately upon expiry (except an awarded penalty kick per IFAB Law 7).
    StrictAtExpiry,
    /// Allow an active dangerous attack or penalty kick to conclude before blowing the whistle.
    #[default]
    AllowAttackCompletion,
    /// End at expiry unless a penalty kick is pending or in progress.
    PenaltyOnlyExtension,
}

/// Versioned IFAB football rules configuration (strictly decoupled from `TimingModelConfig`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesetConfig {
    pub identity: RulesetIdentity,
    pub half_duration_ms: i64,
    pub extra_time_enabled: bool,
    pub extra_time_half_duration_ms: i64,
    pub penalty_shootout_enabled: bool,
    pub max_substitutions: u8,
    pub extra_time_bonus_substitution: u8,
    pub max_in_play_sub_windows: u8,
    pub extra_time_bonus_sub_window: u8,
    pub interval_subs_exempt_from_windows: bool,
    pub allow_quick_free_kick: bool,
    pub allow_penalty_rebound: bool,
    pub half_end_policy: HalfEndPolicy,
    pub added_time_rounding_ms: i64,
    /// Technical safety watchdog against infinite loops; NOT a gameplay stoppage cap.
    pub technical_watchdog_max_added_ms: i64,
    pub max_events_per_same_ms: u32,
}

impl Default for RulesetConfig {
    fn default() -> Self {
        Self::ifab_league_standard()
    }
}

impl RulesetConfig {
    /// Standard IFAB league ruleset (`ifab_league_standard` v1): 2x45', 5 subs in 3 in-play windows.
    pub fn ifab_league_standard() -> Self {
        Self {
            identity: RulesetIdentity::league_standard(),
            half_duration_ms: HALF_DURATION_MS_STANDARD,
            extra_time_enabled: false,
            extra_time_half_duration_ms: EXTRA_TIME_HALF_DURATION_MS,
            penalty_shootout_enabled: false,
            max_substitutions: 5,
            extra_time_bonus_substitution: 0,
            max_in_play_sub_windows: 3,
            extra_time_bonus_sub_window: 0,
            interval_subs_exempt_from_windows: true,
            allow_quick_free_kick: true,
            allow_penalty_rebound: true,
            half_end_policy: HalfEndPolicy::AllowAttackCompletion,
            added_time_rounding_ms: ADDED_TIME_ROUNDING_MS_STANDARD,
            technical_watchdog_max_added_ms: TECHNICAL_WATCHDOG_MAX_ADDED_MS,
            max_events_per_same_ms: MAX_EVENTS_PER_SAME_MS_DEFAULT,
        }
    }

    /// IFAB knockout cup ruleset (`ifab_knockout_cup` v1): adds 2x15' extra time (+1 sub/window) and shootout.
    pub fn ifab_knockout_cup() -> Self {
        Self {
            identity: RulesetIdentity::knockout_cup(),
            half_duration_ms: HALF_DURATION_MS_STANDARD,
            extra_time_enabled: true,
            extra_time_half_duration_ms: EXTRA_TIME_HALF_DURATION_MS,
            penalty_shootout_enabled: true,
            max_substitutions: 5,
            extra_time_bonus_substitution: 1,
            max_in_play_sub_windows: 3,
            extra_time_bonus_sub_window: 1,
            interval_subs_exempt_from_windows: true,
            allow_quick_free_kick: true,
            allow_penalty_rebound: true,
            half_end_policy: HalfEndPolicy::AllowAttackCompletion,
            added_time_rounding_ms: ADDED_TIME_ROUNDING_MS_STANDARD,
            technical_watchdog_max_added_ms: TECHNICAL_WATCHDOG_MAX_ADDED_MS,
            max_events_per_same_ms: MAX_EVENTS_PER_SAME_MS_DEFAULT,
        }
    }

    pub fn from_rules_mode(mode: RulesModeV4) -> Self {
        match mode {
            RulesModeV4::Standard => Self::ifab_league_standard(),
            RulesModeV4::Knockout => Self::ifab_knockout_cup(),
        }
    }

    pub fn max_substitutions_for_period(&self, period: Period) -> u8 {
        if matches!(period, Period::ExtraTimeFirst | Period::ExtraTimeSecond) {
            self.max_substitutions
                .saturating_add(self.extra_time_bonus_substitution)
        } else {
            self.max_substitutions
        }
    }

    pub fn max_in_play_sub_windows_for_period(&self, period: Period) -> u8 {
        if matches!(period, Period::ExtraTimeFirst | Period::ExtraTimeSecond) {
            self.max_in_play_sub_windows
                .saturating_add(self.extra_time_bonus_sub_window)
        } else {
            self.max_in_play_sub_windows
        }
    }

    /// Rounds accumulated compensable lost time upward to the ruleset announcement quantum (default 60s),
    /// bounded only by the technical watchdog rail.
    pub fn round_announced_added_time_ms(&self, compensable_lost_ms: i64) -> i64 {
        let lost = compensable_lost_ms.max(0);
        if lost == 0 {
            return 0;
        }
        let quantum = self.added_time_rounding_ms.max(1_000);
        let rounded = ((lost + quantum - 1) / quantum) * quantum;
        rounded.min(self.technical_watchdog_max_added_ms.max(quantum))
    }
}

/// Truncated triangular duration range in milliseconds (`min_ms <= mode_ms <= max_ms`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundedDurationRangeMs {
    pub min_ms: i64,
    pub mode_ms: i64,
    pub max_ms: i64,
}

impl BoundedDurationRangeMs {
    pub const fn new(min_ms: i64, mode_ms: i64, max_ms: i64) -> Self {
        Self {
            min_ms,
            mode_ms,
            max_ms,
        }
    }

    /// Samples a deterministic duration from a truncated triangular distribution in `[min_ms, max_ms]`.
    /// `context_scale` shifts the mode smoothly within the bounded interval without ever collapsing
    /// to a hardcoded constant when `min_ms < max_ms`.
    pub fn sample_triangular(&self, rng: &mut impl RngSampleSource, context_scale: f32) -> i64 {
        let min_bound = self.min_ms.min(self.max_ms);
        let max_bound = self.max_ms.max(self.min_ms);
        if min_bound == max_bound {
            return min_bound;
        }

        let lo = min_bound as f64;
        let hi = max_bound as f64;
        let scale = if context_scale.is_finite() {
            (context_scale as f64).clamp(0.50, 2.00)
        } else {
            1.0
        };
        let mode = (self.mode_ms as f64 * scale).clamp(lo, hi);
        let u = rng.next_double().clamp(0.0, 1.0);
        let span = hi - lo;
        let fc = ((mode - lo) / span).clamp(0.0, 1.0);

        let drawn = if u < fc && fc > 0.0 {
            lo + (u * span * (mode - lo)).max(0.0).sqrt()
        } else if fc < 1.0 {
            hi - ((1.0 - u) * span * (hi - mode)).max(0.0).sqrt()
        } else {
            lo + (u * span * span).max(0.0).sqrt()
        };

        (drawn.round() as i64).clamp(min_bound, max_bound)
    }
}

/// Separate timing model for ball flight, player reaction/decision, dead-ball phases, and officiating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingModelConfig {
    pub model_id: String,
    pub model_version: u32,
    // Ball kinematics bounds
    pub ball_flight_min_ms: i64,
    pub ball_flight_max_ms: i64,
    // Reaction & decision durations
    pub reaction_base_ms: BoundedDurationRangeMs,
    pub one_touch_reaction_ms: BoundedDurationRangeMs,
    #[serde(default = "default_first_touch_settle_ms")]
    pub first_touch_settle_ms: BoundedDurationRangeMs,
    // Dead-ball & restart durations
    pub goal_celebration_ms: BoundedDurationRangeMs,
    pub goal_return_to_half_ms: BoundedDurationRangeMs,
    pub kickoff_setup_ms: BoundedDurationRangeMs,
    pub throw_in_prep_ms: BoundedDurationRangeMs,
    pub corner_prep_ms: BoundedDurationRangeMs,
    pub goal_kick_prep_ms: BoundedDurationRangeMs,
    pub quick_free_kick_ms: BoundedDurationRangeMs,
    pub set_free_kick_prep_ms: BoundedDurationRangeMs,
    pub offside_restart_prep_ms: BoundedDurationRangeMs,
    pub penalty_prep_ms: BoundedDurationRangeMs,
    pub ball_retrieval_ms: BoundedDurationRangeMs,
    pub restart_execution_ms: BoundedDurationRangeMs,
    // Stoppages, substitutions, and officiating delays
    pub injury_assessment_ms: BoundedDurationRangeMs,
    pub substitution_overhead_per_batch_ms: BoundedDurationRangeMs,
    pub substitution_extra_per_player_ms: BoundedDurationRangeMs,
    pub referee_yellow_card_ms: BoundedDurationRangeMs,
    pub referee_red_card_ms: BoundedDurationRangeMs,
}

impl Default for TimingModelConfig {
    fn default() -> Self {
        Self::des_v1_default()
    }
}

impl TimingModelConfig {
    pub fn des_v1_default() -> Self {
        Self {
            model_id: DEFAULT_TIMING_MODEL_ID.to_string(),
            model_version: DEFAULT_TIMING_MODEL_VERSION,
            ball_flight_min_ms: 140,
            ball_flight_max_ms: 4_500,
            reaction_base_ms: BoundedDurationRangeMs::new(320, 680, 1_350),
            one_touch_reaction_ms: BoundedDurationRangeMs::new(150, 290, 520),
            first_touch_settle_ms: default_first_touch_settle_ms(),
            goal_celebration_ms: BoundedDurationRangeMs::new(14_000, 26_000, 45_000),
            goal_return_to_half_ms: BoundedDurationRangeMs::new(8_000, 14_000, 24_000),
            kickoff_setup_ms: BoundedDurationRangeMs::new(3_000, 6_000, 12_000),
            throw_in_prep_ms: BoundedDurationRangeMs::new(4_000, 8_500, 18_000),
            corner_prep_ms: BoundedDurationRangeMs::new(12_000, 20_000, 34_000),
            goal_kick_prep_ms: BoundedDurationRangeMs::new(8_000, 15_000, 28_000),
            quick_free_kick_ms: BoundedDurationRangeMs::new(1_800, 3_500, 7_000),
            set_free_kick_prep_ms: BoundedDurationRangeMs::new(14_000, 24_000, 42_000),
            offside_restart_prep_ms: BoundedDurationRangeMs::new(6_000, 11_000, 20_000),
            penalty_prep_ms: BoundedDurationRangeMs::new(25_000, 42_000, 70_000),
            ball_retrieval_ms: BoundedDurationRangeMs::new(2_500, 5_500, 11_000),
            restart_execution_ms: BoundedDurationRangeMs::new(500, 1_100, 2_200),
            injury_assessment_ms: BoundedDurationRangeMs::new(18_000, 38_000, 90_000),
            substitution_overhead_per_batch_ms: BoundedDurationRangeMs::new(12_000, 20_000, 32_000),
            substitution_extra_per_player_ms: BoundedDurationRangeMs::new(5_000, 9_000, 16_000),
            referee_yellow_card_ms: BoundedDurationRangeMs::new(8_000, 14_000, 24_000),
            referee_red_card_ms: BoundedDurationRangeMs::new(18_000, 30_000, 55_000),
        }
    }
}

fn default_first_touch_settle_ms() -> BoundedDurationRangeMs {
    BoundedDurationRangeMs::new(150, 300, 650)
}

/// Ball flight classification for kinematic travel time estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BallFlightKind {
    ShortPass,
    LongPass,
    SwitchPass,
    ThroughPass,
    Cross,
    DrivenShot,
    CurledShot,
    HeaderShot,
    OneTouchShot,
    LongRangeShot,
    ReboundShot,
    Clearance,
    SetPieceDelivery,
}

impl From<PassKindV4> for BallFlightKind {
    fn from(kind: PassKindV4) -> Self {
        match kind {
            PassKindV4::Short => BallFlightKind::ShortPass,
            PassKindV4::Long => BallFlightKind::LongPass,
            PassKindV4::Switch => BallFlightKind::SwitchPass,
            PassKindV4::Through => BallFlightKind::ThroughPass,
            PassKindV4::Cross => BallFlightKind::Cross,
        }
    }
}

impl From<ShotKindV4> for BallFlightKind {
    fn from(kind: ShotKindV4) -> Self {
        match kind {
            ShotKindV4::Driven => BallFlightKind::DrivenShot,
            ShotKindV4::Curled => BallFlightKind::CurledShot,
            ShotKindV4::Header => BallFlightKind::HeaderShot,
            ShotKindV4::OneTouch => BallFlightKind::OneTouchShot,
            ShotKindV4::LongRange => BallFlightKind::LongRangeShot,
            ShotKindV4::Rebound => BallFlightKind::ReboundShot,
        }
    }
}

/// Period lifecycle stage in DES v1.4, including explicit half-time and extra-time breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HalfStageV4 {
    PreMatch,
    InHalf {
        #[serde(with = "period_serde")]
        period: Period,
        #[serde(rename = "regulationDurationMs")]
        regulation_duration_ms: i64,
        #[serde(rename = "elapsedInHalfMs")]
        elapsed_in_half_ms: i64,
        #[serde(rename = "addedTimeAnnounced")]
        added_time_announced: bool,
        #[serde(rename = "announcedAddedMs")]
        announced_added_ms: i64,
        #[serde(rename = "additionalAddedMs")]
        additional_added_ms: i64,
    },
    HalfTimeBreak,
    ExtraTimeBreak {
        #[serde(rename = "nextPeriod", with = "period_serde")]
        next_period: Period,
    },
    PenaltyShootout,
    FullTime,
}

impl HalfStageV4 {
    pub fn is_break(&self) -> bool {
        matches!(self, HalfStageV4::HalfTimeBreak | HalfStageV4::ExtraTimeBreak { .. })
    }

    pub fn is_full_time(&self) -> bool {
        matches!(self, HalfStageV4::FullTime)
    }

    pub fn active_period(&self) -> Option<Period> {
        match self {
            HalfStageV4::InHalf { period, .. } => Some(*period),
            HalfStageV4::PenaltyShootout => Some(Period::Shootout),
            HalfStageV4::FullTime => Some(Period::FullTime),
            _ => None,
        }
    }
}

/// Top-level category of an active out-of-play / dead-ball stoppage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeadBallKind {
    Kickoff,
    GoalCelebration,
    ThrowIn,
    Corner,
    GoalKick,
    QuickFreeKick,
    DirectFreeKick,
    IndirectFreeKick,
    Offside,
    PenaltyKick,
    InjuryStoppage,
    SubstitutionWindow,
}

/// Fine-grained phase within a dead-ball stoppage lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeadBallPhase {
    Celebrating,
    ReturningToHalf,
    InjuryAssessment,
    DisciplineReview,
    ProcessingSubstitutions,
    RetrievingBall,
    SettingBall,
    KickoffSetup,
    ExecutingRestart,
}

/// Active dead-ball state machine record. While `dead_ball.is_some()`, live-play actions are forbidden.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadBallState {
    pub kind: DeadBallKind,
    pub phase: DeadBallPhase,
    pub restart_side: TeamSideV4,
    pub restart_spot: Vec3,
    pub started_at_ms: i64,
    pub phase_started_at_ms: i64,
    pub phase_ends_at_ms: i64,
    #[serde(default)]
    pub phase_compensable_ms: i64,
    #[serde(default)]
    pub phase_compensable_credited_ms: i64,
    pub accumulated_compensable_ms: i64,
    pub is_quick_restart: bool,
    pub involves_red_card: bool,
    pub injury_assessed: bool,
    pub discipline_reviewed: bool,
    pub subs_processed: bool,
}

impl DeadBallState {
    pub fn new(
        kind: DeadBallKind,
        initial_phase: DeadBallPhase,
        restart_side: TeamSideV4,
        restart_spot: Vec3,
        now_ms: i64,
        phase_duration_ms: i64,
        compensable_lost_ms: i64,
    ) -> Self {
        let dur = phase_duration_ms.max(1);
        let comp = compensable_lost_ms.clamp(0, dur);
        Self {
            kind,
            phase: initial_phase,
            restart_side,
            restart_spot,
            started_at_ms: now_ms,
            phase_started_at_ms: now_ms,
            phase_ends_at_ms: now_ms.saturating_add(dur),
            phase_compensable_ms: comp,
            phase_compensable_credited_ms: 0,
            accumulated_compensable_ms: comp,
            is_quick_restart: matches!(kind, DeadBallKind::QuickFreeKick),
            involves_red_card: false,
            injury_assessed: false,
            discipline_reviewed: false,
            subs_processed: false,
        }
    }

    /// Marks the current phase's side-effect flag before advancing to the next phase.
    pub fn advance_to(
        &mut self,
        next_phase: DeadBallPhase,
        now_ms: i64,
        phase_duration_ms: i64,
        compensable_lost_ms: i64,
    ) {
        match self.phase {
            DeadBallPhase::InjuryAssessment => self.injury_assessed = true,
            DeadBallPhase::DisciplineReview => self.discipline_reviewed = true,
            DeadBallPhase::ProcessingSubstitutions => self.subs_processed = true,
            _ => {}
        }
        let dur = phase_duration_ms.max(1);
        let comp = compensable_lost_ms.clamp(0, dur);
        self.phase = next_phase;
        self.phase_started_at_ms = now_ms;
        self.phase_ends_at_ms = now_ms.saturating_add(dur);
        self.phase_compensable_ms = comp;
        self.phase_compensable_credited_ms = 0;
        self.accumulated_compensable_ms = self.accumulated_compensable_ms.saturating_add(comp);
    }
}

/// Returns the valid entry phase when opening a new dead-ball stoppage.
pub fn initial_dead_ball_phase(
    kind: DeadBallKind,
    has_injury: bool,
    has_discipline_review: bool,
    has_pending_subs: bool,
) -> DeadBallPhase {
    match kind {
        DeadBallKind::GoalCelebration => DeadBallPhase::Celebrating,
        DeadBallKind::Kickoff => {
            if has_pending_subs {
                DeadBallPhase::ProcessingSubstitutions
            } else {
                DeadBallPhase::KickoffSetup
            }
        }
        DeadBallKind::InjuryStoppage => DeadBallPhase::InjuryAssessment,
        DeadBallKind::SubstitutionWindow => DeadBallPhase::ProcessingSubstitutions,
        DeadBallKind::QuickFreeKick => DeadBallPhase::SettingBall,
        DeadBallKind::ThrowIn | DeadBallKind::Corner | DeadBallKind::GoalKick => {
            if has_injury {
                DeadBallPhase::InjuryAssessment
            } else if has_discipline_review {
                DeadBallPhase::DisciplineReview
            } else {
                DeadBallPhase::RetrievingBall
            }
        }
        DeadBallKind::DirectFreeKick
        | DeadBallKind::IndirectFreeKick
        | DeadBallKind::Offside
        | DeadBallKind::PenaltyKick => {
            if has_injury {
                DeadBallPhase::InjuryAssessment
            } else if has_discipline_review {
                DeadBallPhase::DisciplineReview
            } else if has_pending_subs {
                DeadBallPhase::ProcessingSubstitutions
            } else {
                DeadBallPhase::SettingBall
            }
        }
    }
}

/// Computes the next valid phase of a dead-ball stoppage, or `None` when the restart executes
/// and play resumes live. Strictly prevents illegal shortcuts (e.g., jumping straight from
/// `GoalCelebration::Celebrating` to `KickoffSetup` or `ExecutingRestart`).
pub fn next_dead_ball_phase(
    state: &DeadBallState,
    has_pending_subs: bool,
    has_discipline_review: bool,
    has_injury: bool,
) -> Option<DeadBallPhase> {
    let need_injury = has_injury && !state.injury_assessed;
    let need_discipline = has_discipline_review && !state.discipline_reviewed;
    let need_subs = has_pending_subs && !state.subs_processed;

    match state.kind {
        DeadBallKind::GoalCelebration => match state.phase {
            DeadBallPhase::Celebrating => Some(DeadBallPhase::ReturningToHalf),
            DeadBallPhase::ReturningToHalf => {
                if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::KickoffSetup)
                }
            }
            DeadBallPhase::ProcessingSubstitutions => Some(DeadBallPhase::KickoffSetup),
            DeadBallPhase::KickoffSetup => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            // Defensive recovery for any unexpected intermediate phase
            DeadBallPhase::InjuryAssessment | DeadBallPhase::DisciplineReview => {
                Some(DeadBallPhase::ReturningToHalf)
            }
            DeadBallPhase::RetrievingBall | DeadBallPhase::SettingBall => {
                Some(DeadBallPhase::KickoffSetup)
            }
        },
        DeadBallKind::Kickoff => match state.phase {
            DeadBallPhase::ProcessingSubstitutions => Some(DeadBallPhase::KickoffSetup),
            DeadBallPhase::KickoffSetup => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            _ => Some(DeadBallPhase::KickoffSetup),
        },
        DeadBallKind::QuickFreeKick => match state.phase {
            DeadBallPhase::SettingBall => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            _ => Some(DeadBallPhase::SettingBall),
        },
        DeadBallKind::SubstitutionWindow => match state.phase {
            DeadBallPhase::ProcessingSubstitutions => Some(DeadBallPhase::SettingBall),
            DeadBallPhase::SettingBall => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            _ => Some(DeadBallPhase::ProcessingSubstitutions),
        },
        DeadBallKind::InjuryStoppage => match state.phase {
            DeadBallPhase::InjuryAssessment => {
                if need_discipline {
                    Some(DeadBallPhase::DisciplineReview)
                } else if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::SettingBall)
                }
            }
            DeadBallPhase::DisciplineReview => {
                if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::SettingBall)
                }
            }
            DeadBallPhase::ProcessingSubstitutions => Some(DeadBallPhase::SettingBall),
            DeadBallPhase::SettingBall => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            _ => Some(DeadBallPhase::SettingBall),
        },
        DeadBallKind::ThrowIn
        | DeadBallKind::Corner
        | DeadBallKind::GoalKick
        | DeadBallKind::DirectFreeKick
        | DeadBallKind::IndirectFreeKick
        | DeadBallKind::Offside
        | DeadBallKind::PenaltyKick => match state.phase {
            DeadBallPhase::InjuryAssessment => {
                if need_discipline {
                    Some(DeadBallPhase::DisciplineReview)
                } else if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::SettingBall)
                }
            }
            DeadBallPhase::DisciplineReview => {
                if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::SettingBall)
                }
            }
            DeadBallPhase::RetrievingBall => {
                if need_injury {
                    Some(DeadBallPhase::InjuryAssessment)
                } else if need_discipline {
                    Some(DeadBallPhase::DisciplineReview)
                } else if need_subs {
                    Some(DeadBallPhase::ProcessingSubstitutions)
                } else {
                    Some(DeadBallPhase::SettingBall)
                }
            }
            DeadBallPhase::ProcessingSubstitutions => Some(DeadBallPhase::SettingBall),
            DeadBallPhase::SettingBall => Some(DeadBallPhase::ExecutingRestart),
            DeadBallPhase::ExecutingRestart => None,
            _ => Some(DeadBallPhase::SettingBall),
        },
    }
}

/// Estimates `(phase_duration_ms, compensable_lost_ms)` for a specific dead-ball kind & phase.
///
/// Explicitly separates routine out-of-play time from IFAB Law 7 compensable lost time:
/// - Goal celebrations, substitutions, injury assessments, and card reviews are fully compensable.
/// - Standard restarts (`ThrowIn`, `Corner`, `GoalKick`, `FreeKick`, `KickoffSetup`) are routine
///   out-of-play time except for excessive time-wasting delays, which are counted as compensable.
/// - Penalty kick setup counts the non-routine preparation overhead as compensable.
#[allow(clippy::too_many_arguments)]
pub fn estimate_dead_ball_phase_ms(
    config: &TimingModelConfig,
    kind: DeadBallKind,
    phase: DeadBallPhase,
    goal_diff_for_restart_side: i32,
    sim_time_ms: i64,
    time_wasting: u8,
    ref_strictness: u8,
    pending_sub_count: usize,
    rng: &mut impl RngSampleSource,
) -> (i64, i64) {
    let late_game_factor = ((sim_time_ms as f32 - 3_600_000.0) / 1_800_000.0).clamp(0.0, 1.0);
    let tw_norm = (time_wasting as f32 / 100.0).clamp(0.0, 1.0);
    let ref_norm = (ref_strictness as f32 / 100.0).clamp(0.0, 1.0);

    // Context scale for restarts: leading teams with high time_wasting stretch prep;
    // trailing teams late in the match hurry restarts.
    let restart_urgency_scale = if goal_diff_for_restart_side > 0 {
        let waste_push = tw_norm * (0.20 + 0.35 * late_game_factor) * (1.0 - ref_norm * 0.20);
        (1.0 + waste_push).clamp(0.95, 1.50)
    } else if goal_diff_for_restart_side < 0 {
        let hurry_pull = (0.14 + 0.18 * late_game_factor) * (1.0 - tw_norm * 0.35);
        (1.0 - hurry_pull).clamp(0.68, 1.0)
    } else {
        let mild_waste = (tw_norm - 0.5).max(0.0) * 0.25 * late_game_factor;
        (1.0 + mild_waste).clamp(0.90, 1.20)
    };

    match phase {
        DeadBallPhase::Celebrating => {
            let celebration_scale = if goal_diff_for_restart_side == 0 {
                // Conceding side restarts; an equalizer or late winner triggers longer celebration.
                1.0 + 0.18 * late_game_factor
            } else {
                1.0 + 0.08 * late_game_factor
            };
            let dur = config
                .goal_celebration_ms
                .sample_triangular(rng, celebration_scale);
            (dur, dur)
        }
        DeadBallPhase::ReturningToHalf => {
            let dur = config
                .goal_return_to_half_ms
                .sample_triangular(rng, 1.0 + 0.10 * late_game_factor);
            // IFAB Law 7 compensates goal celebration + return to half overhead.
            (dur, dur)
        }
        DeadBallPhase::InjuryAssessment => {
            let dur = config.injury_assessment_ms.sample_triangular(rng, 1.0);
            (dur, dur)
        }
        DeadBallPhase::DisciplineReview => {
            let is_severe = matches!(kind, DeadBallKind::PenaltyKick | DeadBallKind::InjuryStoppage);
            let range = if is_severe {
                config.referee_red_card_ms
            } else {
                config.referee_yellow_card_ms
            };
            let dur = range.sample_triangular(rng, 0.90 + ref_norm * 0.25);
            (dur, dur)
        }
        DeadBallPhase::ProcessingSubstitutions => {
            let batch_overhead = config
                .substitution_overhead_per_batch_ms
                .sample_triangular(rng, 1.0 + tw_norm * 0.18 * late_game_factor);
            let extra_players = pending_sub_count.saturating_sub(1);
            let mut extra_ms = 0i64;
            for _ in 0..extra_players {
                extra_ms = extra_ms.saturating_add(
                    config
                        .substitution_extra_per_player_ms
                        .sample_triangular(rng, 1.0 + tw_norm * 0.15 * late_game_factor),
                );
            }
            let total = batch_overhead.saturating_add(extra_ms);
            (total, total)
        }
        DeadBallPhase::RetrievingBall => {
            let dur = config
                .ball_retrieval_ms
                .sample_triangular(rng, restart_urgency_scale);
            let routine_baseline = config.ball_retrieval_ms.mode_ms;
            let wasted_excess = if goal_diff_for_restart_side > 0 && time_wasting >= 55 {
                ((dur - routine_baseline).max(0) as f32 * (0.55 + ref_norm * 0.45)).round() as i64
            } else {
                0
            };
            (dur, wasted_excess.min(dur))
        }
        DeadBallPhase::KickoffSetup => {
            let dur = config
                .kickoff_setup_ms
                .sample_triangular(rng, restart_urgency_scale);
            let routine_baseline = config.kickoff_setup_ms.mode_ms;
            let wasted_excess = if goal_diff_for_restart_side > 0 && time_wasting >= 60 {
                ((dur - routine_baseline).max(0) as f32 * (0.50 + ref_norm * 0.50)).round() as i64
            } else {
                0
            };
            (dur, wasted_excess.min(dur))
        }
        DeadBallPhase::SettingBall => {
            let (range, base_compensable_ratio) = match kind {
                DeadBallKind::ThrowIn => (config.throw_in_prep_ms, 0.0f32),
                DeadBallKind::Corner => (config.corner_prep_ms, 0.0f32),
                DeadBallKind::GoalKick => (config.goal_kick_prep_ms, 0.0f32),
                DeadBallKind::QuickFreeKick => (config.quick_free_kick_ms, 0.0f32),
                DeadBallKind::DirectFreeKick | DeadBallKind::IndirectFreeKick => {
                    (config.set_free_kick_prep_ms, 0.15f32)
                }
                DeadBallKind::Offside => (config.offside_restart_prep_ms, 0.0f32),
                DeadBallKind::PenaltyKick => (config.penalty_prep_ms, 0.75f32),
                DeadBallKind::InjuryStoppage | DeadBallKind::SubstitutionWindow => {
                    (config.quick_free_kick_ms, 0.0f32)
                }
                DeadBallKind::Kickoff | DeadBallKind::GoalCelebration => {
                    (config.kickoff_setup_ms, 0.0f32)
                }
            };
            let scale = if matches!(kind, DeadBallKind::QuickFreeKick) {
                0.95
            } else {
                restart_urgency_scale
            };
            let dur = range.sample_triangular(rng, scale);
            let base_comp = (dur as f32 * base_compensable_ratio).round() as i64;
            let wasted_excess = if goal_diff_for_restart_side > 0 && time_wasting >= 55 {
                let over_mode = (dur - range.mode_ms).max(0);
                (over_mode as f32 * (0.60 + ref_norm * 0.40)).round() as i64
            } else {
                0
            };
            let compensable = (base_comp.saturating_add(wasted_excess)).clamp(0, dur);
            (dur, compensable)
        }
        DeadBallPhase::ExecutingRestart => {
            let dur = config.restart_execution_ms.sample_triangular(rng, 1.0);
            (dur, 0)
        }
    }
}

/// Estimates kinematic ball travel duration in milliseconds based on distance, kick type,
/// pitch quality, weather, and controlled stochastic variance.
fn nominal_ball_flight_and_spread(
    distance_m: f32,
    kind: BallFlightKind,
    weather: WeatherCondition,
    pitch: PitchQuality,
) -> (i64,f64) {
    let dist = if distance_m.is_finite() {
        distance_m.clamp(0.5, 115.0)
    } else {
        12.0
    };

    let (base_speed_mps, arc_factor, is_aerial) = match kind {
        BallFlightKind::ShortPass => (17.5f32, 1.00f32, false),
        BallFlightKind::ThroughPass => (19.0f32, 1.02f32, false),
        BallFlightKind::LongPass => (22.0f32, 1.18f32, true),
        BallFlightKind::SwitchPass => (23.5f32, 1.22f32, true),
        BallFlightKind::Cross => (21.0f32, 1.14f32, true),
        BallFlightKind::DrivenShot => (29.5f32, 1.00f32, false),
        BallFlightKind::CurledShot => (25.0f32, 1.06f32, false),
        BallFlightKind::HeaderShot => (18.5f32, 1.04f32, true),
        BallFlightKind::OneTouchShot => (26.5f32, 1.00f32, false),
        BallFlightKind::LongRangeShot => (28.5f32, 1.05f32, false),
        BallFlightKind::ReboundShot => (24.5f32, 1.00f32, false),
        BallFlightKind::Clearance => (23.5f32, 1.26f32, true),
        BallFlightKind::SetPieceDelivery => (21.5f32, 1.18f32, true),
    };

    let surface_speed_mult = if is_aerial {
        match weather {
            WeatherCondition::Windy => 0.94f32,
            WeatherCondition::HeavyRain => 0.96f32,
            _ => 1.0f32,
        }
    } else {
        let pitch_mult = match pitch {
            PitchQuality::Excellent => 1.05f32,
            PitchQuality::Good => 1.00f32,
            PitchQuality::Heavy => 0.91f32,
            PitchQuality::Poor => 0.86f32,
        };
        let weather_mult = match weather {
            WeatherCondition::LightRain => 1.04f32,
            WeatherCondition::HeavyRain => 0.89f32,
            WeatherCondition::Windy => 0.97f32,
            WeatherCondition::Clear | WeatherCondition::Hot => 1.00f32,
        };
        pitch_mult * weather_mult
    };

    let effective_speed = (base_speed_mps * surface_speed_mult).max(8.0);
    let nominal_ms = ((dist * arc_factor / effective_speed) * 1000.0).round() as i64;
    let spread = if matches!(weather, WeatherCondition::Windy) && is_aerial {
        0.14f64
    } else {
        0.08f64
    };
    (nominal_ms,spread)
}

/// Nominal duration for a player's forecast; consumes no RNG.
pub fn nominal_ball_flight_ms(
    config:&TimingModelConfig,distance_m:f32,kind:BallFlightKind,
    weather:WeatherCondition,pitch:PitchQuality,
)->i64 {
    nominal_ball_flight_and_spread(distance_m,kind,weather,pitch).0
        .max(1).clamp(config.ball_flight_min_ms,config.ball_flight_max_ms)
}

/// Actual ball travel duration, using the original distribution.
pub fn estimate_ball_flight_ms(
    config:&TimingModelConfig,distance_m:f32,kind:BallFlightKind,
    weather:WeatherCondition,pitch:PitchQuality,rng:&mut impl RngSampleSource,
)->i64 {
    let (nominal_ms,spread)=nominal_ball_flight_and_spread(distance_m,kind,weather,pitch);
    let lo = ((nominal_ms as f64) * (1.0 - spread)).round() as i64;
    let hi = ((nominal_ms as f64) * (1.0 + spread)).round() as i64;
    let range = BoundedDurationRangeMs::new(lo.max(1), nominal_ms.max(1), hi.max(lo.max(1)));
    range
        .sample_triangular(rng, 1.0)
        .clamp(config.ball_flight_min_ms, config.ball_flight_max_ms)
}

/// Estimates player reaction + decision duration in milliseconds before executing the next action.
#[allow(clippy::too_many_arguments)]
pub fn estimate_reaction_decision_ms(
    config: &TimingModelConfig,
    first_touch: u8,
    vision: u8,
    decisions: u8,
    composure: u8,
    pressure: f32,
    tempo: u8,
    one_touch: bool,
    rng: &mut impl RngSampleSource,
) -> i64 {
    let ft = normalize_attr(first_touch);
    let vis = normalize_attr(vision);
    let dec = normalize_attr(decisions);
    let comp = normalize_attr(composure);
    let pressure_norm = if pressure.is_finite() {
        (pressure / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let tempo_norm = (tempo as f32 / 100.0).clamp(0.0, 1.0);

    // Higher mental/technical skill and higher team tempo shorten decision time;
    // heavy opponent pressure adds hesitation unless offset by composure.
    let skill_score = ft * 0.25 + vis * 0.25 + dec * 0.35 + comp * 0.15;
    let skill_factor = 1.24 - skill_score * 0.48;
    let tempo_factor = 1.14 - tempo_norm * 0.28;
    let pressure_hesitation = 1.0 + pressure_norm * (0.26 - comp * 0.18).max(0.02);
    let context_scale = (skill_factor * tempo_factor * pressure_hesitation).clamp(0.55, 1.65);

    let range = if one_touch {
        config.one_touch_reaction_ms
    } else {
        config.reaction_base_ms
    };
    range.sample_triangular(rng, context_scale)
}

/// Lifecycle state of an interactive or AI command in the DES timeline.
pub use match_contracts::rule_types::CommandLifecycleStatus;

/// Source that issued a match command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandOrigin {
    User,
    AiManager,
    System,
}

/// Queued substitution waiting for the next eligible dead-ball stoppage or interval break.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSubstitutionItem {
    pub command_id: u64,
    pub side: TeamSideV4,
    pub origin: CommandOrigin,
    pub player_out_id: String,
    pub player_in_id: String,
    pub accepted_sim_time_ms: i64,
    pub status: CommandLifecycleStatus,
}

/// Queued tactical instruction change waiting for its natural propagation delay on the pitch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTacticalCommand {
    pub command_id: u64,
    pub side: TeamSideV4,
    pub origin: CommandOrigin,
    pub instructions: TeamInstructionsV4,
    pub accepted_sim_time_ms: i64,
    pub scheduled_effective_sim_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_effective_sim_time_ms: Option<i64>,
    pub status: CommandLifecycleStatus,
}

mod period_serde {
    use super::*;

    pub fn serialize<S>(period: &Period, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let name = match period {
            Period::NotStarted => "NOT_STARTED",
            Period::FirstHalf => "FIRST_HALF",
            Period::SecondHalf => "SECOND_HALF",
            Period::ExtraTimeFirst => "EXTRA_TIME_FIRST",
            Period::ExtraTimeSecond => "EXTRA_TIME_SECOND",
            Period::Shootout => "SHOOTOUT",
            Period::FullTime => "FULL_TIME",
        };
        serializer.serialize_str(name)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Period, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.as_str() {
            "NOT_STARTED" => Ok(Period::NotStarted),
            "FIRST_HALF" => Ok(Period::FirstHalf),
            "SECOND_HALF" => Ok(Period::SecondHalf),
            "EXTRA_TIME_FIRST" => Ok(Period::ExtraTimeFirst),
            "EXTRA_TIME_SECOND" => Ok(Period::ExtraTimeSecond),
            "SHOOTOUT" => Ok(Period::Shootout),
            "FULL_TIME" => Ok(Period::FullTime),
            other => Err(serde::de::Error::custom(format!(
                "unknown Period variant '{other}'"
            ))),
        }
    }
}

