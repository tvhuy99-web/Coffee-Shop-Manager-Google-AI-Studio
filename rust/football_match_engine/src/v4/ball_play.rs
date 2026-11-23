use crate::rng::DeterministicRng;
use crate::v4::attributes::normalize_attr;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PitchQuality, PreferredFoot, WeatherCondition};
use crate::v4::movement::{evaluate_local_context, lookup_attrs, LocalSpatialContext};
use crate::v4::perception::PassKindV4;
use crate::v4::psychology::{check_impact_injury, resolve_foul_discipline};
use crate::v4::spatial::{
    attacking_progress_m, TeamSideV4, Vec2, Vec3, PITCH_HALF_LENGTH_M, PITCH_LENGTH_M,
    PITCH_WIDTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4};

#[derive(Debug, Clone)]
pub struct BallPlayOutcome {
    pub next_side: TeamSideV4,
    pub next_carrier_idx: usize,
    pub stoppage_seconds: i32,
    pub events: Vec<EmittedPlayEvent>,
}

#[derive(Debug, Clone)]
pub struct EmittedPlayEvent {
    pub kind: EventTypeV4,
    pub tier: HighlightTier,
    pub seq_id: Option<u32>,
    pub side: Option<TeamSideV4>,
    pub actor: Option<String>,
    pub target: Option<String>,
    pub ball_pos: Vec3,
    pub metadata: EventMetadataV4,
}

/// Evaluates Law 11 (Offside) at the instant the teammate plays the ball:
/// - Exempt if receiver is in own half (`<= 52.5m`) or behind the ball.
/// - Offside if receiver is ahead of the ball AND ahead of the second-last opponent defender
///   (with `off_the_ball` + `anticipation` governing ability to hold the line vs `offside_trap`).
pub fn is_receiver_offside(
    state: &MatchStateV4,
    att_side: TeamSideV4,
    passer_pos: Vec2,
    receiver_pos: Vec2,
) -> bool {
    let recv_prog = attacking_progress_m(att_side, receiver_pos);
    let ball_prog = attacking_progress_m(att_side, passer_pos);
    // Law 11 exemptions: own half or behind the ball
    if recv_prog <= PITCH_HALF_LENGTH_M || recv_prog <= ball_prog + 0.001 {
        return false;
    }

    let def_side = att_side.opposite();
    let mut def_progs: Vec<f32> = state
        .players
        .iter()
        .filter(|p| p.side == def_side && p.on_pitch && !p.sent_off)
        .map(|p| attacking_progress_m(att_side, p.pos))
        .collect();
    if def_progs.len() < 2 {
        return false;
    }
    // Sort descending: largest attacking_progress_m is deepest defender (usually GK),
    // index 1 is the second-last opponent (the official offside line).
    def_progs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let offside_line = def_progs[1];
    recv_prog > offside_line + 0.001
}

#[allow(clippy::too_many_arguments)]
pub fn execute_pass_and_first_touch(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    passer_idx: usize,
    target_idx: usize,
    kind: PassKindV4,
    distance_m: f32,
    progress_gain_m: f32,
    lane_risk: f32,
    interceptor_idx: Option<usize>,
    passer_ctx: LocalSpatialContext,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let att_side = state.players[passer_idx].side;
    let def_side = att_side.opposite();

    let passer_id = state.players[passer_idx].player_id.clone();
    let target_id = state.players[target_idx].player_id.clone();
    let passer_pos = state.players[passer_idx].pos;
    let target_pos = state.players[target_idx].pos;

    let passer_attrs = *lookup_attrs(input, &state.players[passer_idx]);
    let target_attrs = *lookup_attrs(input, &state.players[target_idx]);
    let passer_fam = state.players[passer_idx].position_familiarity;
    let passer_aero = state.players[passer_idx].fatigue.aerobic;
    let passer_ana = state.players[passer_idx].fatigue.anaerobic;
    let psych_comp = state.players[passer_idx].psychology.composure_delta;

    // Law 11: snapshot the receiver's POSITION when the ball is played. An
    // offside position is not itself an offence; the player must subsequently
    // touch/play the pass or interfere with an opponent. An intercepted or
    // misplaced pass must not automatically be whistled offside.
    let receiver_in_offside_position =
        is_receiver_offside(state, att_side, passer_pos, target_pos);

    let (wet_pen, pitch_pen) = environment_ball_penalties(input);
    let foot_mod = weak_foot_pass_modifier(input, att_side, &passer_id, passer_pos, target_pos);

    let pass_skill = match kind {
        PassKindV4::Cross => {
            normalize_attr(passer_attrs.technical.crossing) * 0.55
                + normalize_attr(passer_attrs.technical.technique) * 0.25
                + normalize_attr(passer_attrs.mental.vision) * 0.20
        }
        PassKindV4::Long | PassKindV4::Switch => {
            normalize_attr(passer_attrs.technical.passing) * 0.50
                + normalize_attr(passer_attrs.technical.technique) * 0.25
                + normalize_attr(passer_attrs.mental.vision) * 0.25
        }
        PassKindV4::Through => {
            normalize_attr(passer_attrs.technical.passing) * 0.42
                + normalize_attr(passer_attrs.mental.vision) * 0.36
                + normalize_attr(passer_attrs.technical.technique) * 0.22
        }
        PassKindV4::Short => {
            normalize_attr(passer_attrs.technical.passing) * 0.62
                + normalize_attr(passer_attrs.technical.technique) * 0.20
                + normalize_attr(passer_attrs.mental.composure) * 0.18
        }
    };

    let pressure_norm = (passer_ctx.pressure / 100.0).clamp(0.0, 1.0);
    let composure = (normalize_attr(passer_attrs.mental.composure) + psych_comp).clamp(0.1, 1.0);
    let pressure_pen = pressure_norm * (0.16 - composure * 0.08);
    let dist_pen = ((distance_m - 14.0).max(0.0) * 0.0032).min(0.16);
    let fatigue_mod = (0.82 + 0.18 * ((passer_aero * 0.65 + passer_ana * 0.35) / 100.0)).clamp(0.75, 1.0);

    let execution_accuracy = ((0.745 + pass_skill * 0.20) * fatigue_mod * foot_mod * (0.90 + 0.10 * passer_fam)
        - pressure_pen
        - dist_pen
        - wet_pen
        - pitch_pen)
        .clamp(0.45, 0.98);

    let ball_z: f32 = match kind {
        PassKindV4::Short => 0.0,
        PassKindV4::Through => 0.15,
        PassKindV4::Cross => 2.1,
        PassKindV4::Long | PassKindV4::Switch => 3.2,
    };

    state.team_state_mut(att_side).passes_attempted += 1;

    if let Some(int_idx) = interceptor_idx {
        let int_attrs = *lookup_attrs(input, &state.players[int_idx]);
        let int_id = state.players[int_idx].player_id.clone();
        let int_pos = state.players[int_idx].pos;
        let flair_deception = normalize_attr(passer_attrs.mental.flair) * 0.08;
        let int_prob = (lane_risk * (0.26 + normalize_attr(int_attrs.mental.anticipation) * 0.18)
            + (1.0 - execution_accuracy) * 0.20
            - flair_deception)
            .clamp(0.015, 0.42);

        if (rng.next_double() as f32) < int_prob {
            let ball_pos = Vec3::new(int_pos.x, int_pos.y, (ball_z * 0.35).min(1.4)).clamp_to_pitch();
            state.ball.pos = ball_pos;
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(int_id.clone());

            let meta = EventMetadataV4 {
                pass_type: Some(kind.as_str().into()),
                distance_m: Some(distance_m),
                pressure: Some(passer_ctx.pressure.round() as u8),
                outcome: Some("INTERCEPTED".into()),
                ..EventMetadataV4::default()
            };
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::PassFailed,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(att_side),
                actor: Some(passer_id),
                target: Some(target_id),
                ball_pos,
                metadata: meta.clone(),
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Interception,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(def_side),
                actor: Some(int_id),
                target: None,
                ball_pos,
                metadata: meta,
            });
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: int_idx,
                stoppage_seconds: 0,
                events,
            };
        }
    }

    // Misplaced pass: check if ball goes out over touchline (Law 15 Throw-In)
    if (rng.next_double() as f32) > execution_accuracy {
        let near_touchline = target_pos.y <= 9.5 || target_pos.y >= (PITCH_WIDTH_M - 9.5);
        let def_idx = evaluate_local_context(state, input, att_side, target_pos)
            .nearest_defender_idx
            .unwrap_or_else(|| first_outfield(state, def_side));
        let def_id = state.players[def_idx].player_id.clone();

        if near_touchline && (rng.next_double() as f32) < 0.55 {
            let touch_y = if target_pos.y < PITCH_WIDTH_M * 0.5 { 0.0 } else { PITCH_WIDTH_M };
            let throw_pos = Vec3::ground(target_pos.x.clamp(2.0, PITCH_LENGTH_M - 2.0), touch_y);
            state.team_state_mut(def_side).throw_ins += 1;
            state.ball.pos = throw_pos;
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(def_id.clone());

            events.push(EmittedPlayEvent {
                kind: EventTypeV4::PassFailed,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(att_side),
                actor: Some(passer_id),
                target: None,
                ball_pos: throw_pos,
                metadata: EventMetadataV4 {
                    pass_type: Some(kind.as_str().into()),
                    distance_m: Some(distance_m),
                    outcome: Some("OUT_OF_PLAY".into()),
                    ..EventMetadataV4::default()
                },
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ThrowIn,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(def_side),
                actor: Some(def_id),
                target: None,
                ball_pos: throw_pos,
                metadata: EventMetadataV4::default(),
            });
            return BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: def_idx,
                stoppage_seconds: 8,
                events,
            };
        }

        let def_pos = state.players[def_idx].pos;
        let ball_pos = Vec3::ground(def_pos.x, def_pos.y).clamp_to_pitch();
        state.ball.pos = ball_pos;
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(def_id.clone());

        let meta = EventMetadataV4 {
            pass_type: Some(kind.as_str().into()),
            distance_m: Some(distance_m),
            pressure: Some(passer_ctx.pressure.round() as u8),
            outcome: Some("MISPLACED".into()),
            ..EventMetadataV4::default()
        };
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::PassFailed,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(att_side),
            actor: Some(passer_id),
            target: Some(def_id),
            ball_pos,
            metadata: meta,
        });
        return BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: def_idx,
            stoppage_seconds: 0,
            events,
        };
    }

    // The pass has evaded interception and reached the intended receiver.
    // Receiver involvement is now established: flag an offence deterministically,
    // independent of flair, timing skill or an arbitrary referee probability.
    if receiver_in_offside_position {
        state.team_state_mut(att_side).offsides += 1;
        let seq_id = state.next_highlight_sequence_id;
        state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);
        let off_pos = Vec3::ground(target_pos.x, target_pos.y).clamp_to_pitch();
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::Offside,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(att_side),
            actor: Some(target_id),
            target: Some(passer_id),
            ball_pos: off_pos,
            metadata: EventMetadataV4 {
                pass_type: Some(kind.as_str().into()),
                outcome: Some("OFFSIDE_INVOLVEMENT".into()),
                ..EventMetadataV4::default()
            },
        });
        let restart_idx = first_outfield(state, def_side);
        state.ball.pos = off_pos;
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(state.players[restart_idx].player_id.clone());
        return BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: restart_idx,
            stoppage_seconds: 12,
            events,
        };
    }

    let target_ctx = evaluate_local_context(state, input, att_side, target_pos);
    let target_pressure_norm = (target_ctx.pressure / 100.0).clamp(0.0, 1.0);
    let first_touch_skill = normalize_attr(target_attrs.technical.first_touch) * 0.50
        + normalize_attr(target_attrs.technical.technique) * 0.22
        + normalize_attr(target_attrs.physical.balance) * 0.14
        + normalize_attr(target_attrs.mental.composure) * 0.14;
    let target_ana_mod = (0.76 + 0.24 * (state.players[target_idx].fatigue.anaerobic / 100.0))
        .clamp(0.70, 1.0);

    let pass_difficulty = match kind {
        PassKindV4::Short => 0.04,
        PassKindV4::Through => 0.14,
        PassKindV4::Long => 0.16,
        PassKindV4::Switch => 0.18,
        PassKindV4::Cross => 0.20,
    };

    let miscontrol_prob = ((pass_difficulty * 0.65 + target_pressure_norm * 0.16 + wet_pen * 1.1 + pitch_pen * 1.0)
        * (1.20 - first_touch_skill * 0.86)
        / target_ana_mod)
        .clamp(0.006, 0.22);

    let arrived_pos = Vec3::new(target_pos.x, target_pos.y, ball_z * 0.15).clamp_to_pitch();
    state.ball.pos = arrived_pos;

    let pass_event_kind = match kind {
        PassKindV4::Through => EventTypeV4::ThroughBall,
        PassKindV4::Cross => EventTypeV4::Cross,
        _ => EventTypeV4::PassCompleted,
    };
    let pass_meta = EventMetadataV4 {
        pass_type: Some(kind.as_str().into()),
        distance_m: Some(distance_m),
        progress_gain: Some(progress_gain_m.round() as i32),
        pressure: Some(passer_ctx.pressure.round() as u8),
        overload: if target_ctx.overload { Some(true) } else { None },
        escaped_pressure: if passer_ctx.pressure >= 48.0 { Some(true) } else { None },
        outcome: Some("COMPLETED".into()),
        ..EventMetadataV4::default()
    };

    state.team_state_mut(att_side).passes_completed += 1;
    events.push(EmittedPlayEvent {
        kind: pass_event_kind,
        tier: HighlightTier::None,
        seq_id: None,
        side: Some(att_side),
        actor: Some(passer_id),
        target: Some(target_id.clone()),
        ball_pos: arrived_pos,
        metadata: pass_meta,
    });

    if (rng.next_double() as f32) < miscontrol_prob {
        let loose_pos = Vec3::ground(
            (target_pos.x + (rng.next_double() as f32 - 0.5) * 4.5).clamp(0.0, PITCH_LENGTH_M),
            (target_pos.y + (rng.next_double() as f32 - 0.5) * 4.5).clamp(0.0, 68.0),
        );
        state.ball.pos = loose_pos;
        state.ball.control = BallControlState::Loose;

        let ft_meta = EventMetadataV4 {
            pressure: Some(target_ctx.pressure.round() as u8),
            outcome: Some("HEAVY_TOUCH_LOST".into()),
            cause: Some("FIRST_TOUCH_ERROR".into()),
            ..EventMetadataV4::default()
        };
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::FirstTouch,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(att_side),
            actor: Some(target_id),
            target: None,
            ball_pos: loose_pos,
            metadata: ft_meta,
        });

        let winner_idx = target_ctx
            .nearest_defender_idx
            .unwrap_or_else(|| first_outfield(state, def_side));
        let winner_id = state.players[winner_idx].player_id.clone();
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(winner_id.clone());

        events.push(EmittedPlayEvent {
            kind: EventTypeV4::LooseBall,
            tier: HighlightTier::None,
            seq_id: None,
            side: None,
            actor: None,
            target: None,
            ball_pos: loose_pos,
            metadata: EventMetadataV4 {
                cause: Some("FIRST_TOUCH_ERROR".into()),
                ..EventMetadataV4::default()
            },
        });
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::SecondBallWon,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(def_side),
            actor: Some(winner_id),
            target: None,
            ball_pos: loose_pos,
            metadata: EventMetadataV4 {
                cause: Some("FIRST_TOUCH_ERROR".into()),
                ..EventMetadataV4::default()
            },
        });

        return BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: winner_idx,
            stoppage_seconds: 0,
            events,
        };
    }

    state.ball.control = BallControlState::Controlled;
    state.ball.owner_side = Some(att_side);
    state.ball.owner_player_id = Some(target_id.clone());
    if target_ctx.pressure >= 35.0 || kind != PassKindV4::Short {
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::FirstTouch,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(att_side),
            actor: Some(target_id),
            target: None,
            ball_pos: arrived_pos,
            metadata: EventMetadataV4 {
                pressure: Some(target_ctx.pressure.round() as u8),
                outcome: Some("CLEAN_CONTROL".into()),
                ..EventMetadataV4::default()
            },
        });
    }

    BallPlayOutcome {
        next_side: att_side,
        next_carrier_idx: target_idx,
        stoppage_seconds: 0,
        events,
    }
}

pub fn execute_dribble_duel(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    carrier_idx: usize,
    defender_idx: Option<usize>,
    carrier_ctx: LocalSpatialContext,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let att_side = state.players[carrier_idx].side;
    let def_side = att_side.opposite();
    let carrier_id = state.players[carrier_idx].player_id.clone();
    let carrier_attrs = *lookup_attrs(input, &state.players[carrier_idx]);

    let dir = match att_side {
        TeamSideV4::Home => 1.0f32,
        TeamSideV4::Away => -1.0f32,
    };

    let Some(def_idx) = defender_idx.filter(|&i| state.players[i].pos.distance(state.players[carrier_idx].pos) <= 6.5) else {
        let gain = 6.0 + normalize_attr(carrier_attrs.physical.pace) * 6.0;
        let new_xy = Vec2::new(
            state.players[carrier_idx].pos.x + dir * gain,
            state.players[carrier_idx].pos.y + (rng.next_double() as f32 - 0.5) * 4.0,
        )
        .clamp_to_pitch();
        state.players[carrier_idx].pos = new_xy;
        state.ball.pos = Vec3::ground(new_xy.x, new_xy.y);
        return BallPlayOutcome {
            next_side: att_side,
            next_carrier_idx: carrier_idx,
            stoppage_seconds: 0,
            events,
        };
    };

    let def_id = state.players[def_idx].player_id.clone();
    let def_attrs = *lookup_attrs(input, &state.players[def_idx]);
    let (wet_pen, pitch_pen) = environment_ball_penalties(input);

    let att_ana = (0.72 + 0.28 * (state.players[carrier_idx].fatigue.anaerobic / 100.0)).clamp(0.65, 1.0);
    let def_ana = (0.72 + 0.28 * (state.players[def_idx].fatigue.anaerobic / 100.0)).clamp(0.65, 1.0);

    let flair_val = normalize_attr(carrier_attrs.mental.flair);
    let effective_def_anticipation =
        (normalize_attr(def_attrs.mental.anticipation) * (1.0 - flair_val * 0.28)).clamp(0.05, 1.0);

    let att_score = (normalize_attr(carrier_attrs.technical.dribbling) * 0.30
        + normalize_attr(carrier_attrs.physical.agility) * 0.18
        + normalize_attr(carrier_attrs.physical.acceleration) * 0.16
        + normalize_attr(carrier_attrs.physical.balance) * 0.14
        + normalize_attr(carrier_attrs.physical.strength) * 0.10
        + flair_val * 0.12)
        * att_ana
        - wet_pen * 0.5
        - pitch_pen * 0.5;

    let def_score = (normalize_attr(def_attrs.technical.tackling) * 0.30
        + normalize_attr(def_attrs.technical.marking) * 0.20
        + effective_def_anticipation * 0.18
        + normalize_attr(def_attrs.physical.agility) * 0.12
        + normalize_attr(def_attrs.physical.strength) * 0.10
        + normalize_attr(def_attrs.mental.bravery) * 0.10)
        * def_ana;

    state.players[carrier_idx].fatigue.apply_interval(
        2.0,
        11.0,
        true,
        carrier_attrs.physical.stamina,
        carrier_attrs.physical.natural_fitness,
    );
    state.players[def_idx].fatigue.apply_interval(
        2.0,
        10.0,
        true,
        def_attrs.physical.stamina,
        def_attrs.physical.natural_fitness,
    );

    let win_prob = (0.50 + (att_score - def_score) * 0.65).clamp(0.18, 0.84);
    let roll = rng.next_double() as f32;

    if roll < win_prob {
        let gain = 6.5 + normalize_attr(carrier_attrs.physical.acceleration) * 5.5;
        let new_xy = Vec2::new(
            state.players[carrier_idx].pos.x + dir * gain,
            state.players[carrier_idx].pos.y + (rng.next_double() as f32 - 0.5) * 4.5,
        )
        .clamp_to_pitch();
        state.players[carrier_idx].pos = new_xy;
        state.ball.pos = Vec3::ground(new_xy.x, new_xy.y);

        events.push(EmittedPlayEvent {
            kind: EventTypeV4::DribbleWon,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(att_side),
            actor: Some(carrier_id),
            target: Some(def_id),
            ball_pos: state.ball.pos,
            metadata: EventMetadataV4 {
                escaped_pressure: if carrier_ctx.pressure >= 42.0 { Some(true) } else { None },
                progress_gain: Some(gain.round() as i32),
                pressure: Some(carrier_ctx.pressure.round() as u8),
                overload: if carrier_ctx.overload { Some(true) } else { None },
                outcome: Some("BEATEN_DEFENDER".into()),
                ..EventMetadataV4::default()
            },
        });
        return BallPlayOutcome {
            next_side: att_side,
            next_carrier_idx: carrier_idx,
            stoppage_seconds: 0,
            events,
        };
    }

    let ref_strict = input.referee_profile.strictness as f32 / 100.0;
    let foul_prob = (0.14
        + normalize_attr(def_attrs.mental.aggression) * 0.14
        + (1.0 - normalize_attr(def_attrs.hidden.temperament)) * 0.08
        + (1.0 - normalize_attr(def_attrs.hidden.sportsmanship)) * 0.06
        - normalize_attr(def_attrs.technical.tackling) * 0.10
        + (ref_strict - 0.5) * 0.08)
        .clamp(0.05, 0.42);

    let duel_pos = Vec3::ground(state.players[carrier_idx].pos.x, state.players[carrier_idx].pos.y);
    if (rng.next_double() as f32) < foul_prob {
        // Advantage Law: referee may let play continue if attacking team retains momentum
        let adv_tend = input.referee_profile.advantage_tendency as f32 / 100.0;
        if carrier_ctx.overload && (rng.next_double() as f32) < (0.12 + adv_tend * 0.28) {
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::AdvantagePlayed,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(att_side),
                actor: Some(carrier_id),
                target: Some(def_id),
                ball_pos: duel_pos,
                metadata: EventMetadataV4 {
                    outcome: Some("ADVANTAGE".into()),
                    ..EventMetadataV4::default()
                },
            });
            return BallPlayOutcome {
                next_side: att_side,
                next_carrier_idx: carrier_idx,
                stoppage_seconds: 0,
                events,
            };
        }

        state.team_state_mut(def_side).fouls += 1;
        let seq_id = state.next_highlight_sequence_id;
        state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::Foul,
            tier: HighlightTier::None,
            seq_id: Some(seq_id),
            side: Some(def_side),
            actor: Some(def_id),
            target: Some(carrier_id),
            ball_pos: duel_pos,
            metadata: EventMetadataV4 {
                cause: Some("TACKLE_FOUL".into()),
                ..EventMetadataV4::default()
            },
        });
        events.extend(resolve_foul_discipline(state, input, def_idx, seq_id, duel_pos, rng));
        if let Some(inj_ev) = check_impact_injury(state, input, carrier_idx, def_idx, rng) {
            events.push(inj_ev);
        }
        return BallPlayOutcome {
            next_side: att_side,
            next_carrier_idx: carrier_idx,
            stoppage_seconds: 15,
            events,
        };
    }

    state.ball.pos = duel_pos;
    state.ball.control = BallControlState::Controlled;
    state.ball.owner_side = Some(def_side);
    state.ball.owner_player_id = Some(def_id.clone());

    events.push(EmittedPlayEvent {
        kind: EventTypeV4::DribbleLost,
        tier: HighlightTier::None,
        seq_id: None,
        side: Some(att_side),
        actor: Some(carrier_id.clone()),
        target: Some(def_id.clone()),
        ball_pos: duel_pos,
        metadata: EventMetadataV4 {
            pressure: Some(carrier_ctx.pressure.round() as u8),
            outcome: Some("TACKLED".into()),
            ..EventMetadataV4::default()
        },
    });
    events.push(EmittedPlayEvent {
        kind: EventTypeV4::TackleWon,
        tier: HighlightTier::None,
        seq_id: None,
        side: Some(def_side),
        actor: Some(def_id),
        target: Some(carrier_id),
        ball_pos: duel_pos,
        metadata: EventMetadataV4::default(),
    });

    BallPlayOutcome {
        next_side: def_side,
        next_carrier_idx: def_idx,
        stoppage_seconds: 0,
        events,
    }
}

fn environment_ball_penalties(input: &MatchInputV4) -> (f32, f32) {
    let wet = match input.match_environment.weather {
        WeatherCondition::HeavyRain => 0.055,
        WeatherCondition::LightRain => 0.022,
        WeatherCondition::Windy => 0.025,
        _ => 0.0,
    };
    let pitch = match input.match_environment.pitch_quality {
        PitchQuality::Poor => 0.05,
        PitchQuality::Heavy => 0.025,
        PitchQuality::Good => 0.005,
        PitchQuality::Excellent => 0.0,
    };
    (wet, pitch)
}

fn weak_foot_pass_modifier(
    input: &MatchInputV4,
    side: TeamSideV4,
    player_id: &str,
    from: Vec2,
    to: Vec2,
) -> f32 {
    let team = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    let Some(snap) = team
        .lineup
        .iter()
        .chain(team.bench.iter())
        .find(|p| p.player_id == player_id)
    else {
        return 1.0;
    };
    if snap.preferred_foot == PreferredFoot::Either {
        return 1.0;
    }
    let dy = to.y - from.y;
    let using_weak = match (side, snap.preferred_foot) {
        (TeamSideV4::Home, PreferredFoot::Right) => dy < -14.0,
        (TeamSideV4::Home, PreferredFoot::Left) => dy > 14.0,
        (TeamSideV4::Away, PreferredFoot::Right) => dy > 14.0,
        (TeamSideV4::Away, PreferredFoot::Left) => dy < -14.0,
        _ => false,
    };
    if using_weak {
        0.80 + (snap.weak_foot.clamp(1, 5) as f32) * 0.04
    } else {
        1.0
    }
}

fn first_outfield(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && !p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .unwrap_or(0)
}
