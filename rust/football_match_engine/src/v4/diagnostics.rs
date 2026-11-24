//! Read-only V4 attacking-chain & DES v1.4 diagnostics.
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
fn is_zero_u32(value: &u32) -> bool { *value == 0 }

/// Summary distribution (milliseconds) for dead-ball durations or DES intervals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurationDistributionMs {
    pub count: u32,
    pub min_ms: i64,
    pub median_ms: i64,
    pub p90_ms: i64,
    pub max_ms: i64,
}

impl DurationDistributionMs {
    /// Computes `(count, min_ms, median_ms, p90_ms, max_ms)` in-place from raw millisecond samples.
    pub fn from_samples(samples: &mut [i64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_unstable();
        let n = samples.len();
        let min_ms = samples[0];
        let max_ms = samples[n - 1];
        let median_ms = if n % 2 == 1 {
            samples[n / 2]
        } else {
            let lo = samples[n / 2 - 1];
            let hi = samples[n / 2];
            lo + (hi - lo) / 2
        };
        // Nearest-rank 90th percentile index: ceil(0.90 * n) - 1, clamped to [0, n - 1]
        let p90_idx = (((n * 90) + 99) / 100).saturating_sub(1).min(n - 1);
        let p90_ms = samples[p90_idx];
        Self {
            count: n as u32,
            min_ms,
            median_ms,
            p90_ms,
            max_ms,
        }
    }

    /// Aggregates multiple per-match `DurationDistributionMs` summaries into a batch-level
    /// distribution (exact `count`/`min_ms`/`max_ms`, and median/p90 across match summaries).
    pub fn merge_summaries(dists: &[DurationDistributionMs]) -> Self {
        let mut total_count = 0u32;
        let mut min_ms = i64::MAX;
        let mut max_ms = i64::MIN;
        let mut medians = Vec::new();
        let mut p90s = Vec::new();

        for d in dists {
            if d.count == 0 {
                continue;
            }
            total_count = total_count.saturating_add(d.count);
            if d.min_ms < min_ms {
                min_ms = d.min_ms;
            }
            if d.max_ms > max_ms {
                max_ms = d.max_ms;
            }
            medians.push(d.median_ms);
            p90s.push(d.p90_ms);
        }

        if total_count == 0 {
            return Self::default();
        }

        let med_dist = Self::from_samples(&mut medians);
        let p90_dist = Self::from_samples(&mut p90s);
        Self {
            count: total_count,
            min_ms,
            median_ms: med_dist.median_ms,
            p90_ms: p90_dist.p90_ms,
            max_ms,
        }
    }
}

/// Categories of dead-ball stoppages tracked in `DeadBallDistributionsV4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeadBallDiagKind {
    GoalCelebration,
    ThrowIn,
    Corner,
    GoalKick,
    FreeKick,
    Penalty,
    Injury,
    Substitution,
}

/// Per-category dead-ball duration distributions for DES v1.4 (Section X).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadBallDistributionsV4 {
    pub goal_celebration: DurationDistributionMs,
    pub throw_in: DurationDistributionMs,
    pub corner: DurationDistributionMs,
    pub goal_kick: DurationDistributionMs,
    pub free_kick: DurationDistributionMs,
    pub penalty: DurationDistributionMs,
    pub injury: DurationDistributionMs,
    pub substitution: DurationDistributionMs,
}

/// DES v1.4 match-level timing, event-density, and tactical-delay diagnostics (Section X).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesMatchDiagnosticsV4 {
    pub ball_in_play_ms: i64,
    pub ball_out_of_play_ms: i64,
    pub compensable_lost_ms: i64,
    pub announced_added_ms: i64,
    pub additional_added_ms: i64,
    pub des_steps_total: u64,
    pub early_interceptions_detected: u32,
    pub one_touch_passes: u32,
    pub dead_ball_distributions: DeadBallDistributionsV4,
    pub avg_player_distance_m: f32,
    pub tactical_commands_delayed: u32,
}

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
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub pass_contacts_loose: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub pass_loose_regained: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub pass_loose_lost: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDiagnosticsV4 {
    pub schema_version: u32,
    pub home: TeamAttackDiagnosticsV4,
    pub away: TeamAttackDiagnosticsV4,
    #[serde(default)]
    pub des: DesMatchDiagnosticsV4,
}

impl MatchDiagnosticsV4 {
    pub fn validate_basic(&self) -> Result<(), String> {
        if self.schema_version != DIAGNOSTICS_SCHEMA_VERSION_V4 {
            return Err("unsupported attacking diagnostics schema version".into());
        }
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
        if self.des.ball_in_play_ms < 0
            || self.des.ball_out_of_play_ms < 0
            || self.des.compensable_lost_ms < 0
            || self.des.announced_added_ms < 0
            || self.des.additional_added_ms < 0
        {
            return Err("DES diagnostics contain negative duration".into());
        }
        if !self.des.avg_player_distance_m.is_finite() || self.des.avg_player_distance_m < 0.0 {
            return Err("DES diagnostics contain invalid avg_player_distance_m".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActivePossession {
    side: TeamSideV4,
    completed_passes: u32,
    reached_final_third: bool,
    reached_box: bool,
    had_shot: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DesDiagnosticsAccumulator {
    ball_in_play_ms: i64,
    ball_out_of_play_ms: i64,
    compensable_lost_ms: i64,
    announced_added_ms: i64,
    additional_added_ms: i64,
    des_steps_total: u64,
    early_interceptions_detected: u32,
    one_touch_passes: u32,
    avg_player_distance_m: f32,
    tactical_commands_delayed: u32,
    goal_celebration_samples: Vec<i64>,
    throw_in_samples: Vec<i64>,
    corner_samples: Vec<i64>,
    goal_kick_samples: Vec<i64>,
    free_kick_samples: Vec<i64>,
    penalty_samples: Vec<i64>,
    injury_samples: Vec<i64>,
    substitution_samples: Vec<i64>,
}

impl DesDiagnosticsAccumulator {
    fn finalize(&mut self) -> DesMatchDiagnosticsV4 {
        DesMatchDiagnosticsV4 {
            ball_in_play_ms: self.ball_in_play_ms,
            ball_out_of_play_ms: self.ball_out_of_play_ms,
            compensable_lost_ms: self.compensable_lost_ms,
            announced_added_ms: self.announced_added_ms,
            additional_added_ms: self.additional_added_ms,
            des_steps_total: self.des_steps_total,
            early_interceptions_detected: self.early_interceptions_detected,
            one_touch_passes: self.one_touch_passes,
            dead_ball_distributions: DeadBallDistributionsV4 {
                goal_celebration: DurationDistributionMs::from_samples(&mut self.goal_celebration_samples),
                throw_in: DurationDistributionMs::from_samples(&mut self.throw_in_samples),
                corner: DurationDistributionMs::from_samples(&mut self.corner_samples),
                goal_kick: DurationDistributionMs::from_samples(&mut self.goal_kick_samples),
                free_kick: DurationDistributionMs::from_samples(&mut self.free_kick_samples),
                penalty: DurationDistributionMs::from_samples(&mut self.penalty_samples),
                injury: DurationDistributionMs::from_samples(&mut self.injury_samples),
                substitution: DurationDistributionMs::from_samples(&mut self.substitution_samples),
            },
            avg_player_distance_m: self.avg_player_distance_m,
            tactical_commands_delayed: self.tactical_commands_delayed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchDiagnosticsCollectorV4 {
    home: TeamAttackDiagnosticsV4,
    away: TeamAttackDiagnosticsV4,
    active: Option<ActivePossession>,
    des: DesDiagnosticsAccumulator,
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

    /// Updates DES v1.4 match clock totals (ball-in-play, out-of-play, added time).
    pub fn record_des_clock_totals(
        &mut self,
        ball_in_play_ms: i64,
        ball_out_of_play_ms: i64,
        compensable_lost_ms: i64,
        announced_added_ms: i64,
        additional_added_ms: i64,
    ) {
        self.des.ball_in_play_ms = ball_in_play_ms.max(0);
        self.des.ball_out_of_play_ms = ball_out_of_play_ms.max(0);
        self.des.compensable_lost_ms = compensable_lost_ms.max(0);
        self.des.announced_added_ms = announced_added_ms.max(0);
        self.des.additional_added_ms = additional_added_ms.max(0);
    }

    /// Increments the total number of executed DES scheduler steps.
    pub fn record_des_step(&mut self) {
        self.des.des_steps_total = self.des.des_steps_total.saturating_add(1);
    }

    /// Increments the count of continuous-interaction early interceptions detected mid-flight.
    pub fn record_early_interception(&mut self) {
        self.des.early_interceptions_detected =
            self.des.early_interceptions_detected.saturating_add(1);
    }

    /// Increments the count of one-touch passes executed in DES live play.
    pub fn record_one_touch_pass(&mut self) {
        self.des.one_touch_passes = self.des.one_touch_passes.saturating_add(1);
    }

    /// Increments the count of tactical commands delayed via natural propagation.
    pub fn record_tactical_command_delayed(&mut self) {
        self.des.tactical_commands_delayed =
            self.des.tactical_commands_delayed.saturating_add(1);
    }

    /// Records a single dead-ball stoppage duration sample (milliseconds) by category.
    pub fn record_dead_ball_duration(&mut self, kind: DeadBallDiagKind, duration_ms: i64) {
        let clamped = duration_ms.max(0);
        match kind {
            DeadBallDiagKind::GoalCelebration => self.des.goal_celebration_samples.push(clamped),
            DeadBallDiagKind::ThrowIn => self.des.throw_in_samples.push(clamped),
            DeadBallDiagKind::Corner => self.des.corner_samples.push(clamped),
            DeadBallDiagKind::GoalKick => self.des.goal_kick_samples.push(clamped),
            DeadBallDiagKind::FreeKick => self.des.free_kick_samples.push(clamped),
            DeadBallDiagKind::Penalty => self.des.penalty_samples.push(clamped),
            DeadBallDiagKind::Injury => self.des.injury_samples.push(clamped),
            DeadBallDiagKind::Substitution => self.des.substitution_samples.push(clamped),
        }
    }

    /// Records the average outfield player distance covered (meters) at match end.
    pub fn set_avg_player_distance_m(&mut self, avg_distance_m: f32) {
        if avg_distance_m.is_finite() && avg_distance_m >= 0.0 {
            self.des.avg_player_distance_m = avg_distance_m;
        }
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
                // The receiver may immediately fumble the first touch.
                // Only a clean first touch or a later controlled-carrier beat
                // proves the ball was actually controlled at the destination.
                self.record_pass(side, metadata, true);
            }
            EventTypeV4::PassFailed => {
                self.record_pass(side, metadata, false);
                if metadata.extra.get("pendingPassRecovery").map(String::as_str) == Some("true") {
                    self.team_mut(side).pass_contacts_loose += 1;
                } else {
                    self.lose_ball(side, metadata.outcome.as_deref().unwrap_or("MISPLACED"));
                }
            }
            EventTypeV4::Offside => {
                // A deflected pass was already counted at contact.
                if metadata.extra.get("passAlreadyCounted").map(String::as_str) != Some("true") {
                    self.record_pass(side, metadata, false);
                }
                self.lose_ball(side, "OFFSIDE");
            }
            EventTypeV4::SecondBallWon if metadata.cause.as_deref() == Some("PASS_CONTACT_LOOSE") => {
                match metadata.extra.get("recoveryOutcome").map(String::as_str) {
                    Some("ATTACK_REGAINED") => self.team_mut(side).pass_loose_regained += 1,
                    Some("DEFENCE_WON") => {
                        self.team_mut(side.opposite()).pass_loose_lost += 1;
                        self.lose_ball(side.opposite(), "INTERCEPTED");
                    }
                    _ => {}
                }
                self.observe_territory(side, ball_pos.xy());
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
        let des = self.des.finalize();
        MatchDiagnosticsV4 {
            schema_version: DIAGNOSTICS_SCHEMA_VERSION_V4,
            home: self.home.clone(),
            away: self.away.clone(),
            des,
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
    fn duration_distribution_computes_percentiles_accurately() {
        let empty = DurationDistributionMs::from_samples(&mut []);
        assert_eq!(empty, DurationDistributionMs::default());

        let mut samples = vec![50_000, 10_000, 30_000, 20_000, 40_000, 60_000, 70_000, 80_000, 90_000, 100_000];
        let dist = DurationDistributionMs::from_samples(&mut samples);
        assert_eq!(dist.count, 10);
        assert_eq!(dist.min_ms, 10_000);
        assert_eq!(dist.median_ms, 55_000);
        assert_eq!(dist.p90_ms, 90_000);
        assert_eq!(dist.max_ms, 100_000);
    }

    #[test]
    fn des_diagnostics_collector_records_and_validates() {
        let mut c = MatchDiagnosticsCollectorV4::default();
        c.record_des_clock_totals(3_300_000, 2_100_000, 420_000, 240_000, 45_000);
        c.record_des_step();
        c.record_des_step();
        c.record_early_interception();
        c.record_one_touch_pass();
        c.record_tactical_command_delayed();
        c.record_dead_ball_duration(DeadBallDiagKind::GoalCelebration, 45_000);
        c.record_dead_ball_duration(DeadBallDiagKind::GoalCelebration, 65_000);
        c.record_dead_ball_duration(DeadBallDiagKind::ThrowIn, 12_000);
        c.set_avg_player_distance_m(10_850.0);

        let report = c.finish();
        report.validate_basic().unwrap();
        assert_eq!(report.des.ball_in_play_ms, 3_300_000);
        assert_eq!(report.des.des_steps_total, 2);
        assert_eq!(report.des.early_interceptions_detected, 1);
        assert_eq!(report.des.one_touch_passes, 1);
        assert_eq!(report.des.tactical_commands_delayed, 1);
        assert_eq!(report.des.dead_ball_distributions.goal_celebration.count, 2);
        assert_eq!(report.des.dead_ball_distributions.goal_celebration.median_ms, 55_000);
        assert_eq!(report.des.dead_ball_distributions.throw_in.count, 1);
    }

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
        let fast = simulate_v4(make_v4_input(90410, SimulationMode::FastBatch, RulesModeV4::Standard)).unwrap();
        let full = simulate_v4(make_v4_input(90410, SimulationMode::FullEvent, RulesModeV4::Standard)).unwrap();
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

    /// Produces a human-readable, per-match artifact alongside the 128-match
    /// aggregate baseline. Deliberately ignored in the normal unit-test suite.
    #[test]
    #[ignore = "manual CI evidence export; 12 fixed synthetic league fixtures"]
    fn export_attacking_diagnostics_sample() {
        let mut matches = Vec::new();
        for seed in 92_000i64..92_012 {
            let result = simulate_v4(make_v4_input(seed, SimulationMode::FastBatch, RulesModeV4::Standard))
                .expect("seeded diagnostic fixture must simulate");
            let d = result.diagnostics.expect("FastBatch must include diagnostics");
            d.validate_basic().expect("diagnostics must have consistent funnel");
            matches.push(serde_json::json!({
                "matchId": result.match_id,
                "seed": seed,
                "summary": {
                    "home": {
                        "goals": result.summary.home.goals,
                        "shots": result.summary.home.shots,
                        "passesAttempted": result.summary.home.passes_attempted,
                        "passesCompleted": result.summary.home.passes_completed,
                        "xg": result.summary.home.xg,
                    },
                    "away": {
                        "goals": result.summary.away.goals,
                        "shots": result.summary.away.shots,
                        "passesAttempted": result.summary.away.passes_attempted,
                        "passesCompleted": result.summary.away.passes_completed,
                        "xg": result.summary.away.xg,
                    }
                },
                "diagnostics": d,
            }));
        }
        let report = serde_json::json!({
            "schemaVersion": DIAGNOSTICS_SCHEMA_VERSION_V4,
            "fixture": {
                "description": "Synthetic, not real teams; uniform home 14 vs away 12 attributes",
                "rules": "STANDARD",
                "simulationMode": "FAST_BATCH",
                "seedStart": 92000,
                "seedEndInclusive": 92011
            },
            "caveats": [
                "Territorial entries are one per possession with controlled ball; not all visits.",
                "Box possessions without a shot are a proxy, not measured clear chances.",
                "No empirical realism pass/fail threshold is enforced.",
                "A missing classified event produces an otherUnclassified loss."
            ],
            "matches": matches,
        });
        let json = serde_json::to_string_pretty(&report).expect("serialize report");
        if let Ok(path) = std::env::var("V4_DIAGNOSTICS_REPORT_PATH") {
            std::fs::write(&path, json).expect("write diagnostic artifact");
            println!("Attacking diagnostics written to {path}");
        } else {
            println!("{json}");
        }
    }
}
