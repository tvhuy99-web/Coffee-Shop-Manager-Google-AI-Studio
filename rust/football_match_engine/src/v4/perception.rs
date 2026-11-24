use serde::{Deserialize, Serialize};
use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite, PlayerAttributes54};
use crate::v4::ball_play::offside_reference_progress_m;
use crate::v4::input::MatchInputV4;
use crate::v4::movement::{
    evaluate_local_context, evaluate_passing_lane, player_attrs, LocalSpatialContext,
};
use crate::v4::rules::{estimate_reaction_decision_ms, TimingModelConfig};
use crate::v4::spatial::{
    attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box, PITCH_WIDTH_M,
};
use crate::v4::state::{MatchStateV4, TacticalPhaseV4};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassKindV4 {
    Short,
    Long,
    Switch,
    Through,
    Cross,
}

impl PassKindV4 {
    pub fn as_str(self) -> &'static str {
        match self {
            PassKindV4::Short => "SHORT",
            PassKindV4::Long => "LONG",
            PassKindV4::Switch => "SWITCH",
            PassKindV4::Through => "THROUGH",
            PassKindV4::Cross => "CROSS",
        }
    }

    pub fn is_aerial(self) -> bool {
        matches!(
            self,
            PassKindV4::Long | PassKindV4::Switch | PassKindV4::Cross
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlannedActionV4 {
    Pass {
        target_idx: usize,
        kind: PassKindV4,
        distance_m: f32,
        progress_gain_m: f32,
        lane_risk: f32,
        interceptor_idx: Option<usize>,
    },
    Dribble {
        defender_idx: Option<usize>,
    },
    HoldAndRecycle,
    Shoot {
        distance_m: f32,
        in_box: bool,
    },
    Clearance,
}

/// DES decision bundle pairing the chosen football action with its kinematic reaction/decision
/// delay (`decision_delay_ms`) and one-touch vs multi-touch execution intent (`is_one_touch`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecisionPlanV4 {
    pub action: PlannedActionV4,
    pub decision_delay_ms: i64,
    pub is_one_touch: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateOption {
    pub action: PlannedActionV4,
    pub utility: f32,
    pub risk: f32,
}

/// Evaluates whether the carrier intends a one-touch release vs taking a controlling touch.
///
/// Deliberately avoids the naive shortcut "better player = always faster":
/// - High `first_touch`, `vision`, `decisions`, high team `tempo`, and tight opponent pressure
///   increase one-touch intent to beat the press.
/// - Conversely, high `composure` + `decisions` under low-to-moderate pressure introduces
///   "la pausa" restraint — taking a beat to draw defenders and pick the optimal release window.
pub fn evaluate_one_touch_intent(
    action: &PlannedActionV4,
    attrs: &PlayerAttributes54,
    pressure_norm: f32,
    tempo: u8,
    time_wasting: u8,
    rng: &mut DeterministicRng,
) -> bool {
    let p_norm = pressure_norm.clamp(0.0, 1.0);
    let ft = normalize_attr(attrs.technical.first_touch);
    let vis = normalize_attr(attrs.mental.vision);
    let dec = normalize_attr(attrs.mental.decisions);
    let comp = normalize_attr(attrs.mental.composure);
    let tempo_norm = (tempo as f32 / 100.0).clamp(0.0, 1.0);
    let tw_norm = (time_wasting as f32 / 100.0).clamp(0.0, 1.0);

    match *action {
        PlannedActionV4::Dribble { .. } | PlannedActionV4::HoldAndRecycle => false,
        PlannedActionV4::Clearance => p_norm >= 0.42 || (rng.next_double() as f32) < 0.65,
        PlannedActionV4::Shoot { in_box, .. } => {
            if !in_box {
                return false;
            }
            // Composed finishers in space take a touch to set body shape; under pressure they strike first-time.
            let calm_set_restraint = (comp * 0.65 + dec * 0.35) * (1.0 - p_norm) * 0.30;
            let prob = (0.18 + ft * 0.26 + dec * 0.12 + p_norm * 0.30 + tempo_norm * 0.10
                - calm_set_restraint)
                .clamp(0.05, 0.78);
            (rng.next_double() as f32) < prob
        }
        PlannedActionV4::Pass {
            kind, distance_m, ..
        } => {
            if matches!(kind, PassKindV4::Switch) || distance_m > 34.0 {
                return false;
            }
            let kind_bias = match kind {
                PassKindV4::Short => 0.14,
                PassKindV4::Through => 0.08,
                PassKindV4::Cross => 0.05,
                PassKindV4::Long => -0.14,
                PassKindV4::Switch => -0.25,
            };
            // "La pausa": composed, high-decision players in space deliberately take a touch
            // to bait the press and wait for teammates' runs instead of rushing one-touch.
            let pausa_restraint = (comp * 0.60 + dec * 0.40)
                * (1.0 - p_norm)
                * (1.0 - tempo_norm * 0.65)
                * 0.38;
            let skill_readiness = ft * 0.34 + vis * 0.24 + dec * 0.16;
            let press_drive = p_norm * (0.24 + ft * 0.14);
            let prob = (0.08
                + kind_bias
                + skill_readiness * 0.36
                + tempo_norm * 0.22
                + press_drive
                - pausa_restraint
                - tw_norm * 0.18)
                .clamp(0.03, 0.72);
            (rng.next_double() as f32) < prob
        }
    }
}

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
