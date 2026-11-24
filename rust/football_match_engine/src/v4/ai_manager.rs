use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PlayerMatchSnapshotV4, TeamMatchSnapshotV4};
use crate::v4::spatial::{TeamSideV4, Vec2, Vec3};
use crate::v4::psychology::substitute_psychology;
use crate::v4::state::{
    compute_position_familiarity, MatchStateV4, PlayerRuntimeStateV4,
    PlayerStaticProfile, TwoPoolFatigueState,
};

fn resolve_bench_profile_idx(
    state: &mut MatchStateV4,
    bench_snap: &PlayerMatchSnapshotV4,
    expected_idx: usize,
) -> usize {
    if let Some(existing) = state.player_profiles.get(expected_idx) {
        if existing.attrs == bench_snap.attributes
            && existing.preferred_foot == bench_snap.preferred_foot
            && existing.weak_foot == bench_snap.weak_foot
        {
            return expected_idx;
        }
    }
    let idx = state.player_profiles.len();
    state.player_profiles.push(PlayerStaticProfile {
        attrs: bench_snap.attributes,
        preferred_foot: bench_snap.preferred_foot,
        weak_foot: bench_snap.weak_foot,
    });
    idx
}

pub const MAX_SUBSTITUTIONS: u8 = 5;
pub const MAX_SUBSTITUTION_WINDOWS: u8 = 3;

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

/// Applies a user-requested interactive substitution (`player_out_id` -> `player_in_id`).
fn apply_manual_substitution_single(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    player_out_id: &str,
    player_in_id: &str,
) -> Result<EmittedPlayEvent, &'static str> {
    if state.team_state(side).substitutions_used >= MAX_SUBSTITUTIONS {
        return Err("substitution limit reached");
    }
    let out_idx = state
        .players
        .iter()
        .position(|p| p.side == side && p.player_id == player_out_id && p.on_pitch && !p.sent_off)
        .ok_or("outgoing player not active on pitch")?;
    if state.players.iter().any(|p| p.player_id == player_in_id) {
        return Err("incoming player already played");
    }
    let team_snap = match side {
        TeamSideV4::Home => &input.home_team,
        TeamSideV4::Away => &input.away_team,
    };
    let (bench_idx, bench_snap) = team_snap
        .bench
        .iter()
        .enumerate()
        .find(|(_, p)| p.player_id == player_in_id && !p.injured && p.eligible)
        .ok_or("incoming player not available on bench")?;

    let target_slot = state.players[out_idx].slot_id.clone();
    let target_slot_idx = state.players[out_idx].slot_idx;
    let is_replacing_gk = state.players[out_idx].is_goalkeeper;
    let out_pos = state.players[out_idx].pos;
    let out_facing = state.players[out_idx].facing_rad;
    let expected_profile_idx = match side {
        TeamSideV4::Home => input.home_team.lineup.len() + bench_idx,
        TeamSideV4::Away => {
            input.home_team.lineup.len()
                + input.home_team.bench.len()
                + input.away_team.lineup.len()
                + bench_idx
        }
    };
    let profile_idx = resolve_bench_profile_idx(state, bench_snap, expected_profile_idx);

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

    state.team_state_mut(side).substitutions_used += 1;

    let seq_id = state.next_highlight_sequence_id;
    state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);

    Ok(EmittedPlayEvent {
        kind: EventTypeV4::Substitution,
        tier: HighlightTier::Extended,
        seq_id: Some(seq_id),
        side: Some(side),
        actor: Some(bench_snap.player_id.clone()),
        target: Some(player_out_id.to_string()),
        ball_pos: Vec3::ground(out_pos.x, out_pos.y).clamp_to_pitch(),
        metadata: EventMetadataV4 {
            cause: Some("USER_MANUAL_SUB".into()),
            ..EventMetadataV4::default()
        },
    })
}

/// Executes a grouped substitution atomically. Changes made at half-time or
/// the extra-time interval use player slots but not one of three in-play windows.
/// The caller must issue one batch per actual stoppage, never infer a shared
/// stoppage from identical game-second values.
pub fn apply_manual_substitution_group(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    side: TeamSideV4,
    changes: &[(String, String)],
    during_interval: bool,
) -> Result<Vec<EmittedPlayEvent>, &'static str> {
    if changes.is_empty() {
        return Err("substitution group cannot be empty");
    }
    if changes.len() > MAX_SUBSTITUTIONS as usize {
        return Err("too many substitutions in one stoppage");
    }
    let team = state.team_state(side);
    if usize::from(team.substitutions_used) + changes.len() > usize::from(MAX_SUBSTITUTIONS) {
        return Err("substitution limit reached");
    }
    if !during_interval && team.substitution_windows_used >= MAX_SUBSTITUTION_WINDOWS {
        return Err("substitution window limit reached");
    }
    let mut outs = std::collections::HashSet::new();
    let mut ins = std::collections::HashSet::new();
    for (out, incoming) in changes {
        if out == incoming || !outs.insert(out.as_str()) || !ins.insert(incoming.as_str()) {
            return Err("duplicate or self-referential substitution");
        }
    }
    // A simultaneous exchange cannot take off someone who is being brought on
    // by another exchange in the SAME group.
    if outs.iter().any(|out| ins.contains(out)) {
        return Err("substitution group cannot chain incoming players");
    }

    // Work on a cloned runtime state: if any incoming player is ineligible,
    // the entire batch fails and no player/slot/window is consumed.
    let mut candidate = state.clone();
    let mut events = Vec::with_capacity(changes.len());
    for (out, incoming) in changes {
        events.push(apply_manual_substitution_single(
            &mut candidate, input, side, out, incoming,
        )?);
    }
    if !during_interval {
        candidate.team_state_mut(side).substitution_windows_used += 1;
    }
    *state = candidate;
    Ok(events)
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
    let mut next_ti = base_ti;

    if down_to_ten {
        next_ti.line_height = next_ti.line_height.saturating_sub(18).max(20);
        next_ti.pressing_intensity = next_ti.pressing_intensity.saturating_sub(14).max(25);
        next_ti.build_up_risk = next_ti.build_up_risk.saturating_sub(15).max(20);
        next_ti.width = next_ti.width.saturating_sub(12).max(30);
    }

    if goal_diff < 0 && game_sec >= 3600 {
        let urgency: u8 = if game_sec >= 4680 { 20 } else { 12 };
        next_ti.line_height = (next_ti.line_height + urgency).min(88);
        next_ti.pressing_intensity = (next_ti.pressing_intensity + urgency).min(90);
        next_ti.tempo = (next_ti.tempo + urgency).min(90);
        next_ti.transition_risk = (next_ti.transition_risk + 14).min(90);
        next_ti.time_wasting = 5;
    } else if goal_diff > 0 && game_sec >= 4320 {
        let lock: u8 = if game_sec >= 4920 { 22 } else { 14 };
        next_ti.line_height = next_ti.line_height.saturating_sub(lock).max(22);
        next_ti.pressing_intensity = next_ti.pressing_intensity.saturating_sub(12).max(28);
        next_ti.tempo = next_ti.tempo.saturating_sub(14).max(25);
        next_ti.build_up_risk = next_ti.build_up_risk.saturating_sub(15).max(20);
        next_ti.time_wasting = if game_sec >= 4800 { 78 } else { 62 };
    } else if (opp_xg - own_xg) >= 0.85 && game_sec >= 2100 {
        next_ti.line_height = next_ti.line_height.saturating_sub(10).max(28);
        next_ti.concentration = (next_ti.concentration + 10).min(90);
    }

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
