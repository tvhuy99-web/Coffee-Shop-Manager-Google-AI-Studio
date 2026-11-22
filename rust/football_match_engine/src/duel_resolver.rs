use crate::{
    contextual::{self, CrossContext, GroundDuelContext},
    defensive,
    discipline,
    events::EventType,
    fatigue::{self, FatigueInput, FatigueState, Task},
    geometry::{self, TeamSide},
    possession_decision::ActionDecision,
    pressure,
    resolver_support::{best_aerial, best_defender, duel_attr, java_round, kv, make_loose, MatchRuntime},
    roster,
    shot_resolver,
    state::RestartType,
};

pub fn resolve_carry(c: &mut dyn MatchRuntime, d: &ActionDecision, pressure_val: i32) {
    let side = match c.state().ball.owner_team {
        Some(s) => s,
        None => return,
    };
    let (ax, ay) = match c.state().player(&d.actor_id) {
        Some(r) => (r.x, r.y),
        None => return,
    };
    let p_pos = c
        .input()
        .team(side)
        .player(&d.actor_id)
        .map(|p| p.primary_position.clone())
        .unwrap_or_default();
    let fast_mode = c.input().simulated_action_minutes() < 90;
    let step = (if fast_mode { 8 } else { 4 }) + c.rng_mut().next_int(if fast_mode { 9 } else { 7 });
    let dir = geometry::attack_direction(side);
    let prog = geometry::attacking_progress(side, ay);
    let mut lateral_shift = c.rng_mut().next_int(7) - 3;
    if prog >= 56
        && (ax - 50).abs() >= 18
        && matches!(p_pos.as_str(), "LW" | "RW" | "AM" | "CF" | "LM" | "RM")
    {
        let inward = 5 + c.rng_mut().next_int(7);
        lateral_shift = if ax < 50 { inward } else { -inward };
    }
    let nx = (ax + lateral_shift).clamp(2, 98);
    let ny = (ay + dir * step).clamp(2, 98);
    let dt = if fast_mode {
        2 + c.rng_mut().next_int(2)
    } else {
        3 + c.rng_mut().next_int(4)
    };
    c.advance_playing_time(dt);
    if let Some(a) = c.state_mut().player_mut(&d.actor_id) {
        a.set_position(nx, ny);
    }
    c.state_mut().ball.set_position(nx, ny);
    c.state_mut().ball.set_controlled(side, d.actor_id.clone());
    c.state_mut().team_mut(side).carries += 1;
    c.emit(
        EventType::Carry,
        Some(side),
        Some(&d.actor_id),
        None,
        vec![kv("fromPressure", pressure_val), kv("progress", step)],
    );

    let defender = best_defender(c, side.opposite(), nx, ny);
    let challenge = defender.as_deref().and_then(|did| {
        let dr = c.state().player(did)?;
        if geometry::distance(dr.x, dr.y, nx, ny) < 9.4 {
            Some(did.to_string())
        } else {
            None
        }
    });
    if let Some(did) = challenge {
        let threshold = (if fast_mode { 40 } else { 38 }) + pressure_val / 2;
        if c.rng_mut().next_int(100) < threshold {
            resolve_challenge(c, &d.actor_id, &did, false, pressure_val);
            return;
        }
    }
    if let Some(stamina) = c.input().team(side).player(&d.actor_id).map(|p| p.attributes.get(11) as i32) {
        let tempo = c.state().team(side).team_instructions.tempo;
        if let Some(r) = c.state_mut().player_mut(&d.actor_id) {
            let task = if r.current_task == "PRESS" {
                Task::Press
            } else if r.current_task.starts_with("RECOVERY") {
                Task::Recovery
            } else if r.current_task == "SUPPORT_RUN" {
                Task::SupportRun
            } else if r.current_task == "ROAMING_RUN" {
                Task::RoamingRun
            } else if r.current_task == "BALL_CARRIER" {
                Task::BallCarrier
            } else {
                Task::Idle
            };
            let mut fs = FatigueState {
                energy: r.energy,
                workload: r.workload as i64,
                distance_tenths: r.distance_covered_tenths as i64,
                fatigue_accumulator: r.fatigue_accumulator,
                acute_fatigue: r.acute_fatigue,
            };
            fatigue::apply(
                &mut fs,
                FatigueInput {
                    duration_seconds: 2,
                    distance_tenths: step * 10,
                    tempo,
                    pressing_intensity: 50,
                    transition_risk: 50,
                    active_outfield: 10,
                    stamina,
                    task,
                    assigned_defensive_role: r.defensive_role != defensive::ROLE_NONE,
                },
                r.on_pitch,
            );
            r.energy = fs.energy;
            r.workload = fs.workload.min(i32::MAX as i64) as i32;
            r.distance_covered_tenths = fs.distance_tenths.min(i32::MAX as i64) as i32;
            r.fatigue_accumulator = fs.fatigue_accumulator;
            r.acute_fatigue = fs.acute_fatigue;
        }
    }
}

pub fn resolve_dribble(c: &mut dyn MatchRuntime, d: &ActionDecision, pressure_val: i32) {
    let side = match c.state().ball.owner_team {
        Some(s) => s,
        None => return,
    };
    let (bx, by) = (c.state().ball.x, c.state().ball.y);
    let defender = match best_defender(c, side.opposite(), bx, by) {
        Some(id) => id,
        None => return,
    };
    c.state_mut().team_mut(side).dribbles_attempted += 1;
    c.emit(
        EventType::DribbleStarted,
        Some(side),
        Some(&d.actor_id),
        Some(&defender),
        vec![kv("pressure", pressure_val)],
    );
    resolve_challenge(c, &d.actor_id, &defender, true, pressure_val);
}

fn resolve_challenge(
    c: &mut dyn MatchRuntime,
    attacker_id: &str,
    defender_id: &str,
    dribble: bool,
    pressure_val: i32,
) {
    let (attack, defend, ar_x, ar_y, ar_energy, dr_x, dr_y, dr_energy, dr_acute) = {
        let s = c.state();
        let ar = match s.player(attacker_id) {
            Some(r) if r.on_pitch => r,
            _ => return,
        };
        let dr = match s.player(defender_id) {
            Some(r) if r.on_pitch => r,
            _ => return,
        };
        (ar.side, dr.side, ar.x, ar.y, ar.energy, dr.x, dr.y, dr.energy, dr.acute_fatigue)
    };
    let ap = match c.input().team(attack).player(attacker_id).cloned() {
        Some(p) => p,
        None => return,
    };
    let dp = match c.input().team(defend).player(defender_id).cloned() {
        Some(p) => p,
        None => return,
    };

    c.state_mut().team_mut(defend).tackles_attempted += 1;
    c.emit(
        EventType::TackleAttempted,
        Some(defend),
        Some(defender_id),
        Some(attacker_id),
        vec![kv("dribble", dribble)],
    );
    let dt = 2 + c.rng_mut().next_int(3);
    c.advance_playing_time(dt);

    let mut aggr = c.state().team(defend).team_instructions.aggression;
    if let Some(dsi) = roster::slot_instructions(c.state(), defend, defender_id) {
        aggr = (aggr + dsi.defensive_aggression) / 2;
    }
    let progress = geometry::attacking_progress(attack, ar_y);
    let atk_ctx = GroundDuelContext {
        dribble,
        pressure: pressure_val,
        progress,
        defensive_aggression: aggr,
        energy: ar_energy,
    };
    let def_ctx = GroundDuelContext {
        dribble,
        pressure: pressure_val,
        progress,
        defensive_aggression: aggr,
        energy: dr_energy,
    };
    let atk_roll = c.rng_mut().next_int(21) - 10;
    let def_roll = c.rng_mut().next_int(21) - 10;
    let atk = contextual::ground_duel_attacker(&ap.attributes, &atk_ctx)
        + c.state().team(attack).match_day_performance * 0.75
        + atk_roll as f64;
    let def = contextual::ground_duel_defender(&dp.attributes, &def_ctx)
        + c.state().team(defend).match_day_performance * 0.75
        + def_roll as f64;

    let pressing = c.state().team(defend).team_instructions.pressing_intensity;
    let base_foul = if c.input().simulated_action_minutes() < 90 { 31 } else { 22 };
    let mut foul_chance = (base_foul
        + (aggr - 50) / 5
        + (74 - dp.attributes.get(1) as i32) / 6
        + (74 - dp.attributes.get(19) as i32) / 7
        + dr_acute / 10
        + (pressing - 50).max(0) / 12)
        .clamp(8, 54);
    if geometry::in_attacking_penalty_area(attack, ar_x, ar_y) {
        foul_chance = (foul_chance / 12).max(1);
    }
    let foul_roll = c.rng_mut().next_int(100);
    #[cfg(test)]
    if c.input().seed == 123456789
        && c.state().clock.game_second == 2962
        && attacker_id == "H10"
        && defender_id == "A02"
    {
        let current_defender_fatigue = c.state().player(defender_id).map(|p| p.acute_fatigue);
        eprintln!(
            "PARITY DEBUG seed={} sec={} dribble={} attack={:?} defend={:?} attacker=({}, {}) current_attacker=({:?}) current_defender=({}, {}) aggr={} pressing={} foul_chance={} foul_roll={} atk_score={:.3} def_score={:.3} attacker_energy={} defender_energy={} defender_acute_before={} defender_acute_now={:?} attr_tackle={} attr_temperament={} rng_state={}",
            c.input().seed,
            c.state().clock.game_second,
            dribble,
            attack,
            defend,
            ar_x,
            ar_y,
            c.state().player(attacker_id).map(|p| (p.x, p.y)),
            dr_x,
            dr_y,
            aggr,
            pressing,
            foul_chance,
            foul_roll,
            atk,
            def,
            ar_energy,
            dr_energy,
            dr_acute,
            current_defender_fatigue,
            dp.attributes.get(1),
            dp.attributes.get(19),
            c.rng_mut().state()
        );
    }
    if foul_roll < foul_chance {
        let advantage = dribble && c.rng_mut().next_int(100) < 18 && progress > 55;
        discipline::handle_foul(c, defender_id, attacker_id, advantage);
        if advantage {
            c.state_mut().team_mut(attack).dribbles_won += 1;
            c.emit(
                EventType::DribbleWon,
                Some(attack),
                Some(attacker_id),
                Some(defender_id),
                vec![kv("advantage", true)],
            );
        }
        return;
    }

    c.emit(
        EventType::GroundDuel,
        Some(attack),
        Some(attacker_id),
        Some(defender_id),
        vec![kv("attackerScore", atk as i32), kv("defenderScore", def as i32)],
    );
    if atk >= def {
        let dir = geometry::attack_direction(attack);
        let step = 5 + c.rng_mut().next_int(8);
        let mut lateral_shift = c.rng_mut().next_int(9) - 4;
        if progress >= 56
            && (ar_x - 50).abs() >= 18
            && matches!(ap.primary_position.as_str(), "LW" | "RW" | "AM" | "CF" | "LM" | "RM")
        {
            let inward = 6 + c.rng_mut().next_int(7);
            lateral_shift = if ar_x < 50 { inward } else { -inward };
        }
        let (base_x, base_y) = c
            .state()
            .player(attacker_id)
            .map(|player| (player.x, player.y))
            .unwrap_or((ar_x, ar_y));
        let nx = (base_x + lateral_shift).clamp(2, 98);
        let ny = (base_y + dir * step).clamp(2, 98);
        if let Some(ar) = c.state_mut().player_mut(attacker_id) {
            ar.set_position(nx, ny);
        }
        c.state_mut().ball.set_position(nx, ny);
        c.state_mut().ball.set_controlled(attack, attacker_id);
        if dribble {
            c.state_mut().team_mut(attack).dribbles_won += 1;
        }
        c.emit(
            if dribble { EventType::DribbleWon } else { EventType::TackleLost },
            Some(attack),
            Some(attacker_id),
            Some(defender_id),
            vec![kv("progress", step)],
        );
    } else {
        c.state_mut().team_mut(defend).tackles_won += 1;
        c.emit(
            EventType::TackleWon,
            Some(defend),
            Some(defender_id),
            Some(attacker_id),
            vec![kv("clean", true)],
        );
        if dribble {
            c.emit(
                EventType::DribbleLost,
                Some(attack),
                Some(attacker_id),
                Some(defender_id),
                vec![kv("cause", "TACKLE")],
            );
        }
        if c.rng_mut().next_int(100) < 52 {
            make_loose(c, Some(defend), Some(defender_id), (ar_x + dr_x) / 2, (ar_y + dr_y) / 2, "TACKLE_DEFLECTION");
        } else {
            c.change_possession(defend, defender_id, "TACKLE_WON");
        }
    }
    discipline::maybe_injury(c, attacker_id, defender_id, aggr);
}

pub fn resolve_cross(c: &mut dyn MatchRuntime, d: &ActionDecision, pressure_val: i32) {
    let side = match c.state().ball.owner_team {
        Some(s) => s,
        None => return,
    };
    let target_id = match d.target_id.as_deref() {
        Some(id) => id,
        None => {
            resolve_carry(c, d, pressure_val);
            return;
        }
    };
    let (tx, ty) = match c.state().player(target_id) {
        Some(r) => (r.x, r.y),
        None => {
            resolve_carry(c, d, pressure_val);
            return;
        }
    };
    let target_dist = geometry::distance(c.state().ball.x, c.state().ball.y, tx, ty);
    let target_pressure = pressure::estimate_at_point(c.input(), c.state(), side.opposite(), tx, ty);
    let quality = match c.input().team(side).player(&d.actor_id) {
        Some(crosser) => {
            let ctx = CrossContext {
                pressure: pressure_val,
                target_distance: target_dist,
                aerial_difficulty: target_pressure,
            };
            74.0 + contextual::cross_execution_adjustment(&crosser.attributes, &ctx) - pressure_val as f64 * 0.22
        }
        None => 50.0,
    };
    let q = java_round(quality).clamp(10, 95);
    let lateral_err = (12 - q / 10).max(2);
    let depth_err = (11 - q / 11).max(2);
    let intended_x = java_round(50.0 + (tx - 50) as f64 * 0.62).clamp(28, 72);
    let intended_y = geometry::attacking_progress(side, ty).max(82).clamp(82, 94);
    let x = (intended_x + c.rng_mut().next_int(lateral_err * 2 + 1) - lateral_err).clamp(25, 75);
    let progress = (intended_y + c.rng_mut().next_int(depth_err * 2 + 1) - depth_err).clamp(78, 96);
    let y = if side == TeamSide::Home { progress } else { 100 - progress };

    c.state_mut().team_mut(side).crosses += 1;
    c.emit(
        EventType::Cross,
        Some(side),
        Some(&d.actor_id),
        Some(target_id),
        vec![kv("pressure", pressure_val), kv("crossQuality", q)],
    );
    let dt = 3 + c.rng_mut().next_int(3);
    c.advance_playing_time(dt);
    let intent = format!("CROSS_TO:{}:QUALITY:{}", target_id, q);
    c.state_mut()
        .ball
        .set_airborne(x, y, Some(side), Some(d.actor_id.clone()), Some(&intent));
    resolve_airborne(c);
}

pub fn resolve_airborne(c: &mut dyn MatchRuntime) {
    let (bx, by, attack, last_player, intent) = {
        let b = &c.state().ball;
        (b.x, b.y, b.last_touch_team, b.last_touch_player_id.clone(), b.movement_intent.clone())
    };
    let attack = match attack {
        Some(a) => a,
        None => {
            make_loose(c, None, None, bx, by, "AIRBORNE_UNKNOWN");
            return;
        }
    };
    let defend = attack.opposite();
    let gk = c.state().team(defend).designated_goalkeeper_id.clone();
    let intended_att = intended_aerial_target(c, attack, &intent);
    let attacker = intended_att.or_else(|| best_aerial(c, attack, bx, by, true));
    let defender = best_aerial(c, defend, bx, by, false);
    let planned_set_piece = intent.starts_with("SET_PIECE_TO:");

    if let Some(ref gid) = gk {
        let gk_check = c.state().player(gid).filter(|r| r.on_pitch).and_then(|gr| {
            let d = geometry::distance(gr.x, gr.y, bx, by);
            if d < 18.0 {
                let gp = c.input().team(defend).player(gid)?;
                Some((d, gr.current_task.clone(), gp.attributes.clone()))
            } else {
                None
            }
        });
        if let Some((gk_dist, gk_task, ga)) = gk_check {
            let sp_adj = if planned_set_piece && gk_task == "SET_PIECE_GK_CLAIM" {
                8
            } else if planned_set_piece && gk_task == "SET_PIECE_GK_HOLD" {
                -4
            } else {
                0
            };
            let raw_claim = (ga.get(0) as f64 * 0.55
                + ga.get(13) as f64 * 0.15
                + ga.get(14) as f64 * 0.12
                + ga.get(9) as f64 * 0.08
                - gk_dist
                + sp_adj as f64) as i32;
            let claim = if planned_set_piece {
                (raw_claim - 12).clamp(18, 52)
            } else {
                raw_claim.clamp(18, 90)
            };
            if c.rng_mut().next_int(100) < claim {
                c.state_mut().ball.set_controlled(defend, gid.clone());
                c.emit(
                    EventType::GoalkeeperClaim,
                    Some(defend),
                    Some(gid),
                    attacker.as_deref(),
                    vec![
                        kv("distance", gk_dist as i32),
                        kv("setPiece", planned_set_piece),
                        kv("plannedAggression", sp_adj),
                    ],
                );
                c.change_possession(defend, gid, "GOALKEEPER_CLAIM");
                return;
            }
            let punch_chance = if planned_set_piece { 46 } else { 55 };
            if c.rng_mut().next_int(100) < punch_chance {
                c.emit(
                    EventType::GoalkeeperPunch,
                    Some(defend),
                    Some(gid),
                    attacker.as_deref(),
                    vec![kv("setPiece", planned_set_piece)],
                );
                make_loose(c, Some(defend), Some(gid), bx, by, "GK_PUNCH");
                return;
            }
        }
    }

    let (att_id, def_id) = match (attacker, defender) {
        (Some(a), Some(d)) => (a, d),
        _ => {
            make_loose(c, Some(attack), last_player.as_deref(), bx, by, "AERIAL_NO_DUEL");
            return;
        }
    };
    let (ap, ar_energy) = match (c.input().team(attack).player(&att_id), c.state().player(&att_id)) {
        (Some(p), Some(r)) => (p.attributes.clone(), r.energy),
        _ => {
            make_loose(c, Some(attack), last_player.as_deref(), bx, by, "AERIAL_NO_DUEL");
            return;
        }
    };
    let (dp, dr_energy) = match (c.input().team(defend).player(&def_id), c.state().player(&def_id)) {
        (Some(p), Some(r)) => (p.attributes.clone(), r.energy),
        _ => {
            make_loose(c, Some(attack), last_player.as_deref(), bx, by, "AERIAL_NO_DUEL");
            return;
        }
    };
    let deliv_q = delivery_quality(&intent);
    let a_roll = c.rng_mut().next_int(26) as f64;
    let d_roll = c.rng_mut().next_int(26) as f64;
    let av = duel_attr(ap.get(7) as i32) * 0.34
        + duel_attr(ap.get(10) as i32) * 0.18
        + duel_attr(ap.get(15) as i32) * 0.18
        + duel_attr(ap.get(9) as i32) * 0.10
        + ar_energy as f64 * 0.05
        + (deliv_q - 50) as f64 * 0.07
        + c.state().team(attack).match_day_performance * 0.65
        + a_roll;
    let dv = duel_attr(dp.get(7) as i32) * 0.30
        + duel_attr(dp.get(10) as i32) * 0.20
        + duel_attr(dp.get(16) as i32) * 0.20
        + duel_attr(dp.get(13) as i32) * 0.10
        + dr_energy as f64 * 0.05
        + c.state().team(defend).match_day_performance * 0.65
        + d_roll;

    c.state_mut().team_mut(attack).add_aerial_duel(av >= dv);
    c.state_mut().team_mut(defend).add_aerial_duel(dv > av);
    c.emit(
        EventType::AerialDuel,
        Some(attack),
        Some(&att_id),
        Some(&def_id),
        vec![kv("attackerWon", av >= dv)],
    );

    let att_prog = geometry::attacking_progress(attack, by);
    if av + (if planned_set_piece { 2.5 } else { 0.0 }) >= dv && att_prog > 82 {
        let p = (pressure::estimate_at_point(c.input(), c.state(), defend, bx, by)
            - (deliv_q - 50) / 5
            - if planned_set_piece { 8 } else { 0 })
            .clamp(0, 100);
        shot_resolver::resolve_shot(
            c,
            attack,
            &att_id,
            true,
            p,
            if planned_set_piece { "SET_PIECE_HEADER" } else { "CROSS_HEADER" },
        );
    } else if dv > av {
        c.state_mut().team_mut(defend).clearances += 1;
        c.emit(
            EventType::Clearance,
            Some(defend),
            Some(&def_id),
            Some(&att_id),
            vec![kv("aerial", true)],
        );
        if att_prog > 90 && c.rng_mut().next_int(1000) < 3 {
            shot_resolver::score_own_goal(c, attack, &def_id, "AERIAL_CLEARANCE");
        } else {
            let out_roll = c.rng_mut().next_int(100);
            if att_prog > 84 && out_roll < 22 {
                c.schedule_restart(
                    RestartType::Corner,
                    attack,
                    "DEFENSIVE_CLEARANCE_GOAL_LINE",
                    if bx < 50 { 0 } else { 100 },
                    geometry::goal_y(attack),
                );
            } else if out_roll < 18 {
                c.schedule_restart(
                    RestartType::ThrowIn,
                    attack,
                    "CLEARANCE_OUT",
                    if bx < 50 { 0 } else { 100 },
                    by,
                );
            } else {
                let clear_dist = 32 + c.rng_mut().next_int(22);
                let cy = (by - geometry::attack_direction(attack) * clear_dist).clamp(2, 98);
                make_loose(c, Some(defend), Some(&def_id), bx, cy, "AERIAL_CLEARANCE");
            }
        }
    } else {
        make_loose(c, Some(attack), Some(&att_id), bx, by, "AERIAL_SECOND_BALL");
    }
}

pub fn resolve_loose_ball(c: &mut dyn MatchRuntime) {
    let (bx, by, last_team) = (c.state().ball.x, c.state().ball.y, c.state().ball.last_touch_team);
    let mut candidates: Vec<(String, TeamSide, i32, i32, f64)> = Vec::new();
    let player_snapshots: Vec<(String, TeamSide, i32, i32)> = c
        .state()
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch)
        .map(|(_, r)| (r.player_id.clone(), r.side, r.x, r.y))
        .collect();

    for (pid, side, rx, ry) in player_snapshots {
        let attributes = match c.input().team(side).player(&pid) {
            Some(p) => p.attributes.clone(),
            None => continue,
        };
        let dist = geometry::distance(rx, ry, bx, by);
        let mdp = c.state().team(side).match_day_performance;
        let roll = c.rng_mut().next_double() * 38.0;
        let score = (25.0 - dist).max(0.0) * 2.0
            + attributes.get(8) as f64 * 0.03
            + attributes.get(9) as f64 * 0.07
            + attributes.get(17) as f64 * 0.14
            + attributes.get(13) as f64 * 0.10
            + attributes.get(15) as f64 * 0.04
            + attributes.get(16) as f64 * 0.04
            + mdp * 0.40
            + if last_team == Some(side) { 0.0 } else { 7.0 }
            + roll;
        candidates.push((pid, side, rx, ry, score));
    }
    if candidates.is_empty() {
        return;
    }
    candidates.sort_by(|a, b| b.4.total_cmp(&a.4).then_with(|| a.0.cmp(&b.0)));
    let dt = 1 + c.rng_mut().next_int(3);
    c.advance_playing_time(dt);

    let (winner_id, winner_side, sampled_x, sampled_y, _) = &candidates[0];
    let previous = c.state().possession.controlling_team;
    let (winner_x, winner_y) = c
        .state()
        .player(winner_id)
        .map(|player| (player.x, player.y))
        .unwrap_or((*sampled_x, *sampled_y));
    c.state_mut().ball.set_position(winner_x, winner_y);
    c.state_mut().ball.set_controlled(*winner_side, winner_id.clone());
    c.emit(
        EventType::SecondBallWon,
        Some(*winner_side),
        Some(winner_id),
        None,
        vec![kv("previousPossession", previous.map(|s| s.as_str()).unwrap_or("NONE"))],
    );
    if previous != Some(*winner_side) {
        c.change_possession(*winner_side, winner_id, "SECOND_BALL");
    }
}

fn intended_aerial_target(c: &dyn MatchRuntime, side: TeamSide, intent: &str) -> Option<String> {
    let valid_prefix = intent.starts_with("CROSS_TO:")
        || intent.starts_with("CORNER_TO:")
        || intent.starts_with("CLOSE_FREE_KICK_TO:")
        || intent.starts_with("FAR_FREE_KICK_TO:")
        || intent.starts_with("SET_PIECE_TO:");
    if !valid_prefix {
        return None;
    }
    let idx = intent.find(':')?;
    let rest = &intent[idx + 1..];
    if rest.is_empty() {
        return None;
    }
    let id = match rest.find(':') {
        Some(end) => &rest[..end],
        None => rest,
    };
    let r = c.state().player(id)?;
    if r.on_pitch && r.side == side {
        Some(id.to_string())
    } else {
        None
    }
}

fn delivery_quality(intent: &str) -> i32 {
    let marker = ":QUALITY:";
    match intent.find(marker) {
        Some(idx) => intent[idx + marker.len()..].parse::<i32>().map(|v| v.clamp(10, 95)).unwrap_or(50),
        None => 50,
    }
}
