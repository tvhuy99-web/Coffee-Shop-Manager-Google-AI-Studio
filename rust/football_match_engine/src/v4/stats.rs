//! V4 P2 result/statistics integrity checks independent from the live counters.
//! A FastBatch result has no event list and is checked via counter-only invariants.
use crate::v4::events::{EventTypeV4, MatchEventV4, MatchResultV4, MatchSummaryV4, TeamMatchSummaryV4};
use crate::v4::spatial::TeamSideV4;

pub fn validate_summary(summary: &MatchSummaryV4) -> Result<(), String> {
    for (label, team) in [("HOME", &summary.home), ("AWAY", &summary.away)] {
        if team.shots != team.shots_on_target + team.shots_off_target + team.shots_blocked {
            return Err(format!("{label}: shot breakdown inconsistent: shots={} on={} off={} blocked={}",
                team.shots, team.shots_on_target, team.shots_off_target, team.shots_blocked));
        }
        if team.passes_completed > team.passes_attempted { return Err(format!("{label}: impossible passing counts")); }
        if team.own_goals_for > team.goals { return Err(format!("{label}: own goals awarded exceed team score")); }
        if team.saves > summary.opponent(label).shots_on_target {
            return Err(format!("{label}: saves greater than opponent shots on target"));
        }
        if !team.possession_pct.is_finite() || !(0.0..=100.0).contains(&team.possession_pct)
            || !team.xg.is_finite() || team.xg < 0.0 {
            return Err(format!("{label}: non-finite possession or xG"));
        }
    }
    if summary.home.own_goals_for != summary.away.own_goals_conceded
        || summary.away.own_goals_for != summary.home.own_goals_conceded {
        return Err("own-goal cross-team attribution mismatch".into());
    }
    let home_goals: usize = summary.players.iter()
        .filter(|p| p.side == TeamSideV4::Home).map(|p| p.goals as usize).sum();
    let away_goals: usize = summary.players.iter()
        .filter(|p| p.side == TeamSideV4::Away).map(|p| p.goals as usize).sum();
    if home_goals + summary.home.own_goals_for as usize != summary.home.goals as usize
        || away_goals + summary.away.own_goals_for as usize != summary.away.goals as usize {
        return Err("player goals plus opponent own-goals do not equal final score".into());
    }
    let home_ogs: usize = summary.players.iter()
        .filter(|p| p.side == TeamSideV4::Home).map(|p| p.own_goals as usize).sum();
    let away_ogs: usize = summary.players.iter()
        .filter(|p| p.side == TeamSideV4::Away).map(|p| p.own_goals as usize).sum();
    if home_ogs != summary.home.own_goals_conceded as usize
        || away_ogs != summary.away.own_goals_conceded as usize {
        return Err("own-goal scorer records inconsistent".into());
    }
    let possession_total = summary.home.possession_pct + summary.away.possession_pct;
    if (possession_total - 100.0).abs() > 0.2 { return Err("possession percentages do not sum to 100".into()); }
    Ok(())
}

trait OppositeTeam {
    fn opponent(&self, label: &str) -> &TeamMatchSummaryV4;
}
impl OppositeTeam for MatchSummaryV4 {
    fn opponent(&self, label: &str) -> &TeamMatchSummaryV4 {
        if label == "HOME" { &self.away } else { &self.home }
    }
}

#[derive(Default, Clone, Copy)]
struct FromEvents {
    goals: u16, own_goals_for: u16, own_goals_conceded: u16,
    shots: u16, shots_on_target: u16, shots_off_target: u16,
    shots_blocked: u16, saves: u16, passes_attempted: u16, passes_completed: u16,
}
fn side_counters<'a>(home: &'a mut FromEvents, away: &'a mut FromEvents, side: TeamSideV4) -> &'a mut FromEvents {
    match side { TeamSideV4::Home => home, TeamSideV4::Away => away }
}
fn record_event(home: &mut FromEvents, away: &mut FromEvents, ev: &MatchEventV4) {
    let Some(side) = ev.side else { return };
    let ours = side_counters(home, away, side);
    match ev.kind {
        EventTypeV4::Goal => ours.goals += 1,
        EventTypeV4::OwnGoal => {
            ours.goals += 1;
            ours.own_goals_for += 1;
            side_counters(home, away, side.opposite()).own_goals_conceded += 1;
        }
        EventTypeV4::Shot => { ours.shots += 1; ours.shots_blocked += 1; }
        EventTypeV4::ShotOnTarget => { ours.shots += 1; ours.shots_on_target += 1; }
        EventTypeV4::ShotOffTarget | EventTypeV4::Header => {
            ours.shots += 1; ours.shots_off_target += 1;
        }
        EventTypeV4::PenaltyKick => { ours.shots += 1; ours.shots_on_target += 1; }
        EventTypeV4::Save => ours.saves += 1,
        EventTypeV4::PassCompleted | EventTypeV4::ThroughBall | EventTypeV4::Cross => {
            ours.passes_attempted += 1; ours.passes_completed += 1;
        }
        EventTypeV4::PassFailed => ours.passes_attempted += 1,
        // An offside offence is an attempted pass terminated by the restart;
        // do not emit a fake PassFailed event, which would violate Law 11 tests.
        EventTypeV4::Offside => ours.passes_attempted += 1,
        _ => {}
    }
}

/// FullEvent integrity audit: event-derived score/shot/pass/save counts must
/// equal the authoritative final summary. FastBatch uses validate_summary only.
pub fn audit_result(result: &MatchResultV4) -> Result<(), String> {
    validate_summary(&result.summary)?;
    // Works even in FastBatch: diagnostics are built by the same event emitter,
    // without retaining a complete event timeline.
    if let Some(d) = &result.diagnostics {
        d.validate_basic()?;
        for (label, diagnostic, summary) in [
            ("HOME", &d.home, &result.summary.home),
            ("AWAY", &d.away, &result.summary.away),
        ] {
            if diagnostic.shots != summary.shots as u32
                || diagnostic.shots_on_target != summary.shots_on_target as u32
                || diagnostic.shots_off_target != summary.shots_off_target as u32
                || diagnostic.shots_blocked != summary.shots_blocked as u32
                || diagnostic.goals != summary.goals as u32
                || diagnostic.passes_attempted != summary.passes_attempted as u32
                || diagnostic.passes_completed != summary.passes_completed as u32
            {
                return Err(format!("{label}: attacking diagnostics differ from authoritative summary"));
            }
        }
    }
    if result.events.is_empty() { return Ok(()); }
    if result.events.len() != result.event_count as usize { return Err("event count mismatch".into()); }
    let (mut home, mut away) = (FromEvents::default(), FromEvents::default());
    for ev in &result.events { record_event(&mut home, &mut away, ev); }
    for (side, actual, inferred) in [("HOME", &result.summary.home, home), ("AWAY", &result.summary.away, away)] {
        let fields = [
            ("goals", actual.goals, inferred.goals),
            ("own_goals_for", actual.own_goals_for, inferred.own_goals_for),
            ("own_goals_conceded", actual.own_goals_conceded, inferred.own_goals_conceded),
            ("shots", actual.shots, inferred.shots),
            ("shots_on_target", actual.shots_on_target, inferred.shots_on_target),
            ("shots_off_target", actual.shots_off_target, inferred.shots_off_target),
            ("shots_blocked", actual.shots_blocked, inferred.shots_blocked),
            ("saves", actual.saves, inferred.saves),
            ("passes_attempted", actual.passes_attempted, inferred.passes_attempted),
            ("passes_completed", actual.passes_completed, inferred.passes_completed),
        ];
        for (field, runtime, events) in fields {
            if runtime != events { return Err(format!("{side} {field}: summary={runtime}, events={events}")); }
        }
    }
    Ok(())
}
