use crate::{
    geometry::{self, TeamSide},
    input::MatchInput,
    possession_decision::{self, PassDecision},
    rng::DeterministicRng,
    roster,
    shot_model::ShotContextV2,
    state::{MatchState, TacticalPhase},
    tactical_models::{self, MatchDecisionContext},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Pass,
    ThroughBall,
    Carry,
    Dribble,
    Cross,
    Shot,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::ThroughBall => "THROUGH_BALL",
            Self::Carry => "CARRY",
            Self::Dribble => "DRIBBLE",
            Self::Cross => "CROSS",
            Self::Shot => "SHOT",
        }
    }

    pub fn prior(self) -> f64 {
        match self {
            Self::Pass => 1.08,
            Self::ThroughBall => 0.85,
            Self::Carry => 0.52,
            Self::Dribble => 0.34,
            Self::Cross => 1.05,
            Self::Shot => 0.625,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub action: Action,
    pub immediate: f64,
    pub future: f64,
    pub perception_adjustment: f64,
    pub perceived: f64,
    pub weight: f64,
}

impl Candidate {
    pub fn new(action: Action, immediate: f64, future: f64, perception_adjustment: f64) -> Self {
        Self {
            action,
            immediate,
            future,
            perception_adjustment,
            perceived: 0.0,
            weight: 0.0,
        }
    }

    pub fn true_utility(&self) -> f64 {
        self.immediate + self.future
    }
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub action: Action,
    pub true_utility: f64,
    pub perceived_utility: f64,
    pub utility_summary: String,
    pub regret: f64,
    pub evaluation_error: f64,
}

fn calibrated(value: i32) -> f64 {
    74.0 + (value - 74) as f64 * 0.62
}

fn add_candidate(out: &mut Vec<Candidate>, action: Action, immediate: f64, future: f64, perception_adjustment: f64) {
    out.push(Candidate::new(action, immediate, future.max(0.0), perception_adjustment));
}

pub fn choose(
    input: &MatchInput,
    state: &MatchState,
    rng: &mut DeterministicRng,
    pressure: i32,
    pass: &PassDecision,
) -> Result<Selection, &'static str> {
    let side = state.ball.owner_team.ok_or("decision v2 missing context")?;
    let actor_id = state.ball.owner_player_id.as_deref().ok_or("decision v2 missing context")?;
    let team = input.team(side);
    let actor = team.player(actor_id).ok_or("decision v2 missing context")?;
    let runtime = state.player(actor_id).ok_or("decision v2 missing context")?;
    let slot = roster::slot_instructions(state, side, actor_id).ok_or("decision v2 missing context")?;

    let a = &actor.attributes;
    let instructions = &state.team(side).team_instructions;
    let match_ctx = MatchDecisionContext::from(input, state, side);
    let phase = state.possession.phase;
    let transition_exposure = tactical_models::defensive_disorganization(state, side.opposite());
    let progress = geometry::attacking_progress(side, state.ball.y);
    let control_bias = (tactical_models::effective_time_wasting(input, state, side) - 50).max(0) as f64 * 0.020;
    let mut candidates: Vec<Candidate> = Vec::with_capacity(6);

    let pass_completion_chance = possession_decision::completion_chance(input, state, pass, pressure);
    let build_up_risk = instructions.build_up_risk;
    let sequence_passes = state.possession.completed_passes_in_sequence;
    let circ_bonus = circulation_bonus(
        build_up_risk,
        instructions.tempo,
        sequence_passes,
        phase == TacticalPhase::BoxAttack,
        state.possession.transition_active,
    );
    let mut pass_immediate = 26.8
        + calibrated(a.get(2) as i32) * 0.095
        + pass.progress_gain.max(0) as f64 * 0.26
        - (-pass.progress_gain).max(0) as f64 * 0.10
        - pass.lane_risk as f64 * 0.055
        - pass.cover_shadow as f64 * 0.010
        - pressure as f64 * 0.075;
    if pass.progress_gain < 0 && progress > 72 {
        pass_immediate -= 4.0;
    }
    if pass.progress_gain <= 3 {
        pass_immediate += control_bias;
    } else {
        pass_immediate -= control_bias * 0.20;
    }
    pass_immediate += (pass_completion_chance - 77) as f64 * 0.15 + circ_bonus;
    pass_immediate += match_ctx.action_adjustment(Action::Pass, pass.progress_gain);
    let tactical_foresight_opp = if state.possession.transition_active {
        1.0
    } else if matches!(phase, TacticalPhase::FinalThird | TacticalPhase::BoxAttack) {
        0.55
    } else {
        0.24
    };
    let dec = a.get(13) as i32;
    let vis = a.get(12) as i32;
    add_candidate(
        &mut candidates,
        Action::Pass,
        pass_immediate.max(5.0),
        pass.next_action_potential * 0.085 * tactical_foresight_opp,
        match_ctx.perceived_match_state_adjustment(Action::Pass, pass.progress_gain, dec)
            - match_ctx.action_adjustment(Action::Pass, pass.progress_gain),
    );

    if state.ball.movement_intent == "KICKOFF_RESTART"
        || state.team(side).designated_goalkeeper_id.as_deref() == Some(actor_id)
    {
        return select(
            &mut candidates,
            dec,
            vis,
            pressure,
            runtime.energy,
            roster::effective_morale(input, state, side, actor_id).clamp(1, 99),
            actor.form,
            rng,
        );
    }

    let advanced = matches!(
        phase,
        TacticalPhase::Progression | TacticalPhase::FinalThird | TacticalPhase::BoxAttack
    );
    if advanced && pass.progress_gain >= 7 {
        let mut through_immediate = 4.8
            + slot.passing_risk as f64 * 0.070
            + calibrated(vis) * 0.075
            + pass.progress_gain as f64 * 0.52
            - pass.lane_risk as f64 * 0.065
            - pressure as f64 * 0.08;
        if state.possession.transition_active {
            through_immediate +=
                2.75 + (instructions.transition_risk - 50) as f64 * 0.030 + transition_exposure as f64 * 0.35;
        }
        through_immediate -= control_bias * 0.45;
        through_immediate -= (82 - pass_completion_chance).max(0) as f64 * 0.12;
        through_immediate -= circ_bonus * 0.28;
        through_immediate += (instructions.transition_risk - 56).max(0) as f64 * 0.19;
        through_immediate += match_ctx.action_adjustment(Action::ThroughBall, pass.progress_gain);
        add_candidate(
            &mut candidates,
            Action::ThroughBall,
            through_immediate.max(1.0),
            pass.next_action_potential * 0.18 * tactical_foresight_opp.max(0.55),
            match_ctx.perceived_match_state_adjustment(Action::ThroughBall, pass.progress_gain, dec)
                - match_ctx.action_adjustment(Action::ThroughBall, pass.progress_gain),
        );
    }

    let p_pos = actor.primary_position.as_str();
    let is_winger = matches!(p_pos, "LW" | "RW" | "LM" | "RM");
    let shot_ctx = if progress >= 58 {
        Some(ShotContextV2::build(input, state, side, actor_id, false, pressure, "DECISION_SHOT"))
    } else {
        None
    };
    let shot_opp = shot_ctx.as_ref().map(|s| s.expected_goals).unwrap_or(0.0);

    let mut carry_immediate = 9.0
        + slot.forward_movement as f64 * 0.105
        + calibrated(a.get(8) as i32) * 0.040
        + calibrated(a.get(4) as i32) * 0.040
        - pressure as f64 * 0.11
        + if state.possession.transition_active {
            4.0 + transition_exposure as f64 * 0.20
        } else {
            0.0
        }
        - control_bias * 0.30
        + match_ctx.action_adjustment(Action::Carry, 0);
    if is_winger && (50..=82).contains(&progress) {
        carry_immediate += 2.4;
    }
    if progress > 82 {
        carry_immediate -= 5.0;
    }
    if shot_opp >= 0.035 {
        carry_immediate -= (shot_opp - 0.025) * 45.0;
    }
    let carry_future = ((82 - progress).max(0) as f64 * 0.025 + (55 - pressure).max(0) as f64 * 0.020)
        * (0.70 + tactical_foresight_opp * 0.30);
    add_candidate(
        &mut candidates,
        Action::Carry,
        carry_immediate.max(2.0),
        carry_future,
        match_ctx.perceived_match_state_adjustment(Action::Carry, 0, dec) - match_ctx.action_adjustment(Action::Carry, 0),
    );

    if progress >= 46 {
        let mut dribble_immediate = 4.0
            + slot.dribbling_tendency as f64 * 0.115
            + calibrated(a.get(5) as i32) * 0.105
            + calibrated(a.get(9) as i32) * 0.030
            + calibrated(a.get(14) as i32) * 0.020
            - pressure as f64 * 0.19
            - control_bias * 0.20
            + transition_exposure as f64 * 0.15
            + match_ctx.action_adjustment(Action::Dribble, 0);
        if is_winger && (54..=84).contains(&progress) {
            dribble_immediate += 2.8;
        }
        if progress >= 70 {
            dribble_immediate += 3.0;
        }
        if shot_opp >= 0.035 {
            dribble_immediate -= (shot_opp - 0.025) * 52.0;
        }
        let dribble_future = ((80 - pressure).max(0) as f64 * 0.018 + (88 - progress).max(0) as f64 * 0.016)
            * (0.70 + tactical_foresight_opp * 0.30);
        add_candidate(
            &mut candidates,
            Action::Dribble,
            dribble_immediate.max(1.0),
            dribble_future,
            match_ctx.perceived_match_state_adjustment(Action::Dribble, 0, dec)
                - match_ctx.action_adjustment(Action::Dribble, 0),
        );
    }

    if progress >= 42 && geometry::wide(state.ball.x) {
        let mut cross_immediate = 7.75
            + (progress - 60).max(0) as f64 * 0.32
            + (slot.crossing_tendency - 50) as f64 * 0.200
            + calibrated(a.get(3) as i32) * 0.105
            + calibrated(a.get(4) as i32) * 0.025
            - pressure as f64 * 0.12
            + (instructions.width - 67).max(0) as f64 * 0.35
            + match_ctx.action_adjustment(Action::Cross, 0);
        if matches!(p_pos, "LW" | "RW") && progress < 76 && instructions.width < 66 {
            cross_immediate -= (76 - progress) as f64 * 0.24;
        }
        let cross_future = best_box_threat(input, state, side, actor_id) * 0.018 * (0.75 + tactical_foresight_opp * 0.25);
        add_candidate(
            &mut candidates,
            Action::Cross,
            cross_immediate.max(1.0),
            cross_future,
            match_ctx.perceived_match_state_adjustment(Action::Cross, 0, dec)
                - match_ctx.action_adjustment(Action::Cross, 0),
        );
    }

    if progress >= 62 {
        if let Some(ref sc) = shot_ctx {
            let geometry_value = sc.expected_goals * 95.0 + (sc.goal_angle_deg - 28.0).max(0.0) * 0.055;
            let decision_shot_value = (dec - 74) as f64 * (shot_opp * 0.45 - 0.015);
            let mut shoot_immediate = 8.15
                + geometry_value
                + (slot.shooting_tendency - 50) as f64 * 0.085
                + calibrated(a.get(6) as i32) * 0.060
                + calibrated(a.get(14) as i32) * 0.035
                + calibrated(dec) * 0.018
                + decision_shot_value
                - pressure as f64 * 0.105
                + match_ctx.action_adjustment(Action::Shot, 0);
            if sc.one_vs_one {
                shoot_immediate += 8.0 + (dec - 50).max(0) as f64 * 0.12;
            }
            if sc.blockers >= 2 {
                shoot_immediate -= 3.0;
            }
            if sc.goal_angle_deg < 20.0 {
                shoot_immediate *= 0.62;
            }
            if !sc.one_vs_one {
                let mut penalty = weak_shot_penalty(
                    shot_opp,
                    pass_completion_chance,
                    pass.next_action_potential,
                    build_up_risk,
                    match_ctx.urgency,
                );
                if input.simulated_action_minutes() < 90 {
                    penalty *= 0.35;
                }
                shoot_immediate -= penalty;
            }
            let actor_slot = state.team(side).tactical_slot(&runtime.slot_id);
            let base_in_prog = match actor_slot {
                None => 75,
                Some(s) => geometry::attacking_progress(side, geometry::oriented_y(side, s.in_y)),
            };
            if sequence_passes >= 1 {
                if base_in_prog <= 52 && progress < 82 && !sc.one_vs_one {
                    shoot_immediate -= 2.8;
                } else if (56..85).contains(&base_in_prog)
                    && progress >= 62
                    && pressure <= 42
                    && sc.goal_angle_deg >= 12.0
                {
                    shoot_immediate += if p_pos == "AM" && progress < 76 { 1.8 } else { 3.4 };
                } else if base_in_prog >= 85 && !sc.one_vs_one && pressure >= 18 {
                    shoot_immediate -= 1.4;
                }
            }
            if matches!(p_pos, "CB" | "SW" | "DM" | "LB" | "RB") && progress < 83 && !sc.one_vs_one {
                shoot_immediate -= 2.5;
            } else if matches!(p_pos, "LW" | "RW") && progress >= 66 {
                shoot_immediate += if (state.ball.x - 50).abs() <= 32 { 3.2 } else { 1.8 };
            } else if matches!(p_pos, "AM" | "CM" | "LM" | "RM") && progress >= 66 {
                shoot_immediate += 1.2;
            } else if matches!(p_pos, "CF" | "F") && progress >= 76 {
                shoot_immediate += 1.8;
            }
            if input.simulated_action_minutes() < 90 && progress >= 64 {
                shoot_immediate += 4.8 + (progress - 68).max(0) as f64 * 0.18;
            }
            add_candidate(
                &mut candidates,
                Action::Shot,
                shoot_immediate.max(0.15),
                0.0,
                match_ctx.perceived_match_state_adjustment(Action::Shot, 0, dec)
                    - match_ctx.action_adjustment(Action::Shot, 0),
            );
        }
    }

    let team_stability = roster::leadership_stability_bonus(input, state, side)
        + roster::concentration_stability_bonus(state, side);
    let selection_morale = (roster::effective_morale(input, state, side, actor_id) + team_stability * 2).clamp(1, 99);
    select(
        &mut candidates,
        dec,
        vis,
        pressure,
        runtime.energy,
        selection_morale,
        actor.form,
        rng,
    )
}

fn best_box_threat(input: &MatchInput, state: &MatchState, side: TeamSide, actor_id: &str) -> f64 {
    let team = input.team(side);
    let mut best = 0.0_f64;
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
        if prog < 68 {
            continue;
        }
        let threat = prog as f64 * 0.45
            + p.attributes.get(15) as f64 * 0.18
            + p.attributes.get(7) as f64 * 0.14
            + p.attributes.get(14) as f64 * 0.08;
        best = best.max(threat);
    }
    best
}

#[allow(clippy::too_many_arguments)]
pub fn select(
    candidates: &mut [Candidate],
    decisions: i32,
    vision: i32,
    pressure: i32,
    energy: i32,
    morale: i32,
    form: i32,
    rng: &mut DeterministicRng,
) -> Result<Selection, &'static str> {
    if candidates.is_empty() {
        return Err("decision v2 needs candidates");
    }
    let d = decisions.clamp(1, 99);
    let v = vision.clamp(1, 99);
    let foresight = (0.28 + d as f64 * 0.0070 + v as f64 * 0.0015).clamp(0.30, 1.0);
    let mental = ((morale + form) / 2).clamp(1, 99);
    let noise_amplitude = 1.4
        + (100 - d) as f64 * 0.105
        + pressure as f64 * 0.020
        + (55 - energy).max(0) as f64 * 0.045
        + (50 - mental).max(0) as f64 * 0.025;
    let mut evaluation_error = 0.0;
    for c in candidates.iter_mut() {
        let noise = (rng.next_double() * 2.0 - 1.0) * noise_amplitude;
        c.perceived = c.immediate + c.future * foresight + c.perception_adjustment + noise;
        evaluation_error += (c.perceived - c.true_utility()).abs();
    }
    let sharpness = if d >= 74 {
        2.0 + (d - 74) as f64 * 0.035
    } else {
        2.0 - (74 - d) as f64 * 0.015
    };
    let mut total = 0.0;
    for c in candidates.iter_mut() {
        c.weight = 0.01_f64.max(c.perceived.max(0.25).powf(sharpness) * c.action.prior());
        total += c.weight;
    }
    let draw = rng.next_double() * total;
    let mut acc = 0.0;
    let mut chosen = candidates.len() - 1;
    for (i, c) in candidates.iter().enumerate() {
        acc += c.weight;
        if draw < acc {
            chosen = i;
            break;
        }
    }
    let selected = &candidates[chosen];
    let best = candidates.iter().map(|c| c.true_utility()).fold(f64::NEG_INFINITY, f64::max);
    let mut order: Vec<&Candidate> = candidates.iter().collect();
    order.sort_by(|a, b| {
        b.true_utility()
            .total_cmp(&a.true_utility())
            .then_with(|| a.action.name().cmp(b.action.name()))
    });
    let mut summary = String::new();
    for c in order {
        if !summary.is_empty() {
            summary.push('|');
        }
        summary.push_str(c.action.name());
        summary.push('=');
        summary.push_str(&((c.true_utility() + 0.5).floor() as i64).to_string());
    }
    Ok(Selection {
        action: selected.action,
        true_utility: selected.true_utility(),
        perceived_utility: selected.perceived,
        utility_summary: summary,
        regret: (best - selected.true_utility()).max(0.0),
        evaluation_error: evaluation_error / candidates.len() as f64,
    })
}

pub fn circulation_bonus(
    build_risk: i32,
    tempo: i32,
    sequence_passes: i32,
    box_attack: bool,
    transition: bool,
) -> f64 {
    if transition || box_attack {
        return 0.0;
    }
    let patient = (60 - build_risk).max(0) as f64 * 0.060 + (55 - tempo).max(0) as f64 * 0.040;
    let early = (1.0 - sequence_passes as f64 / 7.0).max(0.0);
    4.2_f64.min(patient * early)
}

pub fn weak_shot_penalty(
    xg: f64,
    pass_chance: i32,
    next_potential: f64,
    build_risk: i32,
    urgency: f64,
) -> f64 {
    let low_quality = (0.105 - xg).max(0.0) / 0.105;
    let alternative = ((pass_chance - 65) as f64 / 32.0).max(0.0);
    let future = (next_potential / 18.0).clamp(0.0, 1.0);
    let restraint = 7.0 + 2.4 * alternative + 1.4 * future + (55 - build_risk).max(0) as f64 * 0.065;
    low_quality * restraint * (1.0 - 0.62_f64.min(urgency * 0.62))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_select_rust_determinism() {
        let mut c = vec![
            Candidate::new(Action::Pass, 15.0, 6.0, 0.0),
            Candidate::new(Action::Shot, 13.0, 1.0, 0.0),
        ];
        let mut r = DeterministicRng::new(101);
        let s = select(&mut c, 80, 70, 35, 80, 70, 70, &mut r).unwrap();
        assert!(s.action == Action::Pass || s.action == Action::Shot);
        assert!(s.utility_summary.contains("PASS=21"));
    }
}
