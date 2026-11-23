use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite};
use crate::v4::ball_play::{BallPlayOutcome, EmittedPlayEvent};
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::goalkeeper::{choose_gk_distribution, resolve_gk_shot_reaction, GkShotOutcome};
use crate::v4::input::{MatchInputV4, PreferredFoot, WeatherCondition};
use crate::v4::movement::{evaluate_local_context, lookup_attrs};
use crate::v4::own_goal::{maybe_own_goal, DeflectionCause};
use crate::v4::spatial::{
    attacking_goal_center, distance_to_attacking_goal, is_in_attacking_penalty_box,
    point_to_segment_distance_m, TeamSideV4, Vec2, Vec3, PITCH_HALF_WIDTH_M, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotKindV4 {
    Driven,
    Curled,
    Header,
    OneTouch,
    LongRange,
    Rebound,
}

impl ShotKindV4 {
    pub fn as_str(self) -> &'static str {
        match self {
            ShotKindV4::Driven => "DRIVEN",
            ShotKindV4::Curled => "CURLED",
            ShotKindV4::Header => "HEADER",
            ShotKindV4::OneTouch => "ONE_TOUCH",
            ShotKindV4::LongRange => "LONG_RANGE",
            ShotKindV4::Rebound => "REBOUND",
        }
    }
}

/// Complete Stage C shot resolver:
/// - Distinguishes `finishing` (in-box), `long_shots` (outside box), `heading` + `jumping_reach` (headers),
///   `first_touch` (one-touch), `technique` + `flair` (`CURLED`/`finesse`), `oneVsOne`, and `reboundShot`.
/// - Applies `preferredFoot`/`weakFoot`, `position_familiarity`, pressure, angle, and weather.
/// - Checks defender `Block`, woodwork rebounds, and 9-attribute Goalkeeper reactions
///   (`SavedHeld` -> GK distribution, `SavedParriedRebound` -> live rebound follow-up, `SavedTippedCorner` -> corner flag).
#[allow(clippy::too_many_arguments)]
pub fn resolve_shot_v4(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    shooter_idx: usize,
    assist_idx: Option<usize>,
    forced_kind: Option<ShotKindV4>,
    is_rebound: bool,
    rng: &mut DeterministicRng,
) -> (BallPlayOutcome, bool) {
    let mut events = Vec::with_capacity(6);
    let att_side = state.players[shooter_idx].side;
    let def_side = att_side.opposite();
    let gk_idx = find_gk_idx(state, def_side);

    let shooter_pos = state.players[shooter_idx].pos;
    let shooter_id = state.players[shooter_idx].player_id.clone();
    let gk_id = state.players[gk_idx].player_id.clone();

    let shooter_attrs = *lookup_attrs(input, &state.players[shooter_idx]);
    let gk_attrs = *lookup_attrs(input, &state.players[gk_idx]);
    let shooter_fam = state.players[shooter_idx].position_familiarity;
    let gk_fam = state.players[gk_idx].position_familiarity;
    let gk_aero = state.players[gk_idx].fatigue.aerobic;

    let dist_m = distance_to_attacking_goal(att_side, shooter_pos);
    let in_box = is_in_attacking_penalty_box(att_side, shooter_pos);
    let ctx = evaluate_local_context(state, input, att_side, shooter_pos);
    let pressure_norm = (ctx.pressure / 100.0).clamp(0.0, 1.0);

    // Detect 1v1: inside 18m with low defender presence between shooter and goal
    let one_vs_one = in_box && dist_m <= 16.5 && ctx.nearest_defender_dist_m >= 3.8 && !is_rebound;

    // Classify shot kind
    let kind = if is_rebound {
        ShotKindV4::Rebound
    } else if let Some(fk) = forced_kind {
        fk
    } else if !in_box {
        if shooter_attrs.technical.technique >= 14 && (rng.next_double() as f32) < 0.35 {
            ShotKindV4::Curled
        } else {
            ShotKindV4::LongRange
        }
    } else if shooter_attrs.technical.technique >= 13
        && shooter_attrs.mental.flair >= 12
        && (rng.next_double() as f32) < 0.32
    {
        ShotKindV4::Curled
    } else if assist_idx.is_some() && (rng.next_double() as f32) < 0.28 {
        ShotKindV4::OneTouch
    } else {
        ShotKindV4::Driven
    };

    let is_header = kind == ShotKindV4::Header;
    let is_finesse = kind == ShotKindV4::Curled;

    // Angle quality: central shots have higher xG than tight-angle shots
    let lateral_off = (shooter_pos.y - PITCH_HALF_WIDTH_M).abs();
    let angle_factor = (1.0 - (lateral_off / 28.0) * 0.55).clamp(0.38, 1.0);

    // Weak-foot penalty (does not apply to headers)
    let foot_mod = if is_header {
        1.0
    } else {
        weak_foot_shot_modifier(input, att_side, &shooter_id, shooter_pos)
    };

    // Weather slip penalty
    let wet_pen = match input.match_environment.weather {
        WeatherCondition::HeavyRain => 0.05,
        WeatherCondition::LightRain => 0.02,
        _ => 0.0,
    };

    // Attribute-driven finishing quality
    let fin_skill = match kind {
        ShotKindV4::Header => {
            normalize_attr_elite(shooter_attrs.technical.heading) * 0.55
                + normalize_attr(shooter_attrs.physical.jumping_reach) * 0.25
                + normalize_attr(shooter_attrs.mental.bravery) * 0.20
        }
        ShotKindV4::LongRange => {
            normalize_attr_elite(shooter_attrs.technical.long_shots) * 0.62
                + normalize_attr(shooter_attrs.technical.technique) * 0.24
                + normalize_attr(shooter_attrs.mental.composure) * 0.14
        }
        ShotKindV4::OneTouch | ShotKindV4::Rebound => {
            normalize_attr_elite(shooter_attrs.technical.finishing) * 0.48
                + normalize_attr(shooter_attrs.technical.first_touch) * 0.24
                + normalize_attr(shooter_attrs.mental.anticipation) * 0.16
                + normalize_attr(shooter_attrs.mental.composure) * 0.12
        }
        ShotKindV4::Curled => {
            normalize_attr_elite(shooter_attrs.technical.finishing) * 0.42
                + normalize_attr(shooter_attrs.technical.technique) * 0.34
                + normalize_attr(shooter_attrs.mental.composure) * 0.24
        }
        ShotKindV4::Driven => {
            normalize_attr_elite(shooter_attrs.technical.finishing) * 0.56
                + normalize_attr(shooter_attrs.technical.technique) * 0.22
                + normalize_attr(shooter_attrs.mental.composure) * 0.22
        }
    };

    let composure = normalize_attr(shooter_attrs.mental.composure);
    let press_pen = pressure_norm * (0.30 - composure * 0.14);
    let energy_mod = (0.80 + 0.20 * (state.players[shooter_idx].fatigue.anaerobic / 100.0)).clamp(0.74, 1.0);

    let dist_curve = if in_box {
        (0.38 - dist_m * 0.013).clamp(0.08, 0.38)
    } else {
        (0.14 - (dist_m - 16.5).max(0.0) * 0.0065).clamp(0.02, 0.14)
    };
    let one_v_one_bonus = if one_vs_one { 1.32 } else { 1.0 };
    let rebound_bonus = if is_rebound { 1.25 } else { 1.0 };

    let xg = (dist_curve
        * angle_factor
        * one_v_one_bonus
        * rebound_bonus
        * (0.72 + fin_skill * 0.56)
        * foot_mod
        * (0.88 + 0.12 * shooter_fam)
        * energy_mod
        * (1.0 - press_pen - wet_pen))
        .clamp(0.015, 0.72);

    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);
    state.team_state_mut(att_side).shots += 1;
    state.team_state_mut(att_side).xg_millis += (xg * 1000.0).round() as u32;

    let mut base_meta = EventMetadataV4 {
        in_box: Some(in_box),
        distance_m: Some(dist_m),
        xg: Some(xg),
        pressure: Some(ctx.pressure.round() as u8),
        shot_type: Some(kind.as_str().into()),
        finesse: if is_finesse { Some(true) } else { None },
        rebound_shot: if is_rebound { Some(true) } else { None },
        one_vs_one: if one_vs_one { Some(true) } else { None },
        goalkeeper: Some(gk_id.clone()),
        ..EventMetadataV4::default()
    };

    // Emit GoalkeeperRush when an aggressive sweeper-keeper narrows a 1v1
    if one_vs_one && gk_attrs.goalkeeping.rushing_out >= 13 {
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::GoalkeeperRush,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(def_side),
            actor: Some(gk_id.clone()),
            target: Some(shooter_id.clone()),
            ball_pos: Vec3::ground(state.players[gk_idx].pos.x, state.players[gk_idx].pos.y).clamp_to_pitch(),
            metadata: EventMetadataV4 {
                one_vs_one: Some(true),
                in_box: Some(true),
                ..EventMetadataV4::default()
            },
        });
    }

    // 1. Check defender Block along shot corridor to goal
    let goal_center = attacking_goal_center(att_side);
    if !one_vs_one && !is_header {
        if let Some((blk_idx, blk_prob)) = find_shot_blocker(state, input, att_side, shooter_pos, goal_center) {
            if (rng.next_double() as f32) < blk_prob {
                let blk_id = state.players[blk_idx].player_id.clone();
                let blk_pos = Vec3::new(state.players[blk_idx].pos.x, state.players[blk_idx].pos.y, 0.7);
                state.team_state_mut(att_side).shots_blocked += 1;
                let def_attrs = *lookup_attrs(input, &state.players[blk_idx]);
                let own_goal = maybe_own_goal(
                    state, blk_idx, att_side, DeflectionCause::BlockedShot,
                    &def_attrs, seq_id, rng,
                );
                base_meta.scored = Some(own_goal.is_some());
                base_meta.outcome = Some(if own_goal.is_some() { "DEFLECTED_OWN_GOAL" } else { "BLOCKED" }.into());
                events.push(EmittedPlayEvent {
                    kind: EventTypeV4::Shot,
                    tier: if xg >= 0.15 { HighlightTier::Extended } else { HighlightTier::None },
                    seq_id: Some(seq_id),
                    side: Some(att_side),
                    actor: Some(shooter_id),
                    target: Some(blk_id.clone()),
                    ball_pos: blk_pos,
                    metadata: base_meta,
                });
                events.push(EmittedPlayEvent {
                    kind: EventTypeV4::Block,
                    tier: if xg >= 0.15 { HighlightTier::Extended } else { HighlightTier::None },
                    seq_id: Some(seq_id),
                    side: Some(def_side),
                    actor: Some(blk_id.clone()),
                    target: None,
                    ball_pos: blk_pos,
                    metadata: EventMetadataV4::default(),
                });
                if let Some(goal) = own_goal {
                    events.push(goal);
                    return (
                        BallPlayOutcome {
                            next_side: def_side,
                            next_carrier_idx: first_outfield_idx(state, def_side),
                            stoppage_seconds: 35,
                            events,
                        },
                        false,
                    );
                }
                state.ball.pos = blk_pos;
                state.ball.control = BallControlState::Controlled;
                state.ball.owner_side = Some(def_side);
                state.ball.owner_player_id = Some(blk_id);
                return (
                    BallPlayOutcome {
                        next_side: def_side,
                        next_carrier_idx: blk_idx,
                        stoppage_seconds: 0,
                        events,
                    },
                    false,
                );
            }
        }
    }

    // 2. Check on-target vs off-target
    let on_target_prob = (0.34 + fin_skill * 0.34 * foot_mod - press_pen * 0.55 - wet_pen)
        .clamp(0.18, 0.78);
    if (rng.next_double() as f32) > on_target_prob {
        state.team_state_mut(att_side).shots_off_target += 1;
        base_meta.scored = Some(false);
        base_meta.outcome = Some("OFF_TARGET".into());
        let miss_pos = Vec3::new(goal_center.x, (goal_center.y + 5.0).clamp(0.0, 68.0), 1.8);
        events.push(EmittedPlayEvent {
            kind: if is_header { EventTypeV4::Header } else { EventTypeV4::ShotOffTarget },
            tier: if xg >= 0.18 { HighlightTier::Extended } else { HighlightTier::None },
            seq_id: Some(seq_id),
            side: Some(att_side),
            actor: Some(shooter_id),
            target: None,
            ball_pos: miss_pos,
            metadata: base_meta,
        });
        let gk_pos = Vec3::ground(state.players[gk_idx].pos.x, state.players[gk_idx].pos.y).clamp_to_pitch();
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::GoalKick,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(def_side),
            actor: Some(gk_id.clone()),
            target: None,
            ball_pos: gk_pos,
            metadata: EventMetadataV4::default(),
        });
        state.ball.pos = gk_pos;
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(gk_id);
        return (
            BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: gk_idx,
                stoppage_seconds: 12,
                events,
            },
            false,
        );
    }

    // 3. Shot is ON TARGET — resolve via 9-attribute Goalkeeper system
    state.team_state_mut(att_side).shots_on_target += 1;
    let gk_outcome = resolve_gk_shot_reaction(
        &gk_attrs,
        gk_fam,
        gk_aero,
        xg,
        dist_m,
        one_vs_one,
        is_finesse,
        is_header,
        input,
        rng,
    );

    match gk_outcome {
        GkShotOutcome::GoalConceded => {
            state.team_state_mut(att_side).goals += 1;
            state.players[shooter_idx].goals += 1;
            state.players[shooter_idx].rating_tenths =
                (state.players[shooter_idx].rating_tenths + 8).min(100);
            let assist_id = assist_idx
                .filter(|&i| i != shooter_idx && state.players[i].side == att_side)
                .map(|i| {
                    state.players[i].assists += 1;
                    state.players[i].rating_tenths = (state.players[i].rating_tenths + 4).min(100);
                    state.players[i].player_id.clone()
                });
            base_meta.scored = Some(true);
            base_meta.outcome = Some("GOAL".into());
            let goal_pos = Vec3::new(goal_center.x, goal_center.y, 1.1);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOnTarget,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id.clone()),
                target: Some(gk_id),
                ball_pos: goal_pos,
                metadata: base_meta.clone(),
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Goal,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: assist_id,
                ball_pos: goal_pos,
                metadata: base_meta,
            });
            state.ball.pos = Vec3::center_spot();
            let next_kick = first_outfield_idx(state, def_side);
            (
                BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: next_kick,
                    stoppage_seconds: 35,
                    events,
                },
                false,
            )
        }
        GkShotOutcome::SavedHeld => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("SAVED_HELD".into());
            let tier = if xg >= 0.14 { HighlightTier::Extended } else { HighlightTier::None };
            let save_pos = Vec3::new(state.players[gk_idx].pos.x, state.players[gk_idx].pos.y, 1.0);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOnTarget,
                tier,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: Some(gk_id.clone()),
                ball_pos: save_pos,
                metadata: base_meta,
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: None,
                ball_pos: save_pos,
                metadata: EventMetadataV4 {
                    outcome: Some("HELD".into()),
                    ..EventMetadataV4::default()
                },
            });

            // GK distribution (`kicking` / `throwing`)
            let (recv_idx, is_long, dist_pos) = choose_gk_distribution(state, &gk_attrs, def_side, rng);
            let recv_id = state.players[recv_idx].player_id.clone();
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::GoalkeeperDistribution,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(def_side),
                actor: Some(gk_id),
                target: Some(recv_id.clone()),
                ball_pos: dist_pos,
                metadata: EventMetadataV4 {
                    pass_type: Some(if is_long { "LONG".into() } else { "SHORT".into() }),
                    ..EventMetadataV4::default()
                },
            });
            state.ball.pos = dist_pos;
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(recv_id);
            (
                BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: recv_idx,
                    stoppage_seconds: 0,
                    events,
                },
                false,
            )
        }
        GkShotOutcome::SavedParriedRebound => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("PARRIED_REBOUND".into());
            let reb_x = match att_side {
                TeamSideV4::Home => (PITCH_LENGTH_M - 8.5).clamp(0.0, PITCH_LENGTH_M),
                TeamSideV4::Away => 8.5,
            };
            let reb_pos = Vec3::new(reb_x, (PITCH_HALF_WIDTH_M + (rng.next_double() as f32 - 0.5) * 10.0).clamp(16.0, 52.0), 0.3);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOnTarget,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: Some(gk_id.clone()),
                ball_pos: reb_pos,
                metadata: base_meta,
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id),
                target: None,
                ball_pos: reb_pos,
                metadata: EventMetadataV4 {
                    outcome: Some("PARRIED".into()),
                    ..EventMetadataV4::default()
                },
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Rebound,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: None,
                actor: None,
                target: None,
                ball_pos: reb_pos,
                metadata: EventMetadataV4 {
                    in_box: Some(true),
                    ..EventMetadataV4::default()
                },
            });

            // Who reacts fastest to the loose rebound? (`anticipation` + `acceleration`)
            let att_follow = nearest_box_player(state, att_side, reb_pos.xy());
            let def_follow = nearest_box_player(state, def_side, reb_pos.xy());
            let att_react = normalize_attr(lookup_attrs(input, &state.players[att_follow]).mental.anticipation);
            let def_react = normalize_attr(lookup_attrs(input, &state.players[def_follow]).mental.anticipation);
            let att_wins_reb = (rng.next_double() as f32) < (0.42 + (att_react - def_react) * 0.25).clamp(0.20, 0.65);

            state.ball.pos = reb_pos;
            if att_wins_reb && !is_rebound {
                // Attacking follow-up rebound shot!
                state.players[att_follow].pos = reb_pos.xy();
                let (mut follow_outcome, follow_corner) =
                    resolve_shot_v4(state, input, att_follow, None, Some(ShotKindV4::Rebound), true, rng);
                events.append(&mut follow_outcome.events);
                follow_outcome.events = events;
                return (follow_outcome, follow_corner);
            }

            let def_id = state.players[def_follow].player_id.clone();
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(def_id);
            (
                BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: def_follow,
                    stoppage_seconds: 0,
                    events,
                },
                false,
            )
        }
        GkShotOutcome::SavedTippedCorner => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("TIPPED_CORNER".into());
            let save_pos = Vec3::new(goal_center.x, goal_center.y, 2.0);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOnTarget,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: Some(gk_id.clone()),
                ball_pos: save_pos,
                metadata: base_meta,
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id),
                target: None,
                ball_pos: save_pos,
                metadata: EventMetadataV4 {
                    outcome: Some("TIPPED_CORNER".into()),
                    ..EventMetadataV4::default()
                },
            });
            state.team_state_mut(att_side).corners += 1;
            // Signal caller (`session.rs`) to execute a Corner set piece for `att_side`
            (
                BallPlayOutcome {
                    next_side: att_side,
                    next_carrier_idx: shooter_idx,
                    stoppage_seconds: 15,
                    events,
                },
                true,
            )
        }
    }
}

fn find_shot_blocker(
    state: &MatchStateV4,
    input: &MatchInputV4,
    att_side: TeamSideV4,
    shooter_pos: Vec2,
    goal_center: Vec2,
) -> Option<(usize, f32)> {
    let def_side = att_side.opposite();
    let mut best: Option<(usize, f32)> = None;
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != def_side || p.is_goalkeeper {
            continue;
        }
        let d_shooter = p.pos.distance(shooter_pos);
        if d_shooter > 7.0 {
            continue;
        }
        let seg = point_to_segment_distance_m(p.pos, shooter_pos, goal_center);
        if seg <= 2.4 {
            let attrs = lookup_attrs(input, p);
            let skill = normalize_attr(attrs.mental.bravery) * 0.40
                + normalize_attr(attrs.mental.positioning) * 0.35
                + normalize_attr(attrs.mental.anticipation) * 0.25;
            let prob = ((2.4 - seg) / 2.4 * (0.18 + skill * 0.28)).clamp(0.04, 0.38);
            if best.map(|(_, bp)| prob > bp).unwrap_or(true) {
                best = Some((idx, prob));
            }
        }
    }
    best
}

fn weak_foot_shot_modifier(
    input: &MatchInputV4,
    side: TeamSideV4,
    player_id: &str,
    pos: Vec2,
) -> f32 {
    let team = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    let Some(snap) = team.lineup.iter().chain(team.bench.iter()).find(|p| p.player_id == player_id) else {
        return 1.0;
    };
    if snap.preferred_foot == PreferredFoot::Either {
        return 1.0;
    }
    let using_weak = match (side, snap.preferred_foot) {
        (TeamSideV4::Home, PreferredFoot::Right) => pos.y < 24.0,
        (TeamSideV4::Home, PreferredFoot::Left) => pos.y > 44.0,
        (TeamSideV4::Away, PreferredFoot::Right) => pos.y > 44.0,
        (TeamSideV4::Away, PreferredFoot::Left) => pos.y < 24.0,
        _ => false,
    };
    if using_weak {
        0.78 + (snap.weak_foot.clamp(1, 5) as f32) * 0.044
    } else {
        1.0
    }
}

fn find_gk_idx(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .or_else(|| state.players.iter().position(|p| p.side == side && p.on_pitch && !p.sent_off))
        .unwrap_or(0)
}

fn first_outfield_idx(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .unwrap_or(0)
}

fn nearest_box_player(state: &MatchStateV4, side: TeamSideV4, pos: Vec2) -> usize {
    let mut best_idx = first_outfield_idx(state, side);
    let mut best_d = f32::MAX;
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != side || p.is_goalkeeper {
            continue;
        }
        let d = p.pos.distance(pos);
        if d < best_d {
            best_d = d;
            best_idx = idx;
        }
    }
    best_idx
}
