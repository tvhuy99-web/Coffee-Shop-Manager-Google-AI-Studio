use crate::rng::DeterministicRng;
use crate::v4::attributes::normalize_attr;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PitchQuality, PreferredFoot, WeatherCondition};
use crate::v4::interaction::{
    find_pass_contact, BallTrajectory, BoundaryKind, DefenderCandidate,
    EarlyInteractionHit, InteractionKind, PassContact, PassReachCandidate, DEFAULT_GRAVITY_Z_MPS2,
};
use crate::v4::movement::{
    evaluate_local_context, player_attrs, player_max_speed_mps, LocalSpatialContext,
};
use crate::v4::perception::PassKindV4;
use crate::v4::psychology::{check_impact_injury, resolve_foul_discipline};
use crate::v4::rules::{
    estimate_ball_flight_ms, BallFlightKind, DeadBallKind, TimingModelConfig,
};
use crate::v4::spatial::{
    attacking_progress_m, is_in_attacking_penalty_box, TeamSideV4, Vec2, Vec3,
    PITCH_HALF_LENGTH_M, PITCH_LENGTH_M, PITCH_WIDTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4, PlayerRuntimeStateV4};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BallPlayOutcome {
    pub next_side: TeamSideV4,
    pub next_carrier_idx: usize,
    pub stoppage_seconds: i32,
    pub events: Vec<EmittedPlayEvent>,
}

impl BallPlayOutcome {
    /// Infers the DES `DeadBallKind` implied by emitted events, if play stopped.
    pub fn inferred_dead_ball_kind(&self) -> Option<DeadBallKind> {
        for ev in self.events.iter().rev() {
            match ev.kind {
                EventTypeV4::Goal | EventTypeV4::OwnGoal => {
                    return Some(DeadBallKind::GoalCelebration)
                }
                EventTypeV4::Corner => return Some(DeadBallKind::Corner),
                EventTypeV4::GoalKick => return Some(DeadBallKind::GoalKick),
                EventTypeV4::ThrowIn => return Some(DeadBallKind::ThrowIn),
                EventTypeV4::Offside => return Some(DeadBallKind::Offside),
                EventTypeV4::PenaltyAwarded => return Some(DeadBallKind::PenaltyKick),
                EventTypeV4::Foul => return Some(DeadBallKind::DirectFreeKick),
                _ => {}
            }
        }
        None
    }
}

/// DES-aware wrapper around `BallPlayOutcome` carrying exact kinematic flight/duel elapsed time,
/// dead-ball restart classification (so `session.rs` opens `DeadBallState` instead of adding
/// hardcoded stoppage seconds), and any early continuous-interaction hit.
#[derive(Debug, Clone)]
pub struct DesBallPlayOutcome {
    pub trajectory: Option<BallTrajectory>,
    pub planned_flight_ms: i64,
    pub outcome: BallPlayOutcome,
    pub elapsed_ms: i64,
    pub dead_ball_kind: Option<DeadBallKind>,
    pub restart_spot: Option<Vec3>,
    pub early_interaction: Option<EarlyInteractionHit>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Builds kinematic `DefenderCandidate` snapshots for all active outfield defenders of `def_side`.
pub(crate) fn build_defender_candidates(
    state: &MatchStateV4,
    def_side: TeamSideV4,
    is_aerial: bool,
) -> Vec<DefenderCandidate> {
    let mut out = Vec::with_capacity(11);
    for p in &state.players {
        if !p.on_pitch || p.sent_off || p.side != def_side || p.is_goalkeeper {
            continue;
        }
        let attrs = player_attrs(state, p);
        let pace = normalize_attr(attrs.physical.pace);
        let accel = normalize_attr(attrs.physical.acceleration);
        let ant = normalize_attr(attrs.mental.anticipation);
        let pos_skill = normalize_attr(attrs.mental.positioning);
        let conc = normalize_attr(attrs.mental.concentration);
        let max_speed_mps = 4.2 + pace * 2.8 + accel * 1.1;
        let fatigue_factor =
            ((p.fatigue.aerobic * 0.6 + p.fatigue.anaerobic * 0.4) / 100.0).clamp(0.45, 1.0);
        let reach_m = if is_aerial {
            0.85 + pos_skill * 0.25 + normalize_attr(attrs.physical.jumping_reach) * 0.35
        } else {
            0.90 + pos_skill * 0.30 + ant * 0.20
        };
        let reaction_delay_ms = (260.0 - ant * 110.0 - conc * 40.0).round() as i64;
        out.push(DefenderCandidate {
            player_id: p.player_id.clone(),
            side: p.side,
            pos: p.pos,
            max_speed_mps,
            reach_m,
            reaction_delay_ms: reaction_delay_ms.clamp(95, 280),
            anticipation_norm: ant,
            fatigue_factor,
        });
    }
    out
}

/// Build the same pass-contact motion profile for an attacker or defender.
pub(crate) fn pass_reach_candidate(
    state: &MatchStateV4,
    idx: usize,
    is_aerial: bool,
) -> PassReachCandidate {
    let p = &state.players[idx];
    let attrs = player_attrs(state, p);
    let accel = normalize_attr(attrs.physical.acceleration);
    let agility = normalize_attr(attrs.physical.agility);
    let ant = normalize_attr(attrs.mental.anticipation);
    let concentration = normalize_attr(attrs.mental.concentration);
    let fatigue = ((p.fatigue.aerobic * 0.6 + p.fatigue.anaerobic * 0.4) / 100.0).clamp(0.45, 1.0);
    PassReachCandidate {
        player_id: p.player_id.clone(),
        side: p.side,
        pos: p.pos,
        velocity: p.vel,
        speed_mps: player_max_speed_mps(attrs, p),
        acceleration_mps2: (2.4 + 3.0 * accel) * fatigue,
        agility,
        reaction_ms: (310.0 - ant * 120.0 - concentration * 55.0).round() as i64,
        reach_m: 0.85 + agility * 0.25,
        playable_height_m: if is_aerial {
            1.65 + normalize_attr(attrs.physical.jumping_reach) * 0.75
        } else { 1.20 },
    }
}

/// Second-last active opponent, in the attacking side's coordinate system.
/// Shared by perception, off-ball movement and the authoritative Law 11 check.
/// Goalkeepers count as opponents; sent-off/off-pitch players do not.
pub(crate) fn second_last_opponent_progress_m(
    state: &MatchStateV4,
    att_side: TeamSideV4,
) -> Option<f32> {
    let mut deepest = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    let mut count = 0usize;
    for p in state.players.iter().filter(|p| p.side == att_side.opposite() && p.on_pitch && !p.sent_off) {
        let progress = attacking_progress_m(att_side, p.pos);
        count += 1;
        if progress >= deepest {
            second = deepest;
            deepest = progress;
        } else if progress > second {
            second = progress;
        }
    }
    if count >= 2 { Some(second) } else { None }
}

/// Furthest legal receiving position at the instant of a teammate's pass.
/// This is a geometric reference, not an offside offence.
pub(crate) fn offside_reference_progress_m(
    state: &MatchStateV4,
    att_side: TeamSideV4,
    ball_pos: Vec2,
) -> Option<f32> {
    second_last_opponent_progress_m(state, att_side)
        .map(|line| line.max(attacking_progress_m(att_side, ball_pos)).max(PITCH_HALF_LENGTH_M))
}

/// Evaluates Law 11 position at the instant the teammate plays the ball.
/// Actual involvement is adjudicated by the pass resolver, not by perception.
pub fn is_receiver_offside(
    state: &MatchStateV4,
    att_side: TeamSideV4,
    passer_pos: Vec2,
    receiver_pos: Vec2,
) -> bool {
    let recv_prog = attacking_progress_m(att_side, receiver_pos);
    if recv_prog <= PITCH_HALF_LENGTH_M
        || recv_prog <= attacking_progress_m(att_side, passer_pos) + 0.001
    {
        return false;
    }
    second_last_opponent_progress_m(state, att_side)
        .map_or(false, |line| recv_prog > line + 0.001)
}

/// DES two-phase pass & first-touch resolver:
/// 1. Computes analytic `BallTrajectory` and kinematic `flight_ms = estimate_ball_flight_ms(...)`.
/// 2. Transitions ball to `BallControlState::InFlight` and queries
///    the symmetric pass-contact detector to detect
///    defensive `PassInterception` or `BallOutOfBounds` BEFORE arrival at the receiver.
/// 3. Never credits a completed pass when an earlier `PassInterception` or boundary exit occurs.
/// 4. Models one-touch (`is_one_touch`) vs controlled multi-touch reception via `first_touch` & `composure`.
/// 5. Emits `stoppage_seconds: 0` and returns `dead_ball_kind` so `session.rs` opens `DeadBallState`.
#[allow(clippy::too_many_arguments)]
pub fn execute_pass_and_first_touch_des(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    timing: &TimingModelConfig,
    passer_idx: usize,
    target_idx: usize,
    kind: PassKindV4,
    distance_m: f32,
    progress_gain_m: f32,
    _lane_risk: f32,
    _interceptor_idx: Option<usize>,
    passer_ctx: LocalSpatialContext,
    is_one_touch: bool,
    rng: &mut DeterministicRng,
) -> DesBallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let att_side = state.players[passer_idx].side;
    let def_side = att_side.opposite();

    let passer_id = state.players[passer_idx].player_id.clone();
    let target_id = state.players[target_idx].player_id.clone();
    let passer_pos = state.players[passer_idx].pos;
    let target_pos = state.players[target_idx].pos;

    let passer_attrs = *player_attrs(state, &state.players[passer_idx]);
    let target_attrs = *player_attrs(state, &state.players[target_idx]);
    let passer_fam = state.players[passer_idx].position_familiarity;
    let passer_aero = state.players[passer_idx].fatigue.aerobic;
    let passer_ana = state.players[passer_idx].fatigue.anaerobic;
    let psych_comp = state.players[passer_idx].psychology.composure_delta;

    // Law 11: snapshot the receiver's POSITION at the exact instant the ball is played.
    let receiver_in_offside_position =
        is_receiver_offside(state, att_side, passer_pos, target_pos);

    let (wet_pen, pitch_pen) = environment_ball_penalties(input);
    let foot_mod =
        weak_foot_pass_modifier(state, &state.players[passer_idx], passer_pos, target_pos);

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
    let passer_ft = normalize_attr(passer_attrs.technical.first_touch);
    let one_touch_mod = if is_one_touch {
        // One-touch release requires clean first touch & composure to keep accuracy intact.
        (0.91 + passer_ft * 0.065 + composure * 0.045).clamp(0.88, 1.02)
    } else {
        (1.0 + composure * 0.025).clamp(1.0, 1.03)
    };

    let pressure_pen = pressure_norm * (0.16 - composure * 0.08);
    let dist_pen = ((distance_m - 14.0).max(0.0) * 0.0032).min(0.16);
    let fatigue_mod =
        (0.82 + 0.18 * ((passer_aero * 0.65 + passer_ana * 0.35) / 100.0)).clamp(0.75, 1.0);

    let execution_accuracy = ((0.745 + pass_skill * 0.20)
        * fatigue_mod
        * foot_mod
        * one_touch_mod
        * (0.90 + 0.10 * passer_fam)
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

    // Phase 1 (Kick & In-Flight): compute kinematic flight time and analytic 3D trajectory.
    let flight_ms = estimate_ball_flight_ms(
        timing,
        distance_m,
        BallFlightKind::from(kind),
        input.match_environment.weather,
        input.match_environment.pitch_quality,
        rng,
    );
    let t_now_ms = state.des_clock.sim_time_ms;
    let t_arrival_ms = t_now_ms.saturating_add(flight_ms);
    let flight_sec = (flight_ms as f32 / 1000.0).max(0.05);

    let is_misplaced = (rng.next_double() as f32) > execution_accuracy;
    let aim_pos = if is_misplaced {
        let near_left = target_pos.y <= 9.5;
        let near_right = target_pos.y >= (PITCH_WIDTH_M - 9.5);
        let err_mag = (2.8 + (1.0 - execution_accuracy) * 9.0).clamp(2.5, 9.5);
        let err_y = if near_left && (rng.next_double() as f32) < 0.55 {
            -(target_pos.y + 1.8)
        } else if near_right && (rng.next_double() as f32) < 0.55 {
            (PITCH_WIDTH_M - target_pos.y) + 1.8
        } else {
            (rng.next_double() as f32 - 0.5) * 2.0 * err_mag
        };
        let err_x = (rng.next_double() as f32 - 0.5) * err_mag;
        Vec2::new(target_pos.x + err_x, target_pos.y + err_y)
    } else {
        target_pos
    };

    let drag = if kind.is_aerial() { 0.08f32 } else { 0.16f32 };
    let drag_integral = if drag > 1e-5 {
        (1.0 - (-drag * flight_sec).exp()) / drag
    } else {
        flight_sec
    };
    let vx = (aim_pos.x - passer_pos.x) / drag_integral;
    let vy = (aim_pos.y - passer_pos.y) / drag_integral;
    let start_z = if kind.is_aerial() { 0.25 } else { 0.10 };
    let vz = if kind.is_aerial() {
        0.5 * DEFAULT_GRAVITY_Z_MPS2 * flight_sec
    } else {
        (ball_z / drag_integral).max(0.0)
    };

    let traj = BallTrajectory {
        start_time_ms: t_now_ms,
        start_pos: Vec3::new(passer_pos.x, passer_pos.y, start_z),
        initial_vel: Vec3::new(vx, vy, vz),
        gravity_z: DEFAULT_GRAVITY_Z_MPS2,
        drag,
        is_shot: false,
        is_carrier_dribble: false,
    };

    // Ball is now airborne / rolling in flight.
    state.ball.pos = traj.start_pos;
    state.ball.vel = traj.initial_vel;
    state.ball.control = BallControlState::InFlight;
    state.ball.owner_side = None;
    state.ball.owner_player_id = None;
    state.ball.last_touch_side = Some(att_side);
    state.ball.last_touch_player_id = Some(passer_id.clone());

    let defenders: Vec<_> = state.players.iter().enumerate()
        .filter(|(_, p)| p.on_pitch && !p.sent_off && p.side == def_side && !p.is_goalkeeper)
        .map(|(idx, _)| pass_reach_candidate(state, idx, kind.is_aerial()))
        .collect();
    let receiver = pass_reach_candidate(state, target_idx, kind.is_aerial());
    // Recovery of inaccurate passes by arbitrary teammates is a separate
    // stage. For an accurate pass, the intended receiver races defenders.
    let contact = find_pass_contact(&traj, &defenders,
        if is_misplaced { None } else { Some(&receiver) }, t_now_ms, t_arrival_ms);
    let (early_hit, receiver_contact) = match contact {
        Some(PassContact::Other(hit)) => (Some(hit), None),
        Some(PassContact::Receiver { time_ms, ball_pos }) => (None, Some((time_ms, ball_pos))),
        None => (None, None),
    };

    // Phase 2 (Continuous In-Flight Resolution): check earliest interception or boundary exit
    // strictly BEFORE the pass can ever be credited as completed.
    if let Some(hit) = early_hit {
        let elapsed_ms = (hit.time_ms - t_now_ms).max(1);
        match hit.kind {
            InteractionKind::PassInterception => {
                let int_idx = hit
                    .defender_id
                    .as_deref()
                    .and_then(|id| state.players.iter().position(|p| p.player_id == id))
                    .expect("pass contact identifies an active defender");
                let int_id = state.players[int_idx].player_id.clone();
                let int_pos = hit.ball_pos.clamp_to_pitch();

                state.ball.pos = int_pos;
                state.ball.vel = Vec3::default();
                state.ball.control = BallControlState::Controlled;
                state.ball.owner_side = Some(def_side);
                state.ball.owner_player_id = Some(int_id.clone());
                state.ball.last_touch_side = Some(def_side);
                state.ball.last_touch_player_id = Some(int_id.clone());

                let mut meta = EventMetadataV4 {
                    pass_type: Some(kind.as_str().into()),
                    distance_m: Some(distance_m),
                    pressure: Some(passer_ctx.pressure.round() as u8),
                    outcome: Some("INTERCEPTED".into()),
                    ..EventMetadataV4::default()
                };
                meta.extra.insert("passContactPhase".into(), if hit.time_ms < t_arrival_ms {
                    "MID_FLIGHT".into()
                } else { "ARRIVAL".into() });
                events.push(EmittedPlayEvent {
                    kind: EventTypeV4::PassFailed,
                    tier: HighlightTier::None,
                    seq_id: None,
                    side: Some(att_side),
                    actor: Some(passer_id),
                    target: Some(target_id),
                    ball_pos: int_pos,
                    metadata: meta.clone(),
                });
                events.push(EmittedPlayEvent {
                    kind: EventTypeV4::Interception,
                    tier: HighlightTier::None,
                    seq_id: None,
                    side: Some(def_side),
                    actor: Some(int_id),
                    target: None,
                    ball_pos: int_pos,
                    metadata: meta,
                });
                return DesBallPlayOutcome {
                    trajectory: Some(traj.clone()),
                    planned_flight_ms: flight_ms,
                    outcome: BallPlayOutcome {
                        next_side: def_side,
                        next_carrier_idx: int_idx,
                        stoppage_seconds: 0,
                        events,
                    },
                    elapsed_ms,
                    dead_ball_kind: None,
                    restart_spot: None,
                    early_interaction: Some(hit),
                };
            }
            InteractionKind::BallOutOfBounds | InteractionKind::GoalScored => {
                let spot = hit.ball_pos.clamp_to_pitch();
                let boundary = hit.boundary.unwrap_or(BoundaryKind::LeftTouchline);
                let (dead_kind, restart_event, carrier_idx) = match boundary {
                    BoundaryKind::LeftTouchline | BoundaryKind::RightTouchline => {
                        state.team_state_mut(def_side).throw_ins += 1;
                        let def_idx = evaluate_local_context(state, input, att_side, spot.xy())
                            .nearest_defender_idx
                            .unwrap_or_else(|| first_outfield(state, def_side));
                        (DeadBallKind::ThrowIn, EventTypeV4::ThrowIn, def_idx)
                    }
                    BoundaryKind::AwayGoalLine => {
                        if att_side == TeamSideV4::Home {
                            let gk = find_gk_or_outfield(state, def_side);
                            (DeadBallKind::GoalKick, EventTypeV4::GoalKick, gk)
                        } else {
                            state.team_state_mut(def_side).corners += 1;
                            (
                                DeadBallKind::Corner,
                                EventTypeV4::Corner,
                                first_outfield(state, def_side),
                            )
                        }
                    }
                    BoundaryKind::HomeGoalLine => {
                        if att_side == TeamSideV4::Away {
                            let gk = find_gk_or_outfield(state, def_side);
                            (DeadBallKind::GoalKick, EventTypeV4::GoalKick, gk)
                        } else {
                            state.team_state_mut(def_side).corners += 1;
                            (
                                DeadBallKind::Corner,
                                EventTypeV4::Corner,
                                first_outfield(state, def_side),
                            )
                        }
                    }
                };
                let restart_actor_id = state.players[carrier_idx].player_id.clone();

                state.ball.pos = spot;
                state.ball.vel = Vec3::default();
                state.ball.control = BallControlState::DeadBall;
                state.ball.owner_side = Some(def_side);
                state.ball.owner_player_id = Some(restart_actor_id.clone());

                events.push(EmittedPlayEvent {
                    kind: EventTypeV4::PassFailed,
                    tier: HighlightTier::None,
                    seq_id: None,
                    side: Some(att_side),
                    actor: Some(passer_id),
                    target: None,
                    ball_pos: spot,
                    metadata: EventMetadataV4 {
                        pass_type: Some(kind.as_str().into()),
                        distance_m: Some(distance_m),
                        outcome: Some("OUT_OF_PLAY".into()),
                        ..EventMetadataV4::default()
                    },
                });
                events.push(EmittedPlayEvent {
                    kind: restart_event,
                    tier: HighlightTier::None,
                    seq_id: None,
                    side: Some(def_side),
                    actor: Some(restart_actor_id),
                    target: None,
                    ball_pos: spot,
                    metadata: EventMetadataV4::default(),
                });

                return DesBallPlayOutcome {
                    trajectory: Some(traj.clone()),
                    planned_flight_ms: flight_ms,
                    outcome: BallPlayOutcome {
                        next_side: def_side,
                        next_carrier_idx: carrier_idx,
                        stoppage_seconds: 0,
                        events,
                    },
                    elapsed_ms,
                    dead_ball_kind: Some(dead_kind),
                    restart_spot: Some(spot),
                    early_interaction: Some(hit),
                };
            }
            _ => {}
        }
    }

    // Lane risk is a decision input, not a second chance to intercept a
    // trajectory that no defender can physically reach.

    // Legacy loose-ball recovery remains isolated until stage 4.
    // An accurate kick whose intended receiver cannot reach it is not completed.
    if is_misplaced || receiver_contact.is_none() {
        let def_idx = evaluate_local_context(state, input, att_side, target_pos)
            .nearest_defender_idx
            .unwrap_or_else(|| first_outfield(state, def_side));
        let def_id = state.players[def_idx].player_id.clone();
        let def_pos = state.players[def_idx].pos;
        let ball_pos = Vec3::ground(def_pos.x, def_pos.y).clamp_to_pitch();
        state.ball.pos = ball_pos;
        state.ball.vel = Vec3::default();
        state.ball.control = BallControlState::Controlled;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(def_id.clone());
        state.ball.last_touch_side = Some(def_side);
        state.ball.last_touch_player_id = Some(def_id.clone());

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
        return DesBallPlayOutcome {
            trajectory: Some(traj.clone()),
            planned_flight_ms: flight_ms,
            outcome: BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: def_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms: flight_ms,
            dead_ball_kind: None,
            restart_spot: None,
            early_interaction: None,
        };
    }

    let (receiver_time_ms, receiver_ball_pos) = receiver_contact.expect("reachable receiver");
    let contact_elapsed_ms = (receiver_time_ms - t_now_ms).max(1);
    let target_pos = receiver_ball_pos.xy();
    // Phase 3 (Arrival at Receiver): Law 11 Offside involvement check.
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
        state.ball.vel = Vec3::default();
        state.ball.control = BallControlState::DeadBall;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(state.players[restart_idx].player_id.clone());
        return DesBallPlayOutcome {
            trajectory: Some(traj.clone()),
            planned_flight_ms: flight_ms,
            outcome: BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: restart_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms: contact_elapsed_ms,
            dead_ball_kind: Some(DeadBallKind::Offside),
            restart_spot: Some(off_pos),
            early_interaction: None,
        };
    }

    // Reception & First Touch: distinguish one-touch vs multi-touch control via `first_touch` & `composure`.
    let target_ctx = evaluate_local_context(state, input, att_side, target_pos);
    let target_pressure_norm = (target_ctx.pressure / 100.0).clamp(0.0, 1.0);
    let target_ft = normalize_attr(target_attrs.technical.first_touch);
    let target_comp = normalize_attr(target_attrs.mental.composure);
    let first_touch_skill = target_ft * 0.50
        + normalize_attr(target_attrs.technical.technique) * 0.22
        + normalize_attr(target_attrs.physical.balance) * 0.14
        + target_comp * 0.14;
    let target_ana_mod = (0.76 + 0.24 * (state.players[target_idx].fatigue.anaerobic / 100.0))
        .clamp(0.70, 1.0);

    let pass_difficulty = match kind {
        PassKindV4::Short => 0.04,
        PassKindV4::Through => 0.14,
        PassKindV4::Long => 0.16,
        PassKindV4::Switch => 0.18,
        PassKindV4::Cross => 0.20,
    };

    // One-touch passes arrive with slightly higher tempo/spin, demanding higher `first_touch`;
    // multi-touch cushion control rewards `composure` to kill momentum cleanly.
    let touch_style_factor = if is_one_touch {
        (1.08 - target_ft * 0.12).clamp(0.95, 1.12)
    } else {
        (1.0 - target_comp * 0.08).clamp(0.90, 1.0)
    };

    let miscontrol_prob = ((pass_difficulty * 0.65
        + target_pressure_norm * 0.16
        + wet_pen * 1.1
        + pitch_pen * 1.0)
        * (1.20 - first_touch_skill * 0.86)
        * touch_style_factor
        / target_ana_mod)
        .clamp(0.006, 0.22);

    let arrived_pos = receiver_ball_pos.clamp_to_pitch();
    state.ball.pos = arrived_pos;
    state.ball.vel = Vec3::default();

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
        escaped_pressure: if passer_ctx.pressure >= 48.0 {
            Some(true)
        } else {
            None
        },
        outcome: Some(if is_one_touch {
            "COMPLETED_ONE_TOUCH".into()
        } else {
            "COMPLETED".into()
        }),
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
            (target_pos.y + (rng.next_double() as f32 - 0.5) * 4.5).clamp(0.0, PITCH_WIDTH_M),
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
        state.ball.last_touch_side = Some(def_side);
        state.ball.last_touch_player_id = Some(winner_id.clone());

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

        return DesBallPlayOutcome {
            trajectory: Some(traj.clone()),
            planned_flight_ms: flight_ms,
            outcome: BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: winner_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms: contact_elapsed_ms,
            dead_ball_kind: None,
            restart_spot: None,
            early_interaction: None,
        };
    }

    state.ball.control = BallControlState::Controlled;
    state.ball.owner_side = Some(att_side);
    state.ball.owner_player_id = Some(target_id.clone());
    state.ball.last_touch_side = Some(att_side);
    state.ball.last_touch_player_id = Some(target_id.clone());

    if target_ctx.pressure >= 35.0 || kind != PassKindV4::Short || is_one_touch {
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
                outcome: Some(if is_one_touch {
                    "ONE_TOUCH_CONTROL".into()
                } else {
                    "CLEAN_CONTROL".into()
                }),
                ..EventMetadataV4::default()
            },
        });
    }

    DesBallPlayOutcome {
        trajectory: Some(traj.clone()),
        planned_flight_ms: flight_ms,
        outcome: BallPlayOutcome {
            next_side: att_side,
            next_carrier_idx: target_idx,
            stoppage_seconds: 0,
            events,
        },
        elapsed_ms: contact_elapsed_ms,
        dead_ball_kind: None,
        restart_spot: None,
        early_interaction: None,
    }
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
    let default_timing = TimingModelConfig::default();
    execute_pass_and_first_touch_des(
        state,
        input,
        &default_timing,
        passer_idx,
        target_idx,
        kind,
        distance_m,
        progress_gain_m,
        lane_risk,
        interceptor_idx,
        passer_ctx,
        false,
        rng,
    )
    .outcome
}

/// DES dribble / carry duel resolver using actual `dt_sec` and real displacement `moved_m`
/// for `TwoPoolFatigueState::apply_interval(..., is_dead_ball = false, ...)`.
#[allow(clippy::too_many_arguments)]
pub fn execute_dribble_duel_des(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    carrier_idx: usize,
    defender_idx: Option<usize>,
    carrier_ctx: LocalSpatialContext,
    dt_sec: f32,
    rng: &mut DeterministicRng,
) -> DesBallPlayOutcome {
    let mut events = Vec::with_capacity(5);
    let dt = dt_sec.max(0.0);
    let elapsed_ms = (dt * 1000.0).round() as i64;
    let noise_scale = dt.min(1.0).sqrt();

    let att_side = state.players[carrier_idx].side;
    let def_side = att_side.opposite();
    let carrier_id = state.players[carrier_idx].player_id.clone();
    let carrier_attrs = *player_attrs(state, &state.players[carrier_idx]);

    let dir = match att_side {
        TeamSideV4::Home => 1.0f32,
        TeamSideV4::Away => -1.0f32,
    };

    let Some(def_idx) = defender_idx
        .filter(|&i| state.players[i].pos.distance(state.players[carrier_idx].pos) <= 6.5)
    else {
        let max_speed = player_max_speed_mps(&carrier_attrs, &state.players[carrier_idx]);
        // Dribbling with the ball is ~88% of top sprint speed.
        let carry_gain = (max_speed * 0.88 * dt).min(6.0 + normalize_attr(carrier_attrs.physical.pace) * 6.0);
        let old_xy = state.players[carrier_idx].pos;
        let new_xy = Vec2::new(
            old_xy.x + dir * carry_gain,
            old_xy.y + (rng.next_double() as f32 - 0.5) * 4.0 * noise_scale,
        )
        .clamp_to_pitch();
        let moved_m = old_xy.distance(new_xy);
        state.players[carrier_idx].pos = new_xy;
        state.players[carrier_idx].last_moved_m = moved_m;
        state.ball.pos = Vec3::ground(new_xy.x, new_xy.y);
        state.ball.vel = Vec3::default();

        let is_high_intensity = dt > 1e-4 && (moved_m / dt) >= 5.5;
        state.players[carrier_idx].fatigue.apply_interval(
            dt,
            moved_m,
            is_high_intensity,
            false,
            carrier_attrs.physical.stamina,
            carrier_attrs.physical.natural_fitness,
        );

        return DesBallPlayOutcome {
            trajectory: None,
            planned_flight_ms: 0,
            outcome: BallPlayOutcome {
                next_side: att_side,
                next_carrier_idx: carrier_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms,
            dead_ball_kind: None,
            restart_spot: None,
            early_interaction: None,
        };
    };

    let def_id = state.players[def_idx].player_id.clone();
    let def_attrs = *player_attrs(state, &state.players[def_idx]);
    let (wet_pen, pitch_pen) = environment_ball_penalties(input);

    let att_ana =
        (0.72 + 0.28 * (state.players[carrier_idx].fatigue.anaerobic / 100.0)).clamp(0.65, 1.0);
    let def_ana =
        (0.72 + 0.28 * (state.players[def_idx].fatigue.anaerobic / 100.0)).clamp(0.65, 1.0);

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

    let win_prob = (0.50 + (att_score - def_score) * 0.65).clamp(0.18, 0.84);
    let roll = rng.next_double() as f32;

    let carrier_old_xy = state.players[carrier_idx].pos;
    let def_old_xy = state.players[def_idx].pos;
    let carrier_max_speed = player_max_speed_mps(&carrier_attrs, &state.players[carrier_idx]);
    let def_max_speed = player_max_speed_mps(&def_attrs, &state.players[def_idx]);

    if roll < win_prob {
        let accel = normalize_attr(carrier_attrs.physical.acceleration);
        let max_burst = (carrier_max_speed * 0.94 * dt).min(6.5 + accel * 5.5);
        let new_xy = Vec2::new(
            carrier_old_xy.x + dir * max_burst,
            carrier_old_xy.y + (rng.next_double() as f32 - 0.5) * 4.5 * noise_scale,
        )
        .clamp_to_pitch();
        let carrier_moved_m = carrier_old_xy.distance(new_xy);
        let def_chase_m = def_old_xy
            .distance(carrier_old_xy)
            .min(def_max_speed * dt);

        state.players[carrier_idx].pos = new_xy;
        state.players[carrier_idx].last_moved_m = carrier_moved_m;
        state.players[def_idx].last_moved_m = def_chase_m;
        state.ball.pos = Vec3::ground(new_xy.x, new_xy.y);
        state.ball.vel = Vec3::default();

        state.players[carrier_idx].fatigue.apply_interval(
            dt,
            carrier_moved_m,
            true,
            false,
            carrier_attrs.physical.stamina,
            carrier_attrs.physical.natural_fitness,
        );
        state.players[def_idx].fatigue.apply_interval(
            dt,
            def_chase_m,
            true,
            false,
            def_attrs.physical.stamina,
            def_attrs.physical.natural_fitness,
        );

        events.push(EmittedPlayEvent {
            kind: EventTypeV4::DribbleWon,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(att_side),
            actor: Some(carrier_id),
            target: Some(def_id),
            ball_pos: state.ball.pos,
            metadata: EventMetadataV4 {
                escaped_pressure: if carrier_ctx.pressure >= 42.0 {
                    Some(true)
                } else {
                    None
                },
                progress_gain: Some(carrier_moved_m.round() as i32),
                pressure: Some(carrier_ctx.pressure.round() as u8),
                overload: if carrier_ctx.overload {
                    Some(true)
                } else {
                    None
                },
                outcome: Some("BEATEN_DEFENDER".into()),
                ..EventMetadataV4::default()
            },
        });
        return DesBallPlayOutcome {
            trajectory: None,
            planned_flight_ms: 0,
            outcome: BallPlayOutcome {
                next_side: att_side,
                next_carrier_idx: carrier_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms,
            dead_ball_kind: None,
            restart_spot: None,
            early_interaction: None,
        };
    }

    // Defender closed down the carrier: apply real closing displacement to two-pool fatigue.
    let def_close_m = def_old_xy
        .distance(carrier_old_xy)
        .min(def_max_speed * dt);
    let carrier_evade_m = (carrier_max_speed * 0.45 * dt).min(3.5);
    state.players[carrier_idx].last_moved_m = carrier_evade_m;
    state.players[def_idx].last_moved_m = def_close_m;

    state.players[carrier_idx].fatigue.apply_interval(
        dt,
        carrier_evade_m,
        true,
        false,
        carrier_attrs.physical.stamina,
        carrier_attrs.physical.natural_fitness,
    );
    state.players[def_idx].fatigue.apply_interval(
        dt,
        def_close_m,
        true,
        false,
        def_attrs.physical.stamina,
        def_attrs.physical.natural_fitness,
    );

    let ref_strict = input.referee_profile.strictness as f32 / 100.0;
    let foul_prob = (0.14
        + normalize_attr(def_attrs.mental.aggression) * 0.14
        + (1.0 - normalize_attr(def_attrs.hidden.temperament)) * 0.08
        + (1.0 - normalize_attr(def_attrs.hidden.sportsmanship)) * 0.06
        - normalize_attr(def_attrs.technical.tackling) * 0.10
        + (ref_strict - 0.5) * 0.08)
        .clamp(0.05, 0.42);

    let duel_pos = Vec3::ground(carrier_old_xy.x, carrier_old_xy.y).clamp_to_pitch();
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
            return DesBallPlayOutcome {
                trajectory: None,
                planned_flight_ms: 0,
                outcome: BallPlayOutcome {
                    next_side: att_side,
                    next_carrier_idx: carrier_idx,
                    stoppage_seconds: 0,
                    events,
                },
                elapsed_ms,
                dead_ball_kind: None,
                restart_spot: None,
                early_interaction: None,
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
        events.extend(resolve_foul_discipline(
            state, input, def_idx, seq_id, duel_pos, rng,
        ));
        if let Some(inj_ev) = check_impact_injury(state, input, carrier_idx, def_idx, rng) {
            events.push(inj_ev);
        }
        state.ball.pos = duel_pos;
        state.ball.vel = Vec3::default();
        state.ball.control = BallControlState::DeadBall;

        let dead_kind = if is_in_attacking_penalty_box(att_side, carrier_old_xy) {
            DeadBallKind::PenaltyKick
        } else {
            DeadBallKind::DirectFreeKick
        };

        return DesBallPlayOutcome {
            trajectory: None,
            planned_flight_ms: 0,
            outcome: BallPlayOutcome {
                next_side: att_side,
                next_carrier_idx: carrier_idx,
                stoppage_seconds: 0,
                events,
            },
            elapsed_ms,
            dead_ball_kind: Some(dead_kind),
            restart_spot: Some(duel_pos),
            early_interaction: None,
        };
    }

    state.ball.pos = duel_pos;
    state.ball.vel = Vec3::default();
    state.ball.control = BallControlState::Controlled;
    state.ball.owner_side = Some(def_side);
    state.ball.owner_player_id = Some(def_id.clone());
    state.ball.last_touch_side = Some(def_side);
    state.ball.last_touch_player_id = Some(def_id.clone());

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

    DesBallPlayOutcome {
        trajectory: None,
        planned_flight_ms: 0,
        outcome: BallPlayOutcome {
            next_side: def_side,
            next_carrier_idx: def_idx,
            stoppage_seconds: 0,
            events,
        },
        elapsed_ms,
        dead_ball_kind: None,
        restart_spot: None,
        early_interaction: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_dribble_duel(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    carrier_idx: usize,
    defender_idx: Option<usize>,
    carrier_ctx: LocalSpatialContext,
    dt_sec: f32,
    rng: &mut DeterministicRng,
) -> BallPlayOutcome {
    execute_dribble_duel_des(
        state,
        input,
        carrier_idx,
        defender_idx,
        carrier_ctx,
        dt_sec,
        rng,
    )
    .outcome
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

#[inline]
fn weak_foot_pass_modifier(
    state: &MatchStateV4,
    player: &PlayerRuntimeStateV4,
    from: Vec2,
    to: Vec2,
) -> f32 {
    let Some(profile) = state.player_profiles.get(player.profile_idx) else {
        return 1.0;
    };
    if profile.preferred_foot == PreferredFoot::Either {
        return 1.0;
    }
    let dy = to.y - from.y;
    let using_weak = match (player.side, profile.preferred_foot) {
        (TeamSideV4::Home, PreferredFoot::Right) => dy < -14.0,
        (TeamSideV4::Home, PreferredFoot::Left) => dy > 14.0,
        (TeamSideV4::Away, PreferredFoot::Right) => dy > 14.0,
        (TeamSideV4::Away, PreferredFoot::Left) => dy < -14.0,
        _ => false,
    };
    if using_weak {
        0.80 + (profile.weak_foot.clamp(1, 5) as f32) * 0.04
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

fn find_gk_or_outfield(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .unwrap_or_else(|| first_outfield(state, side))
}
