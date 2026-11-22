use crate::{
    contextual::{self, PressureContext},
    defensive,
    geometry::{self, TeamSide},
    input::{MatchInput, PlayerMatchSnapshot, TeamInstructionsSnapshot, TeamMatchSnapshot},
    roster,
    state::{BallControl, MatchState},
    tactical_models,
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

struct Candidate {
    player_id: String,
    energy: i32,
    distance: f64,
    order_distance: f64,
}

pub fn apply_to_ball_carrier(input: &MatchInput, state: &mut MatchState) -> i32 {
    if state.ball.control != BallControl::Controlled {
        return 0;
    }
    let owner_team = match state.ball.owner_team {
        Some(t) => t,
        None => return 0,
    };
    let carrier_id = match state.ball.owner_player_id.clone() {
        Some(id) => id,
        None => return 0,
    };
    let defending_side = owner_team.opposite();
    let defending = input.team(defending_side);
    let instructions = state.team(defending_side).team_instructions.clone();
    let gk_id = state.team(defending_side).designated_goalkeeper_id.clone();
    let ball_x = state.ball.x;
    let ball_y = state.ball.y;

    let candidates = gather_candidates(input, state, defending_side, ball_x, ball_y, Some(&carrier_id));
    let max_pressers = 1 + (instructions.pressing_intensity - 35).max(0) / 30;
    let high_press_reach = (instructions.pressing_intensity - 60).max(0) as f64 * 0.035;
    let radius = 7.0
        + instructions.pressing_intensity as f64 * 0.095
        + instructions.line_height as f64 * 0.015
        + high_press_reach;
    let mut total_pressure = 0;
    let mut used = 0;
    let min_contrib = if instructions.pressing_intensity >= 75 { 3 } else { 5 };

    for cand in &candidates {
        if used >= max_pressers || cand.distance > radius {
            break;
        }
        if gk_id.as_deref() == Some(cand.player_id.as_str()) && cand.distance > 10.0 {
            continue;
        }
        let player = match defending.player(&cand.player_id) {
            Some(p) => p,
            None => continue,
        };
        let mut contribution = pressure_contribution(defending, player, cand.energy, &instructions, cand.distance, radius);
        if used > 0 {
            contribution = java_round(contribution as f64 * 0.40);
        }
        if contribution <= min_contrib {
            continue;
        }
        if let Some(r) = state.player_mut(&cand.player_id) {
            r.pressing_player_id = Some(carrier_id.clone());
            r.current_task = "PRESS".into();
            defensive::note_actual_presser(r);
            r.set_target(ball_x, ball_y);
        }
        state.team_mut(defending_side).add_pressing_action(contribution);
        total_pressure += contribution;
        used += 1;
    }

    if used == 0 && instructions.pressing_intensity >= 75 {
        let ext = radius + 1.25;
        if let Some(marginal) = candidates.iter().take_while(|c| c.distance <= ext).find(|c| {
            !(gk_id.as_deref() == Some(c.player_id.as_str()) && c.distance > 10.0)
        }) {
            if let Some(player) = defending.player(&marginal.player_id) {
                let contribution = pressure_contribution(defending, player, marginal.energy, &instructions, marginal.distance, ext).clamp(1, 5);
                if let Some(r) = state.player_mut(&marginal.player_id) {
                    r.pressing_player_id = Some(carrier_id.clone());
                    r.current_task = "PRESS".into();
                    defensive::note_actual_presser(r);
                    r.set_target(ball_x, ball_y);
                }
                state.team_mut(defending_side).add_pressing_action(contribution);
                total_pressure += contribution;
            }
        }
    }

    defensive::reinforce_after_press(input, state, defending_side, ball_x, ball_y);
    total_pressure += low_block_containment(input, state, defending_side, ball_x, ball_y);
    total_pressure += tactical_models::home_pressure_adjustment(input, defending_side);
    total_pressure += java_round(roster::team_mental_execution_adjustment(input, state, defending_side) * 0.65);
    total_pressure -= tactical_models::defensive_disorganization(state, defending_side);
    total_pressure -= tactical_models::numerical_pressure_penalty(state, defending_side);
    total_pressure = total_pressure.clamp(0, 100);
    if let Some(carrier) = state.player_mut(&carrier_id) {
        carrier.set_pressure_level(total_pressure);
    }
    total_pressure
}

pub fn estimate_at_point(input: &MatchInput, state: &MatchState, defending_side: TeamSide, x: i32, y: i32) -> i32 {
    let defending = input.team(defending_side);
    let instructions = &state.team(defending_side).team_instructions;
    let candidates = gather_candidates(input, state, defending_side, x, y, None);
    let max = candidates.len().min((1 + (instructions.pressing_intensity - 35).max(0) / 32) as usize);
    let mut total = 0;
    for (i, cand) in candidates.iter().take(max).enumerate() {
        let high_press_reach = (instructions.pressing_intensity - 60).max(0) as f64 * 0.030;
        let radius = 8.0 + instructions.pressing_intensity as f64 * 0.085 + high_press_reach;
        if cand.distance > radius {
            break;
        }
        let player = match defending.player(&cand.player_id) {
            Some(p) => p,
            None => continue,
        };
        let divisor = if i == 0 { 2 } else { 6 };
        total += pressure_contribution(defending, player, cand.energy, instructions, cand.distance, radius) / divisor;
    }
    total += low_block_containment(input, state, defending_side, x, y);
    total += tactical_models::home_pressure_adjustment(input, defending_side);
    total += java_round(roster::team_mental_execution_adjustment(input, state, defending_side) * 0.65);
    total -= tactical_models::defensive_disorganization(state, defending_side);
    total -= tactical_models::numerical_pressure_penalty(state, defending_side);
    total.clamp(0, 100)
}

fn low_block_containment(input: &MatchInput, state: &MatchState, defending_side: TeamSide, x: i32, y: i32) -> i32 {
    let line_height = state.team(defending_side).team_instructions.line_height;
    if line_height >= 48 {
        return 0;
    }
    let progress = geometry::attacking_progress(defending_side.opposite(), y);
    if progress < 66 {
        return 0;
    }
    let defending = input.team(defending_side);
    let gk_id = state.team(defending_side).designated_goalkeeper_id.as_deref();
    let mut nearby = 0;
    let mut positioning = 0.0;
    for (_, r) in &state.players {
        if !r.on_pitch || r.side != defending_side || Some(r.player_id.as_str()) == gk_id {
            continue;
        }
        if geometry::distance(r.x, r.y, x, y) > 23.0 {
            continue;
        }
        if let Some(p) = defending.player(&r.player_id) {
            nearby += 1;
            positioning += p.attributes.get(16) as f64;
        }
    }
    if nearby < 2 {
        return 0;
    }
    let avg = positioning / nearby as f64;
    let depth = (48 - line_height) as f64 * 0.34;
    let density = nearby.min(6) as f64 * 1.15;
    let discipline = (avg - 50.0) * 0.055;
    java_round(depth + density + discipline).clamp(0, 18)
}

fn pressure_contribution(
    team: &TeamMatchSnapshot,
    player: &PlayerMatchSnapshot,
    energy: i32,
    instructions: &TeamInstructionsSnapshot,
    distance: f64,
    press_radius: f64,
) -> i32 {
    let collective = &team.collective_state;
    let fam_factor = 0.65 + collective.system_familiarity as f64 * 0.0035;
    let effective_pressing = 50.0 + (instructions.pressing_intensity - 50) as f64 * fam_factor;
    let effective_line = 50.0 + (instructions.line_height - 50) as f64 * fam_factor;
    let ctx = PressureContext {
        distance,
        press_radius,
        pressing_intensity: instructions.pressing_intensity,
        line_height: instructions.line_height,
        energy,
    };
    let value = effective_pressing * 0.10
        + effective_line * 0.035
        + 74.0 * (0.15 + 0.11 + 0.08 + 0.08)
        + contextual::pressure_attribute_adjustment(&player.attributes, &ctx)
        + collective.cohesion as f64 * 0.05
        + energy as f64 * 0.045
        - distance * 1.55;
    java_round(value).clamp(0, 55)
}

fn gather_candidates(
    input: &MatchInput,
    state: &MatchState,
    side: TeamSide,
    x: i32,
    y: i32,
    carrier_id: Option<&str>,
) -> Vec<Candidate> {
    let team = input.team(side);
    let mut out = Vec::with_capacity(11);
    for (_, r) in &state.players {
        if r.side != side || !r.on_pitch {
            continue;
        }
        if team.player(&r.player_id).is_none() {
            continue;
        }
        let dist = geometry::distance(r.x, r.y, x, y);
        let order_dist = dist
            + defensive::assignment_pressing_order_adjustment(r, carrier_id)
            + defensive::coord_pressing_order_adjustment(r);
        out.push(Candidate {
            player_id: r.player_id.clone(),
            energy: r.energy,
            distance: dist,
            order_distance: order_dist,
        });
    }
    out.sort_by(|a, b| a.order_distance.total_cmp(&b.order_distance).then_with(|| a.player_id.cmp(&b.player_id)));
    out
}
