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

#[derive(Debug, Clone, Copy)]
pub struct LocalSpatialContext {
    pub pressure: f32,
    pub nearest_defender_idx: Option<usize>,
    pub nearest_defender_dist_m: f32,
    pub overload: bool,
}

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
    // First choose shape intentions without moving anybody. Keep the
    // existing RNG draw order for the legacy shape policy.
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

    // Integrate once, using the FINAL destination. Previously dist was
    // computed before support adjusted target, which could exceed speed
    // budget or step past the intended destination.
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
