//! Authoritative substitution validation & atomic execution (Engine/Rules boundary).
//!
//! Extracted from `v4/ai_manager.rs` in Phase A3 so manual player substitutions and
//! substitution-window enforcement belong to Engine/Rules rather than `coach_intelligence`.

use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PlayerMatchSnapshotV4};
use crate::v4::psychology::substitute_psychology;
use crate::v4::spatial::{TeamSideV4, Vec2, Vec3};
use crate::v4::state::{
    compute_position_familiarity, MatchStateV4, PlayerRuntimeStateV4, PlayerStaticProfile,
    TwoPoolFatigueState,
};

pub const MAX_SUBSTITUTIONS: u8 = 5;
pub const MAX_SUBSTITUTION_WINDOWS: u8 = 3;

pub(crate) fn resolve_bench_profile_idx(
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

