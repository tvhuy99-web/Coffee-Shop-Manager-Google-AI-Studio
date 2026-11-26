//! Coach evaluation adapter preserving evaluation and commit order.
//! Proposals are not yet separated from state commits.
use crate::v4::ball_play::EmittedPlayEvent;
use crate::contracts::MatchInputV4;
use crate::v4::state::MatchStateV4;
pub(crate) fn evaluate_ai_managers(state: &mut MatchStateV4, input: &MatchInputV4) -> Vec<EmittedPlayEvent> {
    crate::v4::ai_manager::evaluate_ai_managers(state, input)
}
