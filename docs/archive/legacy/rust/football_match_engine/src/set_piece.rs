use std::collections::HashSet;
use crate::{
    duel_resolver,
    events::EventType,
    geometry::{self, TeamSide},
    input::{MatchInput, PlayerMatchSnapshot},
    resolver_support::{best_aerial, choose_central, java_round, kv, nearest_outfield, MatchRuntime},
    rng::DeterministicRng,
    roster,
    shot_resolver,
    state::{MatchState, RestartState, RestartType, TacticalPhase},
    tactical_models,
};

#[derive(Clone, Debug)]
pub struct PreparedDelivery {
    pub target_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub zone: String,
    pub near_count: i32,
    pub far_count: i32,
    pub central_count: i32,
}

#[derive(Clone, Debug)]
struct RoleAssignment {
    player_id: String,
    role: &'static str,
}

pub fn prepare_delivery(
    input: &MatchInput,
    state: &mut MatchState,
    rng: &mut DeterministicRng,
    attack: TeamSide,
    configured_box_slots: &[String],
    taker: &str,
    corner: bool,
) -> PreparedDelivery {
    let mut attackers = attackers_from_slots(state, attack, configured_box_slots, taker);
    if attackers.is_empty() {
        attackers = fallback_attackers(input, state, attack, taker, 4);
    }
    let roles = assign_attacking_roles(input, attack, &attackers);
    place_attacking_roles(state, attack, &roles);
    protect_against_counter(input, state, attack, taker, &roles);
    prepare_defence(input, state, attack, &roles);

    let target = choose_delivery_target(input, state, rng, attack, taker, &roles, corner);
    let direction = geometry::attack_direction(attack);
    let goal_y = geometry::goal_y(attack);
    let mut x = 50;
    let mut y = geometry::clamp(goal_y - direction * 8);
    let mut zone = "CENTRAL";
    let mut target_id = None;
    if let Some(t) = target {
        if let Some(r) = state.player(&t.player_id) {
            x = r.target_x;
            y = r.target_y;
        }
        zone = t.role;
        target_id = Some(t.player_id.clone());
    }
    prepare_goalkeeper(input, state, attack.opposite(), zone, x, y);
    PreparedDelivery {
        target_id,
        x: geometry::clamp(x),
        y: geometry::clamp(y),
        zone: zone.into(),
        near_count: roles.iter().filter(|r| r.role == "NEAR_POST").count() as i32,
        far_count: roles.iter().filter(|r| r.role == "FAR_POST").count() as i32,
        central_count: roles.iter().filter(|r| r.role == "CENTRAL_TARGET").count() as i32,
    }
}

pub fn should_take_direct_free_kick(
    input: &MatchInput,
    state: &MatchState,
    rng: &mut DeterministicRng,
    side: TeamSide,
    taker: &str,
    x: i32,
    y: i32,
) -> bool {
    let progress = geometry::attacking_progress(side, y);
    if !(81..=94).contains(&progress) || (x - 50).abs() > 24 {
        return false;
    }
    let p = match input.team(side).player(taker) {
        Some(p) => p,
        None => return false,
    };
    let a = &p.attributes;
    let direct = a.get(6) as f64 * 0.34
        + a.get(4) as f64 * 0.24
        + a.get(14) as f64 * 0.16
        + a.get(13) as f64 * 0.10
        + (progress - 72) as f64 * 0.65
        - (x - 50).abs() as f64 * 0.28;
    let delivery = a.get(3) as f64 * 0.24
        + a.get(2) as f64 * 0.18
        + a.get(12) as f64 * 0.12
        + state.team(side).team_instructions.build_up_risk as f64 * 0.06
        + 33.0;
    let chance = java_round(50.0 + (direct - delivery) * 1.35).clamp(12, 78);
    rng.next_int(100) < chance
}

pub fn prepare_wall(input: &MatchInput, state: &mut MatchState, attack: TeamSide, ball_x: i32, ball_y: i32) -> i32 {
    let defend = attack.opposite();
    let gk = state.team(defend).designated_goalkeeper_id.clone();
    let def_team = input.team(defend);
    let mut ds: Vec<(String, f64)> = state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == defend && gk.as_deref() != Some(r.player_id.as_str()))
        .map(|(_, r)| {
            let qa = def_team
                .player(&r.player_id)
                .map(|p| p.attributes.get(16) as f64 + p.attributes.get(13) as f64)
                .unwrap_or(0.0)
                - geometry::distance(r.x, r.y, ball_x, ball_y) * 1.2;
            (r.player_id.clone(), qa)
        })
        .collect();
    ds.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let progress = geometry::attacking_progress(attack, ball_y);
    let mut wall_size = (3 + if progress >= 82 { 1 } else { 0 } + if (ball_x - 50).abs() < 12 { 1 } else { 0 }).clamp(3, 5);
    wall_size = wall_size.min(ds.len() as i32);
    let dir = geometry::attack_direction(attack);
    for (i, (id, _)) in ds.iter().take(wall_size as usize).enumerate() {
        let offset = (i as i32 - (wall_size - 1) / 2) * 4;
        let wx = geometry::clamp(ball_x + offset);
        let wy = geometry::clamp(ball_y + dir * 8);
        if let Some(d) = state.player_mut(id) {
            d.set_position(wx, wy);
            d.set_target(wx, wy);
            d.current_task = "SET_PIECE_WALL".into();
            d.marking_player_id = None;
        }
    }
    prepare_goalkeeper(input, state, defend, "DIRECT", 50, geometry::goal_y(attack) - dir * 2);
    wall_size
}

fn attackers_from_slots(state: &MatchState, side: TeamSide, slots: &[String], taker: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let gk = state.team(side).designated_goalkeeper_id.as_deref();
    for slot in slots {
        let id = match roster::active_player_in_slot(state, side, slot) {
            Some(id) => id,
            None => continue,
        };
        if id == taker || gk == Some(id.as_str()) || !seen.insert(id.clone()) {
            continue;
        }
        if state.player(&id).map(|r| r.on_pitch).unwrap_or(false) {
            out.push(id);
        }
    }
    out
}

fn fallback_attackers(input: &MatchInput, state: &MatchState, side: TeamSide, taker: &str, max: usize) -> Vec<String> {
    let gk = state.team(side).designated_goalkeeper_id.as_deref();
    let mut out: Vec<(String, f64)> = roster::active_player_ids_in_slot_order(input, state, side)
        .into_iter()
        .filter(|id| id != taker && gk != Some(id.as_str()) && state.player(id).map(|r| r.on_pitch).unwrap_or(false))
        .map(|id| {
            let score = aerial_attack_score(input, side, &id);
            (id, score)
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(max);
    out.into_iter().map(|(id, _)| id).collect()
}

fn assign_attacking_roles(input: &MatchInput, side: TeamSide, attackers: &[String]) -> Vec<RoleAssignment> {
    let mut pool: Vec<String> = attackers.to_vec();
    let mut out: Vec<RoleAssignment> = Vec::new();
    add_best_role(input, side, &mut pool, &mut out, "CENTRAL_TARGET", 0);
    add_best_role(input, side, &mut pool, &mut out, "NEAR_POST", 1);
    add_best_role(input, side, &mut pool, &mut out, "FAR_POST", 2);
    add_best_role(input, side, &mut pool, &mut out, "EDGE", 3);
    for (n, id) in pool.into_iter().enumerate() {
        out.push(RoleAssignment {
            player_id: id,
            role: if n % 2 == 0 { "CENTRAL_SUPPORT" } else { "SECOND_BALL" },
        });
    }
    out
}

fn add_best_role(
    input: &MatchInput,
    side: TeamSide,
    pool: &mut Vec<String>,
    out: &mut Vec<RoleAssignment>,
    role: &'static str,
    mode: i32,
) {
    if pool.is_empty() {
        return;
    }
    let team = input.team(side);
    let mut best_idx: Option<usize> = None;
    let mut best_v = f64::NEG_INFINITY;
    for (i, id) in pool.iter().enumerate() {
        let p = match team.player(id) {
            Some(p) => p,
            None => continue,
        };
        let a = &p.attributes;
        let q = match mode {
            0 => a.get(7) as f64 * 0.42 + a.get(10) as f64 * 0.25 + a.get(15) as f64 * 0.20 + a.get(13) as f64 * 0.13,
            1 => a.get(15) as f64 * 0.34 + a.get(8) as f64 * 0.25 + a.get(7) as f64 * 0.25 + a.get(13) as f64 * 0.16,
            2 => a.get(7) as f64 * 0.35 + a.get(15) as f64 * 0.30 + a.get(10) as f64 * 0.18 + a.get(8) as f64 * 0.17,
            _ => a.get(6) as f64 * 0.32 + a.get(4) as f64 * 0.24 + a.get(2) as f64 * 0.20 + a.get(13) as f64 * 0.24,
        };
        if best_idx.is_none() || q > best_v || (q == best_v && id < &pool[best_idx.unwrap()]) {
            best_idx = Some(i);
            best_v = q;
        }
    }
    if let Some(idx) = best_idx {
        let id = pool.remove(idx);
        out.push(RoleAssignment { player_id: id, role });
    }
}

fn place_attacking_roles(state: &mut MatchState, side: TeamSide, roles: &[RoleAssignment]) {
    let dir = geometry::attack_direction(side);
    let g = geometry::goal_y(side);
    let mut support = 0;
    for a in roles {
        let (x, y) = match a.role {
            "NEAR_POST" => (38, geometry::clamp(g - dir * 6)),
            "FAR_POST" => (64, geometry::clamp(g - dir * 9)),
            "CENTRAL_TARGET" => (50, geometry::clamp(g - dir * 8)),
            "EDGE" => (50, geometry::clamp(g - dir * 19)),
            _ => {
                let sx = geometry::clamp(43 + (support % 2) * 14);
                support += 1;
                (sx, geometry::clamp(g - dir * 13))
            }
        };
        if let Some(r) = state.player_mut(&a.player_id) {
            r.set_position(x, y);
            r.set_target(x, y);
            r.current_task = format!("SET_PIECE_ATTACK_{}", a.role);
        }
    }
}

fn protect_against_counter(
    input: &MatchInput,
    state: &mut MatchState,
    attack: TeamSide,
    taker: &str,
    roles: &[RoleAssignment],
) {
    let mut used: HashSet<&str> = HashSet::new();
    used.insert(taker);
    for a in roles {
        used.insert(a.player_id.as_str());
    }
    let gk = state.team(attack).designated_goalkeeper_id.as_deref();
    let team = input.team(attack);
    let mut candidates: Vec<(String, f64)> = roster::active_player_ids_in_slot_order(input, state, attack)
        .into_iter()
        .filter(|id| !used.contains(id.as_str()) && gk != Some(id.as_str()) && state.player(id).map(|r| r.on_pitch).unwrap_or(false))
        .map(|id| {
            let s = counter_score(team.player(&id));
            (id, s)
        })
        .collect();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (i, (id, _)) in candidates.into_iter().take(2).enumerate() {
        let x = if i == 0 { 42 } else { 58 };
        let y = y_from_progress(attack, 58 - i as i32 * 4);
        if let Some(r) = state.player_mut(&id) {
            r.set_position(x, y);
            r.set_target(x, y);
            r.current_task = "SET_PIECE_COUNTER_PROTECTION".into();
        }
    }
}

fn prepare_defence(input: &MatchInput, state: &mut MatchState, attack: TeamSide, attackers: &[RoleAssignment]) {
    let defend = attack.opposite();
    let gk = state.team(defend).designated_goalkeeper_id.clone();
    let mut ds: Vec<(String, f64)> = state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == defend && gk.as_deref() != Some(r.player_id.as_str()))
        .map(|(_, r)| (r.player_id.clone(), defence_score(input, defend, &r.player_id)))
        .collect();
    ds.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut pool: Vec<String> = ds.into_iter().map(|(id, _)| id).collect();

    let dir = geometry::attack_direction(attack);
    let goal_y = geometry::goal_y(attack);
    let zones = pool.len().min(3);
    let zx = [38, 50, 62];
    for i in 0..zones {
        let did = pool.remove(0);
        let x = zx[i];
        let y = geometry::clamp(goal_y - dir * if i == 1 { 6 } else { 4 });
        if let Some(d) = state.player_mut(&did) {
            d.set_position(x, y);
            d.set_target(x, y);
            d.current_task = match i {
                0 => "SET_PIECE_ZONE_NEAR",
                1 => "SET_PIECE_ZONE_CENTRAL",
                _ => "SET_PIECE_ZONE_FAR",
            }
            .into();
            d.marking_player_id = None;
        }
    }

    let gy = geometry::own_goal_y_spatial(defend);
    for ar in attackers {
        if pool.is_empty() || ar.role == "EDGE" || ar.role == "SECOND_BALL" {
            continue;
        }
        let (ax, ay) = match state.player(&ar.player_id) {
            Some(r) => (r.x, r.y),
            None => continue,
        };
        let mut best_idx: Option<usize> = None;
        let mut bd = f64::INFINITY;
        for (idx, did) in pool.iter().enumerate() {
            if let Some(d) = state.player(did) {
                let q = geometry::distance(d.x, d.y, ax, ay) - defence_score(input, defend, did) * 0.025;
                if best_idx.is_none() || q < bd {
                    best_idx = Some(idx);
                    bd = q;
                }
            }
        }
        if let Some(idx) = best_idx {
            let did = pool.remove(idx);
            let x = geometry::clamp(java_round(ax as f64 + (50 - ax) as f64 * 0.05));
            let y = geometry::clamp(java_round(ay as f64 + (gy - ay) as f64 * 0.08));
            if let Some(d) = state.player_mut(&did) {
                d.marking_player_id = Some(ar.player_id.clone());
                d.set_position(x, y);
                d.set_target(x, y);
                d.current_task = "SET_PIECE_MAN_MARK".into();
            }
        }
    }
}

fn choose_delivery_target<'a>(
    input: &MatchInput,
    _state: &MatchState,
    rng: &mut DeterministicRng,
    side: TeamSide,
    taker: &str,
    roles: &'a [RoleAssignment],
    corner: bool,
) -> Option<&'a RoleAssignment> {
    if roles.is_empty() {
        return None;
    }
    let crossing = input
        .team(side)
        .player(taker)
        .map(|p| p.attributes.get(3) as f64)
        .unwrap_or(70.0);
    let targetable: Vec<&RoleAssignment> = roles
        .iter()
        .filter(|a| matches!(a.role, "NEAR_POST" | "CENTRAL_TARGET" | "FAR_POST"))
        .collect();
    if targetable.is_empty() {
        return Some(&roles[0]);
    }
    let mut total = 0.0;
    let mut weights = Vec::with_capacity(targetable.len());
    for r in &targetable {
        let mut q = aerial_attack_score(input, side, &r.player_id) * 0.35
            + match r.role {
                "CENTRAL_TARGET" => 22.0,
                "NEAR_POST" => 18.0,
                _ => 16.0,
            }
            + (crossing - 50.0) * 0.08;
        if corner && r.role == "NEAR_POST" {
            q += 3.0;
        }
        let w = q.max(1.0);
        weights.push(w);
        total += w;
    }
    let draw = rng.next_double() * total;
    let mut acc = 0.0;
    for (i, w) in weights.iter().enumerate() {
        acc += *w;
        if draw < acc {
            return Some(targetable[i]);
        }
    }
    targetable.last().copied()
}

fn prepare_goalkeeper(
    input: &MatchInput,
    state: &mut MatchState,
    defend: TeamSide,
    zone: &str,
    delivery_x: i32,
    _delivery_y: i32,
) {
    let g = match state.team(defend).designated_goalkeeper_id.clone() {
        Some(g) => g,
        None => return,
    };
    let p = match input.team(defend).player(&g) {
        Some(p) => p,
        None => return,
    };
    let on_pitch = state.player(&g).map(|r| r.on_pitch).unwrap_or(false);
    if !on_pitch {
        return;
    }
    let dir = geometry::attack_direction(defend);
    let base_y = geometry::clamp(geometry::own_goal_y_spatial(defend) + 3 * dir);
    let q = p.attributes.get(0) as i32 + p.attributes.get(13) as i32 + p.attributes.get(9) as i32;
    let aggressive = q >= 220 && zone != "FAR_POST";
    let x = if aggressive {
        geometry::clamp(java_round(50.0 + (delivery_x - 50) as f64 * 0.18))
    } else {
        50
    };
    let y = if aggressive {
        geometry::clamp(base_y + 2 * dir)
    } else {
        base_y
    };
    if let Some(r) = state.player_mut(&g) {
        r.set_position(x, y);
        r.set_target(x, y);
        r.current_task = if aggressive { "SET_PIECE_GK_CLAIM" } else { "SET_PIECE_GK_HOLD" }.into();
    }
}

fn aerial_attack_score(input: &MatchInput, s: TeamSide, id: &str) -> f64 {
    match input.team(s).player(id) {
        None => 0.0,
        Some(p) => {
            let a = &p.attributes;
            a.get(7) as f64 * 0.38
                + a.get(10) as f64 * 0.20
                + a.get(15) as f64 * 0.24
                + a.get(13) as f64 * 0.10
                + a.get(8) as f64 * 0.08
        }
    }
}

fn defence_score(input: &MatchInput, s: TeamSide, id: &str) -> f64 {
    match input.team(s).player(id) {
        None => 0.0,
        Some(p) => {
            let a = &p.attributes;
            a.get(7) as f64 * 0.25
                + a.get(10) as f64 * 0.18
                + a.get(16) as f64 * 0.32
                + a.get(13) as f64 * 0.17
                + a.get(8) as f64 * 0.08
        }
    }
}

fn counter_score(p: Option<&PlayerMatchSnapshot>) -> f64 {
    match p {
        None => 0.0,
        Some(p) => {
            let a = &p.attributes;
            a.get(16) as f64 * 0.34
                + a.get(8) as f64 * 0.24
                + a.get(13) as f64 * 0.24
                + a.get(17) as f64 * 0.18
        }
    }
}

fn y_from_progress(side: TeamSide, p: i32) -> i32 {
    if side == TeamSide::Home {
        geometry::clamp(p)
    } else {
        geometry::clamp(100 - p)
    }
}

pub fn resolve_restart(c: &mut dyn MatchRuntime) {
    let r = c.state().restart.clone();
    let side = match r.team {
        Some(s) => s,
        None => {
            c.state_mut().restart.clear();
            return;
        }
    };
    c.state_mut().restart.clear();
    let waste = tactical_models::effective_time_wasting(c.input(), c.state(), side);
    let natural_delay = 8 + c.rng_mut().next_int(10);
    let deliberate_delay = java_round(waste as f64 * 0.075);
    let fast_mode = c.input().simulated_action_minutes() < 90;
    let delay = if fast_mode {
        2 + c.rng_mut().next_int(2)
    } else {
        natural_delay + deliberate_delay
    };
    c.advance_playing_time(delay);
    let natural_credit = (natural_delay - 7).max(0);
    let deliberate_credit = java_round(deliberate_delay as f64 * 0.55);
    c.add_stoppage(if fast_mode { 4 } else { natural_credit + deliberate_credit });

    match r.kind {
        RestartType::Kickoff => {
            let p = choose_central(c, side);
            restart_control(c, side, p.as_deref(), 50, 50, EventType::Kickoff, &r.reason);
        }
        RestartType::ThrowIn => {
            let p = nearest_outfield(c, side, r.x, r.y);
            restart_control(c, side, p.as_deref(), r.x, r.y, EventType::ThrowIn, &r.reason);
        }
        RestartType::GoalKick => {
            let g = c.state().team(side).designated_goalkeeper_id.clone();
            restart_control(c, side, g.as_deref(), 50, geometry::goal_kick_y(side), EventType::GoalKick, &r.reason);
        }
        RestartType::FreeKick => resolve_free_kick(c, side, &r),
        RestartType::Corner => resolve_corner(c, side, &r),
        RestartType::Penalty => resolve_penalty(c, side),
        RestartType::None => {}
    }
}

fn resolve_free_kick(c: &mut dyn MatchRuntime, side: TeamSide, r: &RestartState) {
    let progress = geometry::attacking_progress(side, r.y);
    let close = progress > 70;
    let box_slots = if close {
        c.state().team(side).set_piece_plan.close_free_kick_box_slot_ids.clone()
    } else {
        c.state().team(side).set_piece_plan.far_free_kick_box_slot_ids.clone()
    };
    let taker = match choose_set_piece_taker(c, side, r, true) {
        Some(t) => t,
        None => return,
    };
    let take_direct = {
        let input = c.input().clone();
        let state_snap = c.state().clone();
        should_take_direct_free_kick(&input, &state_snap, c.rng_mut(), side, &taker, r.x, r.y)
    };
    if take_direct {
        begin_set_piece_possession(c, side, &taker, r.x, r.y);
        let input = c.input().clone();
        let wall = prepare_wall(&input, c.state_mut(), side, r.x, r.y);
        let opp_gk = c.state().team(side.opposite()).designated_goalkeeper_id.clone();
        c.emit(
            EventType::FreeKick,
            Some(side),
            Some(&taker),
            opp_gk.as_deref(),
            vec![
                kv("reason", &r.reason),
                kv("direct", true),
                kv("wallSize", wall),
                kv("range", if close { "CLOSE" } else { "FAR" }),
            ],
        );
        shot_resolver::resolve_shot(c, side, &taker, false, (wall * 3).max(4), "DIRECT_FREE_KICK");
        return;
    }

    let threshold = if c.input().simulated_action_minutes() < 90 { 48 } else { 55 };
    if progress < threshold {
        restart_control(c, side, Some(&taker), r.x, r.y, EventType::FreeKick, &r.reason);
        return;
    }
    begin_set_piece_possession(c, side, &taker, r.x, r.y);
    let d = {
        let input = c.input().clone();
        let mut st = c.state().clone();
        let res = prepare_delivery(&input, &mut st, c.rng_mut(), side, &box_slots, &taker, false);
        *c.state_mut() = st;
        res
    };
    let target_id = match d.target_id.as_deref() {
        Some(t) => t,
        None => {
            restart_control(c, side, Some(&taker), r.x, r.y, EventType::FreeKick, &r.reason);
            return;
        }
    };
    c.emit(
        EventType::FreeKick,
        Some(side),
        Some(&taker),
        Some(target_id),
        vec![
            kv("reason", &r.reason),
            kv("delivery", true),
            kv("direct", false),
            kv("range", if close { "CLOSE" } else { "FAR" }),
            kv("zone", &d.zone),
            kv("boxTarget", target_id),
        ],
    );
    c.emit(
        EventType::PossessionStarted,
        Some(side),
        Some(&taker),
        Some(target_id),
        vec![kv("restart", "FREE_KICK"), kv("delivery", true), kv("zone", &d.zone)],
    );
    let dt = 2 + c.rng_mut().next_int(3);
    c.advance_playing_time(dt);
    let intent = format!("SET_PIECE_TO:{}:{}", target_id, d.zone);
    c.state_mut()
        .ball
        .set_airborne(d.x, d.y, Some(side), Some(taker), Some(&intent));
    duel_resolver::resolve_airborne(c);
}

fn resolve_corner(c: &mut dyn MatchRuntime, side: TeamSide, r: &RestartState) {
    c.state_mut().team_mut(side).corners += 1;
    let taker = match choose_set_piece_taker(c, side, r, false) {
        Some(t) => t,
        None => return,
    };
    let box_slots = c.state().team(side).set_piece_plan.corner_box_slot_ids.clone();
    begin_set_piece_possession(c, side, &taker, r.x, r.y);
    let d = {
        let input = c.input().clone();
        let mut st = c.state().clone();
        let res = prepare_delivery(&input, &mut st, c.rng_mut(), side, &box_slots, &taker, true);
        *c.state_mut() = st;
        res
    };
    let default_y = (geometry::goal_y(side) - geometry::attack_direction(side) * 8).clamp(3, 97);
    let target = d
        .target_id
        .clone()
        .or_else(|| best_aerial(c, side, 50, default_y, true));
    let (tx, ty) = if d.target_id.is_some() { (d.x, d.y) } else { (50, default_y) };
    let target_str = target.as_deref().unwrap_or("");
    c.emit(
        EventType::Corner,
        Some(side),
        Some(&taker),
        target.as_deref(),
        vec![
            kv("x", r.x),
            kv("y", r.y),
            kv("boxTarget", target_str),
            kv("zone", &d.zone),
            kv("nearPost", d.near_count),
            kv("farPost", d.far_count),
            kv("central", d.central_count),
        ],
    );
    c.emit(
        EventType::PossessionStarted,
        Some(side),
        Some(&taker),
        target.as_deref(),
        vec![kv("restart", "CORNER"), kv("delivery", true), kv("zone", &d.zone)],
    );
    let intent = format!("SET_PIECE_TO:{}:{}", target_str, d.zone);
    c.state_mut()
        .ball
        .set_airborne(tx, ty, Some(side), Some(taker), Some(&intent));
    duel_resolver::resolve_airborne(c);
}

fn resolve_penalty(c: &mut dyn MatchRuntime, side: TeamSide) {
    let taker = match choose_penalty_taker(c, side) {
        Some(t) => t,
        None => return,
    };
    c.state_mut().ball.set_position(50, geometry::penalty_spot_y(side));
    c.state_mut().ball.set_controlled(side, taker.clone());
    let opp_gk = c.state().team(side.opposite()).designated_goalkeeper_id.clone();
    c.emit(EventType::PenaltyKick, Some(side), Some(&taker), opp_gk.as_deref(), Vec::new());
    shot_resolver::resolve_shot(c, side, &taker, false, 0, "PENALTY");
}

fn restart_control(
    c: &mut dyn MatchRuntime,
    side: TeamSide,
    player_opt: Option<&str>,
    x: i32,
    y: i32,
    event_type: EventType,
    reason: &str,
) {
    let mut player = player_opt.map(|s| s.to_string());
    let active = player
        .as_deref()
        .and_then(|id| c.state().player(id))
        .map(|r| r.on_pitch)
        .unwrap_or(false);
    if !active {
        player = nearest_outfield(c, side, x, y);
    }
    let player_id = match player {
        Some(id) => id,
        None => return,
    };
    if let Some(r) = c.state_mut().player_mut(&player_id) {
        r.set_position(x, y);
        r.set_target(x, y);
    }
    c.state_mut().ball.set_position(x, y);
    c.state_mut().ball.set_controlled(side, player_id.clone());
    if event_type == EventType::Kickoff {
        c.state_mut().ball.set_movement_intent("KICKOFF_RESTART");
    }
    let old = c.state().possession.controlling_team;
    let sec = c.state().clock.game_second;
    if event_type == EventType::Kickoff || old.is_none() || old == Some(side) {
        c.state_mut().possession.begin(side, sec);
    } else {
        let _ = c.state_mut().possession.turnover_to(side, sec);
    }
    c.state_mut().team_mut(side).possession_sequences += 1;
    c.state_mut().possession.phase = TacticalPhase::from_ball(side, y);
    let restart_name = match event_type {
        EventType::Kickoff => "KICKOFF",
        EventType::ThrowIn => "THROW_IN",
        EventType::GoalKick => "GOAL_KICK",
        EventType::FreeKick => "FREE_KICK",
        _ => "RESTART",
    };
    c.emit(event_type, Some(side), Some(&player_id), None, vec![kv("reason", reason)]);
    c.emit(
        EventType::PossessionStarted,
        Some(side),
        Some(&player_id),
        None,
        vec![kv("restart", restart_name)],
    );
}

fn begin_set_piece_possession(c: &mut dyn MatchRuntime, side: TeamSide, taker: &str, x: i32, y: i32) {
    if let Some(r) = c.state_mut().player_mut(taker) {
        r.set_position(x, y);
        r.set_target(x, y);
    }
    c.state_mut().ball.set_position(x, y);
    c.state_mut().ball.set_controlled(side, taker);
    let old = c.state().possession.controlling_team;
    let sec = c.state().clock.game_second;
    if old.is_none() || old == Some(side) {
        c.state_mut().possession.begin(side, sec);
    } else {
        let _ = c.state_mut().possession.turnover_to(side, sec);
    }
    c.state_mut().team_mut(side).possession_sequences += 1;
    c.state_mut().possession.phase = TacticalPhase::from_ball(side, y);
}

fn choose_set_piece_taker(c: &dyn MatchRuntime, side: TeamSide, r: &RestartState, free_kick: bool) -> Option<String> {
    let plan = &c.state().team(side).set_piece_plan;
    let slots = if free_kick {
        if geometry::attacking_progress(side, r.y) > 70 {
            &plan.close_free_kick_taker_slot_ids
        } else {
            &plan.far_free_kick_taker_slot_ids
        }
    } else {
        &plan.corner_taker_slot_ids
    };
    for slot in slots {
        if let Some(id) = roster::active_player_in_slot(c.state(), side, slot) {
            return Some(id);
        }
    }
    let team = c.input().team(side);
    let mut best: Option<String> = None;
    let mut best_val = -1;
    for id in roster::active_player_ids_in_slot_order(c.input(), c.state(), side) {
        let p = match team.player(&id) {
            Some(p) => p,
            None => continue,
        };
        let a = &p.attributes;
        let q = if free_kick {
            a.get(6) as i32 + a.get(4) as i32 + a.get(2) as i32
        } else {
            a.get(3) as i32 + a.get(4) as i32
        };
        if q > best_val || (q == best_val && best.as_ref().map(|b| id < *b).unwrap_or(true)) {
            best_val = q;
            best = Some(id);
        }
    }
    best
}

fn choose_penalty_taker(c: &dyn MatchRuntime, side: TeamSide) -> Option<String> {
    for slot in &c.state().team(side).set_piece_plan.penalty_taker_slot_ids {
        if let Some(id) = roster::active_player_in_slot(c.state(), side, slot) {
            return Some(id);
        }
    }
    let team = c.input().team(side);
    let mut best: Option<String> = None;
    let mut best_val = -1;
    for id in roster::active_player_ids_in_slot_order(c.input(), c.state(), side) {
        let p = match team.player(&id) {
            Some(p) => p,
            None => continue,
        };
        let a = &p.attributes;
        let q = a.get(6) as i32 + a.get(14) as i32 + a.get(4) as i32;
        if q > best_val || (q == best_val && best.as_ref().map(|b| id < *b).unwrap_or(true)) {
            best_val = q;
            best = Some(id);
        }
    }
    best
}
