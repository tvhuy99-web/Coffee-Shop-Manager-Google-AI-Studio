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
use super::utility::{ActionValue, is_switch, possession_value, turnover_cost, shot_goal_probability, shot_value};
use crate::v4::spatial::Vec2;

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

    // All choices share a scale of expected positional/opportunity value.
    // Tactical preferences affect risk and time, not hard action quotas.
    let risk_appetite = (ti.build_up_risk as f32 / 100.0).clamp(0.0, 1.0);
    let tempo = ti.tempo as f32 / 100.0;
    let time_wasting = ti.time_wasting as f32 / 100.0;
    let loss = turnover_cost(carrier_progress) * (1.15 - 0.30 * risk_appetite);
    let current_value = possession_value(carrier_progress,carrier.pos.y,pressure_norm);
    let urgency = 0.75 + 0.5 * tempo;
    let counter = if att_phase == TacticalPhaseV4::AttackingTransition {
        (ti.transition_risk as f32 / 100.0) * (0.008 + 0.008 * def_ti.line_height as f32 / 100.0)
    } else { 0.0 };
    let readiness = (carrier.fatigue.anaerobic / 100.0).clamp(0.25,1.0);

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
        let mate_in_box = is_in_attacking_penalty_box(att_side, mate.pos);

        let carrier_wide = carrier.pos.y < 15.0 || carrier.pos.y > (PITCH_WIDTH_M - 15.0);
        let kind = if carrier_wide && carrier_progress > 65.0 && mate_in_box {
            PassKindV4::Cross
        } else if is_switch(carrier.pos.y, mate.pos.y, dist_m) {
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
        let target_pressure = (mate_ctx.pressure / 100.0).clamp(0.0, 1.0);
        let skill = match kind {
            PassKindV4::Short => normalize_attr(attrs.technical.passing) * 0.7 + normalize_attr(attrs.technical.technique) * 0.3,
            PassKindV4::Cross => normalize_attr(attrs.technical.crossing) * 0.7 + normalize_attr(attrs.technical.technique) * 0.3,
            _ => normalize_attr(attrs.technical.passing) * 0.5 + vision * 0.25 + normalize_attr(attrs.technical.technique) * 0.25,
        };
        let touch = normalize_attr(mate_attrs.technical.first_touch);
        // The estimates below describe what the player thinks is achievable,
        // not the engine's actual contact/accuracy calculation.
        let success = (0.76 + 0.18 * skill + 0.07 * touch - 0.46 * lane_risk
            - 0.16 * target_pressure - 0.12 * pressure_norm
            - 0.12 * (dist_m / 65.0) - 0.07 * (1.0 - readiness)).clamp(0.06,0.98);
        let destination = possession_value(mate_progress,mate.pos.y,target_pressure);
        let support = if mate_ctx.overload && !carrier_ctx.overload { 0.010 * teamwork } else { 0.0 };
        let transition = counter * (progress_gain / 22.0).clamp(0.0,1.0);
        let cross_option = if kind == PassKindV4::Cross {
            0.012 * normalize_attr(mate_attrs.technical.heading)
        } else { 0.0 };
        let safe_recycle = 0.008 * time_wasting * (1.0 - target_pressure);
        let gk_cost = if mate.is_goalkeeper && carrier_progress > 35.0 { 0.025 } else { 0.0 };
        let offside_penalty = offside_line.map_or(0.0, |line| {
            let beyond_m = (mate_progress - line - 0.001).max(0.0);
            if beyond_m <= 0.0 { 0.0 } else {
                (0.25 + 0.75 * (beyond_m / 3.0).min(1.0)) * (0.12 + 0.54 * timing_awareness)
            }
        });
        let utility = ActionValue {
            success,
            benefit: destination + support + transition + cross_option + safe_recycle,
            loss: (loss + turnover_cost(mate_progress)) * 0.5,
            cost: urgency * (0.003 + dist_m / 20.0 * 0.004) + gk_cost + offside_penalty,
        }.expected() - current_value;
        let risk = 1.0 - success;

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
        // Perceived shot quality: proximity, angle, technique and pressure.
        // This is a policy estimate, not the stochastic shot resolver.
        let central = (1.0 - (carrier.pos.y - 34.0).abs() / 28.0 * 0.55).clamp(0.38,1.0);
        let distance_quality = if in_box {
            (0.38 - goal_dist * 0.013).clamp(0.08,0.38)
        } else { (0.14 - (goal_dist - 16.5).max(0.0) * 0.0065).clamp(0.02,0.14) };
        // The policy estimates the same *sequence* of outcomes as execution,
        // without using RNG or the opponent's hidden attribute values.
        let one_v_one = in_box && goal_dist<=16.5 && carrier_ctx.nearest_defender_dist_m>=3.8;
        let shot_quality = (distance_quality * central * (0.72 + 0.56 * fin)
            * (1.0 - pressure_norm * (0.30 - composure * 0.14))
            * (0.8 + 0.2 * readiness) * if one_v_one {1.32} else {1.0}).clamp(0.015,0.72);
        let goal_center=crate::v4::spatial::attacking_goal_center(att_side);
        let (block_risk,_)=evaluate_passing_lane(state,input,att_side,carrier.pos,goal_center,false);
        let unblocked=if one_v_one {1.0} else {1.0-0.60*block_risk};
        let on_target=(0.34+0.34*fin-pressure_norm*(0.30-composure*0.14)*0.55).clamp(0.18,0.78);
        // Neutral keeper prior, not omniscient knowledge of GK attributes.
        let conditional_goal=(shot_quality*(1.38-0.60*0.72)).clamp(0.06,0.82);
        let success=shot_goal_probability(unblocked,on_target,conditional_goal);
        // Only a non-scoring on-target shot can plausibly yield a rebound
        // or corner. A rough continuation prior, not guaranteed retention.
        let retain_if_miss=(unblocked*on_target*(1.0-conditional_goal)*0.25
            / (1.0-success).max(0.01)).clamp(0.0,1.0);
        let shot_utility=shot_value(success,retain_if_miss,current_value,
            turnover_cost(99.5),current_value,0.004*urgency+0.01*time_wasting);
        candidates.push(CandidateOption {
            action: PlannedActionV4::Shoot {
                distance_m: goal_dist,
                in_box,
            },
            utility: shot_utility,
            risk: 1.0 - success,
        });
    }

    if !carrier.is_goalkeeper {
        let drib = normalize_attr(attrs.technical.dribbling);
        let agility = normalize_attr(attrs.physical.agility);
        let accel = normalize_attr(attrs.physical.acceleration);
        // Evaluate the forward carry the current engine can actually execute.
        // Do not award value for an imagined lateral dribble before 5.4B.
        let dir = if att_side == crate::v4::spatial::TeamSideV4::Home {1.0} else {-1.0};
        let reach = crate::v4::movement::player_max_speed_mps(attrs, carrier) * 0.88 * 1.6;
        let target = Vec2::new(carrier.pos.x + dir * reach, carrier.pos.y).clamp_to_pitch();
        let end_ctx = evaluate_local_context(state,input,att_side,target);
        let (corridor_risk, _) = evaluate_passing_lane(state,input,att_side,carrier.pos,target,false);
        let target_pressure = end_ctx.pressure / 100.0;
        let progress = attacking_progress_m(att_side,target);
        let destination = possession_value(progress,target.y,target_pressure);
        let success = (0.70 + 0.20 * drib + 0.08 * agility + 0.05 * accel
            - 0.40 * corridor_risk - 0.25 * pressure_norm - 0.15 * target_pressure
            - 0.12 * (1.0-readiness)).clamp(0.06,0.98);
        let drib_utility = ActionValue {
            success, benefit: destination + counter * ((progress-carrier_progress)/12.0).clamp(0.0,1.0),
            loss: (loss+turnover_cost(progress))*0.5, cost: urgency * 0.008 + 0.006*(1.0-readiness),
        }.expected()-current_value;
        candidates.push(CandidateOption {
            action: PlannedActionV4::Dribble {
                defender_idx: carrier_ctx.nearest_defender_idx,
            },
            utility: drib_utility,
            risk: 1.0 - success,
        });
    }

    if carrier_progress < 28.0 && pressure_norm > 0.58 {
        // Clearance sacrifices possession for territorial safety. Value it
        // only when the immediate danger is greater than the loss of control.
        let success = (0.70 + 0.20 * normalize_attr(attrs.technical.technique)
            - 0.15 * pressure_norm).clamp(0.05,0.98);
        candidates.push(CandidateOption {
            action: PlannedActionV4::Clearance,
            // A clearance generally hands possession to the opponent,
            // but farther from our goal if it succeeds.
            utility: -(success*turnover_cost((carrier_progress+32.0).min(100.0))
                +(1.0-success)*loss)-current_value-0.006,
            risk: 1.0 - success,
        });
    }
    // Hold is a real alternative, but standing still has no free forward gain.
    let hold_success = (0.95 - pressure_norm * (0.52 - 0.18 * composure)
        - 0.08 * (1.0-readiness)).clamp(0.05,0.98);
    candidates.push(CandidateOption {
        action: PlannedActionV4::HoldAndRecycle,
        utility: ActionValue {
            success: hold_success,
            benefit: current_value + 0.006 * (1.0-pressure_norm) * composure
                + 0.012 * time_wasting + if carrier_ctx.overload {0.004*teamwork} else {0.0},
            loss, cost: urgency * 0.012,
        }.expected()-current_value,
        risk: 1.0 - hold_success,
    });

    let mental_pressure = normalize_attr(attrs.hidden.pressure);
    // Uncertainty is expressed in the same goal-equivalent units.
    let noise_amplitude = (0.025 * (1.0-decisions*0.78)
        + 0.012 * pressure_norm * (1.0-(composure*0.5+mental_pressure*0.5)*0.65))
        .clamp(0.002,0.04);
    // Shared category uncertainty avoids an automatic lottery advantage
    // from having ten passing options versus one carry or one shot.
    let pass_noise = (rng.next_double() as f32 - 0.5) * 2.0 * noise_amplitude;
    let mut best_action = candidates[0].action;
    let mut best_score = f32::NEG_INFINITY;
    for cand in &candidates {
        let jitter = (rng.next_double() as f32 - 0.5) * 2.0 * noise_amplitude;
        let noise = if matches!(cand.action, PlannedActionV4::Pass { .. }) {
            pass_noise * 0.85 + jitter * 0.15
        } else { jitter };
        let creative_risk = flair * 0.004 * cand.risk;
        let score = cand.utility + creative_risk + noise;
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
