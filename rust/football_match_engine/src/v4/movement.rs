use crate::rng::DeterministicRng;
use crate::player_intelligence::{select_movement_target, MovementObservation, ObservedPlayer, SupportObservation, select_coordinated_support_target, assign_defense, DefenderObservation, DefensiveIntent};
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::ball_play::offside_reference_progress_m;
use crate::v4::input::{MatchInputV4, TeamInstructionsV4};
use crate::v4::spatial::{
    attacking_progress_m, point_to_segment_distance_m, TeamSideV4, Vec2, PITCH_LENGTH_M,
    PITCH_WIDTH_M,
};
use crate::v4::state::{BallControlState, MatchStateV4, PlayerRuntimeStateV4};
use crate::v4::tactics::effective_press_multiplier;

pub use crate::contracts::ports::LocalSpatialContext;

/// O(1) lookup of a player's 54 attributes via `MatchStateV4::player_profiles[player.profile_idx]`.
#[inline]
pub fn player_attrs<'a>(
    state: &'a MatchStateV4,
    player: &PlayerRuntimeStateV4,
) -> &'a PlayerAttributes54 {
    state
        .player_profiles
        .get(player.profile_idx)
        .or_else(|| state.player_profiles.first())
        .map(|profile| &profile.attrs)
        .expect("MatchStateV4.player_profiles must be populated")
}

/// Computes a player's current effective max running speed (m/s) given attributes and two-pool fatigue.
#[inline]
pub fn player_max_speed_mps(attrs: &PlayerAttributes54, player: &PlayerRuntimeStateV4) -> f32 {
    let pace = normalize_attr(attrs.physical.pace);
    let accel = normalize_attr(attrs.physical.acceleration);
    let energy = ((player.fatigue.aerobic * 0.6 + player.fatigue.anaerobic * 0.4) / 100.0)
        .clamp(0.45, 1.0);
    (4.2 + pace * 2.8 + accel * 1.1) * energy
}

pub fn evaluate_local_context(
    state: &MatchStateV4,
    _input: &MatchInputV4,
    att_side: TeamSideV4,
    pos: Vec2,
) -> LocalSpatialContext {
    let def_side = att_side.opposite();
    let def_ti = state.active_instructions(def_side);
    let def_phase = state.team_state(def_side).phase;
    let press_scale = effective_press_multiplier(def_phase, def_ti);

    let mut pressure = 0.0f32;
    let mut nearest_idx: Option<usize> = None;
    let mut nearest_dist = f32::MAX;
    let mut att_near = 0u8;
    let mut def_near = 0u8;

    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off {
            continue;
        }
        let d = p.pos.distance(pos);
        if p.side == att_side {
            if d <= 16.0 {
                att_near = att_near.saturating_add(1);
            }
        } else {
            if d <= 16.0 {
                def_near = def_near.saturating_add(1);
            }
            if !p.is_goalkeeper && d < nearest_dist {
                nearest_dist = d;
                nearest_idx = Some(idx);
            }
            if d <= 12.0 {
                let attrs = player_attrs(state, p);
                let energy_mod = (0.65 + 0.35 * (p.fatigue.anaerobic / 100.0)).clamp(0.55, 1.0);
                let skill_mod = 0.78
                    + normalize_attr(attrs.mental.anticipation) * 0.14
                    + normalize_attr(attrs.mental.work_rate) * 0.14
                    + normalize_attr(attrs.physical.acceleration) * 0.10;
                let proximity = ((12.0 - d) / 12.0).clamp(0.0, 1.0);
                pressure += (proximity * proximity * 62.0 + proximity * 18.0)
                    * press_scale
                    * skill_mod
                    * energy_mod;
            }
        }
    }

    LocalSpatialContext {
        pressure: pressure.clamp(0.0, 100.0),
        nearest_defender_idx: nearest_idx,
        nearest_defender_dist_m: if nearest_idx.is_some() {
            nearest_dist
        } else {
            30.0
        },
        overload: att_near >= 3 && att_near > def_near,
    }
}

pub fn evaluate_passing_lane(
    state: &MatchStateV4,
    _input: &MatchInputV4,
    att_side: TeamSideV4,
    from: Vec2,
    to: Vec2,
    is_aerial: bool,
) -> (f32, Option<usize>) {
    let def_side = att_side.opposite();
    let pass_len = from.distance(to).max(1.0);
    let mut max_risk = 0.0f32;
    let mut total_risk = 0.0f32;
    let mut best_interceptor: Option<usize> = None;

    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != def_side || p.is_goalkeeper {
            continue;
        }
        let seg_dist = point_to_segment_distance_m(p.pos, from, to);
        if seg_dist > 3.8 {
            continue;
        }
        let d_from = p.pos.distance(from);
        let d_to = p.pos.distance(to);
        if d_from > pass_len + 4.0 && d_to > 6.0 {
            continue;
        }

        let attrs = player_attrs(state, p);
        let read_skill = normalize_attr(attrs.mental.anticipation) * 0.42
            + normalize_attr(attrs.mental.positioning) * 0.28
            + normalize_attr(attrs.mental.concentration) * 0.15
            + normalize_attr(attrs.physical.acceleration) * 0.15;
        let reach_m = 1.6 + read_skill * 1.6 + (pass_len * 0.025).min(0.8);
        let proximity = ((reach_m - seg_dist) / reach_m).clamp(0.0, 1.0);
        if proximity <= 0.0 {
            continue;
        }

        let aerial_mod = if is_aerial {
            0.55 + normalize_attr(attrs.physical.jumping_reach) * 0.30
        } else {
            1.0
        };
        let fam_mod = p.position_familiarity;
        let fatigue_mod = (0.72 + 0.28 * (p.fatigue.aerobic / 100.0)).clamp(0.65, 1.0);
        let risk = (proximity * (0.35 + read_skill * 0.55) * aerial_mod * fam_mod * fatigue_mod)
            .clamp(0.0, 0.92);

        total_risk += risk * 0.65;
        if risk > max_risk {
            max_risk = risk;
            best_interceptor = Some(idx);
        }
    }

    let combined = max_risk.max(total_risk.min(0.95));
    (combined, best_interceptor)
}

pub fn advance_off_ball_movement(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    dt_sec: f32,
    rng: &mut DeterministicRng,
) {
    let _=advance_off_ball_movement_with_report(state,input,possession_side,carrier_idx,dt_sec,rng);
}

pub(crate) fn advance_off_ball_movement_with_report(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    dt_sec: f32,
    rng: &mut DeterministicRng,
)->Vec<DefensiveIntent> {
    let dt = dt_sec.max(0.0);
    let (targets, defense) = select_movement_targets(state, input, possession_side, carrier_idx, dt_sec, rng);

    // Integrate once using the final destination to enforce the speed budget
    // and avoid overshooting.
    for (idx,target) in targets.into_iter().enumerate() {
        let Some(target)=target else {
            state.players[idx].last_moved_m=0.0;
            state.players[idx].vel=Vec2::default();
            continue;
        };
        let cur=state.players[idx].pos;
        let dist=cur.distance(target);
        let attrs=player_attrs(state,&state.players[idx]);
        let max_speed_mps=player_max_speed_mps(attrs,&state.players[idx]);
        let max_step = max_speed_mps * dt;
        let new_pos = if dt <= 1e-4 {
            cur
        } else if dist <= max_step {
            target
        } else {
            let t = max_step / dist;
            Vec2::new(
                cur.x + (target.x - cur.x) * t,
                cur.y + (target.y - cur.y) * t,
            )
            .clamp_to_pitch()
        };

        let moved_m = cur.distance(new_pos);
        state.players[idx].last_moved_m = moved_m;
        state.players[idx].target_pos = target;
        state.players[idx].pos = new_pos;
        state.players[idx].vel = if dt > 1e-4 {
            Vec2::new((new_pos.x - cur.x) / dt, (new_pos.y - cur.y) / dt)
        } else {
            Vec2::default()
        };
        if moved_m > 0.05 {
            state.players[idx].facing_rad = (new_pos.y - cur.y).atan2(new_pos.x - cur.x);
        }
    }
    defense
}

/// Experimental runtime integration for actors without a reserved contact path.
/// Called by the DES test experiment, never by the shipping session.
pub(crate) fn advance_experimental_tactical_motion(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    duration_ms: i64,
    rng: &mut DeterministicRng,
    reserved: &[bool],
    budgets: &mut [f32],
) -> Result<Vec<DefensiveIntent>, V4Error> {
    let dt = match_contracts::units::ms_to_seconds_f32(duration_ms);
    let (targets, defense) = select_movement_targets(state, input, possession_side, carrier_idx, dt, rng);
    for (idx, target) in targets.into_iter().enumerate() {
        if reserved[idx] { continue; }
        if let Some(target) = target {
            let attrs = *player_attrs(state, &state.players[idx]);
            budgets[idx] = integrate_experimental_actor(&mut state.players[idx], &attrs, target,
                None, duration_ms)?;
        } else {
            state.players[idx].last_moved_m = 0.0;
            state.players[idx].vel = Vec2::default();
        }
    }
    Ok(defense)
}

/// Does not charge fatigue; the centralized DES interval accounting owns it.
/// Overspeed is braked, not clipped; target and boundary behavior are measured.
pub(crate) fn integrate_experimental_actor(
    player: &mut PlayerRuntimeStateV4,
    attrs: &PlayerAttributes54,
    target: Vec2,
    requested_speed: Option<f32>,
    duration_ms: i64,
) -> Result<f32, V4Error> {
    let cap = MovementCapability::from_attributes(&attrs.physical, player.fatigue.aerobic,
        player.fatigue.anaerobic).map_err(candidate_motion_error)?;
    let intent = MovementIntent { target, intensity: requested_speed.map_or(1.0, |v|
        (v / cap.max_speed_mps).clamp(0.0, 1.0)), desired_facing_rad: None, reaction_ms: 0 };
    let mut current = MotionState { position: player.pos, velocity: player.vel, facing_rad: player.facing_rad };
    let mut remaining = duration_ms;
    let mut distance = 0.0_f32;
    let mut budget = 0.0_f32;
    while remaining > 0 {
        // Explicit technical plan horizon; no extra RNG or condition update.
        let span = remaining.min(match_physics::movement::MAX_MOTION_HORIZON_MS);
        let plan = MotionPlan::new(current, intent, cap).map_err(candidate_motion_error)?;
        let result = plan.sample_at(span).map_err(candidate_motion_error)?;
        let bound = plan.maximum_path_speed_mps() * match_contracts::units::ms_to_seconds_f32(span);
        let eps = 32.0 * f32::EPSILON * PITCH_LENGTH_M.max(PITCH_WIDTH_M) + 4.0 * f32::EPSILON * bound;
        if result.distance_m > bound + eps || current.position.distance(result.state.position) > result.distance_m + eps {
            return Err(candidate_motion_error("experimental motion budget"));
        }
        distance += result.distance_m;
        budget += bound;
        current = result.state;
        remaining -= span;
    }
    player.pos = current.position;
    player.vel = current.velocity;
    player.facing_rad = current.facing_rad;
    player.target_pos = target;
    player.last_moved_m = distance;
    Ok(budget)
}

/// Commit the exact solver path selected by the authoritative contact race.
/// No replan, clipping, velocity snap, or extra condition charge.
pub(crate) fn commit_experimental_contact(
    player: &mut PlayerRuntimeStateV4,
    path: &match_physics::PassContactMotion,
    before_ms: i64,
    now_ms: i64,
) -> Result<f32, V4Error> {
    if player.player_id != path.candidate.player_id || player.side != path.candidate.side
        || before_ms < path.started_at_ms || now_ms < before_ms
    {
        return Err(candidate_motion_error("invalid committed contact actor or time"));
    }
    let plan = path.experimental_plan().map_err(candidate_motion_error)?
        .ok_or_else(|| candidate_motion_error("missing contact locomotion"))?;
    let start = plan.sample_at(before_ms-path.started_at_ms).map_err(candidate_motion_error)?;
    let end = plan.sample_at(now_ms-path.started_at_ms).map_err(candidate_motion_error)?;
    let current = MotionState { position: player.pos, velocity: player.vel, facing_rad: player.facing_rad };
    if current != start.state {
        return Err(V4Error::InvalidInput(format!(
            "INV-02: committed contact origin mismatch for {} at {}: current {:?}, path {:?}",
            player.player_id,before_ms,current,start.state)));
    }
    let distance = end.distance_m-start.distance_m;
    let bound = plan.maximum_path_speed_mps()*match_contracts::units::ms_to_seconds_f32(now_ms-before_ms);
    let eps = 32.0*f32::EPSILON*PITCH_LENGTH_M.max(PITCH_WIDTH_M) + 4.0*f32::EPSILON*end.distance_m;
    if !distance.is_finite() || distance < 0.0 || distance > bound+eps
        || start.state.position.distance(end.state.position)>distance+eps {
        return Err(candidate_motion_error("committed contact path budget"));
    }
    crate::v4::transition::verify_motion_budget(player.pos,end.state.position,bound,&player.player_id)?;
    player.pos=end.state.position;
    player.vel=end.state.velocity;
    player.facing_rad=end.state.facing_rad;
    player.target_pos=path.target;
    player.last_moved_m=distance;
    Ok(bound)
}

/// Select intentions from one immutable world snapshot; shared with the isolated
/// B1.2 candidate adapter. Legacy integration and RNG draw order are unchanged.
fn select_movement_targets(
    state: &MatchStateV4,
    input: &MatchInputV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    dt_sec: f32,
    rng: &mut DeterministicRng,
) -> (Vec<Option<Vec2>>, Vec<DefensiveIntent>) {
    let dt = dt_sec.max(0.0);
    // Scale stochastic target jitter/lapses by sqrt(min(dt, 1.0)) so short DES substeps (< 1s)
    // do not cause players to oscillate or jitter unrealistically.
    let noise_scale = dt.min(1.0).sqrt();
    let ball_xy = state.ball.pos.xy();
    let ball_progress_home = attacking_progress_m(TeamSideV4::Home, ball_xy) / PITCH_LENGTH_M;
    // Read the opponent line once from the same world snapshot for every runner.
    // Do not restrain a run after the pass has already been kicked.
    let receiving_line = if state.ball.control == BallControlState::Controlled {
        let launch_pos = state.players.get(carrier_idx)
            .filter(|p| p.on_pitch && !p.sent_off && p.side == possession_side)
            .map_or(ball_xy, |p| p.pos);
        offside_reference_progress_m(state, possession_side, launch_pos)
    } else {
        None
    };

    // Snapshot every position before any player's movement is integrated.
    // Support choices cannot see teammates' future positions just because
    // they occur earlier in the storage order.
    let controlled=state.ball.control==BallControlState::Controlled;
    let observed:Vec<_>=state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off)
        .map(|(idx,p)|ObservedPlayer{idx,side:p.side,pos:p.pos,is_gk:p.is_goalkeeper}).collect();
    // Choose shape intentions before moving players; preserve RNG draw order.
    let mut targets:Vec<Option<Vec2>>=vec![None;state.players.len()];
    let mut support_order=Vec::new();
    for idx in 0..state.players.len() {
        if !state.players[idx].on_pitch || state.players[idx].sent_off {
            continue;
        }
        let side = state.players[idx].side;
        let is_gk = state.players[idx].is_goalkeeper;
        let in_poss = side == possession_side;
        let phase = state.team_state(side).phase;
        let attrs = *player_attrs(state, &state.players[idx]);
        let ti = state.active_instructions(side).clone();

        let anchor = slot_anchor_m(state, input, &state.players[idx], in_poss);

        let target = select_movement_target(MovementObservation {
            is_carrier: idx == carrier_idx,
            is_gk, in_poss, side, phase,
            attrs: &attrs, instructions: &ti, anchor, ball_xy, ball_progress_home,
            receiving_line, noise_scale,
            slot_id: state.players[idx].slot_id.as_str(),
            position_familiarity: state.players[idx].position_familiarity,
            aerobic: state.players[idx].fatigue.aerobic,
        }, rng);

        targets[idx]=Some(target);
        if controlled && in_poss && !is_gk && idx!=carrier_idx {
            support_order.push(idx);
        }
    }
    // Nearest available support gets first choice; the stable player ID
    // breaks ties rather than incidental storage order. Reservations are
    // shared team intentions, never claimed future coordinates.
    support_order.sort_by(|a,b|{
        state.players[*a].pos.distance(ball_xy).total_cmp(&state.players[*b].pos.distance(ball_xy))
            .then_with(||state.players[*a].player_id.cmp(&state.players[*b].player_id))
    });
    let mut reserved=Vec::with_capacity(support_order.len());
    for idx in support_order {
        let p=&state.players[idx];
        let attrs=player_attrs(state,p);
        let awareness=((normalize_attr(attrs.mental.off_the_ball)*0.5
            +normalize_attr(attrs.mental.anticipation)*0.25+normalize_attr(attrs.mental.decisions)*0.25)
            *p.position_familiarity).clamp(0.0,1.0);
        let legal_limit=receiving_line.map(|line|line-0.6-1.8*awareness+3.0*(1.0-awareness));
        let shape=targets[idx].expect("active support player has a shape");
        let target=select_coordinated_support_target(SupportObservation{idx,side:p.side,pos:p.pos,
            ball:ball_xy,shape,players:&observed,awareness,
            reach_m:player_max_speed_mps(attrs,p)*1.6,legal_limit},&reserved);
        targets[idx]=Some(target);
        reserved.push(target);
    }

    // Assign defensive roles only against controlled possession. Pass/loose
    // ball physical contact paths and in-flight movement remain independent.
    let defense=if controlled {
        let side=possession_side.opposite();
        let ti=state.active_instructions(side);
        let candidates:Vec<_>=state.players.iter().enumerate().filter(|(_,p)|
            p.on_pitch && !p.sent_off && !p.is_goalkeeper && p.side==side)
            .map(|(idx,p)|{
                let attrs=player_attrs(state,p);
                DefenderObservation{idx,id:p.player_id.as_str(),pos:p.pos,shape:targets[idx].unwrap_or(p.pos),
                    speed:player_max_speed_mps(attrs,p),work:normalize_attr(attrs.mental.work_rate),
                    reading:normalize_attr(attrs.mental.positioning)*0.45+normalize_attr(attrs.mental.anticipation)*0.35
                        +normalize_attr(attrs.mental.teamwork)*0.20,
                    energy:(p.fatigue.aerobic*0.6+p.fatigue.anaerobic*0.4)/100.0}
            }).collect();
        assign_defense(side,ball_xy,carrier_idx,&observed,&candidates,
            ti.pressing_intensity as f32/100.0,
            state.team_state(side).phase==crate::v4::state::TacticalPhaseV4::DefensiveTransition)
    }else{Vec::new()};
    for a in &defense {targets[a.idx]=Some(a.target);}

    (targets, defense)
}

use match_physics::movement::{MotionPlan, MotionState, MotionResult, MovementCapability, MovementIntent};
use crate::v4::error::V4Error;

/// Candidate paths are explicit. A committed forecast takes precedence over a
/// newly selected tactical target; no preliminary player displacement is applied.
/// This is an isolated adapter, not yet the MatchSession production integrator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommittedMotion {
    pub player_idx: usize,
    pub plan: MotionPlan,
    pub elapsed_before_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerMotionInterval {
    pub player_idx: usize,
    pub start: MotionResult,
    pub end: MotionResult,
    pub duration_ms: i64,
    pub target: Vec2,
}

impl PlayerMotionInterval {
    pub fn distance_m(&self) -> f32 { self.end.distance_m - self.start.distance_m }
    pub fn acceleration_load_mps(&self) -> f32 {
        self.end.acceleration_load_mps - self.start.acceleration_load_mps
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionIntervalContext {
    pub possession_side: TeamSideV4,
    pub is_dead_ball: bool,
}

fn candidate_motion_error(detail: impl std::fmt::Debug) -> V4Error {
    V4Error::InvalidInput(format!("B1.2 candidate locomotion: {detail:?}"))
}

/// Build candidate plans for the live target policy from the same pre-event
/// state. The caller provides explicit committed contact/carry paths. RNG and
/// world state are committed only if the entire interval validates.
pub fn advance_candidate_live_motion(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    possession_side: TeamSideV4,
    carrier_idx: usize,
    duration_ms: i64,
    rng: &mut DeterministicRng,
    committed: &[CommittedMotion],
) -> Result<(Vec<PlayerMotionInterval>, Vec<DefensiveIntent>), V4Error> {
    if duration_ms <= 0 || duration_ms > match_physics::movement::MAX_MOTION_HORIZON_MS {
        return Err(candidate_motion_error("invalid live duration"));
    }
    crate::v4::transition::verify_commit_structure(state)?;
    let mut next_rng = rng.clone();
    let dt = match_contracts::units::ms_to_seconds_f32(duration_ms);
    let (targets, defense) = select_movement_targets(state, input, possession_side, carrier_idx, dt, &mut next_rng);
    let intents: Vec<_> = targets.into_iter().map(|target| target.map(|target|
        MovementIntent { target, intensity: 1.0, desired_facing_rad: None, reaction_ms: 0 }
    )).collect();
    let rows = advance_candidate_motion(state, &intents, committed, duration_ms,
        MotionIntervalContext { possession_side, is_dead_ball: false })?;
    *rng = next_rng;
    Ok((rows, defense))
}

/// Stage one motion and condition result per active player, verify all rows,
/// then publish both atomically. Do not charge interval fatigue again.
/// For a dead-ball interval the caller supplies low-intensity intents; the
/// same physical solver is used. Administrative kickoff resets are separate.
pub fn advance_candidate_motion(
    state: &mut MatchStateV4,
    intents: &[Option<MovementIntent>],
    committed: &[CommittedMotion],
    duration_ms: i64,
    context: MotionIntervalContext,
) -> Result<Vec<PlayerMotionInterval>, V4Error> {
    if duration_ms <= 0 || duration_ms > match_physics::movement::MAX_MOTION_HORIZON_MS
        || intents.len() != state.players.len()
    {
        return Err(candidate_motion_error("invalid interval or intent count"));
    }
    crate::v4::transition::verify_commit_structure(state)?;
    let mut paths = vec![None; state.players.len()];
    for row in committed {
        let p = state.players.get(row.player_idx).ok_or_else(|| candidate_motion_error("unknown committed actor"))?;
        if !p.on_pitch || p.sent_off || row.elapsed_before_ms < 0
            || paths[row.player_idx].replace(*row).is_some()
        {
            return Err(candidate_motion_error("invalid or duplicate committed actor"));
        }
    }
    let mut rows = Vec::new();
    let mut next_fatigue = Vec::new();
    let dt = match_contracts::units::ms_to_seconds_f32(duration_ms);
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off {
            if intents[idx].is_some() { return Err(candidate_motion_error("ineligible actor intent")); }
            continue;
        }
        let current = MotionState { position: p.pos, velocity: p.vel, facing_rad: p.facing_rad };
        let (plan, elapsed, target) = if let Some(path) = paths[idx] {
            (path.plan, path.elapsed_before_ms, path.plan.target())
        } else {
            let intent = intents[idx].ok_or_else(|| candidate_motion_error("missing active actor intent"))?;
            let profile = state.player_profiles.get(p.profile_idx)
                .ok_or_else(|| candidate_motion_error("missing actor profile"))?;
            let cap = MovementCapability::from_attributes(&profile.attrs.physical, p.fatigue.aerobic, p.fatigue.anaerobic)
                .map_err(candidate_motion_error)?;
            (MotionPlan::new(current, intent, cap).map_err(candidate_motion_error)?, 0, intent.target)
        };
        let end_ms = elapsed.checked_add(duration_ms).ok_or_else(|| candidate_motion_error("interval overflow"))?;
        let start = plan.sample_at(elapsed).map_err(candidate_motion_error)?;
        let end = plan.sample_at(end_ms).map_err(candidate_motion_error)?;
        if start.state != current {
            return Err(candidate_motion_error("committed path does not start at authoritative state"));
        }
        let distance = end.distance_m - start.distance_m;
        let load = end.acceleration_load_mps - start.acceleration_load_mps;
        let roundoff = 32.0 * f32::EPSILON * PITCH_LENGTH_M.max(PITCH_WIDTH_M)
            + 4.0 * f32::EPSILON * end.distance_m;
        if !distance.is_finite() || distance < 0.0 || !load.is_finite() || load < 0.0
            || distance > plan.maximum_path_speed_mps() * dt + roundoff
            || start.state.position.distance(end.state.position) > distance + roundoff
        {
            return Err(candidate_motion_error("invalid realized path length"));
        }
        // Preserve the existing two-pool model and intensity classification,
        // but use the actual path, including during stoppages. No second
        // interval charge may be applied by the caller to these rows.
        let profile = state.player_profiles.get(p.profile_idx)
            .ok_or_else(|| candidate_motion_error("missing fatigue profile"))?;
        let press = state.active_instructions(p.side).pressing_intensity as f32 / 100.0;
        let speed = distance / dt.max(0.05);
        let high = !context.is_dead_ball && !p.is_goalkeeper
            && (speed >= 4.2 || (p.side != context.possession_side && press >= 0.70 && speed >= 3.2));
        let mut fatigue = p.fatigue;
        fatigue.apply_interval(dt, distance, high, context.is_dead_ball,
            profile.attrs.physical.stamina, profile.attrs.physical.natural_fitness);
        if !fatigue.distance_m.is_finite() || fatigue.distance_m < p.fatigue.distance_m
            || !(0.0..=100.0).contains(&fatigue.aerobic)
            || !(0.0..=100.0).contains(&fatigue.anaerobic)
        {
            return Err(candidate_motion_error("invalid resulting condition"));
        }
        next_fatigue.push(fatigue);
        rows.push(PlayerMotionInterval { player_idx: idx, start, end, duration_ms, target });
    }
    for (row, fatigue) in rows.iter().zip(next_fatigue) {
        let p = &mut state.players[row.player_idx];
        p.pos = row.end.state.position;
        p.vel = row.end.state.velocity;
        p.facing_rad = row.end.state.facing_rad;
        p.target_pos = row.target;
        p.last_moved_m = row.distance_m();
        p.fatigue = fatigue;
    }
    for p in &mut state.players {
        if !p.on_pitch || p.sent_off { p.last_moved_m = 0.0; }
    }
    Ok(rows)
}

/// Legacy fallback lookup of a player's 54 attributes from `MatchInputV4` (lineup + bench).
/// Prefer `player_attrs(state, player)` for O(1) runtime access.
pub fn lookup_attrs<'a>(
    input: &'a MatchInputV4,
    player: &PlayerRuntimeStateV4,
) -> &'a PlayerAttributes54 {
    let team = match player.side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    team.lineup
        .iter()
        .chain(team.bench.iter())
        .find(|s| s.player_id == player.player_id)
        .map(|s| &s.attributes)
        .unwrap_or(&team.lineup[0].attributes)
}

pub fn team_instructions(input: &MatchInputV4, side: TeamSideV4) -> &TeamInstructionsV4 {
    match side {
        TeamSideV4::Home => &input.home_team.team_instructions,
        TeamSideV4::Away => &input.away_team.team_instructions,
    }
}

#[inline]
fn slot_anchor_m(
    state: &MatchStateV4,
    input: &MatchInputV4,
    player: &PlayerRuntimeStateV4,
    in_poss: bool,
) -> Vec2 {
    let anchors = match player.side {
        TeamSideV4::Home => &state.home_slot_anchors,
        TeamSideV4::Away => &state.away_slot_anchors,
    };
    if let Some(pair) = anchors.get(player.slot_idx) {
        return if in_poss {
            pair.in_poss_pos
        } else {
            pair.out_poss_pos
        };
    }

    let team = match player.side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    if let Some(slot) = team
        .tactical_slots
        .iter()
        .find(|s| s.slot_id == player.slot_id)
    {
        let (nx, ny) = if in_poss {
            (slot.in_x as f32, slot.in_y as f32)
        } else {
            (slot.out_x as f32, slot.out_y as f32)
        };
        Vec2::from_normalized_slot(player.side, nx, ny)
    } else {
        Vec2::new(PITCH_LENGTH_M * 0.5, PITCH_WIDTH_M * 0.5)
    }
}


#[cfg(test)]
mod candidate_locomotion_tests {
    use super::*;
    use crate::v4::input::{RulesModeV4, SimulationMode};
    use crate::v4::tests::make_v4_input;

    fn fixture() -> (MatchInputV4, MatchStateV4) {
        let input = make_v4_input(12046, SimulationMode::FullEvent, RulesModeV4::Standard);
        let state = MatchStateV4::from_input(&input);
        (input, state)
    }

    fn stationary(state: &MatchStateV4) -> Vec<Option<MovementIntent>> {
        state.players.iter().map(|p| {
            (p.on_pitch && !p.sent_off).then_some(MovementIntent {
                target: p.pos, intensity: 0.0, desired_facing_rad: None, reaction_ms: 0,
            })
        }).collect()
    }

    fn context(dead: bool) -> MotionIntervalContext {
        MotionIntervalContext { possession_side: TeamSideV4::Home, is_dead_ball: dead }
    }

    fn snap(state: &MatchStateV4) -> String { serde_json::to_string(state).unwrap() }

    #[test]
    fn b12_candidate_live_uses_same_target_policy_rng_and_terminal_velocity() {
        let (input, mut state) = fixture();
        let mut legacy = state.clone();
        let mut old_rng = DeterministicRng::new(19);
        let mut new_rng = old_rng.clone();
        advance_off_ball_movement_with_report(&mut legacy, &input, TeamSideV4::Home, 10, 0.5, &mut old_rng);
        let (rows, _) = advance_candidate_live_motion(&mut state, &input, TeamSideV4::Home, 10, 500, &mut new_rng, &[]).unwrap();
        assert_eq!(old_rng.state(), new_rng.state());
        assert_eq!(rows.len(), state.players.iter().filter(|p|p.on_pitch&&!p.sent_off).count());
        for r in rows {
            assert_eq!(state.players[r.player_idx].target_pos, legacy.players[r.player_idx].target_pos);
            assert_eq!(state.players[r.player_idx].vel, r.end.state.velocity);
            assert_eq!(state.players[r.player_idx].last_moved_m, r.distance_m());
        }
    }

    #[test]
    fn b12_candidate_committed_path_wins_and_actual_distance_is_charged_once() {
        let (_, mut state) = fixture();
        let idx = 1;
        let p = &mut state.players[idx];
        p.pos = Vec2::new(50.0, 34.0);
        p.vel = Vec2::new(6.0, 0.0);
        p.facing_rad = 0.0;
        let original = p.fatigue;
        let cap = MovementCapability { max_speed_mps: 8.0, acceleration_mps2: 4.0,
            braking_mps2: 5.0, lateral_mps2: 3.0, body_turn_radps: 3.0 };
        let plan = MotionPlan::new(MotionState { position: p.pos, velocity: p.vel, facing_rad: p.facing_rad },
            MovementIntent { target: Vec2::new(20.0, 34.0), intensity: 1.0, desired_facing_rad: None, reaction_ms: 0 },cap).unwrap();
        let intents = stationary(&state);
        let rows = advance_candidate_motion(&mut state, &intents,
            &[CommittedMotion { player_idx: idx, plan, elapsed_before_ms: 0 }], 2400, context(false)).unwrap();
        let row = rows.iter().find(|r|r.player_idx == idx).unwrap();
        assert_eq!(row.end, plan.sample_at(2400).unwrap());
        assert!(row.distance_m() > row.start.state.position.distance(row.end.state.position) + 2.0);
        assert_eq!(state.players[idx].last_moved_m, row.distance_m());
        assert!((state.players[idx].fatigue.distance_m-original.distance_m-row.distance_m()).abs()<1e-4);
        assert!(row.acceleration_load_mps()>6.0);
        let before = snap(&state);
        assert!(advance_candidate_motion(&mut state, &intents,
            &[CommittedMotion { player_idx: idx, plan, elapsed_before_ms: 0 }], 100, context(false)).is_err());
        assert_eq!(snap(&state), before);
    }

    #[test]
    fn b12_candidate_split_committed_path_preserves_motion_and_distance() {
        let (_, mut whole) = fixture();
        let idx = 1;
        let p = &mut whole.players[idx];
        p.pos = Vec2::new(50.0,34.0);
        p.vel = Vec2::new(4.0,0.0);
        p.facing_rad = 0.0;
        let distance_before = p.fatigue.distance_m;
        let cap = MovementCapability { max_speed_mps: 8.0, acceleration_mps2: 4.0,
            braking_mps2: 5.0, lateral_mps2: 3.0, body_turn_radps: 3.0 };
        let plan = MotionPlan::new(MotionState { position: p.pos, velocity: p.vel, facing_rad: p.facing_rad },
            MovementIntent { target: Vec2::new(20.0,34.0), intensity: 1.0, desired_facing_rad: None, reaction_ms: 0 },cap).unwrap();
        let mut split = whole.clone();
        let row = CommittedMotion { player_idx: idx, plan, elapsed_before_ms: 0 };
        let intents = stationary(&whole);
        advance_candidate_motion(&mut whole,&intents,&[row],1777,context(false)).unwrap();
        let intents = stationary(&split);
        advance_candidate_motion(&mut split,&intents,&[row],623,context(false)).unwrap();
        let intents = stationary(&split);
        advance_candidate_motion(&mut split,&intents,&[CommittedMotion { elapsed_before_ms:623,..row }],1154,context(false)).unwrap();
        let a=&whole.players[idx]; let b=&split.players[idx];
        assert_eq!((a.pos,a.vel,a.facing_rad),(b.pos,b.vel,b.facing_rad));
        assert!((a.fatigue.distance_m-b.fatigue.distance_m).abs()<1e-4);
        assert!((b.fatigue.distance_m-distance_before-plan.sample_at(1777).unwrap().distance_m).abs()<1e-4);
        // Energy and sprint counters intentionally retain the legacy interval
        // classification; this is not a proof of condition split invariance.
    }

    #[test]
    fn b12_candidate_dead_ball_uses_real_distance_and_skips_inactive() {
        let (_, mut state) = fixture();
        state.players[1].on_pitch = false;
        let inactive = state.players[1].fatigue;
        let initial = state.clone();
        let mut intents = stationary(&state);
        let p = &state.players[2];
        intents[2] = Some(MovementIntent { target: Vec2::new(p.pos.x+10.0, p.pos.y),
            intensity: 0.2, desired_facing_rad: None, reaction_ms: 0 });
        let rows = advance_candidate_motion(&mut state, &intents, &[], 1000, context(true)).unwrap();
        assert_eq!(state.players[1].fatigue, inactive);
        assert!(!rows.iter().any(|r|r.player_idx==1));
        for row in rows {
            let delta=state.players[row.player_idx].fatigue.distance_m-initial.players[row.player_idx].fatigue.distance_m;
            assert!((delta-row.distance_m()).abs()<1e-4);
        }
        assert!(state.players[2].last_moved_m>0.0);
        assert_eq!(state.players[3].last_moved_m,0.0);
    }

    #[test]
    fn b12_candidate_invalid_duplicate_or_late_row_rolls_back_world_and_rng() {
        let (input, mut state) = fixture();
        let mut intents = stationary(&state);
        let before = snap(&state);
        intents[20] = Some(MovementIntent { intensity: f32::NAN, ..intents[20].unwrap() });
        assert!(advance_candidate_motion(&mut state,&intents,&[],500,context(false)).is_err());
        assert_eq!(snap(&state),before);
        let p=&state.players[1];
        let cap=MovementCapability::from_attributes(&player_attrs(&state,p).physical,p.fatigue.aerobic,p.fatigue.anaerobic).unwrap();
        let plan=MotionPlan::new(MotionState{position:p.pos,velocity:p.vel,facing_rad:p.facing_rad},
            stationary(&state)[1].unwrap(),cap).unwrap();
        let row=CommittedMotion{player_idx:1,plan,elapsed_before_ms:0};
        let mut rng=DeterministicRng::new(19);
        let rng_before=rng.state();
        assert!(advance_candidate_live_motion(&mut state,&input,TeamSideV4::Home,10,500,&mut rng,&[row,row]).is_err());
        assert_eq!(snap(&state),before);
        assert_eq!(rng.state(),rng_before);
    }
}
