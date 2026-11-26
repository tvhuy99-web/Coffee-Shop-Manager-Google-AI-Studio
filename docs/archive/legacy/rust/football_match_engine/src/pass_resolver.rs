use crate::{
    decision::Action,
    events::EventType,
    geometry::{self, TeamSide},
    input::TeamInstructionsSnapshot,
    possession_decision::{self, ActionDecision},
    pressure,
    resolver_support::{best_defender, java_round, kv, make_loose, MatchRuntime},
    state::{MatchState, RestartType},
    tactical_models,
};

pub fn resolve_pass(c: &mut dyn MatchRuntime, d: &ActionDecision, pressure_val: i32) {
    let side = match c.state().ball.owner_team {
        Some(s) => s,
        None => return,
    };
    let pd = match d.pass_decision.as_ref() {
        Some(p) => p,
        None => return,
    };
    let target_id = match d.target_id.as_deref() {
        Some(t) => t,
        None => return,
    };
    let is_through = d.kind == Action::ThroughBall;
    let mut chance = possession_decision::completion_chance(c.input(), c.state(), pd, pressure_val)
        - if is_through { 7 } else { 0 };
    if is_through {
        chance += through_ball_space_adjustment(c.state(), side);
        chance += pressing_transition_space_adjustment(c.state(), side);
        chance += tactical_models::defensive_disorganization(c.state(), side.opposite());
    }
    chance = chance.clamp(12, 97);

    let start_type = if is_through { EventType::ThroughBall } else { EventType::PassStarted };
    let phase_name = c.state().possession.phase.as_str();
    let lane_int = pd.lane_interceptor_id.as_deref().unwrap_or("NONE");
    c.emit(
        start_type,
        Some(side),
        Some(&d.actor_id),
        Some(target_id),
        vec![
            kv("intention", &d.intention),
            kv("completionChance", chance),
            kv("phase", phase_name),
            kv("pressure", pressure_val),
            kv("progressGain", pd.progress_gain),
            kv("laneRisk", pd.lane_risk),
            kv("coverShadow", pd.cover_shadow),
            kv("laneBlockers", pd.lane_blocker_count),
            kv("laneInterceptor", lane_int),
        ],
    );
    c.state_mut().team_mut(side).add_pass_attempt(pressure_val);

    if is_through && is_offside(c, side, target_id) {
        c.state_mut().team_mut(side).failed_passes += 1;
        c.state_mut().team_mut(side).offsides += 1;
        c.emit(
            EventType::Offside,
            Some(side),
            Some(target_id),
            Some(&d.actor_id),
            vec![kv("cause", "PASS_POSITION"), kv("passer", &d.actor_id)],
        );
        let (tx, ty) = c
            .state()
            .player(target_id)
            .map(|r| (r.x, r.y))
            .unwrap_or((c.state().ball.x, c.state().ball.y));
        c.schedule_restart(RestartType::FreeKick, side.opposite(), "OFFSIDE", tx, ty);
        return;
    }

    let pacing = c.state().team(side).team_instructions.clone();
    let base_dur = 4 + java_round(pd.distance / 22.0);
    let eff_tw = tactical_models::effective_time_wasting(c.input(), c.state(), side);
    let mut duration = action_duration(base_dur, &pacing, eff_tw);
    if c.input().simulated_action_minutes() < 90 {
        duration = java_round(duration as f64 * 0.62).max(2);
    }
    c.advance_playing_time(duration);

    if c.rng_mut().next_int(100) < chance {
        let target_info = c
            .state()
            .player(target_id)
            .filter(|r| r.on_pitch)
            .map(|r| (r.x, r.y));
        let (mut tx, mut ty) = match target_info {
            Some(pos) => pos,
            None => {
                let (bx, by) = (c.state().ball.x, c.state().ball.y);
                make_loose(c, Some(side), Some(&d.actor_id), bx, by, "TARGET_UNAVAILABLE");
                return;
            }
        };
        if is_through && geometry::attacking_progress(side, ty) >= 84 && goalkeeper_rush_wins(c, side, target_id) {
            c.state_mut().team_mut(side).failed_passes += 1;
            if let Some(gk) = c.state().team(side.opposite()).designated_goalkeeper_id.clone() {
                c.emit(
                    EventType::GoalkeeperRush,
                    Some(side.opposite()),
                    Some(&gk),
                    Some(target_id),
                    vec![kv("throughBall", true)],
                );
                c.change_possession(side.opposite(), &gk, "GOALKEEPER_RUSH");
            }
            return;
        }
        let own_gk = c.state().team(side).designated_goalkeeper_id.as_deref();
        if c.input().simulated_action_minutes() < 90 && pd.progress_gain > 0 && own_gk != Some(target_id) {
            let adv = (2 + pd.progress_gain / 4).min(7);
            ty = (ty + geometry::attack_direction(side) * adv).clamp(2, 96);
            if let Some(t) = c.state_mut().player_mut(target_id) {
                t.set_position(tx, ty);
                tx = t.x;
                ty = t.y;
            }
        }
        let prev_intent = c.state().ball.movement_intent.clone();
        c.state_mut().ball.set_position(tx, ty);
        c.state_mut().ball.set_controlled(side, target_id);
        if prev_intent == format!("PASS_FROM:{}", target_id) {
            c.state_mut().ball.set_movement_intent(&format!("WALL_FROM:{}", d.actor_id));
        } else {
            c.state_mut().ball.set_movement_intent(&format!("PASS_FROM:{}", d.actor_id));
        }
        c.state_mut().team_mut(side).completed_passes += 1;
        c.state_mut().possession.record_completed_pass();
        c.emit(
            EventType::PassCompleted,
            Some(side),
            Some(&d.actor_id),
            Some(target_id),
            vec![
                kv("actionType", d.kind.name()),
                kv("intention", &d.intention),
                kv("completionChance", chance),
                kv("progressGain", pd.progress_gain),
                kv("laneRisk", pd.lane_risk),
                kv("coverShadow", pd.cover_shadow),
            ],
        );
        let ft_pressure = pressure::estimate_at_point(c.input(), c.state(), side.opposite(), tx, ty);
        c.emit(
            EventType::FirstTouch,
            Some(side),
            Some(target_id),
            None,
            vec![kv("pressure", ft_pressure)],
        );
        if c.state().team(side).designated_goalkeeper_id.as_deref() == Some(d.actor_id.as_str()) {
            c.emit(
                EventType::GoalkeeperDistribution,
                Some(side),
                Some(&d.actor_id),
                Some(target_id),
                vec![
                    kv("actionType", d.kind.name()),
                    kv("intention", &d.intention),
                    kv("progressGain", pd.progress_gain),
                ],
            );
        }
    } else {
        c.state_mut().team_mut(side).failed_passes += 1;
        c.emit(
            EventType::PassFailed,
            Some(side),
            Some(&d.actor_id),
            Some(target_id),
            vec![
                kv("actionType", d.kind.name()),
                kv("intention", &d.intention),
                kv("completionChance", chance),
                kv("pressure", pressure_val),
                kv("progressGain", pd.progress_gain),
                kv("laneRisk", pd.lane_risk),
                kv("coverShadow", pd.cover_shadow),
                kv("laneBlockers", pd.lane_blocker_count),
            ],
        );
        let (x, y) = c
            .state()
            .player(target_id)
            .map(|r| (r.x, r.y))
            .unwrap_or((c.state().ball.x, c.state().ball.y));
        let origin_progress = geometry::attacking_progress(side, c.state().ball.y);
        let turnover_exposure = risky_turnover_interception_adjustment(pd.tactical_risk, origin_progress);

        let geo_candidate = pd.lane_interceptor_id.as_deref().and_then(|did| {
            let dr = c.state().player(did)?;
            if !dr.on_pitch || dr.side != side.opposite() {
                return None;
            }
            let dp = c.input().team(side.opposite()).player(did)?;
            Some((did.to_string(), dp.attributes.get(16) as i32, dp.attributes.get(13) as i32))
        });

        let (defender, geometric, intercept) = if let Some((did, pos, dec)) = geo_candidate {
            let ch = geometric_interception_chance(
                pd.lane_interception_chance,
                pos,
                dec,
                pd.lane_risk,
                pd.lane_blocker_count,
                turnover_exposure,
            );
            (Some(did), true, ch)
        } else {
            let def_id = best_defender(c, side.opposite(), x, y);
            let ch = def_id
                .as_deref()
                .and_then(|did| {
                    let dp = c.input().team(side.opposite()).player(did)?;
                    let dist = c
                        .state()
                        .player(did)
                        .map(|dr| geometry::distance(dr.x, dr.y, x, y))
                        .unwrap_or(99.0);
                    Some(loose_pass_interception_chance(
                        dist,
                        dp.attributes.get(16) as i32,
                        dp.attributes.get(13) as i32,
                        turnover_exposure,
                    ))
                })
                .unwrap_or(0);
            (def_id, false, ch)
        };

        if let Some(ref did) = defender {
            if intercept > 0 && c.rng_mut().next_int(100) < intercept {
                c.state_mut().team_mut(side.opposite()).interceptions += 1;
                c.emit(
                    EventType::Interception,
                    Some(side.opposite()),
                    Some(did),
                    Some(&d.actor_id),
                    vec![
                        kv("target", target_id),
                        kv("geometry", geometric),
                        kv("laneRisk", pd.lane_risk),
                        kv("coverShadow", pd.cover_shadow),
                        kv("interceptX", if geometric { pd.lane_intercept_x } else { x }),
                        kv("interceptY", if geometric { pd.lane_intercept_y } else { y }),
                    ],
                );
                c.change_possession(side.opposite(), did, "INTERCEPTION");
                return;
            }
        }
        make_loose(c, Some(side), Some(&d.actor_id), x, y, "MISSED_PASS");
    }
}

pub fn geometric_interception_chance(
    lane_chance: i32,
    positioning: i32,
    decisions: i32,
    lane_risk: i32,
    blockers: i32,
    turnover_exposure: i32,
) -> i32 {
    let defender_quality = (positioning as f64 * 0.60 + decisions as f64 * 0.40 - 70.0) * 0.18;
    let chance = 5.0
        + lane_chance as f64 * 0.68
        + defender_quality
        + (lane_risk - 55).max(0) as f64 * 0.06
        + (blockers - 1).clamp(0, 3) as f64 * 1.5
        + turnover_exposure as f64 * 0.32;
    java_round(chance).clamp(5, 78)
}

pub fn loose_pass_interception_chance(
    distance: f64,
    positioning: i32,
    decisions: i32,
    turnover_exposure: i32,
) -> i32 {
    if distance > 15.0 {
        return 0;
    }
    let closeness = (15.0 - distance).max(0.0);
    let chance = 4.0
        + closeness * 2.25
        + (positioning - 60) as f64 * 0.27
        + (decisions - 60) as f64 * 0.16
        + turnover_exposure as f64 * 0.30;
    java_round(chance).clamp(3, 65)
}

fn is_offside(c: &dyn MatchRuntime, attack: TeamSide, target_id: &str) -> bool {
    let s = c.state();
    let target = match s.player(target_id) {
        Some(r) if r.on_pitch => r,
        _ => return false,
    };
    let tp = geometry::attacking_progress(attack, target.y);
    let bp = geometry::attacking_progress(attack, s.ball.y);
    if tp <= 50 || tp <= bp + 3 {
        return false;
    }
    let defending = attack.opposite();
    let mut def: Vec<i32> = s
        .players
        .iter()
        .filter(|(_, r)| r.on_pitch && r.side == defending)
        .map(|(_, r)| geometry::attacking_progress(attack, r.y))
        .collect();
    if def.len() < 2 {
        return false;
    }
    def.sort_by(|a, b| b.cmp(a));
    let second_last = def[1];
    let ti = &s.team(defending).team_instructions;
    let mut margin = 3;
    if ti.offside_trap {
        let dt = c.input().team(defending);
        let at = c.input().team(attack).player(target_id);
        let gk = s.team(defending).designated_goalkeeper_id.as_deref();
        let mut iq = 0.0;
        let mut n = 0;
        for (_, r) in &s.players {
            if !r.on_pitch || r.side != defending || gk == Some(r.player_id.as_str()) {
                continue;
            }
            if geometry::attacking_progress(defending, r.y) > 48 {
                continue;
            }
            if let Some(dp) = dt.player(&r.player_id) {
                iq += (dp.attributes.get(16) as f64 + dp.attributes.get(13) as f64) / 2.0;
                n += 1;
            }
        }
        let trap_iq = if n == 0 { 74.0 } else { iq / n as f64 };
        let runner_iq = at
            .map(|p| (p.attributes.get(15) as f64 + p.attributes.get(13) as f64) / 2.0)
            .unwrap_or(74.0);
        let edge = java_round(1.0 + (trap_iq - runner_iq) * 0.025 + (ti.line_height - 50) as f64 * 0.012).clamp(0, 2);
        margin = (margin - edge).max(1);
    }
    tp > second_last + margin
}

pub fn through_ball_space_adjustment(s: &MatchState, attack: TeamSide) -> i32 {
    let defending = &s.team(attack.opposite()).team_instructions;
    let line = defending.line_height;
    let mut adj = java_round((line - 50).max(0) as f64 * 0.085 - (50 - line).max(0) as f64 * 0.035);
    if defending.offside_trap {
        adj += 1;
    }
    adj.clamp(-2, 6)
}

pub fn pressing_transition_space_adjustment(s: &MatchState, attack: TeamSide) -> i32 {
    if !s.possession.transition_active {
        return 0;
    }
    let defending = &s.team(attack.opposite()).team_instructions;
    java_round((defending.pressing_intensity - 50) as f64 * 0.080).clamp(-2, 5)
}

pub fn risky_turnover_interception_adjustment(tactical_risk: i32, origin_progress: i32) -> i32 {
    let risk = (tactical_risk - 50).max(0);
    let deep = (58 - origin_progress).max(0);
    java_round(risk as f64 * 0.32 + deep as f64 * 0.080).clamp(0, 12)
}

pub fn action_duration(base: i32, instructions: &TeamInstructionsSnapshot, effective_time_wasting: i32) -> i32 {
    let factor = 1.0
        - (instructions.tempo - 50) as f64 * 0.0035
        + (effective_time_wasting - 50).max(0) as f64 * 0.0012;
    java_round(base as f64 * factor).clamp(2, 10)
}

fn goalkeeper_rush_wins(c: &mut dyn MatchRuntime, attack: TeamSide, target_id: &str) -> bool {
    let defend = attack.opposite();
    let (d, gk_attrs, gk_energy, tp_attrs, tp_energy) = {
        let s = c.state();
        let gk = match s.team(defend).designated_goalkeeper_id.as_deref() {
            Some(g) => g,
            None => return false,
        };
        let gr = match s.player(gk) {
            Some(r) if r.on_pitch => r,
            _ => return false,
        };
        let tr = match s.player(target_id) {
            Some(r) => r,
            None => return false,
        };
        let gp = match c.input().team(defend).player(gk) {
            Some(p) => p,
            None => return false,
        };
        let tp = match c.input().team(attack).player(target_id) {
            Some(p) => p,
            None => return false,
        };
        let d = geometry::distance(gr.x, gr.y, tr.x, tr.y);
        if d > 28.0 {
            return false;
        }
        (d, gp.attributes.clone(), gr.energy, tp.attributes.clone(), tr.energy)
    };
    let k_roll = c.rng_mut().next_int(25) as f64;
    let a_roll = c.rng_mut().next_int(25) as f64;
    let keeper = gk_attrs.get(0) as f64 * 0.35
        + gk_attrs.get(13) as f64 * 0.23
        + gk_attrs.get(8) as f64 * 0.12
        + gk_attrs.get(9) as f64 * 0.12
        + gk_energy as f64 * 0.05
        + k_roll;
    let attacker = tp_attrs.get(8) as f64 * 0.27
        + tp_attrs.get(15) as f64 * 0.27
        + tp_attrs.get(14) as f64 * 0.18
        + tp_attrs.get(4) as f64 * 0.10
        + tp_energy as f64 * 0.05
        + a_roll;
    keeper - d * 0.35 > attacker
}
