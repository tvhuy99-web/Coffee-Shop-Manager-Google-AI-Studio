use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::TeamInstructionsV4;
use crate::v4::spatial::{attacking_progress_m, TeamSideV4, Vec3};
use crate::v4::state::{MatchStateV4, TacticalPhaseV4};

/// Context flags emitted onto V4 events to feed `LabCommentaryTemplatePolicy.kt`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TacticalMetadataFlags {
    pub sustained_pressure: bool,
    pub high_defensive_line: bool,
    pub time_wasting: bool,
    pub ten_men: bool,
}

/// Updates the 4-phase tactical state machine (`InPossession`, `AttackingTransition`,
/// `DefensiveTransition`, `OutOfPossession`) on every action beat and emits
/// `TransitionStarted` / `TransitionEnded` / `TacticalPhaseChanged` events when phases shift.
pub fn update_tactical_phases(
    state: &mut MatchStateV4,
    prev_possession: TeamSideV4,
    new_possession: TeamSideV4,
    ball_pos: Vec3,
) -> Vec<EmittedPlayEvent> {
    let mut events = Vec::with_capacity(2);
    let flipped = prev_possession != new_possession;

    if flipped {
        // Turnover triggers a 2-beat transition window
        state.transition_beats_remaining = 2;
        state.team_state_mut(new_possession).phase = TacticalPhaseV4::AttackingTransition;
        state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::DefensiveTransition;
        state.team_state_mut(new_possession.opposite()).sustained_pressure_streak = 0;

        events.push(EmittedPlayEvent {
            kind: EventTypeV4::TransitionStarted,
            tier: HighlightTier::None,
            seq_id: None,
            side: Some(new_possession),
            actor: None,
            target: None,
            ball_pos,
            metadata: EventMetadataV4 {
                outcome: Some("ATTACKING_TRANSITION".into()),
                ..EventMetadataV4::default()
            },
        });
    } else if state.transition_beats_remaining > 0 {
        state.transition_beats_remaining -= 1;
        if state.transition_beats_remaining == 0 {
            state.team_state_mut(new_possession).phase = TacticalPhaseV4::InPossession;
            state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::OutOfPossession;
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::TransitionEnded,
                tier: HighlightTier::None,
                seq_id: None,
                side: Some(new_possession),
                actor: None,
                target: None,
                ball_pos,
                metadata: EventMetadataV4::default(),
            });
        }
    } else {
        state.team_state_mut(new_possession).phase = TacticalPhaseV4::InPossession;
        state.team_state_mut(new_possession.opposite()).phase = TacticalPhaseV4::OutOfPossession;
    }

    // Track sustained pressure in attacking half (progress >= 65m)
    let progress = attacking_progress_m(new_possession, ball_pos.xy());
    if progress >= 65.0 {
        let streak = &mut state.team_state_mut(new_possession).sustained_pressure_streak;
        *streak = streak.saturating_add(1);
    } else {
        state.team_state_mut(new_possession).sustained_pressure_streak = 0;
    }

    events
}

/// Computes live tactical commentary flags for the attacking & defending teams.
pub fn compute_tactical_flags(
    state: &MatchStateV4,
    att_side: TeamSideV4,
    game_second: i32,
) -> TacticalMetadataFlags {
    let def_side = att_side.opposite();
    let att_ti = state.active_instructions(att_side);
    let def_ti = state.active_instructions(def_side);

    let att_team = state.team_state(att_side);
    let def_team = state.team_state(def_side);

    let active_att_players = state
        .players
        .iter()
        .filter(|p| p.side == att_side && p.on_pitch && !p.sent_off)
        .count();
    let active_def_players = state
        .players
        .iter()
        .filter(|p| p.side == def_side && p.on_pitch && !p.sent_off)
        .count();

    let sustained_pressure = att_team.sustained_pressure_streak >= 3;
    let high_defensive_line = def_ti.line_height >= 68;
    let leading = att_team.goals > def_team.goals;
    let time_wasting = leading && game_second >= 4200 && att_ti.time_wasting >= 55;
    let ten_men = active_att_players <= 10 || active_def_players <= 10;

    TacticalMetadataFlags {
        sustained_pressure,
        high_defensive_line,
        time_wasting,
        ten_men,
    }
}

/// Applies live tactical metadata flags onto an existing `EventMetadataV4` without overwriting
/// explicitly set fields.
pub fn attach_tactical_flags(meta: &mut EventMetadataV4, flags: TacticalMetadataFlags) {
    if flags.sustained_pressure && meta.sustained_pressure.is_none() {
        meta.sustained_pressure = Some(true);
    }
    if flags.high_defensive_line && meta.high_defensive_line.is_none() {
        meta.high_defensive_line = Some(true);
    }
    if flags.time_wasting && meta.time_wasting.is_none() {
        meta.time_wasting = Some(true);
    }
    if flags.ten_men && meta.ten_men.is_none() {
        meta.ten_men = Some(true);
    }
}

/// Returns effective pressing multiplier during `DefensiveTransition` (counter-pressing)
/// vs settled `OutOfPossession`.
pub fn effective_press_multiplier(phase: TacticalPhaseV4, ti: &TeamInstructionsV4) -> f32 {
    let base = 0.72 + (ti.pressing_intensity as f32 / 100.0) * 0.56;
    match phase {
        TacticalPhaseV4::DefensiveTransition if ti.pressing_intensity >= 62 => base * 1.18,
        TacticalPhaseV4::DefensiveTransition if ti.pressing_intensity <= 40 => base * 0.85,
        _ => base,
    }
}
