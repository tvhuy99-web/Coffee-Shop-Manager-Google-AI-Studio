use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::input::{MatchInputV4, TeamInstructionsV4};
use crate::v4::spatial::{
    attacking_progress_m, point_to_segment_distance_m, TeamSideV4, Vec2, PITCH_LENGTH_M,
    PITCH_WIDTH_M,
};
use crate::v4::state::{MatchStateV4, PlayerRuntimeStateV4, TacticalPhaseV4};
use crate::v4::tactics::effective_press_multiplier;

#[derive(Debug, Clone, Copy)]
pub struct LocalSpatialContext {
    pub pressure: f32,
    pub nearest_defender_idx: Option<usize>,
    pub nearest_defender_dist_m: f32,
    pub overload: bool,
}

pub fn evaluate_local_context(
    state: &MatchStateV4,
    input: &MatchInputV4,
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
                let attrs = lookup_attrs(input, p);
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
    input: &MatchInputV4,
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

        let attrs = lookup_attrs(input, p);
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
    let ball_xy = state.ball.pos.xy();
    let ball_progress_home = attacking_progress_m(TeamSideV4::Home, ball_xy) / PITCH_LENGTH_M;

    for idx in 0..state.players.len() {
        if !state.players[idx].on_pitch || state.players[idx].sent_off {
            continue;
        }
        let side = state.players[idx].side;
        let is_gk = state.players[idx].is_goalkeeper;
        let in_poss = side == possession_side;
        let phase = state.team_state(side).phase;
        let attrs = lookup_attrs(input, &state.players[idx]);
        let ti = state.active_instructions(side).clone();

        let anchor = slot_anchor_m(input, side, &state.players[idx].slot_id, in_poss);

        let target = if idx == carrier_idx {
            ball_xy
        } else if is_gk {
            let line_shift = (ti.line_height as f32 / 100.0) * 6.5;
            let rush_bonus = normalize_attr(attrs.goalkeeping.rushing_out) * 3.5;
            let gk_x = match side {
                TeamSideV4::Home => (4.5 + ball_progress_home * (line_shift + rush_bonus)).clamp(2.5, 16.5),
                TeamSideV4::Away => {
                    (PITCH_LENGTH_M - 4.5 - (1.0 - ball_progress_home) * (line_shift + rush_bonus))
                        .clamp(PITCH_LENGTH_M - 16.5, PITCH_LENGTH_M - 2.5)
                }
            };
            let gk_y = (34.0 + (ball_xy.y - 34.0) * 0.22).clamp(24.0, 44.0);
            Vec2::new(gk_x, gk_y)
        } else if in_poss {
            let otb = normalize_attr(attrs.mental.off_the_ball);
            let work = normalize_attr(attrs.mental.work_rate);
            let width_factor = 0.80 + (ti.width as f32 / 100.0) * 0.40;
            let transition_surge = if phase == TacticalPhaseV4::AttackingTransition {
                (ti.transition_risk as f32 / 100.0) * 6.5
            } else {
                0.0
            };

            let shift_x = (ball_xy.x - PITCH_LENGTH_M * 0.5) * 0.34;
            let dir = match side {
                TeamSideV4::Home => 1.0f32,
                TeamSideV4::Away => -1.0f32,
            };
            let ball_prog_team = attacking_progress_m(side, ball_xy);
            let sid = state.players[idx].slot_id.as_str();
            let (mut cut_in, mut extra_fwd) = (0.0f32, 0.0f32);
            if (sid == "LW" || sid == "RW") && ball_prog_team > 52.0 {
                cut_in = (34.0 - anchor.y) * (0.38 + otb * 0.25);
                extra_fwd = 4.5 + otb * 4.0;
            } else if (sid == "LCM" || sid == "RCM" || sid == "CM") && ball_prog_team > 58.0 {
                extra_fwd = 8.0 + otb * 9.0;
            } else if (sid == "LB" || sid == "RB") && ball_prog_team > 52.0 {
                extra_fwd = 9.0 + otb * 6.0;
            }
            let pocket_run =
                dir * (otb * 6.0 + work * 2.0 + extra_fwd + transition_surge) * state.players[idx].position_familiarity;
            let jitter_x = (rng.next_double() as f32 - 0.5) * (6.0 - otb * 3.0);
            let jitter_y = (rng.next_double() as f32 - 0.5) * (7.0 - otb * 3.0);
            let stretched_y = 34.0 + (anchor.y - 34.0) * width_factor + cut_in;
            Vec2::new(anchor.x + shift_x + pocket_run + jitter_x, stretched_y + jitter_y)
                .clamp_to_pitch()
        } else {
            let pos_skill = normalize_attr(attrs.mental.positioning);
            let conc = normalize_attr(attrs.mental.concentration);
            let line_bias = ((ti.line_height as f32 - 50.0) / 50.0) * 12.0;
            let counter_press_push = if phase == TacticalPhaseV4::DefensiveTransition && ti.pressing_intensity >= 62 {
                4.5
            } else {
                0.0
            };
            let dir = match side {
                TeamSideV4::Home => 1.0f32,
                TeamSideV4::Away => -1.0f32,
            };
            let shift_x = (ball_xy.x - PITCH_LENGTH_M * 0.5) * 0.42 + dir * (line_bias + counter_press_push);
            let compact_y = anchor.y + (ball_xy.y - anchor.y) * 0.28;

            let fatigue_penalty = ((100.0 - state.players[idx].fatigue.aerobic) / 100.0) * 0.35;
            let lapse_risk = ((1.0 - conc * 0.65 - pos_skill * 0.35) + fatigue_penalty
                + (1.0 - state.players[idx].position_familiarity) * 0.6)
                .clamp(0.05, 0.85);
            let lapse_x = if (rng.next_double() as f32) < lapse_risk * 0.35 {
                (rng.next_double() as f32 - 0.5) * 9.5 * lapse_risk
            } else {
                (rng.next_double() as f32 - 0.5) * 2.4
            };
            let lapse_y = (rng.next_double() as f32 - 0.5) * (3.0 + 4.0 * lapse_risk);

            Vec2::new(anchor.x + shift_x + lapse_x, compact_y + lapse_y).clamp_to_pitch()
        };

        let cur = state.players[idx].pos;
        let dist = cur.distance(target);
        let pace = normalize_attr(attrs.physical.pace);
        let accel = normalize_attr(attrs.physical.acceleration);
        let energy = ((state.players[idx].fatigue.aerobic * 0.6
            + state.players[idx].fatigue.anaerobic * 0.4)
            / 100.0)
            .clamp(0.45, 1.0);
        let max_speed_mps = (4.2 + pace * 2.8 + accel * 1.1) * energy;
        let max_step = max_speed_mps * dt_sec.clamp(1.0, 6.0);
        let new_pos = if dist <= max_step || dist <= 0.05 {
            target
        } else {
            let t = max_step / dist;
            Vec2::new(cur.x + (target.x - cur.x) * t, cur.y + (target.y - cur.y) * t)
                .clamp_to_pitch()
        };

        let moved_m = cur.distance(new_pos);
        state.players[idx].target_pos = target;
        state.players[idx].pos = new_pos;
        state.players[idx].vel = Vec2::new(
            (new_pos.x - cur.x) / dt_sec.max(1.0),
            (new_pos.y - cur.y) / dt_sec.max(1.0),
        );
        if moved_m > 0.1 {
            state.players[idx].facing_rad = (new_pos.y - cur.y).atan2(new_pos.x - cur.x);
        }
    }
}

/// Looks up a player's 54 attributes from BOTH starting lineup and bench (supports live substitutions).
pub fn lookup_attrs<'a>(input: &'a MatchInputV4, player: &PlayerRuntimeStateV4) -> &'a PlayerAttributes54 {
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

fn slot_anchor_m(input: &MatchInputV4, side: TeamSideV4, slot_id: &str, in_poss: bool) -> Vec2 {
    let team = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    if let Some(slot) = team.tactical_slots.iter().find(|s| s.slot_id == slot_id) {
        let (nx, ny) = if in_poss {
            (slot.in_x as f32, slot.in_y as f32)
        } else {
            (slot.out_x as f32, slot.out_y as f32)
        };
        Vec2::from_normalized_slot(side, nx, ny)
    } else {
        Vec2::new(PITCH_LENGTH_M * 0.5, PITCH_WIDTH_M * 0.5)
    }
}
