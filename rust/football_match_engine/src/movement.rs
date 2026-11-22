use crate::{
    defensive,
    fatigue::{self, FatigueInput, FatigueState, Task},
    fingerprints as fp,
    geometry::{self, TeamSide},
    input::MatchInput,
    roster,
    state::MatchState,
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

pub fn plan_targets(input: &MatchInput, state: &mut MatchState) {
    let possession = state.possession.controlling_team;
    let transition_active = state.possession.transition_active;
    let ball_x = state.ball.x;
    let ball_y = state.ball.y;
    let ball_owner = state.ball.owner_player_id.clone();

    let player_ids: Vec<String> = state.players.iter().map(|(id, _)| id.clone()).collect();
    for pid in player_ids {
        let (side, slot_id, on_pitch) = match state.player(&pid) {
            Some(r) => (r.side, r.slot_id.clone(), r.on_pitch),
            None => continue,
        };
        if !on_pitch {
            continue;
        }
        let team = input.team(side);
        let player = match team.player(&pid) {
            Some(p) => p,
            None => continue,
        };
        let gk_id = state.team(side).designated_goalkeeper_id.clone();

        if gk_id.as_deref() == Some(pid.as_str()) {
            let tx = geometry::clamp(java_round(50.0 + (ball_x - 50) as f64 * 0.12));
            let ty = geometry::clamp(geometry::own_goal_y_spatial(side) + 3 * geometry::attack_direction(side));
            if let Some(r) = state.player_mut(&pid) {
                r.pressure_level = 0;
                r.pressing_player_id = None;
                r.marking_player_id = None;
                r.set_target(tx, ty);
                r.current_task = "GOALKEEPER_SHAPE".into();
            }
            continue;
        }

        let slot = match state.team(side).tactical_slot(&slot_id).cloned() {
            Some(s) => s,
            None => continue,
        };
        let slot_instr = match state.team(side).slot_instructions(&slot_id).cloned() {
            Some(s) => s,
            None => continue,
        };
        let instructions = state.team(side).team_instructions.clone();
        let in_possession = Some(side) == possession;
        let mut base_x = geometry::oriented_x(side, if in_possession { slot.in_x } else { slot.out_x });
        let base_y = geometry::oriented_y(side, if in_possession { slot.in_y } else { slot.out_y });
        let familiarity = 0.60 + team.collective_state.system_familiarity as f64 * 0.004;
        let effective_width = geometry::clamp(java_round(50.0 + (instructions.width - 50) as f64 * familiarity));
        let effective_line = geometry::clamp(java_round(50.0 + (instructions.line_height - 50) as f64 * familiarity));
        base_x = geometry::scale_width(base_x, effective_width);

        let roaming_bias = (slot_instr.roaming - 50) as f64 / 50.0;
        let support_pull = 0.08 + (100 - slot_instr.support_distance) as f64 / 100.0 * 0.12;
        let ball_pull = (if in_possession {
            support_pull * (1.0 + roaming_bias * 0.55)
        } else {
            0.08 * (1.0 + roaming_bias * 0.18)
        })
        .clamp(0.025, 0.30);
        let vertical_pull = ((if in_possession { 0.06 } else { 0.09 })
            * (1.0 + roaming_bias * if in_possession { 0.70 } else { 0.15 }))
        .clamp(0.02, 0.18);

        let tx = geometry::clamp(java_round(base_x as f64 + (ball_x - base_x) as f64 * ball_pull));
        let mut ty = geometry::clamp(java_round(base_y as f64 + (ball_y - base_y) as f64 * vertical_pull));
        let direction = geometry::attack_direction(side);
        let mut task: &'static str;

        if in_possession {
            ty = geometry::clamp(ty + direction * ((slot_instr.forward_movement - 50) / 10));
            if transition_active {
                let transition_step = java_round(4.0 + (instructions.transition_risk - 50) as f64 * 0.040);
                ty = geometry::clamp(ty + direction * transition_step);
                task = if slot_instr.roaming >= 70 { "ROAMING_RUN" } else { "SUPPORT_RUN" };
            } else {
                let base_prog = geometry::attacking_progress(side, base_y);
                let commit_w = if base_prog <= 62 { 1.0 } else if base_prog <= 76 { 0.60 } else { 0.25 };
                let build_step = java_round((instructions.build_up_risk - 50) as f64 * 0.045 * commit_w);
                ty = geometry::clamp(ty + direction * build_step);
                task = if slot_instr.roaming >= 70 {
                    "ROAMING_SUPPORT"
                } else if slot_instr.roaming <= 30 {
                    "HOLD_SHAPE_SUPPORT"
                } else {
                    "SUPPORT"
                };
            }
        } else {
            ty = geometry::clamp(ty + direction * ((effective_line - 50) / 6));
            if instructions.offside_trap && geometry::attacking_progress(side, base_y) <= 48 {
                let trap_int = (player.attributes.get(16) as i32 + player.attributes.get(13) as i32) / 2;
                let coord = (trap_int - 74) as f64 * 0.020
                    + (team.collective_state.system_familiarity - 60) as f64 * 0.012;
                let trap_step = java_round(3.0 + (effective_line - 50) as f64 * 0.025 + coord).clamp(0, 5);
                ty = geometry::clamp(ty + direction * trap_step);
            }
            task = if transition_active { "RECOVERY" } else { "DEFENSIVE_SHAPE" };
        }

        if ball_owner.as_deref() == Some(pid.as_str()) {
            task = "BALL_CARRIER";
        }
        if let Some(r) = state.player_mut(&pid) {
            r.pressure_level = 0;
            r.pressing_player_id = None;
            r.marking_player_id = None;
            r.set_target(tx, ty);
            r.current_task = task.into();
        }
    }

    apply_offball_intelligence(input, state);
    defensive::assign_defenders(input, state);
    defensive::coordinate(input, state);
}

pub fn move_players(input: &MatchInput, state: &mut MatchState, duration_seconds: i32) {
    let home_outfield = active_outfield(state, TeamSide::Home);
    let away_outfield = active_outfield(state, TeamSide::Away);
    let sim_mins = input.simulated_action_minutes();
    let home_instr = state.home_team.team_instructions.clone();
    let away_instr = state.away_team.team_instructions.clone();

    for (_, r) in &mut state.players {
        if !r.on_pitch {
            continue;
        }
        let player = match input.team(r.side).player(&r.player_id) {
            Some(p) => p,
            None => continue,
        };
        let (old_x, old_y) = (r.x, r.y);
        let dist = geometry::distance(old_x, old_y, r.target_x, r.target_y);
        if dist > 0.0 {
            let pace = player.attributes.get(8) as f64;
            let work_rate = player.attributes.get(17) as f64;
            let energy_factor = 0.82 + r.energy as f64 * 0.0018;
            let high_work = r.current_task == "PRESS" || r.current_task.starts_with("RECOVERY");
            let task_factor = if high_work {
                1.12 * (1.0 + (work_rate - 74.0) * 0.0025).clamp(0.82, 1.10)
            } else {
                1.0
            };
            let action_scale = if sim_mins < 90 {
                ((90.0 / sim_mins as f64) * 0.45).min(2.15)
            } else {
                1.0
            };
            let max_move = duration_seconds as f64 * action_scale * (0.448 + pace * 0.0020) * energy_factor * task_factor;
            let fraction = (max_move / dist).min(1.0);
            let new_x = geometry::clamp(java_round(old_x as f64 + (r.target_x - old_x) as f64 * fraction));
            let new_y = geometry::clamp(java_round(old_y as f64 + (r.target_y - old_y) as f64 * fraction));
            r.set_position(new_x, new_y);
        }
        let distance_tenths = java_round(geometry::distance(old_x, old_y, r.x, r.y) * 10.0);
        let fatigue_duration = if sim_mins < 90 {
            duration_seconds.max(java_round(duration_seconds as f64 * (90.0 / sim_mins as f64) * 0.72))
        } else {
            duration_seconds
        };
        let instr = if r.side == TeamSide::Home { &home_instr } else { &away_instr };
        let active_out = if r.side == TeamSide::Home { home_outfield } else { away_outfield };
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
        let mut fstate = FatigueState {
            energy: r.energy,
            workload: r.workload as i64,
            distance_tenths: r.distance_covered_tenths as i64,
            fatigue_accumulator: r.fatigue_accumulator,
            acute_fatigue: r.acute_fatigue,
        };
        fatigue::apply(
            &mut fstate,
            FatigueInput {
                duration_seconds: fatigue_duration,
                distance_tenths,
                tempo: instr.tempo,
                pressing_intensity: instr.pressing_intensity,
                transition_risk: instr.transition_risk,
                active_outfield: active_out,
                stamina: player.attributes.get(11) as i32,
                task,
                assigned_defensive_role: r.defensive_role != defensive::ROLE_NONE,
            },
            r.on_pitch,
        );
        r.energy = fstate.energy;
        r.workload = fstate.workload.min(i32::MAX as i64) as i32;
        r.distance_covered_tenths = fstate.distance_tenths.min(i32::MAX as i64) as i32;
        r.fatigue_accumulator = fstate.fatigue_accumulator;
        r.acute_fatigue = fstate.acute_fatigue;
    }
}

pub fn reset_for_kickoff(_input: &MatchInput, state: &mut MatchState) {
    let home_gk = state.home_team.designated_goalkeeper_id.clone();
    let away_gk = state.away_team.designated_goalkeeper_id.clone();
    let home_slots = state.home_team.tactical_slots.clone();
    let away_slots = state.away_team.tactical_slots.clone();

    for (_, r) in &mut state.players {
        let slots = if r.side == TeamSide::Home { &home_slots } else { &away_slots };
        let gk_id = if r.side == TeamSide::Home { &home_gk } else { &away_gk };
        let slot = match slots.iter().find(|(k, _)| k == &r.slot_id).map(|(_, v)| v) {
            Some(s) => s,
            None => continue,
        };
        let mut x = geometry::oriented_x(r.side, slot.out_x);
        let mut y = geometry::oriented_y(r.side, slot.out_y);
        if gk_id.as_deref() == Some(r.player_id.as_str()) {
            x = 50;
            y = geometry::clamp(geometry::own_goal_y_spatial(r.side) + 3 * geometry::attack_direction(r.side));
        }
        r.set_position(x, y);
        r.set_target(x, y);
        r.pressure_level = 0;
        r.pressing_player_id = None;
        r.marking_player_id = None;
        r.current_task = "SHAPE".into();
        r.defensive_role = defensive::ROLE_NONE.into();
    }
}

fn active_outfield(state: &MatchState, side: TeamSide) -> i32 {
    let gk_id = state.team(side).designated_goalkeeper_id.as_deref();
    state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == side && Some(r.player_id.as_str()) != gk_id)
        .count() as i32
}

const MIN_TEAMMATE_TARGET_SPACING: f64 = 5.5;

fn apply_offball_intelligence(input: &MatchInput, state: &mut MatchState) {
    let side = match state.possession.controlling_team {
        Some(s) => s,
        None => return,
    };
    let gk_id = state.team(side).designated_goalkeeper_id.clone();
    let carrier_id = state.ball.owner_player_id.clone();
    let team = input.team(side);
    let ball_x = state.ball.x;
    let ball_y = state.ball.y;
    let ball_prog = geometry::attacking_progress(side, ball_y);
    let direction = geometry::attack_direction(side);
    let offside_line = second_last_defender_progress(state, side);
    let transition = state.possession.transition_active;

    let mut runners: Vec<(String, f64, i32, i32)> = Vec::new();
    for id in roster::active_player_ids_in_slot_order(input, state, side) {
        if gk_id.as_deref() == Some(id.as_str()) || carrier_id.as_deref() == Some(id.as_str()) {
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
        let slot = match roster::slot_instructions(state, side, &id) {
            Some(s) => s,
            None => continue,
        };
        let a = &p.attributes;
        let mut intelligence = calibrated(a.get(15) as i32) * 0.46
            + calibrated(a.get(13) as i32) * 0.24
            + calibrated(a.get(8) as i32) * 0.06
            + slot.roaming as f64 * 0.12
            + slot.forward_movement as f64 * 0.12;
        if transition {
            intelligence += 7.0;
        }
        if r.energy < 45 {
            intelligence -= (45 - r.energy) as f64 * 0.22;
        }
        runners.push((id, intelligence, slot.forward_movement, slot.support_distance));
    }
    runners.sort_by(|a, b| b.1.total_cmp(&a.1));

    let mut occupied: Vec<(i32, i32)> = Vec::new();
    for (rid, intelligence, fwd_move, support_dist) in runners {
        let (anchor_x, cur_ty, cur_task, slot_id) = match state.player(&rid) {
            Some(r) => (r.target_x, r.target_y, r.current_task.clone(), r.slot_id.clone()),
            None => continue,
        };
        let player = match team.player(&rid) {
            Some(p) => p,
            None => continue,
        };
        let mut tx = anchor_x;
        let mut ty = cur_ty;
        let move_int = smooth01((intelligence - 48.0) / 26.0);
        let role_intent = ((fwd_move as f64 - 20.0) / 65.0).clamp(0.0, 1.0);
        let run_strength = if (35..=88).contains(&ball_prog) {
            move_int * (0.45 + role_intent * 0.55)
        } else {
            0.0
        };
        let full_extra = (3 + java_round((intelligence - 58.0).max(0.0) / 6.5)).clamp(3, 13);
        let mut extra = java_round(run_strength * full_extra as f64);
        if transition {
            extra += java_round(run_strength * 2.0);
        }
        extra = extra.clamp(0, 13);
        if fwd_move < 50 {
            extra = extra.min(2);
        }

        let new_task: String;
        if extra > 0 {
            ty = geometry::clamp(ty + direction * extra);
            if (43..=57).contains(&anchor_x) {
                let open_x = choose_more_open_x(input, state, side, &rid, &slot_id, &occupied, 40, 60, ty, 1);
                let open_blend = run_strength * 0.42;
                tx = geometry::clamp(java_round(anchor_x as f64 * (1.0 - open_blend) + open_x as f64 * open_blend));
            } else if (anchor_x - 50).abs() >= 22 {
                let wide_fwd = player.primary_position == "LW" || player.primary_position == "RW";
                let opposite_flank = (anchor_x < 50 && ball_x > 52) || (anchor_x > 50 && ball_x < 48);
                let blend_factor = if wide_fwd && ball_prog >= 56 {
                    if opposite_flank { 0.36 } else { 0.28 }
                } else {
                    0.18
                };
                let inside_blend = run_strength * blend_factor;
                tx = geometry::clamp(java_round(anchor_x as f64 * (1.0 - inside_blend) + 50.0 * inside_blend));
            }

            if offside_line > 0 {
                let target_prog = geometry::attacking_progress(side, ty);
                let safe_prog = (ball_prog + 2).max(offside_line - 1);
                if target_prog > safe_prog {
                    let caution = smooth01((player.attributes.get(13) as f64 - 45.0) / 21.0);
                    let excess = target_prog - safe_prog;
                    let corrected = target_prog - java_round(excess as f64 * caution);
                    ty = y_from_progress(side, corrected);
                }
            }
            new_task = if run_strength >= 0.58 {
                if transition { "OFFBALL_TRANSITION_RUN" } else { "OFFBALL_RUN" }
            } else {
                "OFFBALL_ADVANCE"
            }
            .into();
        } else {
            let dist = 7 + support_dist / 25;
            let lateral = choose_lateral_side(input, state, side, &rid, &slot_id, anchor_x, &occupied, ball_x, ty, dist, 2);
            let support_x = geometry::clamp(ball_x + lateral * dist);
            tx = geometry::clamp(java_round(tx as f64 * 0.76 + support_x as f64 * 0.24));
            new_task = if !cur_task.starts_with("ROAMING") {
                "OFFBALL_SUPPORT".into()
            } else {
                cur_task
            };
        }

        let (sx, sy) = enforce_spacing(input, state, side, &rid, &slot_id, anchor_x, tx, ty, &occupied, direction);
        if let Some(r) = state.player_mut(&rid) {
            r.set_target(sx, sy);
            r.current_task = new_task;
        }
        occupied.push((sx, sy));
    }
}

#[allow(clippy::too_many_arguments)]
fn enforce_spacing(
    input: &MatchInput,
    state: &MatchState,
    side: TeamSide,
    rid: &str,
    slot_id: &str,
    anchor_x: i32,
    x: i32,
    y: i32,
    occupied: &[(i32, i32)],
    attack_direction: i32,
) -> (i32, i32) {
    let (mut tx, mut ty) = (x, y);
    for attempt in 0..4 {
        let collision = occupied
            .iter()
            .any(|&(px, py)| geometry::distance(tx, ty, px, py) < MIN_TEAMMATE_TARGET_SPACING);
        if !collision {
            break;
        }
        let dist = 4 + attempt;
        let lateral = choose_lateral_side(input, state, side, rid, slot_id, anchor_x, occupied, tx, ty, dist, 10 + attempt);
        tx = geometry::clamp(tx + lateral * dist);
        if attempt >= 2 {
            ty = geometry::clamp(ty - attack_direction * 3);
        }
    }
    (tx, ty)
}

#[allow(clippy::too_many_arguments)]
fn choose_more_open_x(
    input: &MatchInput,
    state: &MatchState,
    side: TeamSide,
    rid: &str,
    slot_id: &str,
    occupied: &[(i32, i32)],
    left_x: i32,
    right_x: i32,
    y: i32,
    salt: i32,
) -> i32 {
    let left = space_score(state, side, rid, occupied, left_x, y);
    let right = space_score(state, side, rid, occupied, right_x, y);
    if (left - right).abs() > 0.05 {
        if left > right { left_x } else { right_x }
    } else if tie_break_side(input, state, side, slot_id, salt) < 0 {
        left_x
    } else {
        right_x
    }
}

#[allow(clippy::too_many_arguments)]
fn choose_lateral_side(
    input: &MatchInput,
    state: &MatchState,
    side: TeamSide,
    rid: &str,
    slot_id: &str,
    anchor_x: i32,
    occupied: &[(i32, i32)],
    x: i32,
    y: i32,
    distance: i32,
    salt: i32,
) -> i32 {
    let left_x = geometry::clamp(x - distance);
    let right_x = geometry::clamp(x + distance);
    let left = space_score(state, side, rid, occupied, left_x, y);
    let right = space_score(state, side, rid, occupied, right_x, y);
    if (left - right).abs() > 0.05 {
        return if left > right { -1 } else { 1 };
    }
    if anchor_x != 50 {
        return if anchor_x < 50 { -1 } else { 1 };
    }
    if state.ball.x != 50 {
        return if state.ball.x < 50 { 1 } else { -1 };
    }
    tie_break_side(input, state, side, slot_id, salt)
}

fn space_score(state: &MatchState, side: TeamSide, rid: &str, occupied: &[(i32, i32)], x: i32, y: i32) -> f64 {
    let mut opp_dist = 28.0_f64;
    let mut mate_dist = 22.0_f64;
    for (_, other) in &state.players {
        if !other.on_pitch || other.player_id == rid {
            continue;
        }
        let d = geometry::distance(x, y, other.x, other.y);
        if other.side == side {
            mate_dist = mate_dist.min(d);
        } else {
            opp_dist = opp_dist.min(d);
        }
    }
    for &(tx, ty) in occupied {
        mate_dist = mate_dist.min(geometry::distance(x, y, tx, ty));
    }
    let boundary = x.min(100 - x) as f64;
    opp_dist * 1.35 + mate_dist * 1.10 + boundary.min(18.0) * 0.16
}

fn tie_break_side(input: &MatchInput, state: &MatchState, side: TeamSide, slot_id: &str, salt: i32) -> i32 {
    let mut h = fp::start();
    h = fp::mix_i64(h, input.seed);
    h = fp::mix_i32(h, state.clock.game_second);
    h = fp::mix_i64(h, state.possession.sequence_id);
    h = fp::mix_i32(h, side as i32);
    h = fp::mix_string(h, Some(slot_id));
    h = fp::mix_i32(h, salt);
    if (h & 1) == 0 { -1 } else { 1 }
}

fn second_last_defender_progress(state: &MatchState, attacking_side: TeamSide) -> i32 {
    let mut progress: Vec<i32> = state
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == attacking_side.opposite())
        .map(|(_, r)| geometry::attacking_progress(attacking_side, r.y))
        .collect();
    if progress.len() < 2 {
        return -1;
    }
    progress.sort_by(|a, b| b.cmp(a));
    progress[1]
}

fn y_from_progress(side: TeamSide, progress: i32) -> i32 {
    if side == TeamSide::Home {
        geometry::clamp(progress)
    } else {
        geometry::clamp(100 - progress)
    }
}

fn calibrated(value: i32) -> f64 {
    74.0 + (value - 74) as f64 * 0.60
}

fn smooth01(value: f64) -> f64 {
    let x = value.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
