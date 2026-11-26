use crate::{
    clock::{ClockState, Period},
    fingerprints as fp,
    geometry::TeamSide,
    input::{MatchInput, TacticalSlotSnapshot},
    state::{BallControl, MatchState, RestartType, TacticalPhase},
};

pub const EVENT_SCHEMA_VERSION: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum EventType {
    MatchStarted,
    Kickoff,
    PossessionStarted,
    PressureApplied,
    TacticalPhaseChanged,
    TransitionStarted,
    TransitionEnded,
    PassStarted,
    PassCompleted,
    PassFailed,
    ThroughBall,
    Carry,
    DribbleStarted,
    DribbleWon,
    DribbleLost,
    Cross,
    FirstTouch,
    Press,
    TackleAttempted,
    TackleWon,
    TackleLost,
    Interception,
    Block,
    Clearance,
    GroundDuel,
    AerialDuel,
    LooseBall,
    SecondBallWon,
    Rebound,
    Turnover,
    Shot,
    Header,
    ShotOffTarget,
    ShotOnTarget,
    Goal,
    OwnGoal,
    Save,
    GoalkeeperClaim,
    GoalkeeperPunch,
    GoalkeeperRush,
    GoalkeeperDistribution,
    Foul,
    AdvantagePlayed,
    YellowCard,
    RedCard,
    Offside,
    Injury,
    ThrowIn,
    GoalKick,
    Corner,
    FreeKick,
    PenaltyAwarded,
    PenaltyKick,
    RestartScheduled,
    Substitution,
    GoalkeeperAssignmentRequired,
    GoalkeeperReassigned,
    CommandAccepted,
    CommandRejected,
    AddedTimeAnnounced,
    HalfTime,
    ExtraTimeStarted,
    ExtraTimeHalfTime,
    PenaltyShootoutStarted,
    PenaltyShootoutKick,
    PenaltyShootoutEnded,
    FullTime,
}

impl EventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MatchStarted => "MATCH_STARTED",
            Self::Kickoff => "KICKOFF",
            Self::PossessionStarted => "POSSESSION_STARTED",
            Self::PressureApplied => "PRESSURE_APPLIED",
            Self::TacticalPhaseChanged => "TACTICAL_PHASE_CHANGED",
            Self::TransitionStarted => "TRANSITION_STARTED",
            Self::TransitionEnded => "TRANSITION_ENDED",
            Self::PassStarted => "PASS_STARTED",
            Self::PassCompleted => "PASS_COMPLETED",
            Self::PassFailed => "PASS_FAILED",
            Self::ThroughBall => "THROUGH_BALL",
            Self::Carry => "CARRY",
            Self::DribbleStarted => "DRIBBLE_STARTED",
            Self::DribbleWon => "DRIBBLE_WON",
            Self::DribbleLost => "DRIBBLE_LOST",
            Self::Cross => "CROSS",
            Self::FirstTouch => "FIRST_TOUCH",
            Self::Press => "PRESS",
            Self::TackleAttempted => "TACKLE_ATTEMPTED",
            Self::TackleWon => "TACKLE_WON",
            Self::TackleLost => "TACKLE_LOST",
            Self::Interception => "INTERCEPTION",
            Self::Block => "BLOCK",
            Self::Clearance => "CLEARANCE",
            Self::GroundDuel => "GROUND_DUEL",
            Self::AerialDuel => "AERIAL_DUEL",
            Self::LooseBall => "LOOSE_BALL",
            Self::SecondBallWon => "SECOND_BALL_WON",
            Self::Rebound => "REBOUND",
            Self::Turnover => "TURNOVER",
            Self::Shot => "SHOT",
            Self::Header => "HEADER",
            Self::ShotOffTarget => "SHOT_OFF_TARGET",
            Self::ShotOnTarget => "SHOT_ON_TARGET",
            Self::Goal => "GOAL",
            Self::OwnGoal => "OWN_GOAL",
            Self::Save => "SAVE",
            Self::GoalkeeperClaim => "GOALKEEPER_CLAIM",
            Self::GoalkeeperPunch => "GOALKEEPER_PUNCH",
            Self::GoalkeeperRush => "GOALKEEPER_RUSH",
            Self::GoalkeeperDistribution => "GOALKEEPER_DISTRIBUTION",
            Self::Foul => "FOUL",
            Self::AdvantagePlayed => "ADVANTAGE_PLAYED",
            Self::YellowCard => "YELLOW_CARD",
            Self::RedCard => "RED_CARD",
            Self::Offside => "OFFSIDE",
            Self::Injury => "INJURY",
            Self::ThrowIn => "THROW_IN",
            Self::GoalKick => "GOAL_KICK",
            Self::Corner => "CORNER",
            Self::FreeKick => "FREE_KICK",
            Self::PenaltyAwarded => "PENALTY_AWARDED",
            Self::PenaltyKick => "PENALTY_KICK",
            Self::RestartScheduled => "RESTART_SCHEDULED",
            Self::Substitution => "SUBSTITUTION",
            Self::GoalkeeperAssignmentRequired => "GOALKEEPER_ASSIGNMENT_REQUIRED",
            Self::GoalkeeperReassigned => "GOALKEEPER_REASSIGNED",
            Self::CommandAccepted => "COMMAND_ACCEPTED",
            Self::CommandRejected => "COMMAND_REJECTED",
            Self::AddedTimeAnnounced => "ADDED_TIME_ANNOUNCED",
            Self::HalfTime => "HALF_TIME",
            Self::ExtraTimeStarted => "EXTRA_TIME_STARTED",
            Self::ExtraTimeHalfTime => "EXTRA_TIME_HALF_TIME",
            Self::PenaltyShootoutStarted => "PENALTY_SHOOTOUT_STARTED",
            Self::PenaltyShootoutKick => "PENALTY_SHOOTOUT_KICK",
            Self::PenaltyShootoutEnded => "PENALTY_SHOOTOUT_ENDED",
            Self::FullTime => "FULL_TIME",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchClockSnapshot {
    pub game_second: i32,
    pub period: Period,
    pub period_started_at_second: i32,
    pub minute: i32,
    pub added_time_minute: i32,
    pub display_minute: String,
}

impl MatchClockSnapshot {
    fn ceil_minutes(seconds: i32) -> i32 {
        ((seconds + 59) / 60).max(0)
    }

    pub fn capture(input: &MatchInput, clock: &ClockState) -> Self {
        let rules = &input.match_rules;
        let period = clock.period;
        let second = clock.game_second;
        let elapsed = (second - clock.period_started_at_second).max(0);
        let regulation_minutes = Self::ceil_minutes(rules.regulation_duration_seconds);
        let first_half_minutes = Self::ceil_minutes(rules.half_duration_seconds);
        let extra_time_minutes = Self::ceil_minutes(rules.extra_time_duration_seconds);
        let extra_half_minutes = Self::ceil_minutes(rules.extra_time_duration_seconds / 2);

        let (nominal_seconds, base_minutes) = match period {
            Period::FirstHalf => (rules.half_duration_seconds, 0),
            Period::SecondHalf => (rules.regulation_duration_seconds - rules.half_duration_seconds, first_half_minutes),
            Period::ExtraTimeFirst => (rules.extra_time_duration_seconds / 2, regulation_minutes),
            Period::ExtraTimeSecond => (
                rules.extra_time_duration_seconds - rules.extra_time_duration_seconds / 2,
                regulation_minutes + extra_half_minutes,
            ),
            Period::Shootout => {
                return Self {
                    game_second: second,
                    period,
                    period_started_at_second: clock.period_started_at_second,
                    minute: regulation_minutes + extra_time_minutes,
                    added_time_minute: 0,
                    display_minute: "PEN".into(),
                };
            }
            Period::FullTime => {
                return Self {
                    game_second: second,
                    period,
                    period_started_at_second: clock.period_started_at_second,
                    minute: regulation_minutes.max(Self::ceil_minutes(second)),
                    added_time_minute: 0,
                    display_minute: "FT".into(),
                };
            }
            Period::NotStarted => {
                return Self {
                    game_second: second,
                    period,
                    period_started_at_second: clock.period_started_at_second,
                    minute: 0,
                    added_time_minute: 0,
                    display_minute: "0".into(),
                };
            }
        };

        let nominal_minutes = Self::ceil_minutes(nominal_seconds);
        let added = if elapsed > nominal_seconds {
            Self::ceil_minutes(elapsed - nominal_seconds)
        } else {
            0
        };
        let minute = if added > 0 {
            base_minutes + nominal_minutes
        } else {
            base_minutes + Self::ceil_minutes(elapsed).max(1)
        };
        let display_minute = if added > 0 {
            format!("{}+{}", base_minutes + nominal_minutes, added)
        } else {
            minute.to_string()
        };
        Self {
            game_second: second,
            period,
            period_started_at_second: clock.period_started_at_second,
            minute,
            added_time_minute: added,
            display_minute,
        }
    }

    pub fn legacy(game_second: i32) -> Self {
        let minute = Self::ceil_minutes(game_second).max(1);
        Self {
            game_second,
            period: Period::NotStarted,
            period_started_at_second: 0,
            minute,
            added_time_minute: 0,
            display_minute: minute.to_string(),
        }
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.game_second);
        h = fp::mix_i32(h, self.period as i32);
        h = fp::mix_i32(h, self.period_started_at_second);
        h = fp::mix_i32(h, self.minute);
        h = fp::mix_i32(h, self.added_time_minute);
        fp::mix_string(h, Some(&self.display_minute))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchEventFacts {
    pub clock: MatchClockSnapshot,
    pub home_goals: i32,
    pub away_goals: i32,
    pub ball_x: i32,
    pub ball_y: i32,
    pub ball_owner_team: Option<TeamSide>,
    pub ball_owner_player_id: Option<String>,
    pub ball_control: BallControl,
    pub possession_team: Option<TeamSide>,
    pub previous_possession_team: Option<TeamSide>,
    pub possession_sequence_id: i64,
    pub tactical_phase: TacticalPhase,
    pub transition_active: bool,
    pub restart_type: RestartType,
    pub restart_team: Option<TeamSide>,
    pub restart_reason: String,

    pub home_team_name: String,
    pub away_team_name: String,
    pub competition_context: String,
    pub match_id: String,
    pub neutral_ground: bool,
    pub home_formation: String,
    pub away_formation: String,
    pub home_shots: i32,
    pub away_shots: i32,
    pub home_shots_on_target: i32,
    pub away_shots_on_target: i32,
    pub home_corners: i32,
    pub away_corners: i32,
    pub home_fouls: i32,
    pub away_fouls: i32,
    pub home_completed_passes: i32,
    pub away_completed_passes: i32,
}

impl MatchEventFacts {
    pub fn capture(input: &MatchInput, state: &MatchState) -> Self {
        let home_slots: Vec<TacticalSlotSnapshot> = state.home_team.tactical_slots.iter().map(|(_, s)| s.clone()).collect();
        let away_slots: Vec<TacticalSlotSnapshot> = state.away_team.tactical_slots.iter().map(|(_, s)| s.clone()).collect();
        Self {
            clock: MatchClockSnapshot::capture(input, &state.clock),
            home_goals: state.score.home_goals,
            away_goals: state.score.away_goals,
            ball_x: state.ball.x,
            ball_y: state.ball.y,
            ball_owner_team: state.ball.owner_team,
            ball_owner_player_id: state.ball.owner_player_id.clone(),
            ball_control: state.ball.control,
            possession_team: state.possession.controlling_team,
            previous_possession_team: state.possession.previous_controlling_team,
            possession_sequence_id: state.possession.sequence_id,
            tactical_phase: state.possession.phase,
            transition_active: state.possession.transition_active,
            restart_type: state.restart.kind,
            restart_team: state.restart.team,
            restart_reason: state.restart.reason.clone(),
            home_team_name: input.home_team.display_name.trim().to_string(),
            away_team_name: input.away_team.display_name.trim().to_string(),
            competition_context: input.competition_context.clone(),
            match_id: input.match_id.clone(),
            neutral_ground: input.neutral_ground,
            home_formation: resolve_formation_label(&state.home_team.shape_id, &home_slots),
            away_formation: resolve_formation_label(&state.away_team.shape_id, &away_slots),
            home_shots: state.home_team.shots,
            away_shots: state.away_team.shots,
            home_shots_on_target: state.home_team.shots_on_target,
            away_shots_on_target: state.away_team.shots_on_target,
            home_corners: state.home_team.corners,
            away_corners: state.away_team.corners,
            home_fouls: state.home_team.fouls,
            away_fouls: state.away_team.fouls,
            home_completed_passes: state.home_team.completed_passes,
            away_completed_passes: state.away_team.completed_passes,
        }
    }

    pub fn player_position(input: &MatchInput, player_id: &str) -> String {
        if player_id.is_empty() {
            return String::new();
        }
        let found = input
            .home_team
            .player(player_id)
            .map(|p| (p, &input.home_team))
            .or_else(|| input.away_team.player(player_id).map(|p| (p, &input.away_team)));
        if let Some((p, owner_team)) = found {
            if !p.primary_position.is_empty() {
                return p.primary_position.clone();
            }
            if let Some(slot) = owner_team.slot_for_player(player_id) {
                return infer_slot_position(slot).into();
            }
        }
        String::new()
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = self.clock.fingerprint();
        h = fp::mix_i32(h, self.home_goals);
        h = fp::mix_i32(h, self.away_goals);
        h = fp::mix_i32(h, self.ball_x);
        h = fp::mix_i32(h, self.ball_y);
        h = fp::mix_i32(h, self.ball_owner_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_string(h, self.ball_owner_player_id.as_deref());
        h = fp::mix_i32(h, self.ball_control as i32);
        h = fp::mix_i32(h, self.possession_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_i32(h, self.previous_possession_team.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_i64(h, self.possession_sequence_id);
        h = fp::mix_i32(h, self.tactical_phase as i32);
        h = fp::mix_bool(h, self.transition_active);
        h = fp::mix_i32(h, self.restart_type as i32);
        h = fp::mix_i32(h, self.restart_team.map(|s| s as i32).unwrap_or(-1));
        fp::mix_string(h, Some(&self.restart_reason))
    }
}

fn infer_slot_position(s: &TacticalSlotSnapshot) -> &'static str {
    let (x, y) = (s.out_x, s.out_y);
    if y <= 11 { return "GK"; }
    if y <= 30 {
        if x <= 24 { return "LB"; }
        if x >= 76 { return "RB"; }
        return "CB";
    }
    if y <= 36 && (x <= 20 || x >= 80) { return if x < 50 { "LWB" } else { "RWB" }; }
    if y <= 44 { return "DM"; }
    if y <= 60 {
        if x <= 24 { return "LM"; }
        if x >= 76 { return "RM"; }
        return "CM";
    }
    if y <= 72 {
        if x <= 22 { return "LW"; }
        if x >= 78 { return "RW"; }
        return "AM";
    }
    "CF"
}

fn resolve_formation_label(shape_id: &str, slots: &[TacticalSlotSnapshot]) -> String {
    if !shape_id.is_empty() {
        let mut digits = String::new();
        for c in shape_id.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else if !digits.is_empty() {
                break;
            }
        }
        match digits.as_str() {
            "41212" => return "4-1-2-1-2".into(),
            "4231" => return "4-2-3-1".into(),
            "4312" => return "4-3-1-2".into(),
            "4141" => return "4-1-4-1".into(),
            "4321" => return "4-3-2-1".into(),
            d if d.len() == 3 => {
                let b = d.as_bytes();
                let d1 = (b[0] - b'0') as i32;
                let d2 = (b[1] - b'0') as i32;
                let d3 = (b[2] - b'0') as i32;
                if d1 + d2 + d3 == 10 && (3..=5).contains(&d1) {
                    return format!("{}-{}-{}", d1, d2, d3);
                }
            }
            _ => {}
        }
    }
    if slots.len() < 11 {
        return String::new();
    }
    let (mut def, mut wb, mut dm, mut cm, mut am, mut wing, mut cf) = (0, 0, 0, 0, 0, 0, 0);
    for s in slots {
        let (x, y) = (s.out_x, s.out_y);
        if y <= 11 { continue; }
        if y <= 30 { def += 1; }
        else if y <= 36 && (x <= 20 || x >= 80) { wb += 1; }
        else if y <= 44 && x > 20 && x < 80 { dm += 1; }
        else if y <= 60 { cm += 1; }
        else if y <= 72 && x > 22 && x < 78 { am += 1; }
        else if y <= 72 { wing += 1; }
        else { cf += 1; }
    }
    let back = def + wb;
    if !(3..=5).contains(&back) { return String::new(); }
    if dm == 1 && cm == 2 && am == 1 && wing == 0 && cf == 2 {
        return format!("{}-1-2-1-2", back);
    }
    if dm == 2 && cm == 0 && (am + wing) == 3 && cf == 1 {
        return format!("{}-2-3-1", back);
    }
    let mid = dm + cm + am;
    let fwd = wing + cf;
    if back + mid + fwd != 10 || mid <= 0 || fwd <= 0 {
        return String::new();
    }
    format!("{}-{}-{}", back, mid, fwd)
}

#[derive(Clone, Debug)]
pub struct MatchEvent {
    pub sequence: i64,
    pub game_second: i32,
    pub kind: EventType,
    pub side: Option<TeamSide>,
    pub actor: Option<String>,
    pub target: Option<String>,
    pub ball_x: i32,
    pub ball_y: i32,

    pub metadata: Vec<(String, String)>,
    pub facts_before: Option<MatchEventFacts>,
    pub facts_after: Option<MatchEventFacts>,
}

impl MatchEvent {
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i64(h, self.sequence);
        h = fp::mix_i32(h, self.game_second);
        h = fp::mix_i32(h, self.kind as i32);
        h = fp::mix_i32(h, self.side.map(|s| s as i32).unwrap_or(-1));
        h = fp::mix_string(h, self.actor.as_deref());
        h = fp::mix_string(h, self.target.as_deref());
        h = fp::mix_i32(h, self.ball_x);
        h = fp::mix_i32(h, self.ball_y);
        for (k, v) in &self.metadata {
            h = fp::mix_string(h, Some(k));
            h = fp::mix_string(h, Some(v));
        }
        h
    }

    pub fn evidence_fingerprint(&self) -> u64 {
        let mut h = self.fingerprint();
        h = fp::mix_i32(h, EVENT_SCHEMA_VERSION);
        h = fp::mix_i64(h, self.facts_before.as_ref().map(|f| f.fingerprint() as i64).unwrap_or(0));
        fp::mix_i64(h, self.facts_after.as_ref().map(|f| f.fingerprint() as i64).unwrap_or(0))
    }
}

pub trait MatchEventSink {
    fn on_event(&mut self, event: &MatchEvent);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enum_ordinals_stable() {
        assert_eq!(EventType::MatchStarted as i32, 0);
        assert_eq!(EventType::Goal as i32, 34);
        assert_eq!(EventType::FullTime as i32, 66);
    }
}
