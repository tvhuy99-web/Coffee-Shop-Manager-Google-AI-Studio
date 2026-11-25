//! Phase A4 atomic transition stages and authoritative invariant verifier (INV-01..INV-07).
//!
//! Provides runtime and test verification that `MatchStateV4` remains physically and legally
//! consistent after every DES transition without mutating gameplay state.

use crate::contracts::{is_finite_vec2, is_finite_vec3};
use crate::v4::state::{BallControlState, MatchStateV4};

/// Canonical stages of a single atomic DES transition in `match_engine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStage {
    ValidateGenerationAndTime,
    AdvanceWorldAndRng,
    ResolvePhysicsFacts,
    AdjudicateRules,
    ComputeConditionAndTactics,
    VerifyInvariants,
    CommitAndPublish,
}

/// Structured report of a violated authoritative invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation {
    pub code: &'static str,
    pub detail: String,
}

/// Verifies core authoritative invariants on `MatchStateV4` (read-only, zero side effects).
pub fn verify_authoritative_invariants(state: &MatchStateV4) -> Result<(), Vec<InvariantViolation>> {
    let mut violations = Vec::new();

    // INV-01 & INV-04: Controlled ball must have a valid, on-pitch, non-sent-off owner of matching side.
    if state.ball.control == BallControlState::Controlled {
        match state.ball.owner_player_id.as_deref() {
            None => violations.push(InvariantViolation {
                code: "INV-01",
                detail: "ball is Controlled without owner_player_id".to_string(),
            }),
            Some(owner_id) => match state.players.iter().find(|p| p.player_id == owner_id) {
                None => violations.push(InvariantViolation {
                    code: "INV-01",
                    detail: format!("ball owner_player_id {owner_id} not found in match players"),
                }),
                Some(p) if !p.on_pitch || p.sent_off => violations.push(InvariantViolation {
                    code: "INV-04",
                    detail: format!(
                        "ball carrier {} is ineligible (on_pitch={}, sent_off={})",
                        p.player_id, p.on_pitch, p.sent_off
                    ),
                }),
                Some(p) if state.ball.owner_side != Some(p.side) => violations.push(InvariantViolation {
                    code: "INV-01",
                    detail: format!(
                        "ball owner_side {:?} mismatches carrier {} side {:?}",
                        state.ball.owner_side, p.player_id, p.side
                    ),
                }),
                _ => {}
            },
        }
    }

    // INV-02: Ball and all players must have strictly finite coordinates & velocities.
    if !is_finite_vec3(state.ball.pos) || !is_finite_vec3(state.ball.vel) {
        violations.push(InvariantViolation {
            code: "INV-02",
            detail: "ball position or velocity contains NaN/infinite component".to_string(),
        });
    }
    for p in &state.players {
        if !is_finite_vec2(p.pos) || !is_finite_vec2(p.vel) || !is_finite_vec2(p.target_pos) {
            violations.push(InvariantViolation {
                code: "INV-02",
                detail: format!("player {} has non-finite kinematics", p.player_id),
            });
        }
        // INV-07: Fatigue energy pools must remain within valid finite bounds.
        if !p.fatigue.aerobic.is_finite()
            || !p.fatigue.anaerobic.is_finite()
            || p.fatigue.aerobic < 0.0
            || p.fatigue.aerobic > 100.5
            || p.fatigue.anaerobic < 0.0
            || p.fatigue.anaerobic > 100.5
        {
            violations.push(InvariantViolation {
                code: "INV-07",
                detail: format!(
                    "player {} fatigue out of bounds (aerobic={}, anaerobic={})",
                    p.player_id, p.fatigue.aerobic, p.fatigue.anaerobic
                ),
            });
        }
    }

    // INV-03: DES simulation clock must be non-negative.
    if state.des_clock.sim_time_ms < 0 {
        violations.push(InvariantViolation {
            code: "INV-03",
            detail: format!("negative sim_time_ms: {}", state.des_clock.sim_time_ms),
        });
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v4::input::{RulesModeV4, SimulationMode};
    use crate::v4::tests::make_v4_input;

    #[test]
    fn fresh_match_state_satisfies_authoritative_invariants() {
        let input = make_v4_input(4242, SimulationMode::FullEvent, RulesModeV4::Standard);
        let state = MatchStateV4::from_input(&input);
        assert!(verify_authoritative_invariants(&state).is_ok());
    }
}
