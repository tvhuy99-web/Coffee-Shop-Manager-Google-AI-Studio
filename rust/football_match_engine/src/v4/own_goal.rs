//! Accidental own goals from deflected shots and mishit clearances.
//! A defending player is the actor, but the goal is awarded to the opposition.
//! Unlike normal goals, own goals are not credited as a scorer/assist to attackers.
use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::spatial::{attacking_goal_center, distance_to_attacking_goal, TeamSideV4, Vec2, Vec3};
use crate::v4::state::MatchStateV4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeflectionCause { BlockedShot, MishitAerialClearance, PassContact }
impl DeflectionCause {
    pub fn as_str(self) -> &'static str { match self {
        Self::BlockedShot => "DEFLECTED_BLOCK",
        Self::MishitAerialClearance => "MISHIT_CLEARANCE",
        Self::PassContact => "DEFLECTED_PASS",
    }}
}

/// Own goals are rare and only possible near the goal being defended.
/// Better concentration/technique lowers risk; never use a global random goal.
pub fn own_goal_risk(
    defender_pos: Vec2, beneficiary: TeamSideV4,
    attrs: &PlayerAttributes54, cause: DeflectionCause,
) -> f32 {
    let distance = distance_to_attacking_goal(beneficiary, defender_pos);
    if distance > 16.5 { return 0.0; }
    let proximity = ((16.5 - distance) / 16.5).clamp(0.0, 1.0);
    let control = normalize_attr(attrs.mental.concentration) * 0.35
        + normalize_attr(attrs.mental.anticipation) * 0.25
        + normalize_attr(attrs.technical.heading) * 0.20
        + normalize_attr(attrs.physical.balance) * 0.20;
    let base = match cause {
        DeflectionCause::BlockedShot => 0.020,
        DeflectionCause::MishitAerialClearance => 0.012,
        DeflectionCause::PassContact => 0.0, // physical crossing only
    };
    (base * (0.25 + 0.75 * proximity) * (1.35 - 0.75 * control)).clamp(0.0, 0.03)
}

pub fn maybe_own_goal(
    state: &mut MatchStateV4,
    scorer_idx: usize,
    beneficiary: TeamSideV4,
    cause: DeflectionCause,
    attrs: &PlayerAttributes54,
    sequence_id: u32,
    rng: &mut DeterministicRng,
) -> Option<EmittedPlayEvent> {
    if !state.players[scorer_idx].on_pitch
        || state.players[scorer_idx].sent_off
        || state.players[scorer_idx].side != beneficiary.opposite() {
        return None;
    }
    let risk = own_goal_risk(state.players[scorer_idx].pos, beneficiary, attrs, cause);
    if risk == 0.0 || rng.next_double() as f32 >= risk { return None; }
    Some(award_own_goal(state, scorer_idx, beneficiary, cause, sequence_id))
}

/// Only this function should increment the scorer's own-goals and the beneficiary's score.
/// It does not credit the attacking shooter's goals, assists or shots-on-target.
pub fn award_own_goal(
    state: &mut MatchStateV4,
    scorer_idx: usize,
    beneficiary: TeamSideV4,
    cause: DeflectionCause,
    sequence_id: u32,
) -> EmittedPlayEvent {
    let offender = &state.players[scorer_idx];
    assert!(offender.on_pitch && !offender.sent_off && offender.side == beneficiary.opposite());
    let offender_id = offender.player_id.clone();
    state.team_state_mut(beneficiary).goals += 1;
    state.team_state_mut(beneficiary).own_goals_for += 1;
    state.team_state_mut(beneficiary.opposite()).own_goals_conceded += 1;
    state.players[scorer_idx].own_goals = state.players[scorer_idx].own_goals.saturating_add(1);
    state.players[scorer_idx].rating_tenths = state.players[scorer_idx].rating_tenths.saturating_sub(7).max(10);
    let goal = attacking_goal_center(beneficiary);
    state.ball.pos = Vec3::new(goal.x, goal.y, 1.0);
    state.ball.vel = Vec3::default();
    state.ball.control = crate::v4::state::BallControlState::DeadBall;
    state.ball.owner_side = None;
    state.ball.owner_player_id = None;
    state.ball.last_touch_side = Some(beneficiary.opposite());
    state.ball.last_touch_player_id = Some(offender_id.clone());
    EmittedPlayEvent {
        kind: EventTypeV4::OwnGoal,
        tier: HighlightTier::GoalKey,
        seq_id: Some(sequence_id),
        // Side is the goal BENEFICIARY; actor belongs to the OTHER team.
        side: Some(beneficiary),
        actor: Some(offender_id),
        target: None,
        ball_pos: Vec3::new(goal.x, goal.y, 1.0),
        metadata: EventMetadataV4 {
            scored: Some(true),
            outcome: Some("OWN_GOAL".into()),
            cause: Some(cause.as_str().into()),
            ..EventMetadataV4::default()
        },
    }
}
