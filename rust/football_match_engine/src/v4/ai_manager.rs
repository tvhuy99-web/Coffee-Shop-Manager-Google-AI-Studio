//! Coach Intelligence compatibility module (Phase A3 boundary).
//!
//! Owns AI manager tactical and substitution decisions (`evaluate_ai_managers`).
//! Manual substitution execution (`apply_manual_substitution_group`) now lives in
//! `crate::v4::substitution` and is re-exported here for backward compatibility.

use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PlayerMatchSnapshotV4, TeamMatchSnapshotV4};
use crate::v4::psychology::substitute_psychology;
use crate::v4::spatial::{TeamSideV4, Vec2, Vec3};
use crate::v4::state::{
    compute_position_familiarity, MatchStateV4, PlayerRuntimeStateV4, TwoPoolFatigueState,
};
use crate::v4::substitution::resolve_bench_profile_idx;

pub use crate::v4::substitution::{
    apply_manual_substitution_group, MAX_SUBSTITUTIONS, MAX_SUBSTITUTION_WINDOWS,
};

/// Adaptive AI Manager for Match Engine V4.0:
/// - Immediately prioritizes substituting injured players at ANY minute.
/// - Enforces FIFA substitution limits (max 5 subs in max 3 in-play windows).
/// - Adjusts tactical instructions dynamically for red cards, scoreline, clock, and xG siege.
pub fn evaluate_ai_managers(state: &mut MatchStateV4, input: &MatchInputV4) -> Vec<EmittedPlayEvent> {
    let mut events = Vec::new();
    let game_sec = state.clock.game_second;

    if input.ai_home_manager {
        events.extend(evaluate_side(state, input, TeamSideV4::Home, game_sec));
    }
    if input.ai_away_manager {
        events.extend(evaluate_side(state, input, TeamSideV4::Away, game_sec));
    }
    events
}

fn evaluate_side(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    game_sec: i32,
) -> Vec<EmittedPlayEvent> {
    let mut events = Vec::new();
    let opp = side.opposite();

    let own_goals = state.team_state(side).goals as i32;
    let opp_goals = state.team_state(opp).goals as i32;
    let goal_diff = own_goals - opp_goals;

    let own_xg = state.team_state(side).xg_millis as f32 / 1000.0;
    let opp_xg = state.team_state(opp).xg_millis as f32 / 1000.0;

    let active_count = state
        .players
        .iter()
        .filter(|p| p.side == side && p.on_pitch && !p.sent_off)
        .count();
    let down_to_ten = active_count <= 10;

    let base_ti = match side {
        TeamSideV4::Home => input.home_team.team_instructions.clone(),
        TeamSideV4::Away => input.away_team.team_instructions.clone(),
    };
    let next_ti = coach_intelligence::propose_tactics(base_ti, coach_intelligence::CoachObservation {
        goal_diff, own_xg, opp_xg, down_to_ten, game_sec,
    });

    let prev_ti = state.active_instructions(side).clone();
    if next_ti != prev_ti {
        let high_line = next_ti.line_height >= 68;
        let wasting = next_ti.time_wasting >= 55;
        let ball_pos = state.ball.pos;
        *state.active_instructions_mut(side) = next_ti;
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::TacticalPhaseChanged,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(side),
            actor: None,
            target: None,
            ball_pos,
            metadata: EventMetadataV4 {
                high_defensive_line: if high_line { Some(true) } else { None },
                time_wasting: if wasting { Some(true) } else { None },
                ten_men: if down_to_ten { Some(true) } else { None },
                cause: Some("AI_MANAGER_ADAPTATION".into()),
                ..EventMetadataV4::default()
            },
        });
    }

    let team_snap = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };

    let bench_profile_base = match side {
        TeamSideV4::Home => input.home_team.lineup.len(),
        TeamSideV4::Away => {
            input.home_team.lineup.len()
                + input.home_team.bench.len()
                + input.away_team.lineup.len()
        }
    };

    let subs_used = state.team_state(side).substitutions_used;
    let windows_used = state.team_state(side).substitution_windows_used;

    if subs_used < MAX_SUBSTITUTIONS && windows_used < MAX_SUBSTITUTION_WINDOWS {
        if let Some(inj_sub) =
            try_make_substitution(state, input, team_snap, bench_profile_base, side, game_sec, true)
        {
            events.push(inj_sub);
            return events;
        }
        if game_sec >= 3360 {
            if let Some(sub_ev) =
                try_make_substitution(state, input, team_snap, bench_profile_base, side, game_sec, false)
            {
                events.push(sub_ev);
            }
        }
    }

    events
}

fn try_make_substitution(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    team_snap: &TeamMatchSnapshotV4,
    bench_profile_base: usize,
    side: TeamSideV4,
    game_sec: i32,
    injury_only: bool,
) -> Option<EmittedPlayEvent> {
    if team_snap.bench.is_empty() {
        return None;
    }

    let out_idx = if injury_only {
        state
            .players
            .iter()
            .position(|p| p.side == side && p.on_pitch && !p.sent_off && p.injury.is_some())?
    } else {
        let fatigue_threshold = if game_sec >= 4500 { 74.0 } else { 66.0 };
        let mut worst_idx: Option<usize> = None;
        let mut worst_score = f32::MAX;
        for (idx, p) in state.players.iter().enumerate() {
            if p.side != side || !p.on_pitch || p.sent_off || p.is_goalkeeper {
                continue;
            }
            let card_pen = if p.yellow_cards > 0 { 10.0 } else { 0.0 };
            let readiness = p.fatigue.aerobic - card_pen;
            if readiness < fatigue_threshold && readiness < worst_score {
                worst_score = readiness;
                worst_idx = Some(idx);
            }
        }
        worst_idx?
    };

    let target_slot = state.players[out_idx].slot_id.clone();
    let is_replacing_gk = state.players[out_idx].is_goalkeeper;

    let mut best_bench_idx: Option<usize> = None;
    let mut best_fit = -1.0f32;

    for (b_idx, bp) in team_snap.bench.iter().enumerate() {
        if bp.injured || !bp.eligible {
            continue;
        }
        let already_entered = state.players.iter().any(|rp| rp.player_id == bp.player_id);
        if already_entered {
            continue;
        }
        let fam = compute_position_familiarity(
            &bp.primary_position,
            &bp.secondary_position,
            &target_slot,
        );
        let fit = fam * 100.0 + (bp.condition as f32 * 0.25);
        if fit > best_fit {
            best_fit = fit;
            best_bench_idx = Some(b_idx);
        }
    }

    let bench_idx = best_bench_idx?;
    let bench_snap = &team_snap.bench[bench_idx];
    let out_id = state.players[out_idx].player_id.clone();
    let target_slot_idx = state.players[out_idx].slot_idx;
    let out_pos = state.players[out_idx].pos;
    let out_facing = state.players[out_idx].facing_rad;
    let profile_idx = resolve_bench_profile_idx(state, bench_snap, bench_profile_base + bench_idx);

    state.players[out_idx].on_pitch = false;
    let fam = compute_position_familiarity(
        &bench_snap.primary_position,
        &bench_snap.secondary_position,
        &target_slot,
    );
    state.players.push(PlayerRuntimeStateV4 {
        player_id: bench_snap.player_id.clone(),
        side,
        slot_id: target_slot,
        profile_idx,
        slot_idx: target_slot_idx,
        last_moved_m: 0.0,
        pos: out_pos,
        target_pos: out_pos,
        vel: Vec2::default(),
        facing_rad: out_facing,
        on_pitch: true,
        is_goalkeeper: is_replacing_gk,
        sent_off: false,
        yellow_cards: 0,
        injury: None,
        position_familiarity: fam,
        fatigue: TwoPoolFatigueState::from_condition(bench_snap.condition),
        psychology: substitute_psychology(input,side,bench_snap),
        goals: 0,
        own_goals: 0,
        assists: 0,
        rating_tenths: 60,
    });

    state.team_state_mut(side).substitutions_used =
        state.team_state(side).substitutions_used.saturating_add(1);
    state.team_state_mut(side).substitution_windows_used =
        state.team_state(side).substitution_windows_used.saturating_add(1);

    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);

    Some(EmittedPlayEvent {
        kind: EventTypeV4::Substitution,
        tier: HighlightTier::Extended,
        seq_id: Some(seq_id),
        side: Some(side),
        actor: Some(bench_snap.player_id.clone()),
        target: Some(out_id),
        ball_pos: Vec3::ground(out_pos.x, out_pos.y).clamp_to_pitch(),
        metadata: EventMetadataV4 {
            cause: Some(if injury_only {
                "INJURY_FORCED_SUB".into()
            } else {
                "AI_MANAGER_SUB".into()
            }),
            ..EventMetadataV4::default()
        },
    })
}
