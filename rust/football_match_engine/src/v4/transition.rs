//! Phase A4 transition vocabulary, monotonic commit gate and read-only state verifier.
//!
//! Provides runtime and test verification that `MatchStateV4` remains physically and legally
//! consistent without mutating gameplay state. The public API transaction gate is separate.

use crate::v4::error::V4Error;

/// Runtime commit gate shared by all public mutation entrypoints.
pub(crate) fn verify_commit_progress(before_ms:i64,after_ms:i64,before_events:u32,after_events:u32,before_steps:u64,after_steps:u64)->Result<(),V4Error>{
    if after_ms<before_ms || after_events<before_events || after_steps<before_steps {
        return Err(V4Error::InvalidClock("INV-03: transaction regressed time, event sequence or DES step count".into()));
    }
    Ok(())
}

use crate::contracts::{is_finite_vec2, is_finite_vec3};
use crate::v4::state::{BallControlState, MatchStateV4};

/// Stable commit-boundary validation. No clamping, no corrections, no RNG.
/// This intentionally checks structural facts, not legacy restart ownership.
pub(crate) fn verify_commit_structure(state:&MatchStateV4)->Result<(),V4Error>{
    let mut ids=std::collections::HashSet::new();
    if !is_finite_vec3(state.ball.pos)||!is_finite_vec3(state.ball.vel){return Err(V4Error::InvalidInput("INV-02: non-finite ball kinematics".into()));}
    for p in &state.players {
        if !ids.insert(p.player_id.as_str())||p.profile_idx>=state.player_profiles.len(){return Err(V4Error::InvalidInput(format!("INV-08: duplicate player ID or invalid profile: {}",p.player_id)));}
        if !is_finite_vec2(p.pos)||!is_finite_vec2(p.vel)||!is_finite_vec2(p.target_pos)||!p.facing_rad.is_finite(){return Err(V4Error::InvalidInput(format!("INV-02: non-finite player kinematics: {}",p.player_id)));}
        if !(0.0..=100.0).contains(&p.fatigue.aerobic)||!(0.0..=100.0).contains(&p.fatigue.anaerobic)||!p.fatigue.distance_m.is_finite()||p.fatigue.distance_m<0.0{return Err(V4Error::InvalidInput(format!("INV-07: invalid condition: {}",p.player_id)));}
    }
    Ok(())
}

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
