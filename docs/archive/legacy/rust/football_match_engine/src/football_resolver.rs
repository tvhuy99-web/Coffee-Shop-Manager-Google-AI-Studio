use crate::{
    decision::Action,
    duel_resolver,
    events::EventType,
    movement,
    pass_resolver,
    possession_decision,
    pressure,
    resolver_support::{kv, MatchRuntime},
    set_piece,
    shot_resolver,
    state::{BallControl, RestartType, TacticalPhase},
};

pub fn step(c: &mut dyn MatchRuntime) {
    if c.state().restart.kind != RestartType::None {
        set_piece::resolve_restart(c);
        return;
    }
    match c.state().ball.control {
        BallControl::Loose => {
            duel_resolver::resolve_loose_ball(c);
        }
        BallControl::Airborne => {
            duel_resolver::resolve_airborne(c);
        }
        BallControl::Controlled => {
            resolve_controlled(c);
        }
    }
}

fn resolve_controlled(c: &mut dyn MatchRuntime) {
    let (side, actor) = match (c.state().ball.owner_team, c.state().ball.owner_player_id.clone()) {
        (Some(s), Some(a)) => (s, a),
        _ => return,
    };
    if !c.state().player(&actor).map(|r| r.on_pitch).unwrap_or(false) {
        return;
    }
    update_phase(c, true);
    settle_transition(c);
    {
        let input = c.input().clone();
        movement::plan_targets(&input, c.state_mut());
    }
    let pressure_val = {
        let input = c.input().clone();
        pressure::apply_to_ball_carrier(&input, c.state_mut())
    };
    if pressure_val > 0 {
        let phase_name = c.state().possession.phase.as_str();
        let meta = vec![kv("pressure", pressure_val), kv("phase", phase_name)];
        c.emit(EventType::PressureApplied, Some(side.opposite()), None, Some(&actor), meta.clone());
        c.emit(EventType::Press, Some(side.opposite()), None, Some(&actor), meta.clone());
        if pressure_val >= 28 && c.state().team(side.opposite()).team_instructions.pressing_intensity >= 55 {
            c.emit(EventType::Press, Some(side.opposite()), None, Some(&actor), meta);
        }
    }
    let decision = {
        let input = c.input().clone();
        let state_snap = c.state().clone();
        match possession_decision::choose_action(&input, &state_snap, c.rng_mut(), pressure_val) {
            Ok(d) => d,
            Err(_) => return,
        }
    };
    match decision.kind {
        Action::Pass | Action::ThroughBall => pass_resolver::resolve_pass(c, &decision, pressure_val),
        Action::Carry => duel_resolver::resolve_carry(c, &decision, pressure_val),
        Action::Dribble => duel_resolver::resolve_dribble(c, &decision, pressure_val),
        Action::Cross => duel_resolver::resolve_cross(c, &decision, pressure_val),
        Action::Shot => shot_resolver::resolve_shot(c, side, &actor, false, pressure_val, "OPEN_PLAY"),
    }
    update_phase(c, true);
    settle_transition(c);
}

fn update_phase(c: &mut dyn MatchRuntime, emit: bool) {
    let side = match c.state().possession.controlling_team {
        Some(s) => s,
        None => return,
    };
    let old = c.state().possession.phase;
    let next = TacticalPhase::from_ball(side, c.state().ball.y);
    if next == old {
        return;
    }
    c.state_mut().possession.phase = next;
    c.state_mut().team_mut(side).record_phase_entry(next);
    if emit {
        let owner = c.state().ball.owner_player_id.clone();
        c.emit(
            EventType::TacticalPhaseChanged,
            Some(side),
            owner.as_deref(),
            None,
            vec![kv("from", old.as_str()), kv("to", next.as_str())],
        );
    }
}

fn settle_transition(c: &mut dyn MatchRuntime) {
    if !c.state().possession.transition_active {
        return;
    }
    let elapsed = c.state().clock.game_second - c.state().possession.transition_started_at_second;
    let max_elapsed = if c.input().simulated_action_minutes() < 90 { 28 } else { 15 };
    if c.state().possession.completed_passes_in_sequence < 2 && elapsed < max_elapsed {
        return;
    }
    let controlling = c.state().possession.controlling_team;
    let owner = c.state().ball.owner_player_id.clone();
    c.state_mut().possession.settle_transition();
    c.emit(
        EventType::TransitionEnded,
        controlling,
        owner.as_deref(),
        None,
        vec![kv("elapsedSeconds", elapsed)],
    );
}
