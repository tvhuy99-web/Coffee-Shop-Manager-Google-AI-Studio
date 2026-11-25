use crate::{
    commands::MatchCommand,
    events::EventType,
    geometry::{self, TeamSide},
    input::MatchInput,
    resolver_support::{kv, MatchRuntime},
    state::{MatchState, RestartType},
};

pub fn handle_foul(c: &mut dyn MatchRuntime, offender: &str, victim: &str, advantage: bool) {
    let (foul_side, attack, vx, vy) = {
        let s = c.state();
        let or = match s.player(offender) {
            Some(r) => r,
            None => return,
        };
        let vr = match s.player(victim) {
            Some(r) => r,
            None => return,
        };
        (or.side, vr.side, vr.x, vr.y)
    };
    c.state_mut().team_mut(foul_side).fouls += 1;
    c.emit(
        EventType::Foul,
        Some(foul_side),
        Some(offender),
        Some(victim),
        vec![kv("x", vx), kv("y", vy)],
    );
    c.add_stoppage(8);
    issue_card(c, offender, victim);
    let aggr = c.state().team(foul_side).team_instructions.aggression;
    maybe_injury(c, victim, offender, aggr);
    let victim_on_pitch = c.state().player(victim).map(|r| r.on_pitch).unwrap_or(false);
    if advantage && victim_on_pitch {
        c.emit(EventType::AdvantagePlayed, Some(attack), Some(victim), Some(offender), Vec::new());
        return;
    }
    if geometry::in_attacking_penalty_area(attack, vx, vy) {
        c.state_mut().team_mut(attack).penalties += 1;
        c.emit(
            EventType::PenaltyAwarded,
            Some(attack),
            Some(victim),
            Some(offender),
            vec![kv("cause", "FOUL_IN_BOX")],
        );
        c.schedule_restart(
            RestartType::Penalty,
            attack,
            "FOUL_IN_BOX",
            50,
            geometry::penalty_spot_y(attack),
        );
    } else {
        c.state_mut().team_mut(attack).free_kicks += 1;
        c.schedule_restart(RestartType::FreeKick, attack, "FOUL", vx, vy);
    }
}

fn issue_card(c: &mut dyn MatchRuntime, offender: &str, victim: &str) {
    let (side, acute_fatigue, yellow_cards, aggression, temperament) = {
        let s = c.state();
        let r = match s.player(offender) {
            Some(r) => r,
            None => return,
        };
        let p = match c.input().team(r.side).player(offender) {
            Some(p) => p,
            None => return,
        };
        (
            r.side,
            r.acute_fatigue,
            r.yellow_cards,
            s.team(r.side).team_instructions.aggression,
            p.attributes.get(19) as i32,
        )
    };
    let roll = c.rng_mut().next_int(46);
    let severity = 24 + (aggression - 50) / 6 + (74 - temperament) / 4 + acute_fatigue / 5 + roll;
    let direct_red = severity >= 94 || (severity >= 72 && c.rng_mut().next_int(100) < 2);
    if direct_red {
        if let Some(r) = c.state_mut().player_mut(offender) {
            r.set_red_card(true);
        }
        c.state_mut().team_mut(side).red_cards += 1;
        c.emit(
            EventType::RedCard,
            Some(side),
            Some(offender),
            Some(victim),
            vec![kv("direct", true), kv("severity", severity)],
        );
        c.add_stoppage(20);
        c.ensure_goalkeeper(side, "RED_CARD");
    } else {
        let yellow_threshold = if yellow_cards > 0 { 72 } else { 66 };
        if severity >= yellow_threshold {
            let new_yellows = {
                let r = c.state_mut().player_mut(offender).unwrap();
                r.yellow_cards += 1;
                r.yellow_cards
            };
            c.state_mut().team_mut(side).yellow_cards += 1;
            c.emit(
                EventType::YellowCard,
                Some(side),
                Some(offender),
                Some(victim),
                vec![kv("yellowCount", new_yellows), kv("severity", severity)],
            );
            c.add_stoppage(12);
            if new_yellows >= 2 {
                if let Some(r) = c.state_mut().player_mut(offender) {
                    r.set_red_card(true);
                }
                c.state_mut().team_mut(side).red_cards += 1;
                c.emit(
                    EventType::RedCard,
                    Some(side),
                    Some(offender),
                    Some(victim),
                    vec![kv("secondYellow", true)],
                );
                c.ensure_goalkeeper(side, "SECOND_YELLOW");
            }
        }
    }
}

pub fn maybe_injury(c: &mut dyn MatchRuntime, player_id: &str, collider_id: &str, aggression: i32) {
    let (side, acute_fatigue, condition, ix, iy, was_owner) = {
        let s = c.state();
        let r = match s.player(player_id) {
            Some(r) if r.on_pitch => r,
            _ => return,
        };
        let p = match c.input().team(r.side).player(player_id) {
            Some(p) => p,
            None => return,
        };
        let was_owner = s.ball.owner_player_id.as_deref() == Some(player_id);
        (r.side, r.acute_fatigue, p.condition, r.x, r.y, was_owner)
    };
    let hazard = 1 + acute_fatigue / 12 + (60 - condition).max(0) / 12 + aggression / 20;
    if c.rng_mut().next_int(5000) < hazard {
        if let Some(r) = c.state_mut().player_mut(player_id) {
            r.remove_from_pitch_injured();
        }
        if was_owner {
            c.state_mut()
                .ball
                .set_loose(ix, iy, Some(side), Some(player_id.into()), Some("INJURY_LOOSE"));
        }
        c.state_mut().team_mut(side).injuries += 1;
        c.emit(
            EventType::Injury,
            Some(side),
            Some(player_id),
            Some(collider_id),
            vec![kv("unableToContinue", true), kv("hazard", hazard)],
        );
        let stop = 45 + c.rng_mut().next_int(61);
        c.add_stoppage(stop);
        c.ensure_goalkeeper(side, "INJURY");
    }
}

pub fn resolve_goalkeeper_continuity(input: &MatchInput, state: &MatchState, side: TeamSide) -> Option<MatchCommand> {
    if !state.team(side).requires_goalkeeper_assignment {
        return None;
    }
    let team = input.team(side);
    let mut best_id: Option<String> = None;
    let mut best_score = i32::MIN;
    for (_, p) in &state.players {
        if p.side != side || !p.on_pitch || p.red_card || p.injured {
            continue;
        }
        let snap = match team.player(&p.player_id) {
            Some(s) => s,
            None => continue,
        };
        let a = &snap.attributes;
        let score = a.get(0) as i32 * 5 + a.get(9) as i32 + a.get(13) as i32 + a.get(14) as i32;
        if best_id.is_none()
            || score > best_score
            || (score == best_score && p.player_id < *best_id.as_ref().unwrap())
        {
            best_id = Some(p.player_id.clone());
            best_score = score;
        }
    }
    let id = best_id?;
    MatchCommand::assign_goalkeeper(state.clock.game_second, side, id).ok()
}
