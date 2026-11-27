use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite};
use crate::v4::ball_play::{BallPlayOutcome, EmittedPlayEvent};
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::goalkeeper::{resolve_gk_aerial_claim, resolve_gk_shot_reaction, GkAerialOutcome, GkShotOutcome};
use crate::v4::input::{MatchInputV4, WeatherCondition};
use crate::v4::movement::lookup_attrs;
use crate::v4::own_goal::{maybe_own_goal, DeflectionCause};
use crate::v4::shooting::{resolve_shot_v4, ShotKindV4};
use crate::v4::spatial::{
    attacking_goal_center, distance_to_attacking_goal, penalty_spot, TeamSideV4, Vec2, Vec3,
    PITCH_HALF_WIDTH_M, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};

/// Executes a Corner Kick using `corners`, `crossing`, `heading`, `jumping_reach`,
/// `marking`, `command_of_area`, `aerial_reach`, and `punching`.
///
/// Prioritizes designated takers from `SetPiecePlanV4::corner_taker_slot_ids` before falling
/// back to the highest-rated specialist. Stoppage duration is left at `0` so the DES
/// `DeadBallState` clock accounts for actual dead-ball time without double counting.
pub fn execute_corner_v4(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    att_side: TeamSideV4,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    let mut events = Vec::with_capacity(6);
    let def_side = att_side.opposite();
    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);

    let plan = match att_side {
        TeamSideV4::Home => &input.home_team.set_piece_plan,
        TeamSideV4::Away => &input.away_team.set_piece_plan,
    };
    let taker_idx = pick_planned_or_best_specialist(
        state,
        input,
        att_side,
        &plan.corner_taker_slot_ids,
        |a| a.technical.corners,
    );
    let taker_id = state.players[taker_idx].player_id.clone();
    let taker_attrs = *lookup_attrs(input, &state.players[taker_idx]);

    let corner_x = match att_side {
        TeamSideV4::Home => PITCH_LENGTH_M,
        TeamSideV4::Away => 0.0,
    };
    let corner_pos = Vec3::new(corner_x, if rng.next_boolean() { 0.5 } else { 67.5 }, 0.0);
    let delivery_x = match att_side {
        TeamSideV4::Home => PITCH_LENGTH_M - 8.0,
        TeamSideV4::Away => 8.0,
    };
    let delivery_pos = Vec3::new(
        delivery_x,
        (PITCH_HALF_WIDTH_M + (rng.next_double() as f32 - 0.5) * 10.0).clamp(24.0, 44.0),
        2.35,
    );

    events.push(EmittedPlayEvent {
        kind: EventTypeV4::Corner,
        tier: HighlightTier::Extended,
        seq_id: Some(seq_id),
        side: Some(att_side),
        actor: Some(taker_id),
        target: None,
        ball_pos: corner_pos,
        metadata: EventMetadataV4 {
            in_box: Some(true),
            ..EventMetadataV4::default()
        },
    });

    // 1. Goalkeeper aerial claim / punch check (`command_of_area`, `aerial_reach`, `punching`)
    let gk_idx = find_gk_idx(state, def_side);
    let gk_id = state.players[gk_idx].player_id.clone();
    let gk_attrs = *lookup_attrs(input, &state.players[gk_idx]);
    let gk_height = lookup_height(input, def_side, &gk_id);

    match resolve_gk_aerial_claim(&gk_attrs, gk_height, true, delivery_pos.z, input, rng) {
        GkAerialOutcome::Claimed => {
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::GoalkeeperClaim,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: None,
                ball_pos: delivery_pos,
                metadata: EventMetadataV4 {
                    in_box: Some(true),
                    outcome: Some("CLAIMED".into()),
                    ..EventMetadataV4::default()
                },
            });
            state.ball.pos = delivery_pos;
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(gk_id);
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: gk_idx,
                stoppage_seconds: 0,
                events,
            };
        }
        GkAerialOutcome::PunchedClear => {
            let clear_idx = pick_best_specialist(state, input, def_side, |a| a.mental.anticipation);
            let clear_id = state.players[clear_idx].player_id.clone();
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::GoalkeeperPunch,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id),
                target: None,
                ball_pos: delivery_pos,
                metadata: EventMetadataV4 {
                    in_box: Some(true),
                    outcome: Some("PUNCHED_CLEAR".into()),
                    ..EventMetadataV4::default()
                },
            });
            state.ball.pos = Vec3::ground(state.players[clear_idx].pos.x, state.players[clear_idx].pos.y);
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(clear_id);
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: clear_idx,
                stoppage_seconds: 0,
                events,
            };
        }
        GkAerialOutcome::StayedOnLine => {}
    }

    // 2. Aerial duel in the box (`heading`, `jumping_reach`, `strength`, `bravery`, `height_cm` vs `marking`)
    let att_idx = pick_best_aerial(state, input, att_side, Some(taker_idx));
    let def_idx = pick_best_aerial(state, input, def_side, None);
    let att_id = state.players[att_idx].player_id.clone();
    let def_id = state.players[def_idx].player_id.clone();

    let delivery_quality = normalize_attr(taker_attrs.technical.corners) * 0.65
        + normalize_attr(taker_attrs.technical.crossing) * 0.35;
    let att_won = resolve_aerial_duel(state, input, att_idx, def_idx, delivery_quality, rng);

    events.push(EmittedPlayEvent {
        kind: EventTypeV4::AerialDuel,
        tier: HighlightTier::None,
        seq_id: Some(seq_id),
        side: Some(if att_won { att_side } else { def_side }),
        actor: Some(if att_won { att_id.clone() } else { def_id.clone() }),
        target: Some(if att_won { def_id.clone() } else { att_id }),
        ball_pos: delivery_pos,
        metadata: EventMetadataV4 {
            in_box: Some(true),
            outcome: Some(if att_won { "ATTACKER_WON".into() } else { "DEFENDER_CLEARED".into() }),
            ..EventMetadataV4::default()
        },
    });

    if att_won {
        state.players[att_idx].pos = delivery_pos.xy();
        let (mut shot_outcome, _) = resolve_shot_v4(
            state,
            input,
            att_idx,
            Some(taker_idx),
            Some(ShotKindV4::Header),
            false,
            rng,
        );
        events.append(&mut shot_outcome.events);
        shot_outcome.events = events;
        shot_outcome.stoppage_seconds = 0;
        shot_outcome
    } else {
        // The defending player has actually reached the delivered ball.
        // Resolve the accidental header from the point of contact, not the
        // player's former off-ball formation slot.
        state.players[def_idx].pos = delivery_pos.xy();
        // A pressured defensive header from a corner may be accidentally
        // directed into its own goal. The event credits the attacking team,
        // while the defender receives an own-goal (not an ordinary goal).
        let def_attrs = *lookup_attrs(input, &state.players[def_idx]);
        if let Some(goal) = maybe_own_goal(
            state, def_idx, att_side, DeflectionCause::MishitAerialClearance,
            &def_attrs, seq_id, rng,
        ) {
            events.push(goal);
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: pick_best_specialist(state, input, def_side, |a| a.technical.passing),
                stoppage_seconds: 0,
                events,
            };
        }
        state.ball.pos = Vec3::ground(state.players[def_idx].pos.x, state.players[def_idx].pos.y);
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(def_id);
        BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: def_idx,
            stoppage_seconds: 0,
            events,
        }
    }
}

/// Executes a Direct or Lofted Free Kick (`freeKickExecuted = true`) using
/// `free_kicks`, `technique`, `long_shots`, `crossing`, and `heading`.
///
/// Prioritizes designated takers from `SetPiecePlanV4::close_free_kick_taker_slot_ids`
/// (when `dist_m <= 28.0`) or `far_free_kick_taker_slot_ids` (when `dist_m > 28.0`)
/// before falling back to the highest-rated specialist.
pub fn execute_free_kick_v4(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    att_side: TeamSideV4,
    foul_pos: Vec2,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let def_side = att_side.opposite();
    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);

    let dist_m = distance_to_attacking_goal(att_side, foul_pos);
    let plan = match att_side {
        TeamSideV4::Home => &input.home_team.set_piece_plan,
        TeamSideV4::Away => &input.away_team.set_piece_plan,
    };
    let planned_slots = if dist_m <= 28.0 {
        &plan.close_free_kick_taker_slot_ids
    } else {
        &plan.far_free_kick_taker_slot_ids
    };
    let taker_idx = pick_planned_or_best_specialist(
        state,
        input,
        att_side,
        planned_slots,
        |a| a.technical.free_kicks,
    );
    let taker_id = state.players[taker_idx].player_id.clone();
    let taker_attrs = *lookup_attrs(input, &state.players[taker_idx]);

    let fk_pos = Vec3::ground(foul_pos.x, foul_pos.y).clamp_to_pitch();
    events.push(EmittedPlayEvent {
        kind: EventTypeV4::FreeKick,
        tier: if dist_m <= 28.0 { HighlightTier::Extended } else { HighlightTier::None },
        seq_id: Some(seq_id),
        side: Some(att_side),
        actor: Some(taker_id.clone()),
        target: None,
        ball_pos: fk_pos,
        metadata: EventMetadataV4 {
            free_kick_executed: Some(true),
            distance_m: Some(dist_m),
            ..EventMetadataV4::default()
        },
    });

    if dist_m <= 28.0 {
        // Direct shot on goal from free kick
        let gk_idx = find_gk_idx(state, def_side);
        let gk_id = state.players[gk_idx].player_id.clone();
        let gk_attrs = *lookup_attrs(input, &state.players[gk_idx]);

        let fk_skill = normalize_attr_elite(taker_attrs.technical.free_kicks) * 0.58
            + normalize_attr(taker_attrs.technical.technique) * 0.24
            + normalize_attr(taker_attrs.technical.long_shots) * 0.18;
        let xg = ((0.16 - (dist_m - 17.0).max(0.0) * 0.009) * (0.65 + fk_skill * 0.70)).clamp(0.02, 0.24);

        state.team_state_mut(att_side).shots += 1;
        state.team_state_mut(att_side).xg_millis += (xg * 1000.0).round() as u32;

        let on_target = (rng.next_double() as f32) < (0.28 + fk_skill * 0.36).clamp(0.20, 0.68);
        let goal_center = attacking_goal_center(att_side);

        if !on_target {
            state.team_state_mut(att_side).shots_off_target += 1;
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOffTarget,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(taker_id),
                target: None,
                ball_pos: Vec3::new(goal_center.x, goal_center.y, 2.8),
                metadata: EventMetadataV4 {
                    free_kick_executed: Some(true),
                    distance_m: Some(dist_m),
                    xg: Some(xg),
                    shot_type: Some("CURLED".into()),
                    finesse: Some(true),
                    scored: Some(false),
                    outcome: Some("OFF_TARGET".into()),
                    ..EventMetadataV4::default()
                },
            });
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: gk_idx,
                stoppage_seconds: 0,
                events,
            };
        }

        state.team_state_mut(att_side).shots_on_target += 1;
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::ShotOnTarget,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(att_side),
            actor: Some(taker_id.clone()),
            target: Some(gk_id.clone()),
            ball_pos: Vec3::new(goal_center.x, goal_center.y, 1.5),
            metadata: EventMetadataV4 {
                free_kick_executed: Some(true),
                distance_m: Some(dist_m),
                xg: Some(xg),
                shot_type: Some("CURLED".into()),
                ..EventMetadataV4::default()
            },
        });
        let gk_res = resolve_gk_shot_reaction(
            &gk_attrs,
            state.players[gk_idx].position_familiarity,
            state.players[gk_idx].fatigue.aerobic,
            xg,
            dist_m,
            false,
            true,
            false,
            input,
            rng,
        );
        if gk_res == GkShotOutcome::GoalConceded {
            state.team_state_mut(att_side).goals += 1;
            state.players[taker_idx].goals += 1;
            state.players[taker_idx].rating_tenths = (state.players[taker_idx].rating_tenths + 9).min(100);
            let goal_pos = Vec3::new(goal_center.x, goal_center.y, 1.8);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Goal,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(taker_id),
                target: None,
                ball_pos: goal_pos,
                metadata: EventMetadataV4 {
                    free_kick_executed: Some(true),
                    distance_m: Some(dist_m),
                    xg: Some(xg),
                    shot_type: Some("CURLED".into()),
                    finesse: Some(true),
                    scored: Some(true),
                    outcome: Some("GOAL".into()),
                    goalkeeper: Some(gk_id),
                    ..EventMetadataV4::default()
                },
            });
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: pick_best_specialist(state, input, def_side, |a| a.technical.passing),
                stoppage_seconds: 0,
                events,
            };
        }

        state.team_state_mut(def_side).saves += 1;
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::Save,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(def_side),
            actor: Some(gk_id.clone()),
            target: Some(taker_id),
            ball_pos: Vec3::new(goal_center.x, goal_center.y, 1.5),
            metadata: EventMetadataV4 {
                free_kick_executed: Some(true),
                distance_m: Some(dist_m),
                xg: Some(xg),
                outcome: Some("SAVED".into()),
                ..EventMetadataV4::default()
            },
        });
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(gk_id);
        return BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: gk_idx,
            stoppage_seconds: 0,
            events,
        };
    }

    // Far free kick: restart possession cleanly
    state.ball.pos = fk_pos;
    state.ball.control = BallControlState::Controlled;
    state.ball.owner_side = Some(att_side);
    state.ball.owner_player_id = Some(taker_id);
    BallPlayOutcome {
        next_side: att_side,
        next_carrier_idx: taker_idx,
        stoppage_seconds: 0,
        events,
    }
}

/// Executes an in-match Penalty Kick (`penalties`, `composure`, `pressure` vs `reflexes`, `one_on_ones`, `handling`)
/// with IFAB standard `allow_penalty_rebound = true`.
pub fn execute_penalty_kick_v4(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    att_side: TeamSideV4,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    execute_penalty_kick_v4_with_rules(state, input, att_side, true, rng)
}

/// Executes an in-match Penalty Kick with explicit ruleset control over live rebounds (`allow_penalty_rebound`).
///
/// Prioritizes designated takers from `SetPiecePlanV4::penalty_taker_slot_ids` before falling
/// back to the highest-rated specialist. When a save is parried rather than held cleanly and
/// `allow_penalty_rebound == true`, leaves the ball in `BallControlState::Loose` to open a
/// live rebound contest instead of forcing `Controlled`.
pub fn execute_penalty_kick_v4_with_rules(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    att_side: TeamSideV4,
    allow_penalty_rebound: bool,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let def_side = att_side.opposite();
    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);

    let plan = match att_side {
        TeamSideV4::Home => &input.home_team.set_piece_plan,
        TeamSideV4::Away => &input.away_team.set_piece_plan,
    };
    let taker_idx = pick_planned_or_best_specialist(
        state,
        input,
        att_side,
        &plan.penalty_taker_slot_ids,
        |a| a.technical.penalties,
    );
    let gk_idx = find_gk_idx(state, def_side);
    let taker_id = state.players[taker_idx].player_id.clone();
    let gk_id = state.players[gk_idx].player_id.clone();
    let taker_attrs = *lookup_attrs(input, &state.players[taker_idx]);
    let gk_attrs = *lookup_attrs(input, &state.players[gk_idx]);

    let spot = penalty_spot(att_side);
    let spot_pos = Vec3::ground(spot.x, spot.y);

    events.push(EmittedPlayEvent {
        kind: EventTypeV4::PenaltyAwarded,
        tier: HighlightTier::GoalKey,
        seq_id: Some(seq_id),
        side: Some(att_side),
        actor: Some(taker_id.clone()),
        target: None,
        ball_pos: spot_pos,
        metadata: EventMetadataV4 {
            in_box: Some(true),
            ..EventMetadataV4::default()
        },
    });

    let pen_skill = normalize_attr_elite(taker_attrs.technical.penalties) * 0.50
        + normalize_attr(taker_attrs.mental.composure) * 0.30
        + normalize_attr(taker_attrs.hidden.pressure) * 0.20;
    let gk_skill = normalize_attr_elite(gk_attrs.goalkeeping.reflexes) * 0.55
        + normalize_attr(gk_attrs.goalkeeping.one_on_ones) * 0.45;

    let xg = 0.78f32;
    state.team_state_mut(att_side).shots += 1;
    state.team_state_mut(att_side).xg_millis += 780;

    let score_prob = (0.76 + (pen_skill - gk_skill) * 0.20).clamp(0.56, 0.93);
    let scored = (rng.next_double() as f32) < score_prob;

    let mut meta = EventMetadataV4 {
        in_box: Some(true),
        distance_m: Some(11.0),
        xg: Some(xg),
        scored: Some(scored),
        goalkeeper: Some(gk_id.clone()),
        outcome: Some(if scored { "GOAL".into() } else { "SAVED".into() }),
        ..EventMetadataV4::default()
    };

    events.push(EmittedPlayEvent {
        kind: EventTypeV4::PenaltyKick,
        tier: HighlightTier::GoalKey,
        seq_id: Some(seq_id),
        side: Some(att_side),
        actor: Some(taker_id.clone()),
        target: Some(gk_id.clone()),
        ball_pos: spot_pos,
        metadata: meta.clone(),
    });

    let goal_center = attacking_goal_center(att_side);

    if scored {
        state.team_state_mut(att_side).shots_on_target += 1;
        state.team_state_mut(att_side).goals += 1;
        state.players[taker_idx].goals += 1;
        state.players[taker_idx].rating_tenths = (state.players[taker_idx].rating_tenths + 7).min(100);
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::Goal,
            tier: HighlightTier::GoalKey,
            seq_id: Some(seq_id),
            side: Some(att_side),
            actor: Some(taker_id),
            target: None,
            ball_pos: Vec3::new(goal_center.x, goal_center.y, 1.0),
            metadata: meta,
        });
        BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: pick_best_specialist(state, input, def_side, |a| a.technical.passing),
            stoppage_seconds: 0,
            events,
        }
    } else {
        state.team_state_mut(att_side).shots_on_target += 1;
        state.team_state_mut(def_side).saves += 1;

        let handling = normalize_attr(gk_attrs.goalkeeping.handling);
        let wet_slip = match input.match_environment.weather {
            WeatherCondition::HeavyRain => 0.14f32,
            WeatherCondition::LightRain => 0.06f32,
            _ => 0.0f32,
        };
        let hold_prob = (0.26 + handling * 0.44 - wet_slip).clamp(0.12, 0.78);
        let held_cleanly = !allow_penalty_rebound || (rng.next_double() as f32) < hold_prob;

        if held_cleanly {
            meta.outcome = Some("PENALTY_SAVED".into());
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: Some(taker_id),
                ball_pos: Vec3::new(goal_center.x, goal_center.y, 1.0),
                metadata: meta,
            });
            state.ball.pos = Vec3::ground(goal_center.x, goal_center.y).clamp_to_pitch();
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(gk_id.clone());
            state.ball.last_touch_side = Some(def_side);
            state.ball.last_touch_player_id = Some(gk_id);
            BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: gk_idx,
                stoppage_seconds: 0,
                events,
            }
        } else {
            meta.outcome = Some("PENALTY_PARRIED_REBOUND".into());
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: Some(taker_id),
                ball_pos: Vec3::new(goal_center.x, goal_center.y, 1.0),
                metadata: meta,
            });
            let rebound_dist = 4.5 + (rng.next_double() as f32) * 4.5;
            let rebound_x = match att_side {
                TeamSideV4::Home => (PITCH_LENGTH_M - rebound_dist).clamp(0.0, PITCH_LENGTH_M),
                TeamSideV4::Away => rebound_dist.clamp(0.0, PITCH_LENGTH_M),
            };
            let rebound_y = (PITCH_HALF_WIDTH_M + (rng.next_double() as f32 - 0.5) * 10.0)
                .clamp(16.0, 52.0);
            let rebound_pos = Vec3::ground(rebound_x, rebound_y).clamp_to_pitch();

            events.push(EmittedPlayEvent {
                kind: EventTypeV4::LooseBall,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: None,
                ball_pos: rebound_pos,
                metadata: EventMetadataV4 {
                    in_box: Some(true),
                    outcome: Some("PENALTY_REBOUND".into()),
                    ..EventMetadataV4::default()
                },
            });

            state.ball.pos = rebound_pos;
            state.ball.control = BallControlState::Loose;
            state.ball.owner_side = None;
            state.ball.owner_player_id = None;
            state.ball.last_touch_side = Some(def_side);
            state.ball.last_touch_player_id = Some(gk_id);
            BallPlayOutcome {
                next_side: att_side,
                next_carrier_idx: taker_idx,
                stoppage_seconds: 0,
                events,
            }
        }
    }
}

fn resolve_aerial_duel(
    state: &MatchStateV4,
    input: &MatchInputV4,
    att_idx: usize,
    def_idx: usize,
    delivery_quality: f32,
    rng: &mut DeterministicRng,
) -> bool {
    let att_p = &state.players[att_idx];
    let def_p = &state.players[def_idx];
    let att_a = lookup_attrs(input, att_p);
    let def_a = lookup_attrs(input, def_p);
    let att_h = lookup_height(input, att_p.side, &att_p.player_id) as f32;
    let def_h = lookup_height(input, def_p.side, &def_p.player_id) as f32;

    let att_score = (normalize_attr(att_a.physical.jumping_reach) * 0.34
        + normalize_attr(att_a.technical.heading) * 0.24
        + normalize_attr(att_a.physical.strength) * 0.16
        + normalize_attr(att_a.mental.bravery) * 0.14
        + normalize_attr(att_a.mental.off_the_ball) * 0.12)
        * att_p.position_familiarity
        + ((att_h - 180.0) * 0.008)
        + (delivery_quality - 0.5) * 0.16;

    let def_score = (normalize_attr(def_a.physical.jumping_reach) * 0.32
        + normalize_attr(def_a.technical.marking) * 0.24
        + normalize_attr(def_a.mental.positioning) * 0.18
        + normalize_attr(def_a.physical.strength) * 0.16
        + normalize_attr(def_a.mental.bravery) * 0.10)
        * def_p.position_familiarity
        + ((def_h - 180.0) * 0.008);

    let win_prob = (0.42 + (att_score - def_score) * 0.55).clamp(0.14, 0.72);
    (rng.next_double() as f32) < win_prob
}

pub(crate) fn pick_planned_or_best_specialist<F>(
    state: &MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    planned_slot_ids: &[String],
    metric: F,
) -> usize
where
    F: Fn(&crate::v4::attributes::PlayerAttributes54) -> u8,
{
    for slot_id in planned_slot_ids {
        if let Some(idx) = state.players.iter().position(|p| {
            p.side == side && p.on_pitch && !p.sent_off && p.slot_id == *slot_id
        }) {
            return idx;
        }
    }
    pick_best_specialist(state, input, side, metric)
}

fn pick_best_specialist<F>(
    state: &MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    metric: F,
) -> usize
where
    F: Fn(&crate::v4::attributes::PlayerAttributes54) -> u8,
{
    let mut best_idx = 0usize;
    let mut best_val = 0u8;
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != side || p.is_goalkeeper {
            continue;
        }
        let v = metric(lookup_attrs(input, p));
        if v >= best_val {
            best_val = v;
            best_idx = idx;
        }
    }
    best_idx
}

fn pick_best_aerial(
    state: &MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    exclude_idx: Option<usize>,
) -> usize {
    let mut best_idx = 0usize;
    let mut best_score = 0u16;
    for (idx, p) in state.players.iter().enumerate() {
        if Some(idx) == exclude_idx || !p.on_pitch || p.sent_off || p.side != side || p.is_goalkeeper {
            continue;
        }
        let a = lookup_attrs(input, p);
        let score = (a.physical.jumping_reach as u16) * 2 + (a.technical.heading as u16);
        if score >= best_score {
            best_score = score;
            best_idx = idx;
        }
    }
    best_idx
}

fn find_gk_idx(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .or_else(|| state.players.iter().position(|p| p.side == side && p.on_pitch && !p.sent_off))
        .unwrap_or(0)
}

fn lookup_height(input: &MatchInputV4, side: TeamSideV4, player_id: &str) -> u16 {
    let team = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    team.lineup
        .iter()
        .chain(team.bench.iter())
        .find(|p| p.player_id == player_id)
        .map(|p| p.height_cm)
        .unwrap_or(182)
}
