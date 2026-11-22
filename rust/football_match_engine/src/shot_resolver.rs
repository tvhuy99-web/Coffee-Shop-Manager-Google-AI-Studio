use crate::{
    events::EventType,
    geometry::{self, TeamSide},
    resolver_support::{calibrated_finishing_attr, java_round, kv, make_loose, MatchRuntime},
    shot_model::{ShotContextV2, ShotQuality},
    state::RestartType,
};

pub fn resolve_shot(
    c: &mut dyn MatchRuntime,
    side: TeamSide,
    shooter_id: &str,
    header: bool,
    pressure: i32,
    source: &str,
) {
    let sp = match c.input().team(side).player(shooter_id).cloned() {
        Some(p) => p,
        None => return,
    };
    if !c.state().player(shooter_id).map(|r| r.on_pitch).unwrap_or(false) {
        return;
    }
    let context = ShotContextV2::build(c.input(), c.state(), side, shooter_id, header, pressure, source);
    let shot_quality = match ShotQuality::calculate(c.input(), c.state(), side, shooter_id, &context) {
        Ok(q) => q,
        Err(_) => return,
    };
    let quality = shot_quality.quality;
    let xg = shot_quality.expected_goals_milli;
    c.emit(
        if header { EventType::Header } else { EventType::Shot },
        Some(side),
        Some(shooter_id),
        None,
        vec![
            kv("shotQuality", quality),
            kv("xgMilli", xg),
            kv("pressure", pressure),
            kv("source", source),
            kv("progress", context.progress),
            kv("centrality", context.centrality),
            kv("nearbyDefenders", context.nearby_defenders),
            kv("shotType", &shot_quality.shot_type),
            kv("distanceDeciM", java_round(context.distance_m * 10.0)),
            kv("goalAngleDeciDeg", java_round(context.goal_angle_deg * 10.0)),
            kv("blockers", context.blockers),
            kv("laneWidthDeciM", java_round(context.shooting_lane_width_m * 10.0)),
            kv("oneVsOne", context.one_vs_one),
        ],
    );
    let dt = 1 + c.rng_mut().next_int(3);
    c.advance_playing_time(dt);

    let block_chance = context.block_chance(c.input(), c.state());
    if !context.penalty {
        if let Some(ref blocker_id) = context.best_blocker_id {
            if c.rng_mut().next_int(100) < block_chance {
                c.state_mut().team_mut(side).add_shot(false);
                c.state_mut().team_mut(side.opposite()).blocks += 1;
                c.emit(
                    EventType::Block,
                    Some(side.opposite()),
                    Some(blocker_id),
                    Some(shooter_id),
                    vec![
                        kv("shotQuality", quality),
                        kv("xgMilli", xg),
                        kv("blockChance", block_chance),
                        kv("blockerDistanceDeciM", java_round(context.best_blocker_distance_m * 10.0)),
                        kv("blockerPerpDeciM", java_round(context.best_blocker_perp_m * 10.0)),
                    ],
                );
                let bx = c.state().ball.x;
                let by = c.state().ball.y;
                if c.rng_mut().next_int(100) < 82 {
                    c.schedule_restart(
                        RestartType::Corner,
                        side,
                        "SHOT_BLOCKED_OUT",
                        if bx < 50 { 0 } else { 100 },
                        geometry::goal_y(side),
                    );
                } else {
                    make_loose(c, Some(side.opposite()), Some(blocker_id), bx, by, "SHOT_BLOCK_REBOUND");
                }
                return;
            }
        }
    }

    let a = &sp.attributes;
    let mut placement = java_round(
        quality as f64 * 0.58
            + calibrated_finishing_attr(a.get(4) as i32) * 0.18
            + calibrated_finishing_attr(a.get(14) as i32) * 0.16
            + context.body_balance as f64 * 0.08
            - pressure as f64 * 0.06,
    )
    .clamp(18, 98);
    let mut power = java_round(
        calibrated_finishing_attr(if header { a.get(7) } else { a.get(6) } as i32) * 0.58
            + calibrated_finishing_attr(a.get(4) as i32) * 0.20
            + context.body_balance as f64 * 0.12
            + quality as f64 * 0.10
            - pressure as f64 * 0.04,
    )
    .clamp(20, 98);
    if header {
        placement = (placement - 6).clamp(12, 94);
        power = java_round(power as f64 * 0.90).clamp(18, 94);
    }
    if context.penalty {
        placement = (placement + 10).clamp(35, 99);
        power = (power + 5).clamp(35, 99);
    }

    let on_target_chance = java_round(
        23.5 + quality as f64 * 0.30
            + context.goal_angle_deg * 0.11
            - context.distance_m * 0.12
            + placement as f64 * 0.08
            - pressure as f64 * 0.035
            - if context.header { 3.0 } else { 0.0 }
            + if context.penalty { 20.0 } else { 0.0 },
    )
    .clamp(24, 92);
    let on_target = c.rng_mut().next_int(100) < on_target_chance;
    c.state_mut().team_mut(side).add_shot(on_target);
    if !on_target {
        c.emit(
            EventType::ShotOffTarget,
            Some(side),
            Some(shooter_id),
            None,
            vec![
                kv("shotQuality", quality),
                kv("xgMilli", xg),
                kv("placement", placement),
                kv("power", power),
                kv("onTargetChance", on_target_chance),
            ],
        );
        c.schedule_restart(
            RestartType::GoalKick,
            side.opposite(),
            "SHOT_WIDE",
            50,
            geometry::goal_kick_y(side.opposite()),
        );
        return;
    }
    c.emit(
        EventType::ShotOnTarget,
        Some(side),
        Some(shooter_id),
        None,
        vec![
            kv("shotQuality", quality),
            kv("xgMilli", xg),
            kv("placement", placement),
            kv("power", power),
            kv("onTargetChance", on_target_chance),
        ],
    );

    let gk = context.goalkeeper_id.clone();
    let positioning = context.goalkeeper_positioning_quality(c.input(), c.state());
    let on_target_p = on_target_chance as f64 / 100.0;
    let block_p = block_chance as f64 / 100.0;
    let mut base_conditional = (context.expected_goals / ((1.0 - block_p) * on_target_p).max(0.12))
        .clamp(0.04, if context.penalty { 0.93 } else { 0.72 });
    let shooter_delta =
        (placement - 74) as f64 * 0.0027 + (power - 74) as f64 * 0.0016 + (quality - 74) as f64 * 0.0017;
    let finishing_skill = calibrated_finishing_attr(if header { a.get(7) } else { a.get(6) } as i32) * 0.55
        + calibrated_finishing_attr(a.get(4) as i32) * 0.20
        + calibrated_finishing_attr(a.get(14) as i32) * 0.20
        + calibrated_finishing_attr(a.get(13) as i32) * 0.05;
    let finishing_skill_bonus = (finishing_skill - 74.0) * 0.0020;
    let mut gk_skill_bonus = 0.0;
    let mut gk_fatigue_penalty = 0.0;
    let gk_active = gk.as_deref().and_then(|gid| {
        let gr = c.state().player(gid)?;
        if !gr.on_pitch {
            return None;
        }
        let gp = c.input().team(side.opposite()).player(gid)?;
        Some((gp.attributes.clone(), gr.energy))
    });
    if let Some((ga, gk_energy)) = gk_active {
        let mut gk_skill = calibrated_finishing_attr(ga.get(0) as i32) * 0.46
            + calibrated_finishing_attr(ga.get(9) as i32) * 0.22
            + calibrated_finishing_attr(ga.get(13) as i32) * 0.13
            + calibrated_finishing_attr(ga.get(14) as i32) * 0.09
            + positioning as f64 * 0.10;
        gk_skill += c.state().team(side.opposite()).match_day_performance * 0.80;
        gk_skill_bonus = (gk_skill - 74.0) * 0.0028;
        gk_fatigue_penalty = goalkeeper_fatigue_penalty(gk_energy);
    } else {
        base_conditional = base_conditional.max(0.88);
    }
    let mode_finishing_boost = (if c.input().simulated_action_minutes() < 90 { 0.055 } else { 0.0 })
        + if source == "SET_PIECE_HEADER" { 0.095 } else { 0.0 };
    let goal_conditional = (base_conditional
        + shooter_delta
        + finishing_skill_bonus
        - gk_skill_bonus
        + gk_fatigue_penalty
        + 0.072
        + mode_finishing_boost)
        .clamp(0.02, 0.94);
    let save = java_round((1.0 - goal_conditional) * 100.0).clamp(if context.penalty { 4 } else { 3 }, 97);

    if c.rng_mut().next_int(100) < save {
        c.state_mut().team_mut(side.opposite()).saves += 1;
        c.emit(
            EventType::Save,
            Some(side.opposite()),
            gk.as_deref(),
            Some(shooter_id),
            vec![
                kv("shotQuality", quality),
                kv("xgMilli", xg),
                kv("saveChance", save),
                kv("gkPositioning", positioning),
                kv("placement", placement),
                kv("power", power),
                kv("oneVsOne", context.one_vs_one),
            ],
        );
        let gk_stat = gk
            .as_deref()
            .and_then(|gid| c.input().team(side.opposite()).player(gid))
            .map(|p| p.attributes.get(0) as i32)
            .unwrap_or(0);
        let claim_chance = (48 + gk_stat / 5).clamp(50, 72);
        let save_outcome = c.rng_mut().next_int(100);
        if save_outcome < claim_chance {
            if let Some(ref gid) = gk {
                c.state_mut().ball.set_controlled(side.opposite(), gid.clone());
                c.emit(
                    EventType::GoalkeeperClaim,
                    Some(side.opposite()),
                    Some(gid),
                    Some(shooter_id),
                    vec![kv("afterSave", true)],
                );
                c.change_possession(side.opposite(), gid, "GOALKEEPER_SAVE_CLAIM");
            }
        } else if save_outcome < (claim_chance + 18).min(92) {
            c.emit(
                EventType::Rebound,
                Some(side.opposite()),
                gk.as_deref(),
                Some(shooter_id),
                vec![kv("outForCorner", true)],
            );
            let bx = c.state().ball.x;
            c.schedule_restart(
                RestartType::Corner,
                side,
                "SAVE_PARRIED_OUT",
                if bx < 50 { 0 } else { 100 },
                geometry::goal_y(side),
            );
        } else {
            c.emit(
                EventType::Rebound,
                Some(side.opposite()),
                gk.as_deref(),
                Some(shooter_id),
                Vec::new(),
            );
            let ry = (geometry::goal_y(side) - geometry::attack_direction(side) * 10).clamp(5, 95);
            make_loose(c, Some(side.opposite()), gk.as_deref(), 50, ry, "SAVE_REBOUND");
        }
        return;
    }

    score_goal(c, side, shooter_id, quality, xg, source);
}

fn score_goal(c: &mut dyn MatchRuntime, side: TeamSide, scorer: &str, quality: i32, xg_milli: i32, source: &str) {
    c.state_mut().score.add_goal(side);
    c.state_mut().team_mut(side).goals += 1;
    c.state_mut().ball.set_position(50, geometry::goal_y(side));
    let score = format!("{}-{}", c.state().score.home_goals, c.state().score.away_goals);
    c.emit(
        EventType::Goal,
        Some(side),
        Some(scorer),
        None,
        vec![
            kv("shotQuality", quality),
            kv("xgMilli", xg_milli),
            kv("source", source),
            kv("score", score),
        ],
    );
    c.add_stoppage(30);
    c.schedule_restart(RestartType::Kickoff, side.opposite(), "AFTER_GOAL", 50, 50);
}

pub fn score_own_goal(c: &mut dyn MatchRuntime, scoring_side: TeamSide, defender: &str, source: &str) {
    c.state_mut().score.add_goal(scoring_side);
    c.state_mut().team_mut(scoring_side).goals += 1;
    let score = format!("{}-{}", c.state().score.home_goals, c.state().score.away_goals);
    c.emit(
        EventType::OwnGoal,
        Some(scoring_side.opposite()),
        Some(defender),
        None,
        vec![kv("scoringSide", scoring_side.as_str()), kv("source", source)],
    );
    c.emit(
        EventType::Goal,
        Some(scoring_side),
        Some(defender),
        None,
        vec![kv("ownGoal", true), kv("source", source), kv("score", score)],
    );
    c.add_stoppage(30);
    c.schedule_restart(RestartType::Kickoff, scoring_side.opposite(), "AFTER_OWN_GOAL", 50, 50);
}

pub fn goalkeeper_fatigue_penalty(energy: i32) -> f64 {
    (60 - energy.clamp(0, 100)).max(0) as f64 * 0.00075
}
