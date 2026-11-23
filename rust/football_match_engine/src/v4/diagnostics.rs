//! Read-only V4 attacking-chain diagnostics.
//!
//! Independent from the event storage policy: every emitted event is observed
//! in FullEvent AND FastBatch, so no timeline needs to be retained in memory.
//! No RNG, tactical decision, ball state or event fingerprint is modified.
//!
//! Definitions:
//! - A possession starts with a kickoff, a controlled carrier observation, or
//!   the first attacking action after a restart.
//! - A final-third / box entry is counted at most once per possession, only
//!   where the team controls the ball (not at a shot's goal-line destination).
//! - "Build-up" means >= 2 completed passes in one possession.
//! - "Box without shot" is a useful proxy for a stalled attack, NOT a count of
//!   objectively clear goal-scoring chances.
//! - An error and its defensive follow-up are ONE turnover: PassFailed +
//!   Interception, FirstTouch + LooseBall + SecondBallWon, DribbleLost + TackleWon.
//! This is observational data, not a calibrated evaluation of football realism.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::v4::events::{EventMetadataV4, EventTypeV4};
use crate::v4::spatial::{
    attacking_progress_m, is_in_attacking_penalty_box, TeamSideV4, Vec2, Vec3,
};

pub const DIAGNOSTICS_SCHEMA_VERSION_V4: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassBreakdownV4 {
    pub attempted: u32,
    pub completed: u32,
    pub intercepted: u32,
    pub misplaced: u32,
    pub out_of_play: u32,
    pub offside: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnoverBreakdownV4 {
    pub pass_intercepted: u32,
    pub pass_misplaced: u32,
    pub pass_out_of_play: u32,
    pub first_touch_error: u32,
    pub dribble_tackled: u32,
    pub offside: u32,
    /// A change of controlled carrier with no classified loss event.
    pub other_unclassified: u32,
}

impl TurnoverBreakdownV4 {
    pub fn total(&self) -> u32 {
        self.pass_intercepted
            + self.pass_misplaced
            + self.pass_out_of_play
            + self.first_touch_error
            + self.dribble_tackled
            + self.offside
            + self.other_unclassified
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamAttackDiagnosticsV4 {
    pub possessions: u32,
    pub controlled_ball_samples: u32,
    pub build_up_possessions: u32,
    pub final_third_possessions: u32,
    pub penalty_area_possessions: u32,
    pub penalty_area_without_shot: u32,
    pub possessions_with_shot: u32,
    pub passes_in_shot_possessions: u32,
    pub passes_attempted: u32,
    pub passes_completed: u32,
    pub passes_under_high_pressure: u32,
    pub failed_passes_under_high_pressure: u32,
    pub shots: u32,
    pub shots_on_target: u32,
    pub shots_off_target: u32,
    pub shots_blocked: u32,
    pub shots_from_box: u32,
    pub goals: u32,
    pub shot_xg: f32,
    pub clearances_ending_possession: u32,
    pub turnovers: TurnoverBreakdownV4,
    pub passes_by_type: BTreeMap<String, PassBreakdownV4>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDiagnosticsV4 {
    pub schema_version: u32,
    pub home: TeamAttackDiagnosticsV4,
    pub away: TeamAttackDiagnosticsV4,
}

impl MatchDiagnosticsV4 {
    pub fn validate_basic(&self) -> Result<(), String> {
        for (side, team) in [("HOME", &self.home), ("AWAY", &self.away)] {
            if team.passes_completed > team.passes_attempted {
                return Err(format!("{side}: completed passes exceed attempts"));
            }
            if team.shots != team.shots_blocked + team.shots_off_target + team.shots_on_target {
                return Err(format!("{side}: shot breakdown inconsistent"));
            }
            if team.final_third_possessions > team.possessions
                || team.penalty_area_possessions > team.final_third_possessions
                || team.possessions_with_shot > team.possessions
                || team.penalty_area_without_shot > team.penalty_area_possessions
                || team.build_up_possessions > team.possessions
            {
                return Err(format!("{side}: attacking funnel counters inconsistent"));
            }
            if !team.shot_xg.is_finite() || team.shot_xg < 0.0 {
                return Err(format!("{side}: invalid shot xG"));
            }
            let attempted: u32 = team.passes_by_type.values().map(|p| p.attempted).sum();
            let completed: u32 = team.passes_by_type.values().map(|p| p.completed).sum();
            if attempted != team.passes_attempted || completed != team.passes_completed {
                return Err(format!("{side}: per-type passing breakdown inconsistent"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ActivePossession {
    side: TeamSideV4,
    completed_passes: u32,
    reached_final_third: bool,
    reached_box: bool,
    had_shot: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MatchDiagnosticsCollectorV4 {
    home: TeamAttackDiagnosticsV4,
    away: TeamAttackDiagnosticsV4,
    active: Option<ActivePossession>,
}

impl MatchDiagnosticsCollectorV4 {
    fn team_mut(&mut self, side: TeamSideV4) -> &mut TeamAttackDiagnosticsV4 {
        match side {
            TeamSideV4::Home => &mut self.home,
            TeamSideV4::Away => &mut self.away,
        }
    }

    fn close_active(&mut self) {
        if let Some(active) = self.active.take() {
            let team = self.team_mut(active.side);
            if active.completed_passes >= 2 {
                team.build_up_possessions += 1;
            }
            if active.had_shot {
                team.possessions_with_shot += 1;
                team.passes_in_shot_possessions += active.completed_passes;
            } else if active.reached_box {
                team.penalty_area_without_shot += 1;
            }
        }
    }

    fn open(&mut self, side: TeamSideV4) {
        if self.active.as_ref().map(|p| p.side) == Some(side) {
            return;
        }
        if let Some((old_side, false)) = self.active.as_ref().map(|p| (p.side, p.had_shot)) {
            self.team_mut(old_side).turnovers.other_unclassified += 1;
        }
        self.close_active();
        self.team_mut(side).possessions += 1;
        self.active = Some(ActivePossession {
            side,
            completed_passes: 0,
            reached_final_third: false,
            reached_box: false,
            had_shot: false,
        });
    }

    fn observe_territory(&mut self, side: TeamSideV4, pos: Vec2) {
        self.open(side);
        let reached_box = is_in_attacking_penalty_box(side, pos);
        let reached_third = attacking_progress_m(side, pos) >= 70.0 || reached_box;
        let (new_third, new_box) = {
            let active = self.active.as_mut().expect("possession just opened");
            let new_third = reached_third && !active.reached_final_third;
            let new_box = reached_box && !active.reached_box;
            if new_third { active.reached_final_third = true; }
            if new_box { active.reached_box = true; }
            (new_third, new_box)
        };
        let team = self.team_mut(side);
        if new_third { team.final_third_possessions += 1; }
        if new_box { team.penalty_area_possessions += 1; }
    }

    /// Called once per play beat for the team's actual controlled carrier.
    /// No time-consuming tracing or storage of position history.
    pub fn observe_controlled_ball(&mut self, side: TeamSideV4, pos: Vec2) {
        self.observe_territory(side, pos);
        self.team_mut(side).controlled_ball_samples += 1;
    }

    fn record_pass(&mut self, side: TeamSideV4, metadata: &EventMetadataV4, completed: bool) {
        self.open(side);
        let pass_type = metadata.pass_type.as_deref().unwrap_or("UNKNOWN");
        let high_pressure = metadata.pressure.unwrap_or(0) >= 50;
        let outcome = metadata.outcome.as_deref().unwrap_or("");
        let team = self.team_mut(side);
        team.passes_attempted += 1;
        if completed { team.passes_completed += 1; }
        if high_pressure {
            team.passes_under_high_pressure += 1;
            if !completed { team.failed_passes_under_high_pressure += 1; }
        }
        let by_type = team.passes_by_type.entry(pass_type.to_owned()).or_default();
        by_type.attempted += 1;
        if completed {
            by_type.completed += 1;
        } else {
            match outcome {
                "INTERCEPTED" => by_type.intercepted += 1,
                "OUT_OF_PLAY" => by_type.out_of_play += 1,
                "OFFSIDE_INVOLVEMENT" => by_type.offside += 1,
                _ => by_type.misplaced += 1,
            }
        }
        if completed {
            if let Some(active) = self.active.as_mut() {
                active.completed_passes += 1;
            }
        }
    }

    fn lose_ball(&mut self, side: TeamSideV4, reason: &str) {
        self.open(side);
        let breakdown = &mut self.team_mut(side).turnovers;
        match reason {
            "INTERCEPTED" => breakdown.pass_intercepted += 1,
            "OUT_OF_PLAY" => breakdown.pass_out_of_play += 1,
            "FIRST_TOUCH_ERROR" => breakdown.first_touch_error += 1,
            "DRIBBLE_TACKLED" => breakdown.dribble_tackled += 1,
            "OFFSIDE" => breakdown.offside += 1,
            _ => breakdown.pass_misplaced += 1,
        }
        self.close_active();
    }

    /// Intercepts the single authoritative emitter, including FastBatch.
    /// Defensive follow-up events are deliberately not counted as new losses.
    pub fn on_event(
        &mut self,
        kind: EventTypeV4,
        side: Option<TeamSideV4>,
        ball_pos: Vec3,
        metadata: &EventMetadataV4,
    ) {
        if matches!(kind, EventTypeV4::HalfTime | EventTypeV4::ExtraTimeHalfTime | EventTypeV4::FullTime) {
            self.close_active();
            return;
        }
        let Some(side) = side else { return };
        match kind {
            EventTypeV4::Kickoff => {
                // A goal or previous half never belongs to this new possession.
                self.close_active();
                self.observe_territory(side, ball_pos.xy());
            }
            EventTypeV4::PassCompleted | EventTypeV4::ThroughBall | EventTypeV4::Cross => {
                self.record_pass(side, metadata, true);
                self.observe_territory(side, ball_pos.xy());
            }
            EventTypeV4::PassFailed => {
                self.record_pass(side, metadata, false);
                self.lose_ball(side, metadata.outcome.as_deref().unwrap_or("MISPLACED"));
            }
            EventTypeV4::Offside => {
                // Law 11: offside IS an attempted pass but not PassFailed.
                self.record_pass(side, metadata, false);
                self.lose_ball(side, "OFFSIDE");
            }
            EventTypeV4::FirstTouch if metadata.outcome.as_deref() == Some("HEAVY_TOUCH_LOST") => {
                self.lose_ball(side, "FIRST_TOUCH_ERROR");
            }
            EventTypeV4::FirstTouch | EventTypeV4::DribbleWon | EventTypeV4::Carry => {
                self.observe_territory(side, ball_pos.xy());
            }
            EventTypeV4::DribbleLost => {
                self.lose_ball(side, "DRIBBLE_TACKLED");
            }
            EventTypeV4::Clearance => {
                self.open(side);
                self.team_mut(side).clearances_ending_possession += 1;
                self.close_active();
            }
            EventTypeV4::Shot | EventTypeV4::ShotOnTarget | EventTypeV4::ShotOffTarget
                | EventTypeV4::Header | EventTypeV4::PenaltyKick => {
                self.open(side);
                if metadata.in_box == Some(true) {
                    // The event position is often the GOAL, not the shooter.
                    // Metadata describes the original shooting location.
                    // This is a box possession even if the play originated at a restart.
                    let (new_third, new_box) = {
                        let active = self.active.as_mut().expect("possession just opened");
                        let nt = !active.reached_final_third;
                        let nb = !active.reached_box;
                        active.reached_final_third = true;
                        active.reached_box = true;
                        (nt, nb)
                    };
                    let team = self.team_mut(side);
                    if new_third { team.final_third_possessions += 1; }
                    if new_box { team.penalty_area_possessions += 1; }
                }
                if let Some(active) = self.active.as_mut() { active.had_shot = true; }
                let team = self.team_mut(side);
                team.shots += 1;
                if metadata.in_box == Some(true) { team.shots_from_box += 1; }
                team.shot_xg += metadata.xg.unwrap_or(0.0);
                match kind {
                    EventTypeV4::Shot => team.shots_blocked += 1,
                    EventTypeV4::ShotOffTarget | EventTypeV4::Header => team.shots_off_target += 1,
                    _ => team.shots_on_target += 1,
                }
            }
            EventTypeV4::Goal | EventTypeV4::OwnGoal => {
                self.open(side);
                self.team_mut(side).goals += 1;
                self.close_active();
            }
            // Interception/TackleWon/LooseBall/SecondBallWon follow the
            // offensive loss above; counting them would inflate turnovers.
            _ => {}
        }
    }

    pub fn finish(&mut self) -> MatchDiagnosticsV4 {
        self.close_active();
        MatchDiagnosticsV4 {
            schema_version: DIAGNOSTICS_SCHEMA_VERSION_V4,
            home: self.home.clone(),
            away: self.away.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v4::input::{RulesModeV4, SimulationMode};
    use crate::v4::session::simulate_v4;
    use crate::v4::tests::make_v4_input;

    #[test]
    fn one_turnover_per_failed_pass_and_defender_followup() {
        let mut c = MatchDiagnosticsCollectorV4::default();
        c.on_event(EventTypeV4::Kickoff, Some(TeamSideV4::Home), Vec3::center_spot(), &EventMetadataV4::default());
        let meta = EventMetadataV4 {
            pass_type: Some("SHORT".into()),
            outcome: Some("INTERCEPTED".into()),
            ..EventMetadataV4::default()
        };
        c.on_event(EventTypeV4::PassFailed, Some(TeamSideV4::Home), Vec3::ground(65.0, 34.0), &meta);
        c.on_event(EventTypeV4::Interception, Some(TeamSideV4::Away), Vec3::ground(65.0, 34.0), &meta);
        let report = c.finish();
        assert_eq!(report.home.possessions, 1);
        assert_eq!(report.home.turnovers.total(), 1);
        assert_eq!(report.home.turnovers.pass_intercepted, 1);
        assert_eq!(report.home.passes_attempted, 1);
        assert_eq!(report.away.possessions, 0);
        report.validate_basic().unwrap();
    }

    #[test]
    fn distinct_box_entry_once_and_stalled_attack() {
        let mut c = MatchDiagnosticsCollectorV4::default();
        c.observe_controlled_ball(TeamSideV4::Home, Vec2::new(52.5, 34.0));
        c.observe_controlled_ball(TeamSideV4::Home, Vec2::new(80.0, 34.0));
        c.observe_controlled_ball(TeamSideV4::Home, Vec2::new(92.0, 34.0));
        c.observe_controlled_ball(TeamSideV4::Home, Vec2::new(94.0, 34.0));
        c.observe_controlled_ball(TeamSideV4::Away, Vec2::new(30.0, 34.0));
        let report = c.finish();
        assert_eq!(report.home.final_third_possessions, 1);
        assert_eq!(report.home.penalty_area_possessions, 1);
        assert_eq!(report.home.penalty_area_without_shot, 1);
        assert_eq!(report.home.turnovers.other_unclassified, 1);
        assert_eq!(report.away.final_third_possessions, 1);
        report.validate_basic().unwrap();
    }

    #[test]
    fn fast_batch_and_full_event_diagnostics_match() {
        let fast = simulate_v4(make_v4_input(90410, SimulationMode::FastBatch, RulesModeV4::League)).unwrap();
        let full = simulate_v4(make_v4_input(90410, SimulationMode::FullEvent, RulesModeV4::League)).unwrap();
        assert_eq!(fast.diagnostics, full.diagnostics);
        assert_eq!(fast.event_timeline_fingerprint, full.event_timeline_fingerprint);
        assert!(fast.events.is_empty());
        let d = full.diagnostics.expect("diagnostics exported to match JSON");
        d.validate_basic().unwrap();
        assert_eq!(d.home.shots, full.summary.home.shots as u32);
        assert_eq!(d.away.shots, full.summary.away.shots as u32);
        assert_eq!(d.home.goals, full.summary.home.goals as u32);
        assert_eq!(d.away.goals, full.summary.away.goals as u32);
        assert_eq!(d.home.passes_attempted, full.summary.home.passes_attempted as u32);
        assert_eq!(d.away.passes_attempted, full.summary.away.passes_attempted as u32);
    }
}
