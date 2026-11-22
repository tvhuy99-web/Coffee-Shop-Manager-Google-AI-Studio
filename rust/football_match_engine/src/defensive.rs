use std::collections::{HashMap, HashSet};
use crate::{
    geometry::{self, TeamSide},
    input::{MatchInput, TeamMatchSnapshot},
    roster,
    state::{MatchState, PlayerRuntimeState},
};

pub const ROLE_NONE: &str = "NONE";
pub const ROLE_PRESSER: &str = "PRESSER";
pub const ROLE_COVER: &str = "COVER";
pub const ROLE_SCREEN: &str = "SCREEN";
pub const ROLE_BALANCE: &str = "BALANCE";

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

pub fn assign_defenders(input: &MatchInput, state: &mut MatchState) {
    let possession = match state.possession.controlling_team {
        Some(s) => s,
        None => return,
    };
    let defending_side = possession.opposite();
    let def_gk = state.team(defending_side).designated_goalkeeper_id.clone();
    let att_gk = state.team(possession).designated_goalkeeper_id.clone();
    let ball_x = state.ball.x;
    let ball_y = state.ball.y;
    let ball_owner = state.ball.owner_player_id.clone();

    let defender_ids: Vec<String> = state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == defending_side && def_gk.as_deref() != Some(r.player_id.as_str()))
        .map(|(_, r)| r.player_id.clone())
        .collect();
    if defender_ids.is_empty() {
        return;
    }

    let attacking = input.team(possession);
    let mut threats: Vec<(String, i32, i32, f64)> = Vec::new();
    for (_, r) in &state.players {
        if !r.on_pitch || r.side != possession || att_gk.as_deref() == Some(r.player_id.as_str()) {
            continue;
        }
        let p = match attacking.player(&r.player_id) {
            Some(p) => p,
            None => continue,
        };
        let progress = geometry::attacking_progress(possession, r.y);
        let ball_dist = geometry::distance(r.x, r.y, ball_x, ball_y);
        let a = &p.attributes;
        let mut danger = progress as f64 * 0.68
            + a.get(15) as f64 * 0.14
            + a.get(6) as f64 * 0.08
            + a.get(8) as f64 * 0.015
            + a.get(15) as f64 * 0.035
            - ball_dist.min(35.0) * 0.22;
        if ball_owner.as_deref() == Some(r.player_id.as_str()) {
            danger += 18.0;
        }
        threats.push((r.player_id.clone(), r.x, r.y, danger));
    }
    threats.sort_by(|a, b| b.3.total_cmp(&a.3).then_with(|| a.0.cmp(&b.0)));

    let pressing = state.team(defending_side).team_instructions.pressing_intensity;
    let cover_reserve = if defender_ids.len() >= 6 && pressing < 72 { 2 } else { 1 };
    let marking_limit = defender_ids.len().saturating_sub(cover_reserve);
    let mut used: HashSet<String> = HashSet::new();
    let mut marked = 0usize;

    let defending = input.team(defending_side);
    for (att_id, att_x, att_y, danger) in &threats {
        if marked >= marking_limit {
            break;
        }
        let marker_id = best_marker(defending, state, &defender_ids, &used, *att_x, *att_y);
        let marker_id = match marker_id {
            Some(id) => id,
            None => continue,
        };
        used.insert(marker_id.clone());
        apply_marking_target(input, state, &marker_id, att_id, *att_x, *att_y, *danger);
        marked += 1;
    }

    let mut cover_ids: Vec<(String, f64)> = defender_ids
        .iter()
        .filter(|id| !used.contains(*id))
        .filter_map(|id| {
            state
                .player(id)
                .map(|r| (id.clone(), geometry::distance(r.x, r.y, ball_x, ball_y)))
        })
        .collect();
    cover_ids.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    for (i, (cid, _)) in cover_ids.iter().enumerate() {
        apply_assignment_cover_target(state, cid, ball_x, ball_y, i as i32);
    }
}

pub fn assignment_pressing_order_adjustment(defender: &PlayerRuntimeState, carrier_id: Option<&str>) -> f64 {
    let mut adj = 0.0;
    if let (Some(cid), Some(mid)) = (carrier_id, defender.marking_player_id.as_deref()) {
        if cid == mid {
            adj -= 2.4;
        }
    }
    if defender.current_task.starts_with("COVER") {
        adj += 3.2;
    }
    if defender.current_task.starts_with("MARK") {
        adj -= 0.5;
    }
    adj
}

fn best_marker(
    defending: &TeamMatchSnapshot,
    state: &MatchState,
    defender_ids: &[String],
    used: &HashSet<String>,
    att_x: i32,
    att_y: i32,
) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_cost = f64::INFINITY;
    for id in defender_ids {
        if used.contains(id) {
            continue;
        }
        let d = match state.player(id) {
            Some(d) => d,
            None => continue,
        };
        let p = match defending.player(id) {
            Some(p) => p,
            None => continue,
        };
        let current_dist = geometry::distance(d.x, d.y, att_x, att_y);
        let anchor_dist = geometry::distance(d.target_x, d.target_y, att_x, att_y);
        let lateral_mismatch = (d.target_x - att_x).abs() as f64;
        let a = &p.attributes;
        let quality = a.get(16) as f64 * 0.055 + a.get(13) as f64 * 0.025 + a.get(8) as f64 * 0.006;
        let cost = current_dist * 0.56 + anchor_dist * 0.31 + lateral_mismatch * 0.10 - quality;
        if best.is_none() || cost < best_cost || (cost == best_cost && id < best.as_ref().unwrap()) {
            best = Some(id.clone());
            best_cost = cost;
        }
    }
    best
}

fn apply_marking_target(
    input: &MatchInput,
    state: &mut MatchState,
    marker_id: &str,
    att_id: &str,
    att_x: i32,
    att_y: i32,
    danger: f64,
) {
    let (side, cur_tx, cur_ty) = match state.player(marker_id) {
        Some(r) => (r.side, r.target_x, r.target_y),
        None => return,
    };
    let defending = input.team(side);
    let p = defending.player(marker_id);
    let slot_aggr = roster::slot_instructions(state, side, marker_id).map(|s| s.defensive_aggression);
    let team_aggr = state.team(side).team_instructions.aggression;
    let aggression = match slot_aggr {
        Some(sa) => (team_aggr + sa) / 2,
        None => team_aggr,
    };
    let discipline = p.map(|x| x.attributes.get(16) as f64).unwrap_or(70.0) / 100.0;
    let decisions = p.map(|x| x.attributes.get(13) as f64).unwrap_or(70.0) / 100.0;
    let blend = (0.020
        + aggression as f64 * 0.00075
        + discipline * 0.100
        + decisions * 0.020
        + danger.min(100.0) * 0.00025)
        .clamp(0.08, 0.24);
    let goal_y = geometry::own_goal_y_spatial(side);
    let goal_side_y = geometry::clamp(java_round(att_y as f64 + (goal_y - att_y) as f64 * 0.10));
    let goal_side_x = geometry::clamp(java_round(att_x as f64 + (50 - att_x) as f64 * 0.05));
    let tx = geometry::clamp(java_round(cur_tx as f64 + (goal_side_x - cur_tx) as f64 * blend));
    let ty = geometry::clamp(java_round(cur_ty as f64 + (goal_side_y - cur_ty) as f64 * blend));
    let transition = state.possession.transition_active;
    if let Some(m) = state.player_mut(marker_id) {
        m.marking_player_id = Some(att_id.into());
        m.set_target(tx, ty);
        m.current_task = if transition { "RECOVERY_MARK" } else { "MARK" }.into();
    }
}

fn apply_assignment_cover_target(state: &mut MatchState, defender_id: &str, ball_x: i32, ball_y: i32, rank: i32) {
    let (side, cur_tx, cur_ty) = match state.player(defender_id) {
        Some(r) => (r.side, r.target_x, r.target_y),
        None => return,
    };
    let goal_y = geometry::own_goal_y_spatial(side);
    let depth = if rank == 0 { 0.34 } else { 0.26 };
    let desired_y = geometry::clamp(java_round(ball_y as f64 + (goal_y - ball_y) as f64 * depth));
    let lateral_offset = if rank == 0 { 0 } else if rank % 2 == 0 { 7 } else { -7 };
    let desired_x = geometry::clamp(java_round(ball_x as f64 + (50 - ball_x) as f64 * 0.22) + lateral_offset);
    let tx = geometry::clamp(java_round(cur_tx as f64 * 0.84 + desired_x as f64 * 0.16));
    let ty = geometry::clamp(java_round(cur_ty as f64 * 0.80 + desired_y as f64 * 0.20));
    if let Some(d) = state.player_mut(defender_id) {
        d.set_target(tx, ty);
        d.current_task = if rank == 0 { "COVER" } else { "COVER_SECONDARY" }.into();
    }
}

pub fn coordinate(input: &MatchInput, state: &mut MatchState) {
    let attacking_side = match state.possession.controlling_team {
        Some(s) => s,
        None => return,
    };
    let defending_side = attacking_side.opposite();
    let ball_x = state.ball.x;
    let ball_y = state.ball.y;
    let gk_id = state.team(defending_side).designated_goalkeeper_id.clone();

    let mut previous_roles: HashMap<String, String> = HashMap::new();
    let mut defender_ids: Vec<String> = Vec::new();
    for (_, r) in &mut state.players {
        if !r.on_pitch {
            continue;
        }
        if r.side == attacking_side {
            r.defensive_role = ROLE_NONE.into();
            continue;
        }
        previous_roles.insert(r.player_id.clone(), r.defensive_role.clone());
        if gk_id.as_deref() != Some(r.player_id.as_str()) {
            defender_ids.push(r.player_id.clone());
        }
        r.defensive_role = ROLE_NONE.into();
    }
    if defender_ids.is_empty() {
        return;
    }

    let defending = input.team(defending_side);
    let instructions = state.team(defending_side).team_instructions.clone();
    let familiarity = defending.collective_state.system_familiarity as f64;
    let presser_bypassed = was_presser_bypassed(&previous_roles, state, &defender_ids, attacking_side, ball_y);
    let short_handed = defender_ids.len() < 10;

    let presser_id = choose_presser(defending, state, &defender_ids, ball_x, ball_y, &instructions);
    let mut assigned: HashSet<String> = HashSet::new();
    if let Some(ref pid) = presser_id {
        assigned.insert(pid.clone());
        if let Some(p) = state.player_mut(pid) {
            p.defensive_role = ROLE_PRESSER.into();
            apply_presser_shadow_target(p, ball_x, ball_y, &instructions, familiarity);
        }
    }

    let mut cover_count = 1;
    if presser_bypassed || state.possession.transition_active || instructions.pressing_intensity >= 76 {
        cover_count += 1;
    }
    for rank in 0..cover_count {
        let cover_id = choose_cover(
            defending,
            state,
            &defender_ids,
            &assigned,
            presser_id.as_deref(),
            ball_x,
            ball_y,
        );
        let cid = match cover_id {
            Some(c) => c,
            None => break,
        };
        assigned.insert(cid.clone());
        if let Some(c) = state.player_mut(&cid) {
            c.defensive_role = ROLE_COVER.into();
            apply_coord_cover_target(defending, c, ball_x, ball_y, rank, familiarity, presser_bypassed, short_handed);
        }
    }

    let mut screen_count = 1;
    if instructions.line_height < 42 || state.possession.transition_active {
        screen_count += 1;
    }
    for rank in 0..screen_count {
        let screen_id = choose_screen(defending, state, &defender_ids, &assigned, ball_x, ball_y);
        let sid = match screen_id {
            Some(s) => s,
            None => break,
        };
        assigned.insert(sid.clone());
        if let Some(s) = state.player_mut(&sid) {
            s.defensive_role = ROLE_SCREEN.into();
            apply_screen_target(defending, s, ball_x, ball_y, rank, familiarity, short_handed);
        }
    }

    for did in &defender_ids {
        if assigned.contains(did) {
            continue;
        }
        if let Some(d) = state.player_mut(did) {
            d.defensive_role = ROLE_BALANCE.into();
            apply_balance_target(defending, d, ball_x, ball_y, &instructions, familiarity, short_handed);
        }
    }

    if presser_bypassed {
        recover_bypassed_pressers(&previous_roles, state, &defender_ids, attacking_side, ball_x, ball_y);
    }
}

pub fn coord_pressing_order_adjustment(defender: &PlayerRuntimeState) -> f64 {
    match defender.defensive_role.as_str() {
        ROLE_PRESSER => -0.5,
        ROLE_COVER => 1.4,
        ROLE_SCREEN => 0.4,
        ROLE_BALANCE => 0.5,
        _ => 0.0,
    }
}

pub fn note_actual_presser(runtime: &mut PlayerRuntimeState) {
    runtime.defensive_role = ROLE_PRESSER.into();
}

pub fn reinforce_after_press(input: &MatchInput, state: &mut MatchState, defending_side: TeamSide, ball_x: i32, ball_y: i32) {
    let gk_id = state.team(defending_side).designated_goalkeeper_id.clone();
    let mut has_cover = false;
    let mut pool: Vec<String> = Vec::new();
    for (_, r) in &state.players {
        if !r.on_pitch || r.side != defending_side || gk_id.as_deref() == Some(r.player_id.as_str()) {
            continue;
        }
        if r.defensive_role == ROLE_COVER && r.pressing_player_id.is_none() {
            has_cover = true;
        }
        if r.pressing_player_id.is_none() {
            pool.push(r.player_id.clone());
        }
    }
    if has_cover || pool.is_empty() {
        return;
    }
    let defending = input.team(defending_side);
    let empty: HashSet<String> = HashSet::new();
    if let Some(cid) = choose_cover(defending, state, &pool, &empty, None, ball_x, ball_y) {
        let fam = defending.collective_state.system_familiarity as f64;
        let short = pool.len() < 10;
        if let Some(c) = state.player_mut(&cid) {
            c.defensive_role = ROLE_COVER.into();
            apply_coord_cover_target(defending, c, ball_x, ball_y, 0, fam, false, short);
        }
    }
}

fn choose_presser(
    defending: &TeamMatchSnapshot,
    state: &MatchState,
    defender_ids: &[String],
    ball_x: i32,
    ball_y: i32,
    instructions: &crate::input::TeamInstructionsSnapshot,
) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_cost = f64::INFINITY;
    let owner_id = state.ball.owner_player_id.as_deref();
    for id in defender_ids {
        let r = match state.player(id) {
            Some(r) => r,
            None => continue,
        };
        let p = match defending.player(id) {
            Some(p) => p,
            None => continue,
        };
        let dist = geometry::distance(r.x, r.y, ball_x, ball_y);
        let u = unit(defending, r);
        let deep_penalty = if u == 0 && geometry::attacking_progress(r.side.opposite(), ball_y) < 62 {
            4.8
        } else {
            0.0
        };
        let assignment = assignment_pressing_order_adjustment(r, owner_id);
        let a = &p.attributes;
        let quality = (a.get(17) as f64 - 74.0) * 0.039
            + (a.get(8) as f64 - 74.0) * 0.008
            + (a.get(13) as f64 - 74.0) * 0.020
            + (a.get(16) as f64 - 74.0) * 0.016;
        let intensity = (instructions.pressing_intensity - 50) as f64 * 0.018;
        let cost = dist + deep_penalty + assignment - quality - intensity;
        if best.is_none() || cost < best_cost || (cost == best_cost && id < best.as_ref().unwrap()) {
            best = Some(id.clone());
            best_cost = cost;
        }
    }
    best
}

fn choose_cover(
    defending: &TeamMatchSnapshot,
    state: &MatchState,
    defender_ids: &[String],
    used: &HashSet<String>,
    presser_id: Option<&str>,
    ball_x: i32,
    ball_y: i32,
) -> Option<String> {
    let first_side = defender_ids.first().and_then(|id| state.player(id)).map(|r| r.side)?;
    let goal_y = geometry::own_goal_y_spatial(first_side);
    let presser_pos = presser_id.and_then(|id| state.player(id)).map(|r| (r.x, r.y));
    let mut best: Option<String> = None;
    let mut best_cost = f64::INFINITY;
    for id in defender_ids {
        if used.contains(id) {
            continue;
        }
        let r = match state.player(id) {
            Some(r) if r.pressing_player_id.is_none() => r,
            _ => continue,
        };
        let p = match defending.player(id) {
            Some(p) => p,
            None => continue,
        };
        let ball_dist = geometry::distance(r.x, r.y, ball_x, ball_y);
        let goal_side = (r.y - goal_y).abs() as f64 * 0.045;
        let centrality = (r.x - 50).abs() as f64 * 0.075;
        let presser_link = presser_pos
            .map(|(px, py)| geometry::distance(r.x, r.y, px, py) * 0.12)
            .unwrap_or(0.0);
        let a = &p.attributes;
        let quality = (a.get(16) as f64 - 74.0) * 0.045
            + (a.get(13) as f64 - 74.0) * 0.025
            + (a.get(8) as f64 - 74.0) * 0.004;
        let unit_bonus = if unit(defending, r) == 0 { -2.5 } else { 0.0 };
        let cost = ball_dist * 0.34 + goal_side + centrality + presser_link - quality + unit_bonus;
        if best.is_none() || cost < best_cost || (cost == best_cost && id < best.as_ref().unwrap()) {
            best = Some(id.clone());
            best_cost = cost;
        }
    }
    best
}

fn choose_screen(
    defending: &TeamMatchSnapshot,
    state: &MatchState,
    defender_ids: &[String],
    used: &HashSet<String>,
    ball_x: i32,
    ball_y: i32,
) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_cost = f64::INFINITY;
    for id in defender_ids {
        if used.contains(id) {
            continue;
        }
        let r = match state.player(id) {
            Some(r) if r.pressing_player_id.is_none() => r,
            _ => continue,
        };
        let p = match defending.player(id) {
            Some(p) => p,
            None => continue,
        };
        let u = unit(defending, r);
        let unit_cost = if u == 1 { 0.0 } else if u == 0 { 3.0 } else { 5.0 };
        let centrality = (r.x - 50).abs() as f64 * 0.12;
        let goal_y = geometry::own_goal_y_spatial(r.side);
        let sx = geometry::clamp(java_round(50.0 + (ball_x - 50) as f64 * 0.35));
        let sy = geometry::clamp(java_round(ball_y as f64 + (goal_y - ball_y) as f64 * 0.16));
        let lane_dist = geometry::distance(r.x, r.y, sx, sy) * 0.38;
        let a = &p.attributes;
        let quality = (a.get(16) as f64 - 74.0) * 0.040
            + (a.get(13) as f64 - 74.0) * 0.035
            + (a.get(17) as f64 - 74.0) * 0.012;
        let cost = unit_cost + centrality + lane_dist - quality;
        if best.is_none() || cost < best_cost || (cost == best_cost && id < best.as_ref().unwrap()) {
            best = Some(id.clone());
            best_cost = cost;
        }
    }
    best
}

fn apply_presser_shadow_target(
    presser: &mut PlayerRuntimeState,
    ball_x: i32,
    ball_y: i32,
    instructions: &crate::input::TeamInstructionsSnapshot,
    familiarity: f64,
) {
    let blend = (0.075
        + instructions.pressing_intensity as f64 * 0.00065
        + (familiarity - 60.0).max(0.0) * 0.0005)
        .clamp(0.075, 0.16);
    let tx = geometry::clamp(java_round(presser.target_x as f64 + (ball_x - presser.target_x) as f64 * blend));
    let ty = geometry::clamp(java_round(presser.target_y as f64 + (ball_y - presser.target_y) as f64 * blend));
    presser.set_target(tx, ty);
}

#[allow(clippy::too_many_arguments)]
fn apply_coord_cover_target(
    defending: &TeamMatchSnapshot,
    defender: &mut PlayerRuntimeState,
    ball_x: i32,
    ball_y: i32,
    rank: i32,
    familiarity: f64,
    emergency: bool,
    short_handed: bool,
) {
    let side = defender.side;
    let goal_y = geometry::own_goal_y_spatial(side);
    let p = defending.player(&defender.player_id);
    let positioning = p.map(|x| x.attributes.get(16) as f64).unwrap_or(74.0);
    let decisions = p.map(|x| x.attributes.get(13) as f64).unwrap_or(74.0);
    let intelligence = (positioning + decisions) / 2.0;
    let depth = 0.30 + rank as f64 * 0.06 + if emergency { 0.07 } else { 0.0 };
    let ideal_y = geometry::clamp(java_round(ball_y as f64 + (goal_y - ball_y) as f64 * depth));
    let ideal_x = geometry::clamp(java_round(ball_x as f64 + (50 - ball_x) as f64 * (0.34 + rank as f64 * 0.08)));
    let mut blend = 0.100
        + (intelligence - 74.0) * 0.0010
        + (familiarity - 60.0) * 0.0007
        + if emergency { 0.045 } else { 0.0 };
    let danger_prog = geometry::attacking_progress(side.opposite(), ball_y);
    let danger_scale = if emergency {
        1.0
    } else if danger_prog < 45 {
        0.55
    } else if danger_prog < 65 {
        0.80
    } else {
        1.0
    };
    blend *= danger_scale * if short_handed { 0.78 } else { 1.0 };
    blend = blend.clamp(0.035, 0.22);
    let tx = geometry::clamp(java_round(defender.target_x as f64 + (ideal_x - defender.target_x) as f64 * blend));
    let ty = geometry::clamp(java_round(defender.target_y as f64 + (ideal_y - defender.target_y) as f64 * blend));
    defender.set_target(tx, ty);
    if emergency {
        defender.current_task = "RECOVERY_COVER".into();
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_screen_target(
    defending: &TeamMatchSnapshot,
    defender: &mut PlayerRuntimeState,
    ball_x: i32,
    ball_y: i32,
    rank: i32,
    familiarity: f64,
    short_handed: bool,
) {
    let side = defender.side;
    let goal_y = geometry::own_goal_y_spatial(side);
    let p = defending.player(&defender.player_id);
    let positioning = p.map(|x| x.attributes.get(16) as f64).unwrap_or(74.0);
    let decisions = p.map(|x| x.attributes.get(13) as f64).unwrap_or(74.0);
    let ideal_x = geometry::clamp(java_round(50.0 + (ball_x - 50) as f64 * if rank == 0 { 0.32 } else { 0.48 }));
    let ideal_y = geometry::clamp(java_round(ball_y as f64 + (goal_y - ball_y) as f64 * if rank == 0 { 0.16 } else { 0.11 }));
    let mut blend = 0.060 + ((positioning + decisions) / 2.0 - 74.0) * 0.0009 + (familiarity - 60.0) * 0.0005;
    let danger_prog = geometry::attacking_progress(side.opposite(), ball_y);
    let danger_scale = if danger_prog < 50 { 0.30 } else if danger_prog < 70 { 0.65 } else { 1.0 };
    blend *= danger_scale * if short_handed { 0.72 } else { 1.0 };
    blend = blend.clamp(0.014, 0.14);
    let tx = geometry::clamp(java_round(defender.target_x as f64 + (ideal_x - defender.target_x) as f64 * blend));
    let ty = geometry::clamp(java_round(defender.target_y as f64 + (ideal_y - defender.target_y) as f64 * blend));
    defender.set_target(tx, ty);
}

#[allow(clippy::too_many_arguments)]
fn apply_balance_target(
    defending: &TeamMatchSnapshot,
    defender: &mut PlayerRuntimeState,
    ball_x: i32,
    ball_y: i32,
    instructions: &crate::input::TeamInstructionsSnapshot,
    familiarity: f64,
    short_handed: bool,
) {
    let p = defending.player(&defender.player_id);
    let positioning = p.map(|x| x.attributes.get(16) as f64).unwrap_or(74.0);
    let decisions = p.map(|x| x.attributes.get(13) as f64).unwrap_or(74.0);
    let ball_left = ball_x < 46;
    let ball_right = ball_x > 54;
    let weak_side = (ball_left && defender.target_x > 55) || (ball_right && defender.target_x < 45);
    let mut narrow = if weak_side { 0.085 } else { 0.035 };
    narrow *= 0.85 + (100 - instructions.width) as f64 * 0.0030;
    narrow *= 0.92 + (positioning + decisions - 148.0) * 0.0020 + (familiarity - 60.0) * 0.0015;
    let danger_prog = geometry::attacking_progress(defender.side.opposite(), ball_y);
    narrow *= 0.62 + (danger_prog as f64 / 75.0).min(1.0) * 0.38;
    if short_handed {
        narrow *= 0.55;
    }
    narrow = narrow.clamp(0.020, 0.14);
    let mut shifted_x = geometry::clamp(java_round(defender.target_x as f64 + (50 - defender.target_x) as f64 * narrow));
    let ball_shift = if weak_side { 0.015 } else { 0.035 };
    shifted_x = geometry::clamp(java_round(shifted_x as f64 + (ball_x - shifted_x) as f64 * ball_shift));
    let ty = defender.target_y;
    defender.set_target(shifted_x, ty);
}

fn recover_bypassed_pressers(
    previous_roles: &HashMap<String, String>,
    state: &mut MatchState,
    defender_ids: &[String],
    attacking_side: TeamSide,
    ball_x: i32,
    ball_y: i32,
) {
    let _ = ball_x;
    let goal_y = geometry::own_goal_y_spatial(attacking_side.opposite());
    let att_prog = geometry::attacking_progress(attacking_side, ball_y);
    for id in defender_ids {
        if previous_roles.get(id).map(|s| s.as_str()) != Some(ROLE_PRESSER) {
            continue;
        }
        if let Some(r) = state.player_mut(id) {
            if r.defensive_role == ROLE_PRESSER {
                continue;
            }
            let def_prog = geometry::attacking_progress(attacking_side, r.y);
            if att_prog <= def_prog + 5 {
                continue;
            }
            let ry = geometry::clamp(java_round(ball_y as f64 + (goal_y - ball_y) as f64 * 0.20));
            let rx = geometry::clamp(java_round(r.target_x as f64 + (50 - r.target_x) as f64 * 0.18));
            r.set_target(rx, ry);
            r.current_task = "RECOVERY".into();
        }
    }
}

fn was_presser_bypassed(
    previous_roles: &HashMap<String, String>,
    state: &MatchState,
    defender_ids: &[String],
    attacking_side: TeamSide,
    ball_y: i32,
) -> bool {
    let att_prog = geometry::attacking_progress(attacking_side, ball_y);
    for id in defender_ids {
        if previous_roles.get(id).map(|s| s.as_str()) != Some(ROLE_PRESSER) {
            continue;
        }
        if let Some(r) = state.player(id) {
            if att_prog > geometry::attacking_progress(attacking_side, r.y) + 5 {
                return true;
            }
        }
    }
    false
}

fn unit(team: &TeamMatchSnapshot, runtime: &PlayerRuntimeState) -> i32 {
    let slot = match team.slot(&runtime.slot_id) {
        Some(s) => s,
        None => return 1,
    };
    if slot.out_y <= 36 {
        0
    } else if slot.out_y <= 62 {
        1
    } else {
        2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefensiveMetrics {
    pub width: i32,
    pub depth: i32,
    pub largest_vertical_gap: i32,
    pub pressers: i32,
    pub covers: i32,
    pub screens: i32,
    pub balances: i32,
}

pub fn metrics(state: &MatchState, side: TeamSide) -> DefensiveMetrics {
    let gk_id = state.team(side).designated_goalkeeper_id.as_deref();
    let mut xs: Vec<i32> = Vec::new();
    let mut progress: Vec<i32> = Vec::new();
    let (mut pressers, mut covers, mut screens, mut balances) = (0, 0, 0, 0);
    for (_, r) in &state.players {
        if !r.on_pitch || r.side != side || gk_id == Some(r.player_id.as_str()) {
            continue;
        }
        xs.push(r.target_x);
        progress.push(geometry::attacking_progress(side, r.target_y));
        match r.defensive_role.as_str() {
            ROLE_PRESSER => pressers += 1,
            ROLE_COVER => covers += 1,
            ROLE_SCREEN => screens += 1,
            ROLE_BALANCE => balances += 1,
            _ => {}
        }
    }
    if xs.is_empty() {
        return DefensiveMetrics { width: 0, depth: 0, largest_vertical_gap: 0, pressers, covers, screens, balances };
    }
    xs.sort();
    progress.sort();
    let width = xs[xs.len() - 1] - xs[0];
    let depth = progress[progress.len() - 1] - progress[0];
    let mut largest_gap = 0;
    for i in 1..progress.len() {
        largest_gap = largest_gap.max(progress[i] - progress[i - 1]);
    }
    DefensiveMetrics { width, depth, largest_vertical_gap: largest_gap, pressers, covers, screens, balances }
}

pub fn reinforce_cover_after_press(state: &mut MatchState, defending_side: TeamSide, ball_x: i32, ball_y: i32) {
    let gk_id = state.team(defending_side).designated_goalkeeper_id.as_deref();
    let goal_y = geometry::own_goal_y_spatial(defending_side);
    let mut best: Option<String> = None;
    let mut best_score = f64::INFINITY;
    let mut existing_cover = false;
    for (_, r) in &state.players {
        if !r.on_pitch || r.side != defending_side || gk_id == Some(r.player_id.as_str()) {
            continue;
        }
        if r.pressing_player_id.is_some() {
            continue;
        }
        if r.current_task.starts_with("COVER") {
            existing_cover = true;
        }
        let score = geometry::distance(r.x, r.y, ball_x, ball_y)
            + (r.x - 50).abs() as f64 * 0.08
            + (r.y - goal_y).abs() as f64 * 0.03;
        if best.is_none() || score < best_score || (score == best_score && r.player_id < *best.as_ref().unwrap()) {
            best = Some(r.player_id.clone());
            best_score = score;
        }
    }
    if !existing_cover {
        if let Some(cid) = best {
            apply_assignment_cover_target(state, &cid, ball_x, ball_y, 0);
        }
    }
}
