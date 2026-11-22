use crate::{
    contextual::{self, PassContext},
    decision::{self, Action},
    geometry::{self, TeamSide},
    input::MatchInput,
    passing_lane::{self, Evaluation as LaneEvaluation},
    pressure,
    rng::DeterministicRng,
    roster,
    state::{MatchState, TacticalPhase},
    tactical_models::{self, MatchDecisionContext},
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

#[derive(Clone, Debug)]
pub struct PassDecision {
    pub passer_id: String,
    pub target_id: String,
    pub intention: String,
    pub discovered_options: i32,
    pub distance: f64,
    pub target_pressure: i32,
    pub progress_gain: i32,
    pub tactical_risk: i32,
    pub utility: f64,
    pub lane_risk: i32,
    pub cover_shadow: i32,
    pub lane_interceptor_id: Option<String>,
    pub lane_interception_chance: i32,
    pub lane_intercept_x: i32,
    pub lane_intercept_y: i32,
    pub lane_blocker_count: i32,
    pub next_action_potential: f64,
}

#[derive(Clone, Debug)]
struct PassOption {
    target_id: String,
    distance: f64,
    target_pressure: i32,
    progress_gain: i32,
    lane: LaneEvaluation,
    next_action_potential: f64,
}

pub fn choose_pass(
    input: &MatchInput,
    state: &MatchState,
    rng: &mut DeterministicRng,
    carrier_pressure: i32,
) -> Result<PassDecision, &'static str> {
    let side = state.ball.owner_team.ok_or("decision needs controlled ball")?;
    let passer_id = state.ball.owner_player_id.as_deref().ok_or("decision needs controlled ball")?;
    let team = input.team(side);
    let passer = team.player(passer_id).ok_or("passer missing")?;
    let passer_runtime = state.player(passer_id).ok_or("passer missing")?;

    let attributes = &passer.attributes;
    let collective = &team.collective_state;
    let match_ctx = MatchDecisionContext::from(input, state, side);
    let mut discovered: Vec<PassOption> = Vec::new();
    let mut all: Vec<PassOption> = Vec::new();
    let lane_analyzer = passing_lane::Analyzer::new(input, state, side, passer_id);
    let ids = roster::active_player_ids_in_slot_order(input, state, side);

    for target_id in ids {
        if target_id == passer_id {
            continue;
        }
        let target_runtime = match state.player(&target_id) {
            Some(r) if r.on_pitch => r,
            _ => continue,
        };
        if team.player(&target_id).is_none() {
            continue;
        }
        let distance = geometry::distance(passer_runtime.x, passer_runtime.y, target_runtime.x, target_runtime.y);
        let target_pressure = pressure::estimate_at_point(input, state, side.opposite(), target_runtime.x, target_runtime.y);
        let progress_gain = geometry::progress_delta(side, state.ball.y, target_runtime.y);
        let lane = lane_analyzer.evaluate_target(state, &target_id);
        let next_potential = next_action_potential(input, state, side, passer_id, &target_id, target_runtime, target_pressure, progress_gain);
        let option = PassOption {
            target_id: target_id.clone(),
            distance,
            target_pressure,
            progress_gain,
            lane,
            next_action_potential: next_potential,
        };

        let vision = attributes.get(12) as f64;
        let mut perception = vision * 0.72
            + attributes.get(13) as f64 * 0.055
            + collective.system_familiarity as f64 * 0.075
            + collective.cohesion as f64 * 0.045
            - distance * 0.45
            - carrier_pressure as f64 * 0.14
            - option.lane.cover_shadow as f64 * 0.030
            + (next_potential * 0.075).min(2.0) * (0.45 + vision * 0.0055)
            + tactical_models::home_perception_adjustment(input, side);
        let perception_noise = 2.0 + (100.0 - vision) * 0.085 + carrier_pressure as f64 * 0.015;
        perception += (rng.next_double() * 2.0 - 1.0) * perception_noise;
        if perception >= 27.0 {
            discovered.push(option.clone());
        }
        all.push(option);
    }

    if discovered.is_empty() {
        if all.is_empty() {
            return Err("no pass target");
        }
        all.sort_by(|a, b| a.distance.total_cmp(&b.distance).then_with(|| a.target_id.cmp(&b.target_id)));
        discovered.push(all[0].clone());
    }

    let instructions = &state.team(side).team_instructions;
    let effective_tw = tactical_models::effective_time_wasting(input, state, side);
    let passer_instr = roster::slot_instructions(state, side, passer_id);
    let mut tactical_risk = instructions.build_up_risk;
    if state.possession.transition_active {
        tactical_risk = (tactical_risk + instructions.transition_risk * 2) / 3;
    }
    if let Some(pi) = passer_instr {
        tactical_risk = (tactical_risk * 2 + pi.passing_risk) / 3;
    }
    tactical_risk = (tactical_risk + (collective.confidence - 50) / 20).clamp(0, 100);
    tactical_risk -= (effective_tw - 50).max(0) / 6;
    tactical_risk += match_ctx.risk_adjustment();
    tactical_risk = (50 + java_round((tactical_risk - 50) as f64 * 0.50)).clamp(0, 100);

    let decisions = attributes.get(13) as i32;
    let mental_stability = roster::mental_stability(input, state, side, passer_id);
    let foresight = (0.14 + decisions as f64 * 0.0032 + attributes.get(12) as f64 * 0.0010).clamp(0.22, 0.62);
    let phase = state.possession.phase;
    let future_weight = if state.possession.transition_active {
        0.17
    } else if matches!(phase, TacticalPhase::FinalThird | TacticalPhase::BoxAttack) {
        0.10
    } else {
        0.070
    };
    let noise_amplitude = 1.4
        + (100 - decisions) as f64 * 0.105
        + carrier_pressure as f64 * 0.020
        + (55 - passer_runtime.energy).max(0) as f64 * 0.045
        + (50 - mental_stability).max(0) as f64 * 0.025;

    let mut best: Option<&PassOption> = None;
    let mut best_perceived = f64::NEG_INFINITY;
    let mut best_true = f64::NEG_INFINITY;
    let gk_id = state.team(side).designated_goalkeeper_id.as_deref();
    let own_prog_check = geometry::attacking_progress(side, state.ball.y);

    for option in &discovered {
        let target = match team.player(&option.target_id) {
            Some(t) => t,
            None => continue,
        };
        let tr = match state.player(&option.target_id) {
            Some(r) => r,
            None => continue,
        };
        let progress_weight = 0.80 + tactical_risk as f64 * 0.0075;
        let distance_penalty = 0.55 - tactical_risk as f64 / 500.0;
        let safety = (100 - tactical_risk) as f64;
        let waste_progress = (effective_tw - 50).max(0) as f64 * 0.0030;
        let mut immediate = option.progress_gain as f64 * (progress_weight - waste_progress)
            - option.distance * distance_penalty
            - option.target_pressure as f64 * 0.28
            - option.lane.lane_risk as f64 * 0.12
            - option.lane.cover_shadow as f64 * 0.025
            + target.attributes.get(15) as f64 * 0.08
            + target.attributes.get(4) as f64 * 0.035
            + target.attributes.get(13) as f64 * 0.035;
        if option.progress_gain < -3 {
            immediate += safety * 0.028;
        }
        if option.progress_gain > 3 {
            immediate -= option.progress_gain as f64 * safety * 0.035;
        }
        if gk_id == Some(option.target_id.as_str()) && carrier_pressure < 35 {
            immediate -= 3.2;
        }
        let intent = state.ball.movement_intent.as_str();
        if intent == format!("WALL_FROM:{}", option.target_id) && own_prog_check < 65 && option.progress_gain <= 3 {
            immediate -= 14.0;
        } else if intent == format!("PASS_FROM:{}", option.target_id) && own_prog_check < 55 && option.progress_gain <= 2 {
            immediate -= 2.4;
        }
        let t_slot = state.team(side).tactical_slot(&tr.slot_id);
        let t_base_in_prog = match t_slot {
            None => 60,
            Some(s) => geometry::attacking_progress(side, geometry::oriented_y(side, s.in_y)),
        };
        let t_pos = target.primary_position.as_str();
        let lateral_dx = (passer_runtime.x - tr.x).abs() as f64;
        let target_wide_channel = (tr.x - 50).abs() >= 22
            || matches!(t_pos, "LW" | "RW" | "LM" | "RM" | "LB" | "RB" | "LWB" | "RWB");
        if own_prog_check < 48 && geometry::wide(tr.x) && t_base_in_prog <= 48 && (-2..=14).contains(&option.progress_gain) {
            immediate += 1.8;
        }
        if target_wide_channel && (passer_runtime.x - 50).abs() <= 24 && option.target_pressure <= 36 && option.progress_gain >= -3 {
            immediate += (lateral_dx * 0.14).min(4.8) + if option.target_pressure <= 22 { 1.6 } else { 0.7 };
            if matches!(t_pos, "LW" | "RW" | "LM" | "RM") && own_prog_check >= 36 && option.progress_gain >= 0 {
                immediate += 2.1;
            }
        } else if (passer_runtime.x - 50).abs() >= 22 && (tr.x - 50).abs() <= 22 && own_prog_check >= 50 && option.progress_gain >= -5 {
            immediate += (lateral_dx * 0.14).min(4.5)
                + if geometry::attacking_progress(side, tr.y) >= 72 { 1.8 } else { 0.7 };
        }
        if own_prog_check >= 58 && state.possession.completed_passes_in_sequence >= 1 {
            if t_base_in_prog >= 85 && option.target_pressure >= 18 {
                immediate -= 3.6;
            } else if (56..85).contains(&t_base_in_prog) && option.target_pressure <= 34 {
                immediate += 0.6;
            }
        }
        if phase == TacticalPhase::FinalThird {
            immediate += option.progress_gain.max(0) as f64 * 0.25;
        }
        if phase == TacticalPhase::BoxAttack {
            immediate -= (option.distance - 18.0).max(0.0) * 0.25;
        }

        let predicted = PassDecision {
            passer_id: passer_id.into(),
            target_id: option.target_id.clone(),
            intention: "EVALUATE".into(),
            discovered_options: discovered.len() as i32,
            distance: option.distance,
            target_pressure: option.target_pressure,
            progress_gain: option.progress_gain,
            tactical_risk,
            utility: 0.0,
            lane_risk: option.lane.lane_risk,
            cover_shadow: option.lane.cover_shadow,
            lane_interceptor_id: option.lane.interceptor_id.clone(),
            lane_interception_chance: option.lane.interception_chance,
            lane_intercept_x: option.lane.intercept_x,
            lane_intercept_y: option.lane.intercept_y,
            lane_blocker_count: option.lane.blocker_count,
            next_action_potential: option.next_action_potential,
        };
        let completion = completion_chance(input, state, &predicted, carrier_pressure) as f64 / 100.0;
        let mut retain_weight = 0.85
            + (60 - instructions.build_up_risk).max(0) as f64 * 0.016
            + (55 - instructions.tempo).max(0) as f64 * 0.009;
        if state.possession.transition_active {
            retain_weight *= 0.82;
        }
        let loss_cost = 9.0 + (48 - own_prog_check).max(0) as f64 * 0.15;
        let reliability = (completion - 0.75) * 23.0 * retain_weight
            - (1.0 - completion) * loss_cost * retain_weight * 0.48;
        let true_score = immediate * 0.78
            + option.next_action_potential * future_weight
            + reliability
            + option.progress_gain.max(0) as f64 * completion * 0.14;
        let perceived = immediate * 0.78
            + option.next_action_potential * future_weight * foresight
            + reliability
            + option.progress_gain.max(0) as f64 * completion * 0.14
            + (rng.next_double() * 2.0 - 1.0) * noise_amplitude;
        if best.is_none() || perceived > best_perceived || (perceived == best_perceived && option.target_id < best.unwrap().target_id) {
            best = Some(option);
            best_perceived = perceived;
            best_true = true_score;
        }
    }

    let best = best.ok_or("decision without target")?;
    let intention = if best.progress_gain > 5 {
        "PROGRESS"
    } else if best.progress_gain < -5 {
        "RECYCLE"
    } else {
        "CONNECT"
    };
    Ok(PassDecision {
        passer_id: passer_id.into(),
        target_id: best.target_id.clone(),
        intention: intention.into(),
        discovered_options: discovered.len() as i32,
        distance: best.distance,
        target_pressure: best.target_pressure,
        progress_gain: best.progress_gain,
        tactical_risk,
        utility: best_true,
        lane_risk: best.lane.lane_risk,
        cover_shadow: best.lane.cover_shadow,
        lane_interceptor_id: best.lane.interceptor_id.clone(),
        lane_interception_chance: best.lane.interception_chance,
        lane_intercept_x: best.lane.intercept_x,
        lane_intercept_y: best.lane.intercept_y,
        lane_blocker_count: best.lane.blocker_count,
        next_action_potential: best.next_action_potential,
    })
}

#[allow(clippy::too_many_arguments)]
fn next_action_potential(
    input: &MatchInput,
    state: &MatchState,
    side: TeamSide,
    passer_id: &str,
    target_id: &str,
    target_runtime: &crate::state::PlayerRuntimeState,
    target_pressure: i32,
    progress_gain: i32,
) -> f64 {
    let team = input.team(side);
    let target_prog = geometry::attacking_progress(side, target_runtime.y);
    let mut value = progress_gain.max(0) as f64 * 0.24 + (65 - target_pressure).max(0) as f64 * 0.055;
    if target_prog >= 70 && geometry::central(target_runtime.x) {
        value += 2.5;
    }
    if target_prog >= 28 && geometry::wide(target_runtime.x) {
        value += 6.5;
    }
    let mut best_cont = 0.0_f64;
    for id in roster::active_player_ids_in_slot_order(input, state, side) {
        if id == passer_id || id == target_id {
            continue;
        }
        let r = match state.player(&id) {
            Some(r) if r.on_pitch => r,
            _ => continue,
        };
        let p = match team.player(&id) {
            Some(p) => p,
            None => continue,
        };
        let gain = geometry::progress_delta(side, target_runtime.y, r.y);
        if gain <= 2 {
            continue;
        }
        let dist = geometry::distance(target_runtime.x, target_runtime.y, r.x, r.y);
        if dist > 34.0 {
            continue;
        }
        let cont = gain as f64 * 0.22 - dist * 0.035
            + p.attributes.get(15) as f64 * 0.031
            + p.attributes.get(8) as f64 * 0.006;
        best_cont = best_cont.max(cont);
    }
    (value + best_cont).clamp(0.0, 22.0)
}

pub fn completion_chance(
    input: &MatchInput,
    state: &MatchState,
    decision: &PassDecision,
    carrier_pressure: i32,
) -> i32 {
    let side = match state.ball.owner_team {
        Some(s) => s,
        None => return 50,
    };
    let team = input.team(side);
    let passer = match team.player(&decision.passer_id) {
        Some(p) => p,
        None => return 50,
    };
    let target = team.player(&decision.target_id);
    let runtime = match state.player(&decision.passer_id) {
        Some(r) => r,
        None => return 50,
    };
    let slot = roster::slot_instructions(state, side, &decision.passer_id);
    let c = &team.collective_state;
    let t = &state.team(side).team_instructions;

    let ctx = PassContext {
        carrier_pressure,
        target_pressure: decision.target_pressure,
        distance: decision.distance,
        progress_gain: decision.progress_gain,
        tactical_risk: decision.tactical_risk,
        lane_risk: decision.lane_risk,
        cover_shadow: decision.cover_shadow,
    };
    let execution = 71.9
        + contextual::pass_execution_adjustment(&passer.attributes, &ctx)
        + (c.cohesion - 60) as f64 * 0.05 * 0.60
        + (c.system_familiarity - 60) as f64 * 0.03 * 0.45
        + (passer.form - 50) as f64 * 0.020
        + target.map(|tg| (tg.attributes.get(15) as f64 - 74.0) * 0.040).unwrap_or(0.0)
        + roster::team_mental_execution_adjustment(input, state, side) * 0.65
        + tactical_models::home_execution_adjustment(input, side);
    let distance_diff = (decision.distance - 8.0).max(0.0) * 0.34;
    let pressure_diff = carrier_pressure as f64 * 0.33 + decision.target_pressure as f64 * 0.12;
    let prog_diff = decision.progress_gain.max(0) as f64 * 0.07;
    let tempo_diff = (t.tempo - 50).max(0) as f64 * 0.045;
    let tempo_bonus = (50 - t.tempo).max(0) as f64 * 0.025;
    let energy_diff = (55 - runtime.energy).max(0) as f64 * 0.08;
    let slot_risk_diff = slot.map(|s| (s.passing_risk - 60).max(0) as f64 * 0.025).unwrap_or(0.0);
    let team_risk_diff = (decision.tactical_risk - 50).max(0) as f64 * 0.130;
    let risk_prog_diff = (decision.tactical_risk - 50).max(0) as f64 * decision.progress_gain.max(0) as f64 * 0.016;
    let lane_diff = decision.lane_risk as f64 * 0.10 + decision.cover_shadow as f64 * 0.020;
    let chance = java_round(
        execution + 27.0 + tempo_bonus
            - distance_diff
            - pressure_diff
            - prog_diff
            - tempo_diff
            - energy_diff
            - slot_risk_diff
            - team_risk_diff
            - risk_prog_diff
            - lane_diff,
    );
    chance.clamp(20, 97)
}

#[derive(Clone, Debug)]
pub struct ActionDecision {
    pub kind: Action,
    pub actor_id: String,
    pub target_id: Option<String>,
    pub intention: String,
    pub pass_decision: Option<PassDecision>,
    pub chosen_utility: f64,
    pub utility_summary: String,
}

pub fn choose_action(
    input: &MatchInput,
    state: &MatchState,
    rng: &mut DeterministicRng,
    pressure: i32,
) -> Result<ActionDecision, &'static str> {
    let side = state.ball.owner_team.ok_or("action needs controlled ball")?;
    let actor_id = state.ball.owner_player_id.clone().ok_or("action actor missing")?;
    if input.team(side).player(&actor_id).is_none() {
        return Err("action actor missing");
    }
    let pass = choose_pass(input, state, rng, pressure)?;
    let selection = decision::choose(input, state, rng, pressure, &pass)?;
    match selection.action {
        Action::Pass | Action::ThroughBall => Ok(ActionDecision {
            kind: selection.action,
            actor_id,
            target_id: Some(pass.target_id.clone()),
            intention: pass.intention.clone(),
            pass_decision: Some(pass),
            chosen_utility: selection.true_utility,
            utility_summary: selection.utility_summary,
        }),
        Action::Cross => match best_box_target(input, state, side, &actor_id) {
            None => Ok(ActionDecision {
                kind: Action::Carry,
                actor_id,
                target_id: None,
                intention: "CARRY".into(),
                pass_decision: None,
                chosen_utility: selection.true_utility,
                utility_summary: selection.utility_summary,
            }),
            Some(target) => Ok(ActionDecision {
                kind: Action::Cross,
                actor_id,
                target_id: Some(target),
                intention: "CROSS".into(),
                pass_decision: None,
                chosen_utility: selection.true_utility,
                utility_summary: selection.utility_summary,
            }),
        },
        other => Ok(ActionDecision {
            kind: other,
            actor_id,
            target_id: None,
            intention: other.name().into(),
            pass_decision: None,
            chosen_utility: selection.true_utility,
            utility_summary: selection.utility_summary,
        }),
    }
}

fn java_string_hash_code(s: &str) -> i32 {
    let mut h: i32 = 0;
    for unit in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as i32);
    }
    h
}

fn best_box_target(input: &MatchInput, state: &MatchState, side: TeamSide, actor_id: &str) -> Option<String> {
    let team = input.team(side);
    let mut best: Option<String> = None;
    let mut score = f64::NEG_INFINITY;
    let sec = state.clock.game_second;
    for id in roster::active_player_ids_in_slot_order(input, state, side) {
        if id == actor_id {
            continue;
        }
        let r = match state.player(&id) {
            Some(r) if r.on_pitch => r,
            _ => continue,
        };
        let p = match team.player(&id) {
            Some(p) => p,
            None => continue,
        };
        let prog = geometry::attacking_progress(side, r.y);
        let pos = p.primary_position.as_str();
        let pos_bonus = if matches!(pos, "CF" | "F") {
            4.5
        } else if matches!(pos, "LW" | "RW" | "AM") {
            2.5
        } else {
            0.0
        };

        let rot = if prog >= 60 {
            let h = java_string_hash_code(&r.slot_id).wrapping_mul(29).wrapping_add(sec.wrapping_mul(13));
            (h.unsigned_abs() % 9) as f64 * 0.85
        } else {
            0.0
        };
        let mut s = prog as f64 * 0.6
            + p.attributes.get(7) as f64 * 0.25
            + p.attributes.get(15) as f64 * 0.25
            - p.attributes.get(16) as f64 * 0.02
            + pos_bonus
            + rot;
        if r.current_task.starts_with("OFFBALL_") {
            s += 4.0;
        }
        if best.is_none() || s > score || (s == score && id < *best.as_ref().unwrap()) {
            best = Some(id);
            score = s;
        }
    }
    best
}
