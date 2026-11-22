use std::collections::HashMap;
use crate::{
    geometry::TeamSide,
    input::{MatchInput, SlotInstructionsSnapshot},
    state::MatchState,
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

pub fn active_player_ids_in_slot_order(input: &MatchInput, state: &MatchState, side: TeamSide) -> Vec<String> {
    let team = input.team(side);
    let mut order: HashMap<&str, usize> = HashMap::with_capacity(team.tactical_slots.len());
    for (i, s) in team.tactical_slots.iter().enumerate() {
        order.insert(s.slot_id.as_str(), i);
    }
    let mut active: Vec<(&str, &str)> = state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == side)
        .map(|(_, r)| (r.slot_id.as_str(), r.player_id.as_str()))
        .collect();
    active.sort_by(|a, b| {
        let ai = *order.get(a.0).unwrap_or(&999);
        let bi = *order.get(b.0).unwrap_or(&999);
        ai.cmp(&bi).then_with(|| a.1.cmp(b.1))
    });
    active.into_iter().map(|(_, id)| id.to_string()).collect()
}

pub fn active_player_in_slot(state: &MatchState, side: TeamSide, slot_id: &str) -> Option<String> {
    let mut best: Option<&str> = None;
    for (_, r) in &state.players {
        if r.on_pitch && r.side == side && r.slot_id == slot_id {
            if best.map(|b| r.player_id.as_str() < b).unwrap_or(true) {
                best = Some(r.player_id.as_str());
            }
        }
    }
    best.map(|s| s.to_string())
}

pub fn effective_morale(input: &MatchInput, state: &MatchState, side: TeamSide, player_id: &str) -> i32 {
    let runtime = state.player(player_id);
    let player = input.team(side).player(player_id).expect("unknown player");
    match runtime {
        Some(r) if r.morale_override >= 0 => r.morale_override,
        _ => player.morale,
    }
}

pub fn slot_instructions<'a>(
    state: &'a MatchState,
    side: TeamSide,
    player_id: &str,
) -> Option<&'a SlotInstructionsSnapshot> {
    let r = state.player(player_id)?;
    if r.side != side {
        return None;
    }
    state.team(side).slot_instructions(&r.slot_id)
}

pub fn leadership_stability_bonus(input: &MatchInput, state: &MatchState, side: TeamSide) -> i32 {
    let team = input.team(side);
    let plan = &state.team(side).set_piece_plan;
    if plan.captain_slot_ids.is_empty() {
        return 0;
    }
    let mut captain_id = None;
    for slot_id in &plan.captain_slot_ids {
        if let Some(id) = active_player_in_slot(state, side, slot_id) {
            captain_id = Some(id);
            break;
        }
    }
    let captain_id = match captain_id {
        Some(id) => id,
        None => return 0,
    };
    let captain = match team.player(&captain_id) {
        Some(c) => c,
        None => return 0,
    };
    let charisma = captain.attributes.get(18) as i32;
    let morale = effective_morale(input, state, side, &captain_id);
    let raw = (charisma - 50) / 8 + (morale - 50) / 25;
    raw.clamp(-5, 5)
}

pub fn concentration_stability_bonus(state: &MatchState, side: TeamSide) -> i32 {
    let concentration = state.team(side).team_instructions.concentration;
    java_round((concentration - 50) as f64 / 10.0).clamp(-3, 3)
}

pub fn mental_stability(input: &MatchInput, state: &MatchState, side: TeamSide, player_id: &str) -> i32 {
    let player = input.team(side).player(player_id).expect("unknown player");
    let base = (effective_morale(input, state, side, player_id) + player.form) / 2;
    (base
        + leadership_stability_bonus(input, state, side)
        + concentration_stability_bonus(state, side)
        + java_round(state.team(side).match_day_performance * 0.60))
    .clamp(1, 99)
}

pub fn team_mental_execution_adjustment(input: &MatchInput, state: &MatchState, side: TeamSide) -> f64 {
    leadership_stability_bonus(input, state, side) as f64 * 0.18
        + concentration_stability_bonus(state, side) as f64 * 0.28
        + state.team(side).match_day_performance
}
