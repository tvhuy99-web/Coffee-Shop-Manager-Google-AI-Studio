use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite};
use crate::v4::ball_play::offside_reference_progress_m;
use crate::v4::input::MatchInputV4;
use crate::v4::movement::{evaluate_local_context, evaluate_passing_lane, player_attrs, LocalSpatialContext};
use crate::v4::rules::{estimate_reaction_decision_ms, TimingModelConfig};
use crate::v4::spatial::{attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box, PITCH_WIDTH_M};
use crate::v4::state::{MatchStateV4, TacticalPhaseV4};
use super::intent::{CandidateOption, DecisionPlanV4, PassKindV4, PlannedActionV4};
use super::timing::evaluate_one_touch_intent;

/// DES entry point returning the chosen action along with `decision_delay_ms` and `is_one_touch`.
pub fn perceive_evaluate_and_decide_des(
    state: &MatchStateV4,
    input: &MatchInputV4,
    timing: &TimingModelConfig,
    carrier_idx: usize,
    carrier_ctx: LocalSpatialContext,
    rng: &mut DeterministicRng,
) -> DecisionPlanV4 {
    let carrier = &state.players[carrier_idx];
    let att_side = carrier.side;
    let def_side = att_side.opposite();
    let attrs = player_attrs(state, carrier);
    let ti = state.active_instructions(att_side);
    let def_ti = state.active_instructions(def_side);
    let att_phase = state.team_state(att_side).phase;

    let vision = normalize_attr(attrs.mental.vision);
    let decisions = normalize_attr(attrs.mental.decisions);
    let composure = normalize_attr(attrs.mental.composure);
    let concentration = normalize_attr(attrs.mental.concentration);
    let flair = normalize_attr(attrs.mental.flair);
    let teamwork = normalize_attr(attrs.mental.teamwork);
    let pressure_norm = (carrier_ctx.pressure / 100.0).clamp(0.0, 1.0);

    let perception_quality = (vision * 0.45
        + normalize_attr(attrs.mental.anticipation) * 0.25
        + concentration * 0.15
        + carrier.position_familiarity * 0.15
        - pressure_norm * (0.28 - composure * 0.14))
        .clamp(0.18, 1.0);

    let max_vision_range_m = 22.0 + perception_quality * 40.0;
    let carrier_progress = attacking_progress_m(att_side, carrier.pos);
    let goal_dist = distance_to_attacking_goal(att_side, carrier.pos);
    let in_box = is_in_attacking_penalty_box(att_side, carrier.pos);

    // Tactical style identity modifiers
    let counter_boost =
        if att_phase == TacticalPhaseV4::AttackingTransition && ti.transition_risk >= 60 {
            11.0 + if def_ti.line_height >= 65 { 7.0 } else { 0.0 }
        } else {
            0.0
        };
    let possession_short_boost = if ti.tempo <= 48 && ti.build_up_risk <= 52 {
        9.0
    } else {
        0.0
    };
    let time_waste_safe_boost = if ti.time_wasting >= 55 { 10.0 } else { 0.0 };

    let mut candidates: Vec<CandidateOption> = Vec::with_capacity(14);
    let offside_line = offside_reference_progress_m(state, att_side, carrier.pos);
    // Perception is fallible, especially under pressure; the Law 11 check remains exact.
    let timing_awareness = (vision * 0.40 + decisions * 0.40 + concentration * 0.20)
        * (1.0 - pressure_norm * 0.25);

    for (t_idx, mate) in state.players.iter().enumerate() {
        if t_idx == carrier_idx || !mate.on_pitch || mate.sent_off || mate.side != att_side {
            continue;
        }
        let dist_m = carrier.pos.distance(mate.pos);
        if dist_m < 3.5 || dist_m > max_vision_range_m {
            continue;
        }

        let mate_progress = attacking_progress_m(att_side, mate.pos);
        let progress_gain = mate_progress - carrier_progress;
        let lateral_span = (mate.pos.y - carrier.pos.y).abs();
        let mate_in_box = is_in_attacking_penalty_box(att_side, mate.pos);

        let carrier_wide = carrier.pos.y < 15.0 || carrier.pos.y > (PITCH_WIDTH_M - 15.0);
        let kind = if carrier_wide && carrier_progress > 65.0 && mate_in_box {
            PassKindV4::Cross
        } else if lateral_span >= 30.0 && dist_m >= 32.0 {
            PassKindV4::Switch
        } else if progress_gain >= 16.0 && mate_progress >= 75.0 && (14.0..=34.0).contains(&dist_m)
        {
            PassKindV4::Through
        } else if dist_m >= 28.0 {
            PassKindV4::Long
        } else {
            PassKindV4::Short
        };

        if matches!(kind, PassKindV4::Through | PassKindV4::Switch) {
            let notice_prob = (0.30 + vision * 0.65 - pressure_norm * 0.20).clamp(0.15, 0.96);
            if (rng.next_double() as f32) > notice_prob {
                continue;
            }
        }

        let (lane_risk, interceptor_idx) = evaluate_passing_lane(
            state,
            input,
            att_side,
            carrier.pos,
            mate.pos,
            kind.is_aerial(),
        );
        let mate_ctx = evaluate_local_context(state, input, att_side, mate.pos);
        let mate_attrs = player_attrs(state, mate);
        let mate_space = (1.0 - mate_ctx.pressure / 100.0).clamp(0.0, 1.0);

        let base_reward = match kind {
            PassKindV4::Through => {
                31.0 + progress_gain * 0.45 + mate_space * 18.0 + counter_boost
            }
            PassKindV4::Cross => {
                26.0 + normalize_attr(attrs.technical.crossing) * 14.0
                    + normalize_attr(mate_attrs.technical.heading) * 10.0
            }
            PassKindV4::Switch => 28.0 + mate_space * 22.0 + (ti.width as f32 * 0.10),
            PassKindV4::Long => {
                23.0 + progress_gain * 0.30 + mate_space * 12.0 + counter_boost * 0.7
            }
            PassKindV4::Short => {
                31.0 + progress_gain * 0.28
                    + mate_space * 14.0
                    + possession_short_boost
                    + time_waste_safe_boost
            }
        };
        let overload_bonus = if mate_ctx.overload { 7.0 } else { 0.0 };
        let teamwork_bonus = teamwork * 8.0;
        let gk_penalty = if mate.is_goalkeeper && carrier_progress > 35.0 {
            -18.0
        } else {
            0.0
        };
        let risk = (lane_risk * 0.72 + (mate_ctx.pressure / 100.0) * 0.28).clamp(0.0, 1.0);
        let tempo_risk_tolerance = 0.65 + (ti.build_up_risk as f32 / 100.0) * 0.45;
        // A runner already beyond both the ball and second-last opponent is a poor
        // receiving option. Penalize obvious mistiming, not legal forward runs.
        // Marginal errors and low-skill decisions remain possible.
        let offside_penalty = offside_line.map_or(0.0, |line| {
            let beyond_m = (mate_progress - line - 0.001).max(0.0);
            if beyond_m <= 0.0 { 0.0 } else {
                let clarity = 0.25 + 0.75 * (beyond_m / 3.0).min(1.0);
                clarity * (12.0 + 54.0 * timing_awareness)
            }
        });
        let utility = base_reward + overload_bonus + teamwork_bonus + gk_penalty
            - risk * (34.0 / tempo_risk_tolerance) - offside_penalty;

        candidates.push(CandidateOption {
            action: PlannedActionV4::Pass {
                target_idx: t_idx,
                kind,
                distance_m: dist_m,
                progress_gain_m: progress_gain,
                lane_risk,
                interceptor_idx,
            },
            utility,
            risk,
        });
    }

    if !carrier.is_goalkeeper && goal_dist <= 28.5 {
        let fin = if in_box {
            normalize_attr_elite(attrs.technical.finishing)
        } else {
            normalize_attr_elite(attrs.technical.long_shots)
        };
        let dist_appeal = ((28.5 - goal_dist) / 24.0).clamp(0.0, 1.18);
        // V4 P2 calibration: unboosted shooting was selected only ~2 times per 90 min.
        // Reward legitimate attempts inside the box while keeping distance, pressure,
        // skill and shot opportunity costs active.
        let shot_utility = (if in_box { 48.0 } else { 24.0 })
            + dist_appeal * 13.5
            + fin * 10.0
            + composure * 3.2
            - pressure_norm * 16.0
            - time_waste_safe_boost * 0.6;
        candidates.push(CandidateOption {
            action: PlannedActionV4::Shoot {
                distance_m: goal_dist,
                in_box,
            },
            utility: shot_utility,
            risk: (0.25 + pressure_norm * 0.45).clamp(0.1, 0.9),
        });
    }

    if !carrier.is_goalkeeper {
        let drib = normalize_attr(attrs.technical.dribbling);
        let agility = normalize_attr(attrs.physical.agility);
        let accel = normalize_attr(attrs.physical.acceleration);
        let ana_ready = (carrier.fatigue.anaerobic / 100.0).clamp(0.25, 1.0);
        let drib_utility = 14.0
            + (drib * 16.0 + agility * 8.0 + accel * 6.0 + flair * 7.0) * ana_ready
            + if carrier_ctx.overload { 5.0 } else { 0.0 }
            - pressure_norm * 16.0;
        candidates.push(CandidateOption {
            action: PlannedActionV4::Dribble {
                defender_idx: carrier_ctx.nearest_defender_idx,
            },
            utility: drib_utility,
            risk: (0.25 + pressure_norm * 0.55 - drib * 0.25).clamp(0.08, 0.92),
        });
    }

    if carrier_progress < 28.0 && pressure_norm > 0.58 {
        candidates.push(CandidateOption {
            action: PlannedActionV4::Clearance,
            utility: 22.0 + pressure_norm * 24.0 - composure * 10.0,
            risk: 0.12,
        });
    } else {
        candidates.push(CandidateOption {
            action: PlannedActionV4::HoldAndRecycle,
            utility: 12.0 + composure * 8.0 + time_waste_safe_boost - pressure_norm * 12.0,
            risk: 0.15 + pressure_norm * 0.35,
        });
    }

    let mental_pressure = normalize_attr(attrs.hidden.pressure);
    let noise_amplitude = (18.0 * (1.0 - decisions * 0.68)
        + pressure_norm * 10.0 * (1.0 - (composure * 0.5 + mental_pressure * 0.5) * 0.65))
        .clamp(1.8, 24.0);

    let mut best_action = candidates[0].action;
    let mut best_score = f32::NEG_INFINITY;

    for cand in &candidates {
        let risk_sensitivity = 4.0 + decisions * 12.0;
        let noise = (rng.next_double() as f32 - 0.5) * 2.0 * noise_amplitude;
        let score = cand.utility - cand.risk * risk_sensitivity + noise;
        if score > best_score {
            best_score = score;
            best_action = cand.action;
        }
    }

    let is_one_touch = evaluate_one_touch_intent(
        &best_action,
        attrs,
        pressure_norm,
        ti.tempo,
        ti.time_wasting,
        rng,
    );

    let base_delay_ms = estimate_reaction_decision_ms(
        timing,
        attrs.technical.first_touch,
        attrs.mental.vision,
        attrs.mental.decisions,
        attrs.mental.composure,
        carrier_ctx.pressure,
        ti.tempo,
        is_one_touch,
        rng,
    );

    // Deliberate "la pausa" timing hold: when not playing one-touch and under low pressure,
    // composed high-decision players may hold the ball an extra fraction of a second to pick
    // the optimal release window rather than always acting mechanically faster.
    let pausa_hold_ms = if !is_one_touch && pressure_norm < 0.40 {
        let tempo_norm = (ti.tempo as f32 / 100.0).clamp(0.0, 1.0);
        let action_hold_weight = match best_action {
            PlannedActionV4::HoldAndRecycle => 1.25,
            PlannedActionV4::Pass { .. } => 0.85,
            _ => 0.0,
        };
        let hold = (composure * 0.60 + decisions * 0.40)
            * (1.0 - pressure_norm / 0.40)
            * (1.0 - tempo_norm * 0.60)
            * 210.0
            * action_hold_weight;
        hold.round() as i64
    } else {
        0
    };

    let decision_delay_ms = base_delay_ms
        .saturating_add(pausa_hold_ms)
        .clamp(timing.one_touch_reaction_ms.min_ms, timing.reaction_base_ms.max_ms);

    DecisionPlanV4 {
        action: best_action,
        decision_delay_ms,
        is_one_touch,
    }
}

pub fn perceive_evaluate_and_decide(
    state: &MatchStateV4,
    input: &MatchInputV4,
    carrier_idx: usize,
    carrier_ctx: LocalSpatialContext,
    rng: &mut DeterministicRng,
) -> PlannedActionV4 {
    let default_timing = TimingModelConfig::default();
    perceive_evaluate_and_decide_des(
        state,
        input,
        &default_timing,
        carrier_idx,
        carrier_ctx,
        rng,
    )
    .action
}
