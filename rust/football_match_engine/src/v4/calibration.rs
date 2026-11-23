use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::v4::error::V4Error;
use crate::v4::input::{MatchInputV4, SimulationMode};
use crate::v4::session::{simulate_batch_fast_v4, simulate_v4};

/// Aggregated Monte Carlo statistical & throughput benchmark report for Match Engine V4.0.
/// Target football realism bands (90-minute regulation):
/// - Goals/match: 2.65 – 2.95
/// - Shots/match: 23 – 27
/// - Pass accuracy: 79% – 84%
/// - Fouls/match: 20 – 24
/// - Yellows/match: 3.4 – 4.3 | Reds/match: 0.12 – 0.18
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonteCarloReportV4 {
    pub match_count: usize,
    pub avg_goals_per_match: f32,
    pub avg_shots_per_match: f32,
    pub avg_shots_blocked_per_match: f32,
    pub avg_shots_off_target_per_match: f32,
    pub avg_saves_per_match: f32,
    pub avg_own_goals_per_match: f32,
    pub pass_accuracy_pct: f32,
    pub avg_fouls_per_match: f32,
    pub avg_yellow_cards_per_match: f32,
    pub avg_red_cards_per_match: f32,
    pub home_win_rate_pct: f32,
    pub draw_rate_pct: f32,
    pub away_win_rate_pct: f32,
    pub elapsed_ms: u128,
    pub matches_per_sec: f32,
}

/// Runs a parallel `FastBatch` Monte Carlo simulation over `inputs` via Rayon and
/// computes full statistical balance + performance throughput metrics.
pub fn run_monte_carlo_v4(inputs: Vec<MatchInputV4>) -> Result<MonteCarloReportV4, V4Error> {
    if inputs.is_empty() {
        return Err(V4Error::InvalidInput("Monte Carlo input batch cannot be empty".into()));
    }
    let start = Instant::now();
    let outcomes = simulate_batch_fast_v4(inputs);
    let elapsed_ms = start.elapsed().as_millis();

    let mut total_goals = 0u64;
    let mut total_shots = 0u64;
    let mut total_blocked = 0u64;
    let mut total_off_target = 0u64;
    let mut total_saves = 0u64;
    let mut total_own_goals = 0u64;
    let mut total_passes_att = 0u64;
    let mut total_passes_comp = 0u64;
    let mut total_fouls = 0u64;
    let mut total_yellows = 0u64;
    let mut total_reds = 0u64;
    let mut home_wins = 0usize;
    let mut draws = 0usize;
    let mut away_wins = 0usize;
    let mut count = 0usize;

    for outcome in outcomes {
        let res = outcome?;
        let h = &res.summary.home;
        let a = &res.summary.away;
        total_goals += (h.goals + a.goals) as u64;
        total_shots += (h.shots + a.shots) as u64;
        total_blocked += (h.shots_blocked + a.shots_blocked) as u64;
        total_off_target += (h.shots_off_target + a.shots_off_target) as u64;
        total_saves += (h.saves + a.saves) as u64;
        total_own_goals += (h.own_goals_for + a.own_goals_for) as u64;
        total_passes_att += (h.passes_attempted + a.passes_attempted) as u64;
        total_passes_comp += (h.passes_completed + a.passes_completed) as u64;
        total_fouls += (h.fouls + a.fouls) as u64;
        total_yellows += (h.yellow_cards + a.yellow_cards) as u64;
        total_reds += (h.red_cards + a.red_cards) as u64;
        if h.goals > a.goals {
            home_wins += 1;
        } else if h.goals == a.goals {
            draws += 1;
        } else {
            away_wins += 1;
        }
        count += 1;
    }

    let n = count.max(1) as f32;
    let secs = (elapsed_ms as f32 / 1000.0).max(0.001);

    Ok(MonteCarloReportV4 {
        match_count: count,
        avg_goals_per_match: total_goals as f32 / n,
        avg_shots_per_match: total_shots as f32 / n,
        avg_shots_blocked_per_match: total_blocked as f32 / n,
        avg_shots_off_target_per_match: total_off_target as f32 / n,
        avg_saves_per_match: total_saves as f32 / n,
        avg_own_goals_per_match: total_own_goals as f32 / n,
        pass_accuracy_pct: (total_passes_comp as f32 / total_passes_att.max(1) as f32) * 100.0,
        avg_fouls_per_match: total_fouls as f32 / n,
        avg_yellow_cards_per_match: total_yellows as f32 / n,
        avg_red_cards_per_match: total_reds as f32 / n,
        home_win_rate_pct: (home_wins as f32 / n) * 100.0,
        draw_rate_pct: (draws as f32 / n) * 100.0,
        away_win_rate_pct: (away_wins as f32 / n) * 100.0,
        elapsed_ms,
        matches_per_sec: n / secs,
    })
}

/// Verifies the Global Seed-Replay invariant: running a match in `FastBatch` mode
/// (used for global day-processing) and later replaying the exact same immutable
/// `MatchInputV4` snapshot in `FullEvent` mode produces identical score, summary,
/// `event_count`, and `event_timeline_fingerprint`.
pub fn verify_seed_replay_parity(input: MatchInputV4) -> Result<bool, V4Error> {
    let mut fast_inp = input.clone();
    fast_inp.simulation_mode = SimulationMode::FastBatch;
    let mut full_inp = input;
    full_inp.simulation_mode = SimulationMode::FullEvent;

    let fast_res = simulate_v4(fast_inp)?;
    let full_res = simulate_v4(full_inp)?;

    Ok(fast_res.events.is_empty()
        && full_res.events.len() as u32 == full_res.event_count
        && fast_res.event_count == full_res.event_count
        && fast_res.event_timeline_fingerprint == full_res.event_timeline_fingerprint
        && fast_res.summary == full_res.summary)
}
