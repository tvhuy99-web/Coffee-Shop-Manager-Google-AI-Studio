use serde::{Deserialize, Serialize};
use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite};
use crate::v4::ball_play::{build_defender_candidates, BallPlayOutcome, EmittedPlayEvent};
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::goalkeeper::{choose_gk_distribution, resolve_gk_shot_reaction, GkShotOutcome};
use crate::v4::input::{MatchInputV4, PreferredFoot, WeatherCondition};
use crate::v4::interaction::{
    find_earliest_ball_interaction, BallTrajectory, EarlyInteractionHit, InteractionKind,
    DEFAULT_GRAVITY_Z_MPS2,
};
use crate::v4::movement::{evaluate_local_context, player_attrs, player_max_speed_mps};
use crate::v4::own_goal::{maybe_own_goal, DeflectionCause};
use crate::v4::rules::{
    estimate_ball_flight_ms, BallFlightKind, DeadBallKind, TimingModelConfig,
};
use crate::v4::spatial::{
    attacking_goal_center, distance_to_attacking_goal, is_in_attacking_penalty_box,
    point_to_segment_distance_m, TeamSideV4, Vec2, Vec3, PITCH_HALF_WIDTH_M, PITCH_LENGTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4, PlayerRuntimeStateV4};

pub use match_contracts::rule_types::ShotKindV4;

/// DES shot resolution result carrying exact ball flight time (`flight_ms`),
/// dead-ball restart classification (`GoalCelebration`, `GoalKick`, `Corner`),
/// and any live-ball block interaction before the goal line.
#[derive(Debug, Clone)]
pub struct DesShotOutcome<C = Option<EarlyInteractionHit>> {
    pub trajectory: BallTrajectory,
    pub planned_flight_ms: i64,
    pub outcome: BallPlayOutcome,
    pub won_corner: bool,
    pub flight_ms: i64,
    pub dead_ball_kind: Option<DeadBallKind>,
    pub restart_spot: Option<Vec3>,
    pub early_interaction: C,
}

/// One discriminated result: a rejected opportunity never carries a path;
/// an experimental block cannot exist without its hit AND reserved motion.
/// Legacy results retain the original public Option<EarlyInteractionHit>.
#[derive(Debug, Clone)]
pub(crate) enum ShotContactResolution {
    None,
    LegacyHit(EarlyInteractionHit),
    CommittedBlock(match_physics::ShotBlockOpportunity),
    CommittedSave(match_physics::GoalkeeperContact),
}

impl<C> DesShotOutcome<C> {
    fn map_contact<T>(self, map:impl FnOnce(C)->T)->DesShotOutcome<T> {
        DesShotOutcome {
            trajectory:self.trajectory, planned_flight_ms:self.planned_flight_ms,
            outcome:self.outcome, won_corner:self.won_corner, flight_ms:self.flight_ms,
            dead_ball_kind:self.dead_ball_kind, restart_spot:self.restart_spot,
            early_interaction:map(self.early_interaction),
        }
    }
}

/// Both opportunities use the same launch snapshot. Resolve commitment in
/// chronological order; a rejected opportunity never reserves a path.
/// At the same integer timestamp, the stable player ID breaks the tie.
/// This deliberately does not retry later contacts by a rejected actor.
fn resolve_experimental_shot_race(
    block: Option<match_physics::ShotBlockOpportunity>,
    keeper: Option<match_physics::GoalkeeperContact>,
    rng: &mut DeterministicRng,
    mut commit_block: impl FnMut(&match_physics::ShotBlockOpportunity, &mut DeterministicRng) -> bool,
    mut commit_keeper: impl FnMut(&match_physics::GoalkeeperContact, &mut DeterministicRng) -> GkShotOutcome,
) -> (ShotContactResolution, Option<GkShotOutcome>) {
    let keeper_first = match (&block, &keeper) {
        (Some(b), Some(k)) => (k.time_ms, k.motion.candidate.player_id.as_str())
            < (b.hit.time_ms, b.motion.candidate.player_id.as_str()),
        (None, Some(_)) => true,
        _ => false,
    };
    let mut keeper = keeper;
    if keeper_first {
        if let Some(k) = keeper.take() {
            let outcome = commit_keeper(&k, rng);
            if outcome != GkShotOutcome::GoalConceded {
                return (ShotContactResolution::CommittedSave(k), Some(outcome));
            }
        }
    }
    if let Some(b) = block {
        if commit_block(&b, rng) {
            return (ShotContactResolution::CommittedBlock(b), None);
        }
    }
    if let Some(k) = keeper {
        let outcome = commit_keeper(&k, rng);
        if outcome != GkShotOutcome::GoalConceded {
            return (ShotContactResolution::CommittedSave(k), Some(outcome));
        }
    }
    (ShotContactResolution::None, None)
}

fn shot_block_commit_probability(state:&MatchStateV4,idx:usize)->f32 {
    let attrs=player_attrs(state,&state.players[idx]);
    let skill=normalize_attr(attrs.mental.bravery)*0.40
        +normalize_attr(attrs.mental.positioning)*0.35
        +normalize_attr(attrs.mental.anticipation)*0.25;
    (0.36+skill*0.44).clamp(0.25,0.82)
}

/// DES-native shot resolver:
/// - Computes kinematic `shot_flight_ms = estimate_ball_flight_ms(...)` and constructs a 3D
///   `BallTrajectory` (`is_shot: true`), setting `BallControlState::InFlight`.
/// - Checks live-ball defender blocks via `find_earliest_ball_interaction` BEFORE the ball reaches
///   the goal line.
/// - Uses O(1) `player_profiles` for attributes and `weak_foot_shot_modifier`.
/// - Eliminates instantaneous player teleportation on rebounds (moves chasing player only within
///   physically reachable kinematic distance).
/// - Leaves `stoppage_seconds: 0` on `Goal`, `GoalKick`, and `Corner` so `DeadBallState` in
///   `session.rs` owns the complete dead-ball lifecycle.
#[allow(clippy::too_many_arguments)]
pub fn resolve_shot_v4_des(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    timing: &TimingModelConfig,
    shooter_idx: usize,
    assist_idx: Option<usize>,
    forced_kind: Option<ShotKindV4>,
    is_rebound: bool,
    rng: &mut DeterministicRng,
) -> DesShotOutcome {
    resolve_shot_v4_des_with_motion(state,input,timing,shooter_idx,assist_idx,forced_kind,is_rebound,false,rng)
        .map_contact(|contact|match contact {
            ShotContactResolution::None=>None,
            ShotContactResolution::LegacyHit(hit)=>Some(hit),
            ShotContactResolution::CommittedBlock(_) | ShotContactResolution::CommittedSave(_)=>unreachable!("legacy shot selected experimental contact"),
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_shot_v4_des_with_motion(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    timing: &TimingModelConfig,
    shooter_idx: usize,
    assist_idx: Option<usize>,
    forced_kind: Option<ShotKindV4>,
    is_rebound: bool,
    experimental: bool,
    rng: &mut DeterministicRng,
) -> DesShotOutcome<ShotContactResolution> {
    resolve_shot_with_restart(state,input,timing,shooter_idx,assist_idx,forced_kind,is_rebound,experimental,None,rng)
}

/// A set-piece shot uses the same DES flight and contact witness contract.
/// The existing specialist skill formulas are supplied at the launch spot;
/// ordinary shots and legacy callers continue through the original wrapper.
pub(crate) fn resolve_shot_with_restart(
    state:&mut MatchStateV4,input:&MatchInputV4,timing:&TimingModelConfig,
    shooter_idx:usize,assist_idx:Option<usize>,forced_kind:Option<ShotKindV4>,
    is_rebound:bool,experimental:bool,restart:Option<(DeadBallKind,Vec2,bool)>,
    rng:&mut DeterministicRng,
)->DesShotOutcome<ShotContactResolution>{
    let penalty=restart.is_some_and(|(kind,_,_)|kind==DeadBallKind::PenaltyKick);
    let mut events = Vec::with_capacity(6);
    let att_side = state.players[shooter_idx].side;
    let def_side = att_side.opposite();
    let gk_idx = find_gk_idx(state, def_side);

    let shooter_pos = restart.map_or(state.players[shooter_idx].pos,|(_,spot,_)|spot);
    let shooter_id = state.players[shooter_idx].player_id.clone();
    let gk_id = state.players[gk_idx].player_id.clone();

    let shooter_attrs = *player_attrs(state, &state.players[shooter_idx]);
    let gk_attrs = *player_attrs(state, &state.players[gk_idx]);
    let shooter_fam = state.players[shooter_idx].position_familiarity;
    let gk_fam = state.players[gk_idx].position_familiarity;
    let gk_aero = state.players[gk_idx].fatigue.aerobic;

    let dist_m = distance_to_attacking_goal(att_side, shooter_pos);
    let in_box = is_in_attacking_penalty_box(att_side, shooter_pos);
    let ctx = evaluate_local_context(state, input, att_side, shooter_pos);
    let pressure_norm = (ctx.pressure / 100.0).clamp(0.0, 1.0);

    // Detect 1v1: inside 18m with low defender presence between shooter and goal
    let one_vs_one =
        restart.is_none() && in_box && dist_m <= 16.5 && ctx.nearest_defender_dist_m >= 3.8 && !is_rebound;

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

    // O(1) weak-foot penalty via player_profiles (does not apply to headers)
    let foot_mod = if is_header {
        1.0
    } else {
        weak_foot_shot_modifier(state, &state.players[shooter_idx], shooter_pos)
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
    let energy_mod =
        (0.80 + 0.20 * (state.players[shooter_idx].fatigue.anaerobic / 100.0)).clamp(0.74, 1.0);

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

    let fk_skill=normalize_attr_elite(shooter_attrs.technical.free_kicks)*0.58
        +normalize_attr(shooter_attrs.technical.technique)*0.24
        +normalize_attr(shooter_attrs.technical.long_shots)*0.18;
    let xg=if penalty{0.78}else if restart.is_some(){
        ((0.16-(dist_m-17.0).max(0.0)*0.009)*(0.65+fk_skill*0.70)).clamp(0.02,0.24)
    }else{xg};
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

    if let Some((restart_kind,_,_))=restart {
        base_meta.extra.insert("restartShot".into(),format!("{restart_kind:?}"));
        if !penalty {base_meta.free_kick_executed=Some(true);}
    }

    // Emit GoalkeeperRush when an aggressive sweeper-keeper narrows a 1v1
    if one_vs_one && gk_attrs.goalkeeping.rushing_out >= 13 {
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::GoalkeeperRush,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(def_side),
            actor: Some(gk_id.clone()),
            target: Some(shooter_id.clone()),
            ball_pos: Vec3::ground(state.players[gk_idx].pos.x, state.players[gk_idx].pos.y)
                .clamp_to_pitch(),
            metadata: EventMetadataV4 {
                one_vs_one: Some(true),
                in_box: Some(true),
                ..EventMetadataV4::default()
            },
        });
    }

    // Compute kinematic shot flight time to the goal line and set ball InFlight.
    let goal_center = attacking_goal_center(att_side);
    let shot_flight_ms = estimate_ball_flight_ms(
        timing,
        dist_m,
        BallFlightKind::from(kind),
        input.match_environment.weather,
        input.match_environment.pitch_quality,
        rng,
    );
    let t_now_ms = state.des_clock.sim_time_ms;
    let t_goal_ms = t_now_ms.saturating_add(shot_flight_ms);
    let flight_sec = match_contracts::units::ms_to_seconds_f32(shot_flight_ms).max(0.05);

    let shot_start_z = if is_header { 2.05 } else { 0.35 };
    let target_goal_z = if is_header { 1.35 } else { 1.10 };
    let specialist_on_target=restart.map(|_| {
        // The historical penalty model has goal/save only; keep that
        // limitation explicit until technical execution B1.5.
        penalty || (rng.next_double() as f32)<(0.28+fk_skill*0.36).clamp(0.20,0.68)
    });
    // For a specialist miss use its actual nominal miss direction; normal
    // shot geometry is unchanged in this locomotion integration step.
    let target_y=if specialist_on_target==Some(false){goal_center.y+5.0}else{goal_center.y};
    let drag = 0.06f32;
    let drag_integral = (1.0 - (-drag * flight_sec).exp()) / drag;
    let vx = (goal_center.x - shooter_pos.x) / drag_integral;
    let vy = (target_y - shooter_pos.y) / drag_integral;
    let vz = (target_goal_z - shot_start_z + 0.5 * DEFAULT_GRAVITY_Z_MPS2 * flight_sec * flight_sec)
        / drag_integral;

    let shot_traj = BallTrajectory {
        start_time_ms: t_now_ms,
        start_pos: Vec3::new(shooter_pos.x, shooter_pos.y, shot_start_z),
        initial_vel: Vec3::new(vx, vy, vz),
        gravity_z: DEFAULT_GRAVITY_Z_MPS2,
        drag,
        is_shot: true,
        is_carrier_dribble: false,
    };

    state.ball.pos = shot_traj.start_pos;
    state.ball.vel = shot_traj.initial_vel;
    state.ball.control = BallControlState::InFlight;
    state.ball.owner_side = None;
    state.ball.owner_player_id = None;
    state.ball.last_touch_side = Some(att_side);
    state.ball.last_touch_player_id = Some(shooter_id.clone());

    // The experiment still uses the nominal center-directed trajectory and
    // legacy on-target probability. Draw accuracy once before the contact
    // race so the keeper is eligible only for the nominal on-target shot.
    // Legacy draw order remains block, accuracy, keeper, unchanged.
    let on_target_prob =
        (0.34 + fin_skill * 0.34 * foot_mod - press_pen * 0.55 - wet_pen).clamp(0.18, 0.78);
    let experimental_on_target = specialist_on_target.or_else(||experimental.then(|| (rng.next_double() as f32) <= on_target_prob));
    let mut experimental_save = None;
    let mut experimental_save_outcome = None;

    // 1. Live-ball block check BEFORE reaching the goal line via continuous interaction detector.
    if experimental || (!one_vs_one && !is_header) {
        let blocked_candidate:Option<(usize,Vec3,i64,ShotContactResolution)>=if experimental {
            let defenders:Vec<_>=state.players.iter().enumerate()
                .filter(|(_,p)|!penalty && p.on_pitch && !p.sent_off && p.side==def_side && !p.is_goalkeeper)
                .map(|(idx,_)|crate::v4::ball_play::pass_reach_candidate_for_motion(state,idx,true,true))
                .collect();
            let block=match_physics::find_locomotion_shot_block(&shot_traj,&defenders,t_now_ms,t_goal_ms);
            let keeper=if experimental_on_target==Some(true) {
                crate::v4::goalkeeper::shot_contact_candidate(state,gk_idx)
                    .and_then(|p|match_physics::find_locomotion_goalkeeper_contact(&shot_traj,&p,t_now_ms,t_goal_ms))
            } else {None};
            let (winner, save_outcome)=resolve_experimental_shot_race(block,keeper,rng,
                |opportunity,rng| {
                    let idx=state.players.iter().position(|p|
                        p.player_id==opportunity.motion.candidate.player_id && p.side==def_side)
                        .expect("shot block candidate belongs to the authoritative roster");
                    (rng.next_double() as f32)<shot_block_commit_probability(state,idx)
                },
                |_,rng| {
                    if penalty {
                        let pen_skill=normalize_attr_elite(shooter_attrs.technical.penalties)*0.50
                            +normalize_attr(shooter_attrs.mental.composure)*0.30
                            +normalize_attr(shooter_attrs.hidden.pressure)*0.20;
                        let gk_skill=normalize_attr_elite(gk_attrs.goalkeeping.reflexes)*0.55
                            +normalize_attr(gk_attrs.goalkeeping.one_on_ones)*0.45;
                        // Provisional reuse of the old specialist skill roll
                        // conditional on an actually reachable hand contact.
                        let score_prob=(0.76+(pen_skill-gk_skill)*0.20).clamp(0.56,0.93);
                        if (rng.next_double() as f32)<score_prob {GkShotOutcome::GoalConceded}
                        else {
                            let wet_slip=match input.match_environment.weather {
                                WeatherCondition::HeavyRain=>0.14,WeatherCondition::LightRain=>0.06,_=>0.0,
                            };
                            let hold=(0.26+normalize_attr(gk_attrs.goalkeeping.handling)*0.44-wet_slip).clamp(0.12,0.78);
                            if restart.is_some_and(|(_,_,allow)|!allow)||(rng.next_double() as f32)<hold {
                                GkShotOutcome::SavedHeld
                            }else{GkShotOutcome::SavedParriedRebound}
                        }
                    }else{
                        resolve_gk_shot_reaction(&gk_attrs,gk_fam,gk_aero,xg,dist_m,
                            one_vs_one,is_finesse,is_header,input,rng)
                    }
                });
            match winner {
                ShotContactResolution::CommittedBlock(opportunity)=>{
                    let idx=state.players.iter().position(|p|
                        p.player_id==opportunity.motion.candidate.player_id && p.side==def_side)
                        .expect("shot block candidate belongs to the authoritative roster");
                    let elapsed=(opportunity.hit.time_ms-t_now_ms).max(1);
                    let pos=opportunity.hit.ball_pos.clamp_to_pitch();
                    Some((idx,pos,elapsed,ShotContactResolution::CommittedBlock(opportunity)))
                }
                ShotContactResolution::CommittedSave(contact)=>{
                    experimental_save=Some(contact);
                    experimental_save_outcome=save_outcome;
                    None
                }
                ShotContactResolution::None=>None,
                ShotContactResolution::LegacyHit(_)=>unreachable!("experimental shot race returned legacy contact"),
            }
        } else {
            let defenders=build_defender_candidates(state,def_side,false);
            let early_hit=find_earliest_ball_interaction(&shot_traj,&defenders,t_now_ms,t_goal_ms);
            if let Some(hit)=early_hit.filter(|h|h.kind==InteractionKind::ShotBlocked) {
                let blk_idx=hit.defender_id.as_deref()
                    .and_then(|id|state.players.iter().position(|p|p.player_id==id))
                    .unwrap_or_else(||first_outfield_idx(state,def_side));
                if (rng.next_double() as f32)<shot_block_commit_probability(state,blk_idx) {
                    let hit_elapsed=(hit.time_ms-t_now_ms).max(1);
                    let pos=hit.ball_pos.clamp_to_pitch();
                    Some((blk_idx,pos,hit_elapsed,ShotContactResolution::LegacyHit(hit)))
                } else {None}
            } else if let Some((blk_idx,blk_prob))=find_shot_blocker(state,att_side,shooter_pos,goal_center) {
                if (rng.next_double() as f32)<blk_prob {
                    let pos=Vec3::new(state.players[blk_idx].pos.x,state.players[blk_idx].pos.y,0.7).clamp_to_pitch();
                    let frac=(shooter_pos.distance(pos.xy())/dist_m.max(1.0)).clamp(0.10,0.90);
                    let hit_elapsed=((shot_flight_ms as f32)*frac).round().max(1.0) as i64;
                    Some((blk_idx,pos,hit_elapsed,ShotContactResolution::None))
                } else {None}
            } else {None}
        };

        if let Some((blk_idx, blk_pos, blk_elapsed_ms, contact)) = blocked_candidate {
            let blk_id = state.players[blk_idx].player_id.clone();
            state.team_state_mut(att_side).shots_blocked += 1;
            let def_attrs = *player_attrs(state, &state.players[blk_idx]);
            let own_goal = maybe_own_goal(
                state,
                blk_idx,
                att_side,
                DeflectionCause::BlockedShot,
                &def_attrs,
                seq_id,
                rng,
            );
            base_meta.scored = Some(own_goal.is_some());
            base_meta.outcome = Some(
                if own_goal.is_some() {
                    "DEFLECTED_OWN_GOAL"
                } else {
                    "BLOCKED"
                }
                .into(),
            );
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Shot,
                tier: if xg >= 0.15 {
                    HighlightTier::Extended
                } else {
                    HighlightTier::None
                },
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: Some(blk_id.clone()),
                ball_pos: blk_pos,
                metadata: base_meta,
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Block,
                tier: if xg >= 0.15 {
                    HighlightTier::Extended
                } else {
                    HighlightTier::None
                },
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(blk_id.clone()),
                target: None,
                ball_pos: blk_pos,
                metadata: EventMetadataV4::default(),
            });
            if let Some(goal) = own_goal {
                events.push(goal);
                state.ball.pos = Vec3::center_spot();
                state.ball.vel = Vec3::default();
                state.ball.control = BallControlState::DeadBall;
                return DesShotOutcome {
                    trajectory: shot_traj.clone(),
                    planned_flight_ms: shot_flight_ms,
                    outcome: BallPlayOutcome {
                        next_side: def_side,
                        next_carrier_idx: first_outfield_idx(state, def_side),
                        stoppage_seconds: 0,
                        events,
                    },
                    won_corner: false,
                    flight_ms: blk_elapsed_ms,
                    dead_ball_kind: Some(DeadBallKind::GoalCelebration),
                    restart_spot: Some(Vec3::center_spot()),
                    early_interaction: contact,
                };
            }
            state.ball.pos = blk_pos;
            state.ball.vel = Vec3::default();
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(blk_id.clone());
            state.ball.last_touch_side = Some(def_side);
            state.ball.last_touch_player_id = Some(blk_id);
            return DesShotOutcome {
                trajectory: shot_traj.clone(),
                planned_flight_ms: shot_flight_ms,
                outcome: BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: blk_idx,
                    stoppage_seconds: 0,
                    events,
                },
                won_corner: false,
                flight_ms: blk_elapsed_ms,
                dead_ball_kind: None,
                restart_spot: None,
                early_interaction: contact,
            };
        }
    }

    // 2. Check on-target vs off-target; the experimental draw is already fixed.
    let on_target=experimental_on_target.unwrap_or_else(|| (rng.next_double() as f32)<=on_target_prob);
    if !on_target {
        state.team_state_mut(att_side).shots_off_target += 1;
        base_meta.scored = Some(false);
        base_meta.outcome = Some("OFF_TARGET".into());
        let miss_pos = if restart.is_some(){shot_traj.position_at_ms(t_goal_ms).clamp_to_pitch()}else{
            Vec3::new(goal_center.x, (goal_center.y + 5.0).clamp(0.0, 68.0), 1.8)
        };
        events.push(EmittedPlayEvent {
            kind: if is_header {
                EventTypeV4::Header
            } else {
                EventTypeV4::ShotOffTarget
            },
            tier: if xg >= 0.18 {
                HighlightTier::Extended
            } else {
                HighlightTier::None
            },
            seq_id: Some(seq_id),
            side: Some(att_side),
            actor: Some(shooter_id),
            target: None,
            ball_pos: miss_pos,
            metadata: base_meta,
        });
        let gk_pos = Vec3::ground(state.players[gk_idx].pos.x, state.players[gk_idx].pos.y)
            .clamp_to_pitch();
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
        state.ball.vel = Vec3::default();
        state.ball.control = BallControlState::DeadBall;
        state.ball.owner_side = Some(def_side);
        state.ball.owner_player_id = Some(gk_id);
        return DesShotOutcome {
            trajectory: shot_traj.clone(),
            planned_flight_ms: shot_flight_ms,
            outcome: BallPlayOutcome {
                next_side: def_side,
                next_carrier_idx: gk_idx,
                stoppage_seconds: 0,
                events,
            },
            won_corner: false,
            flight_ms: shot_flight_ms,
            dead_ball_kind: Some(DeadBallKind::GoalKick),
            restart_spot: Some(gk_pos),
            early_interaction: ShotContactResolution::None,
        };
    }

    // 3. Shot is ON TARGET — resolve via 9-attribute Goalkeeper system
    state.team_state_mut(att_side).shots_on_target += 1;
    let gk_outcome = if experimental {
        experimental_save_outcome.unwrap_or(GkShotOutcome::GoalConceded)
    } else {
        resolve_gk_shot_reaction(
            &gk_attrs, gk_fam, gk_aero, xg, dist_m, one_vs_one,
            is_finesse, is_header, input, rng,
        )
    };
    // The chronological race owns the only experimental save witness.
    // No second skill roll or second reach search at resolution.
    let save_contact=experimental_save;
    let save_elapsed=save_contact.as_ref().map_or(shot_flight_ms,|c|c.time_ms-t_now_ms);
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
                    state.players[i].rating_tenths =
                        (state.players[i].rating_tenths + 4).min(100);
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
            state.ball.vel = Vec3::default();
            state.ball.control = BallControlState::DeadBall;
            let next_kick = first_outfield_idx(state, def_side);
            DesShotOutcome {
                trajectory: shot_traj.clone(),
                planned_flight_ms: shot_flight_ms,
                outcome: BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: next_kick,
                    stoppage_seconds: 0,
                    events,
                },
                won_corner: false,
                flight_ms: shot_flight_ms,
                dead_ball_kind: Some(DeadBallKind::GoalCelebration),
                restart_spot: Some(Vec3::center_spot()),
                early_interaction: ShotContactResolution::None,
            }
        }
        GkShotOutcome::SavedHeld => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("SAVED_HELD".into());
            let tier = if xg >= 0.14 {
                HighlightTier::Extended
            } else {
                HighlightTier::None
            };
            let save_pos = save_contact.as_ref().map_or_else(||Vec3::new(state.players[gk_idx].pos.x,state.players[gk_idx].pos.y,1.0),|c|c.ball_pos);
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

            // Catch and subsequent distribution are different actions.
            // The keeper must actually own the ball before PI chooses a
            // new pass. Legacy instantaneous distribution stays isolated.
            if let Some(contact)=save_contact {
                state.ball.pos=save_pos;
                state.ball.vel=Vec3::default();
                state.ball.control=BallControlState::Controlled;
                state.ball.owner_side=Some(def_side);
                state.ball.owner_player_id=Some(gk_id.clone());
                state.ball.last_touch_side=Some(def_side);
                state.ball.last_touch_player_id=Some(gk_id);
                return DesShotOutcome {
                    trajectory:shot_traj.clone(),planned_flight_ms:shot_flight_ms,
                    outcome:BallPlayOutcome{next_side:def_side,next_carrier_idx:gk_idx,stoppage_seconds:0,events},
                    won_corner:false,flight_ms:save_elapsed,dead_ball_kind:None,restart_spot:None,
                    early_interaction:ShotContactResolution::CommittedSave(contact),
                };
            }
            // GK distribution (`kicking` / `throwing`)
            let (recv_idx, is_long, dist_pos) =
                choose_gk_distribution(state, &gk_attrs, def_side, rng);
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
            state.ball.vel = Vec3::default();
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(recv_id.clone());
            state.ball.last_touch_side = Some(def_side);
            state.ball.last_touch_player_id = Some(recv_id);
            DesShotOutcome {
                trajectory: shot_traj.clone(),
                planned_flight_ms: shot_flight_ms,
                outcome: BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: recv_idx,
                    stoppage_seconds: 0,
                    events,
                },
                won_corner: false,
                flight_ms: shot_flight_ms,
                dead_ball_kind: None,
                restart_spot: None,
                early_interaction: ShotContactResolution::None,
            }
        }
        GkShotOutcome::SavedParriedRebound => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("PARRIED_REBOUND".into());
            let reb_x = match att_side {
                TeamSideV4::Home => (PITCH_LENGTH_M - 8.5).clamp(0.0, PITCH_LENGTH_M),
                TeamSideV4::Away => 8.5,
            };
            let legacy_reb_pos = Vec3::new(
                reb_x,
                (PITCH_HALF_WIDTH_M + (rng.next_double() as f32 - 0.5) * 10.0).clamp(16.0, 52.0),
                0.3,
            );
            let reb_pos=if restart.is_some(){save_contact.as_ref().map_or(legacy_reb_pos,|c|c.ball_pos)}
                else{legacy_reb_pos};
            let contact_pos=save_contact.as_ref().map_or(reb_pos,|c|c.ball_pos);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::ShotOnTarget,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(att_side),
                actor: Some(shooter_id),
                target: Some(gk_id.clone()),
                ball_pos: contact_pos,
                metadata: base_meta,
            });
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::Save,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(def_side),
                actor: Some(gk_id.clone()),
                target: None,
                ball_pos: contact_pos,
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

            // Experimental shots are single-flight transactions. A save
            // cannot recursively resolve a second shot from launch-state
            // coordinates or publish a second shot's contact as this one's.
            // Keep the legacy parry location as an explicit provisional
            // loose-ball spawn until goalkeeper/deflection physics lands.
            if let Some(contact)=save_contact {
                state.ball.pos=reb_pos;
                state.ball.vel=if restart.is_some(){
                    let keeper_pos=contact.motion.candidate.contact_position_toward(
                        contact.motion.target,contact.time_ms-t_now_ms)
                        .expect("committed keeper path is reachable");
                    match_physics::blocked_ball_velocity(shot_traj.velocity_at_ms(contact.time_ms),
                        keeper_pos,contact.ball_pos)
                }else{Vec3::default()};
                state.ball.control=BallControlState::Loose;
                state.ball.owner_side=None;
                state.ball.owner_player_id=None;
                state.ball.last_touch_side=Some(def_side);
                state.ball.last_touch_player_id=Some(gk_id);
                return DesShotOutcome {
                    trajectory:shot_traj.clone(),
                    planned_flight_ms:shot_flight_ms,
                    outcome:BallPlayOutcome {
                        next_side:att_side,
                        // Placeholder, never treated as a controlled owner.
                        next_carrier_idx:shooter_idx,
                        stoppage_seconds:0,
                        events,
                    },
                    won_corner:false,
                    flight_ms:save_elapsed,
                    dead_ball_kind:None,
                    restart_spot:None,
                    early_interaction:ShotContactResolution::CommittedSave(contact),
                };
            }

            // Legacy-only combined rebound path: preserve its RNG, counters
            // and timing exactly for production golden parity.
            let att_follow = nearest_box_player(state, att_side, reb_pos.xy());
            let def_follow = nearest_box_player(state, def_side, reb_pos.xy());

            let att_attrs = *player_attrs(state, &state.players[att_follow]);
            let def_attrs = *player_attrs(state, &state.players[def_follow]);
            let att_speed = player_max_speed_mps(&att_attrs, &state.players[att_follow]).max(2.0);
            let def_speed = player_max_speed_mps(&def_attrs, &state.players[def_follow]).max(2.0);
            let att_react = normalize_attr(att_attrs.mental.anticipation);
            let def_react = normalize_attr(def_attrs.mental.anticipation);

            let att_dist = state.players[att_follow].pos.distance(reb_pos.xy());
            let def_dist = state.players[def_follow].pos.distance(reb_pos.xy());
            let att_eta_sec = (0.30 - att_react * 0.14) + att_dist / att_speed;
            let def_eta_sec = (0.30 - def_react * 0.14) + def_dist / def_speed;

            // Physical reach window for an immediate rebound follow-up shot (~1.4s).
            const REBOUND_WINDOW_SEC: f32 = 1.40;
            let att_can_reach = att_eta_sec <= REBOUND_WINDOW_SEC;
            let eta_advantage = ((def_eta_sec - att_eta_sec) * 0.35).clamp(-0.25, 0.25);
            let att_win_prob =
                (0.42 + (att_react - def_react) * 0.22 + eta_advantage).clamp(0.18, 0.72);
            let att_wins_reb = att_can_reach && (rng.next_double() as f32) < att_win_prob;

            state.ball.pos = reb_pos;
            state.ball.vel = Vec3::default();
            state.ball.control = BallControlState::Loose;

            if att_wins_reb && !is_rebound {
                // Advance attacker smoothly toward `reb_pos` within physically reachable step (no teleport).
                let cur = state.players[att_follow].pos;
                let max_step = att_speed * att_eta_sec.clamp(0.15, REBOUND_WINDOW_SEC);
                let new_pos = if att_dist <= max_step || att_dist <= 0.05 {
                    reb_pos.xy()
                } else {
                    let t = max_step / att_dist;
                    Vec2::new(
                        cur.x + (reb_pos.x - cur.x) * t,
                        cur.y + (reb_pos.y - cur.y) * t,
                    )
                    .clamp_to_pitch()
                };
                let moved_m = cur.distance(new_pos);
                state.players[att_follow].pos = new_pos;
                state.players[att_follow].last_moved_m = moved_m;
                state.ball.pos = Vec3::new(new_pos.x, new_pos.y, 0.25);

                let rebound_chase_ms = match_contracts::units::seconds_to_ms_nearest_f32(att_eta_sec.clamp(0.15, REBOUND_WINDOW_SEC));
                let mut follow = resolve_shot_v4_des(
                    state,
                    input,
                    timing,
                    att_follow,
                    None,
                    Some(ShotKindV4::Rebound),
                    true,
                    rng,
                );
                events.append(&mut follow.outcome.events);
                follow.outcome.events = events;
                follow.flight_ms = shot_flight_ms
                    .saturating_add(rebound_chase_ms)
                    .saturating_add(follow.flight_ms);
                return follow.map_contact(|hit|hit.map_or(ShotContactResolution::None,ShotContactResolution::LegacyHit));
            }

            // Defender clears / collects the loose rebound after realistic movement.
            let def_cur = state.players[def_follow].pos;
            let def_step = def_speed * def_eta_sec.clamp(0.15, REBOUND_WINDOW_SEC);
            let def_new = if def_dist <= def_step || def_dist <= 0.05 {
                reb_pos.xy()
            } else {
                let t = def_step / def_dist;
                Vec2::new(
                    def_cur.x + (reb_pos.x - def_cur.x) * t,
                    def_cur.y + (reb_pos.y - def_cur.y) * t,
                )
                .clamp_to_pitch()
            };
            state.players[def_follow].last_moved_m = def_cur.distance(def_new);
            state.players[def_follow].pos = def_new;

            let def_id = state.players[def_follow].player_id.clone();
            state.ball.pos = Vec3::ground(def_new.x, def_new.y);
            state.ball.control = BallControlState::Controlled;
            state.ball.owner_side = Some(def_side);
            state.ball.owner_player_id = Some(def_id.clone());
            state.ball.last_touch_side = Some(def_side);
            state.ball.last_touch_player_id = Some(def_id);
            DesShotOutcome {
                trajectory: shot_traj.clone(),
                planned_flight_ms: shot_flight_ms,
                outcome: BallPlayOutcome {
                    next_side: def_side,
                    next_carrier_idx: def_follow,
                    stoppage_seconds: 0,
                    events,
                },
                won_corner: false,
                flight_ms: shot_flight_ms
                    .saturating_add(match_contracts::units::seconds_to_ms_nearest_f32(def_eta_sec.clamp(0.15, REBOUND_WINDOW_SEC))),
                dead_ball_kind: None,
                restart_spot: None,
                early_interaction: ShotContactResolution::None,
            }
        }
        GkShotOutcome::SavedTippedCorner => {
            state.team_state_mut(def_side).saves += 1;
            base_meta.scored = Some(false);
            base_meta.outcome = Some("TIPPED_CORNER".into());
            let save_pos=save_contact.as_ref().map_or(Vec3::new(goal_center.x,goal_center.y,2.0),|c|c.ball_pos);
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
            let corner_spot = Vec3::ground(
                goal_center.x,
                if shooter_pos.y < PITCH_HALF_WIDTH_M {
                    0.5
                } else {
                    67.5
                },
            );
            state.ball.pos = corner_spot;
            state.ball.vel = Vec3::default();
            state.ball.control = BallControlState::DeadBall;

            // Signal caller (`session.rs`) to open a Corner `DeadBallState` for `att_side`
            DesShotOutcome {
                trajectory: shot_traj.clone(),
                planned_flight_ms: shot_flight_ms,
                outcome: BallPlayOutcome {
                    next_side: att_side,
                    next_carrier_idx: shooter_idx,
                    stoppage_seconds: 0,
                    events,
                },
                won_corner: true,
                flight_ms: save_elapsed,
                dead_ball_kind: Some(DeadBallKind::Corner),
                restart_spot: Some(corner_spot),
                early_interaction: save_contact.map_or(ShotContactResolution::None,ShotContactResolution::CommittedSave),
            }
        }
    }
}

/// Shot resolver wrapper delegating to `resolve_shot_v4_des`.
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
    let default_timing = TimingModelConfig::default();
    let res = resolve_shot_v4_des(
        state,
        input,
        &default_timing,
        shooter_idx,
        assist_idx,
        forced_kind,
        is_rebound,
        rng,
    );
    (res.outcome, res.won_corner)
}

fn find_shot_blocker(
    state: &MatchStateV4,
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
            let attrs = player_attrs(state, p);
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

#[inline]
fn weak_foot_shot_modifier(
    state: &MatchStateV4,
    player: &PlayerRuntimeStateV4,
    pos: Vec2,
) -> f32 {
    let Some(profile) = state.player_profiles.get(player.profile_idx) else {
        return 1.0;
    };
    if profile.preferred_foot == PreferredFoot::Either {
        return 1.0;
    }
    let using_weak = match (player.side, profile.preferred_foot) {
        (TeamSideV4::Home, PreferredFoot::Right) => pos.y < 24.0,
        (TeamSideV4::Home, PreferredFoot::Left) => pos.y > 44.0,
        (TeamSideV4::Away, PreferredFoot::Right) => pos.y > 44.0,
        (TeamSideV4::Away, PreferredFoot::Left) => pos.y < 24.0,
        _ => false,
    };
    if using_weak {
        0.78 + (profile.weak_foot.clamp(1, 5) as f32) * 0.044
    } else {
        1.0
    }
}

fn find_gk_idx(state: &MatchStateV4, side: TeamSideV4) -> usize {
    state
        .players
        .iter()
        .position(|p| p.side == side && p.is_goalkeeper && p.on_pitch && !p.sent_off)
        .or_else(|| {
            state
                .players
                .iter()
                .position(|p| p.side == side && p.on_pitch && !p.sent_off)
        })
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


#[cfg(test)]
mod batch1_shot_race_tests {
    use super::*;
    use std::cell::RefCell;

    fn opportunity_pair(side: TeamSideV4, keeper_first: bool)
        -> (match_physics::ShotBlockOpportunity, match_physics::GoalkeeperContact) {
        let mirror=|x:f32| if side==TeamSideV4::Away {x} else {105.0-x};
        let sign=if side==TeamSideV4::Away {1.0}else{-1.0};
        let traj=BallTrajectory { start_time_ms:100,start_pos:Vec3::new(mirror(85.0),34.0,1.1),
            initial_vel:Vec3::new(sign*20.0,0.0,0.0),gravity_z:0.0,drag:0.0,
            is_shot:true,is_carrier_dribble:false };
        let candidate=|id:&str,x:f32,reach:f32,reaction_ms:i64| match_physics::PassReachCandidate {
            player_id:id.into(),side,pos:Vec2::new(mirror(x),34.0),velocity:Vec2::default(),
            speed_mps:0.0,acceleration_mps2:4.0,agility:0.5,reaction_ms,
            reach_m:reach,playable_height_m:2.6,
            locomotion:Some(match_physics::ReachLocomotion {
                capability:match_physics::movement::MovementCapability {
                    max_speed_mps:8.0,acceleration_mps2:4.0,braking_mps2:5.0,
                    lateral_mps2:3.5,body_turn_radps:4.0 },facing_rad:0.0 }),
        };
        let block=candidate("B",if keeper_first {102.0}else{93.0},1.0,150);
        let keeper=candidate("K",if keeper_first {94.0}else{102.0},1.5,150);
        let b=match_physics::find_locomotion_shot_block(&traj,&[block],100,1100).unwrap();
        let k=match_physics::find_locomotion_goalkeeper_contact(&traj,&keeper,100,1100).unwrap();
        assert_eq!(k.time_ms<b.hit.time_ms,keeper_first);
        (b,k)
    }

    #[test]
    fn b12_batch1_shot_race_respects_time_commitment_and_single_witness() {
        for side in [TeamSideV4::Home,TeamSideV4::Away] {
            for keeper_first in [false,true] {
                let (b,k)=opportunity_pair(side,keeper_first);
                for block_accepts in [false,true] {
                    for keeper_accepts in [false,true] {
                        let calls=RefCell::new(Vec::new());
                        let mut rng=DeterministicRng::new(13);
                        let (winner,outcome)=resolve_experimental_shot_race(Some(b.clone()),Some(k.clone()),&mut rng,
                            |_,_| {calls.borrow_mut().push("block");block_accepts},
                            |_,_| {calls.borrow_mut().push("keeper");if keeper_accepts {GkShotOutcome::SavedHeld}else{GkShotOutcome::GoalConceded}});
                        let first=if keeper_first {"keeper"}else{"block"};
                        let second=if keeper_first {"block"}else{"keeper"};
                        let first_accepts=if keeper_first {keeper_accepts}else{block_accepts};
                        assert_eq!(*calls.borrow(),if first_accepts {vec![first]}else{vec![first,second]});
                        match winner {
                            ShotContactResolution::CommittedBlock(hit)=>{
                                assert!(block_accepts && (!keeper_first || !keeper_accepts));
                                assert_eq!(hit.hit,b.hit);
                                assert_eq!(hit.motion,b.motion);
                                assert_eq!(outcome,None);
                            }
                            ShotContactResolution::CommittedSave(hit)=>{
                                assert!(keeper_accepts && (keeper_first || !block_accepts));
                                assert_eq!(hit.time_ms,k.time_ms);
                                assert_eq!(hit.motion,k.motion);
                                assert_eq!(outcome,Some(GkShotOutcome::SavedHeld));
                            }
                            ShotContactResolution::None=>{
                                assert!(!block_accepts && !keeper_accepts);
                                assert_eq!(outcome,None);
                            }
                            _=>panic!("legacy contact in experimental race"),
                        }
                    }
                }
            }
            // Tie fixture tests arbitration only; ordinary witnesses above
            // are produced by actual physics, not fabricated outcomes.
            let (b,mut k)=opportunity_pair(side,false);
            k.time_ms=b.hit.time_ms;
            for id in ["A","Z"] {
                k.motion.candidate.player_id=id.into();
                let (winner,_)=resolve_experimental_shot_race(Some(b.clone()),Some(k.clone()),
                    &mut DeterministicRng::new(19),|_,_|true,|_,_|GkShotOutcome::SavedHeld);
                assert_eq!(matches!(winner,ShotContactResolution::CommittedSave(_)),id=="A");
            }
        }
    }
}
