//! Immutable, index-preserving PI observation. Opponent attributes are not included.
use crate::{attributes::PlayerAttributes54, spatial::{TeamSideV4,Vec2}, tactical_phase::TacticalPhaseV4, team_instructions::TeamInstructionsV4};
#[derive(Debug,Clone,Copy)]
pub struct DecisionPlayer {
    pub pos: Vec2, pub side: TeamSideV4, pub on_pitch: bool, pub sent_off: bool,
    pub is_goalkeeper: bool, pub first_touch: u8, pub heading: u8,
}
pub struct DecisionObservation<'a> {
    pub players: Vec<DecisionPlayer>, pub attrs: &'a PlayerAttributes54,
    pub own_instructions: &'a TeamInstructionsV4, pub opponent_line_height: u8,
    pub phase: TacticalPhaseV4, pub position_familiarity: f32, pub anaerobic: f32,
    pub concentration_level: f32, pub match_form_multiplier: f32, pub composure_delta: f32,
    pub personal_confidence_delta: f32, pub personal_confidence_bound: f32,
}
