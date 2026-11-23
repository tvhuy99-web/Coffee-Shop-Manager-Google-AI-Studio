//! Controlled counterfactual tests: identical seed, roster, tactics and environment,
//! changing only one named attribute across the relevant player cohort.
//! An input fingerprint changing is NOT proof that football behavior changed.

use serde_json::{json, Value};

use crate::rng::DeterministicRng;
use crate::v4::attributes::PlayerAttributes54;
use crate::v4::calibration::run_monte_carlo_v4;
use crate::v4::goalkeeper::{choose_gk_distribution, resolve_gk_aerial_claim, GkAerialOutcome};
use crate::v4::input::{RulesModeV4, SimulationMode};
use crate::v4::session::simulate_v4;
use crate::v4::state::{MatchStateV4, TwoPoolFatigueState};
use crate::v4::stats::validate_summary;
use super::tests::{make_v4_input, make_v4_input_teams, make_v4_team, make_v4_team_with_attrs};

const CAREER_ONLY: [&str; 4] = ["professionalism", "ambition", "adaptability", "loyalty"];
const GOALKEEPING: [&str; 9] = [
    "reflexes", "handling", "one_on_ones", "command_of_area", "aerial_reach",
    "kicking", "throwing", "rushing_out", "punching",
];

fn modify_one_field(profile: PlayerAttributes54, name: &str, value: u8) -> PlayerAttributes54 {
    let mut tree = serde_json::to_value(profile).expect("serializable player attributes");
    let mut matches = 0;
    for group in ["technical", "mental", "physical", "goalkeeping", "hidden"] {
        if let Some(field) = tree.get_mut(group).and_then(Value::as_object_mut)
            .and_then(|object| object.get_mut(name))
        {
            *field = Value::from(value);
            matches += 1;
        }
    }
    assert_eq!(matches, 1, "attribute must occur in exactly one group: {name}");
    let updated: PlayerAttributes54 = serde_json::from_value(tree).unwrap();
    updated.validate().unwrap();
    let changed = profile.all_named().iter().zip(updated.all_named())
        .filter(|(old, new)| old.1 != new.1).count();
    assert_eq!(changed, 1, "no other attribute may change when mutating {name}");
    updated
}

fn paired_fixture(seed: i64, attribute: &str, value: u8) -> crate::v4::MatchInputV4 {
    let gk_only = GOALKEEPING.contains(&attribute);
    let home = make_v4_team_with_attrs("H", "HOME_IMPACT", |slot| {
        let base = PlayerAttributes54::uniform(12).unwrap();
        if (gk_only && slot == 0) || (!gk_only && slot != 0) {
            modify_one_field(base, attribute, value)
        } else {
            base
        }
    });
    let mut input = make_v4_input_teams(
        seed, SimulationMode::FastBatch, RulesModeV4::Standard,
        home, make_v4_team("A", "AWAY_IMPACT", 12),
    );
    // Different match contexts activate different hidden attributes.
    if attribute == "big_matches" || attribute == "pressure" {
        input.match_environment.match_importance = 95;
    }
    input.ai_home_manager = false;
    input.ai_away_manager = false;
    input
}

fn write_json_report(env_key: &str, value: &Value) {
    if let Ok(path) = std::env::var(env_key) {
        let path = std::path::Path::new(&path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create report directory");
        }
        std::fs::write(path, serde_json::to_vec_pretty(value).unwrap())
            .expect("write attribute audit report");
    }
}

#[test]
fn goalkeeper_aerial_skills_have_isolated_directional_effects() {
    let input = make_v4_input(777, SimulationMode::FastBatch, RulesModeV4::Standard);
    let base = PlayerAttributes54::uniform(12).unwrap();
    let command_low = modify_one_field(base, "command_of_area", 4);
    let command_high = modify_one_field(base, "command_of_area", 19);
    let handling_low = modify_one_field(base, "handling", 4);
    let handling_high = modify_one_field(base, "handling", 19);
    let punching_low = modify_one_field(base, "punching", 4);
    let punching_high = modify_one_field(base, "punching", 19);

    let mut command_out_low = 0;
    let mut command_out_high = 0;
    let mut claim_low = 0;
    let mut claim_high = 0;
    let mut punches_low = 0;
    let mut punches_high = 0;

    for seed in 1..=800i64 {
        // Same PRNG seed on BOTH sides of each isolated counterfactual.
        let roll = |attributes: &PlayerAttributes54| {
            let mut rng = DeterministicRng::new(seed * 31);
            resolve_gk_aerial_claim(attributes, 190, true, 2.4, &input, &mut rng)
        };
        if roll(&command_low) != GkAerialOutcome::StayedOnLine { command_out_low += 1; }
        if roll(&command_high) != GkAerialOutcome::StayedOnLine { command_out_high += 1; }
        if roll(&handling_low) == GkAerialOutcome::Claimed { claim_low += 1; }
        if roll(&handling_high) == GkAerialOutcome::Claimed { claim_high += 1; }
        if roll(&punching_low) == GkAerialOutcome::PunchedClear { punches_low += 1; }
        if roll(&punching_high) == GkAerialOutcome::PunchedClear { punches_high += 1; }
    }
    assert!(command_out_high > command_out_low + 50,
        "command_of_area did not increase proactive aerial decisions: {command_out_low} -> {command_out_high}");
    assert!(claim_high > claim_low + 30,
        "handling did not improve clean claims: {claim_low} -> {claim_high}");
    assert!(punches_high > punches_low + 20,
        "punching did not influence aerial clearance choice: {punches_low} -> {punches_high}");
}

#[test]
fn goalkeeper_distribution_and_fatigue_attributes_have_directional_effects() {
    let input = make_v4_input(1188, SimulationMode::FastBatch, RulesModeV4::Standard);
    let state = MatchStateV4::from_input(&input);
    let base = PlayerAttributes54::uniform(12).unwrap();
    let low = modify_one_field(base, "kicking", 4);
    let high = modify_one_field(base, "kicking", 19);
    let mut low_long = 0;
    let mut high_long = 0;
    for seed in 1..=640i64 {
        let mut rng_low = DeterministicRng::new(seed * 53);
        let mut rng_high = DeterministicRng::new(seed * 53);
        if choose_gk_distribution(&state, &low, crate::v4::spatial::TeamSideV4::Home, &mut rng_low).1 {
            low_long += 1;
        }
        if choose_gk_distribution(&state, &high, crate::v4::spatial::TeamSideV4::Home, &mut rng_high).1 {
            high_long += 1;
        }
    }
    assert!(high_long > low_long + 60,
        "kicking did not increase long distribution selection: {low_long} -> {high_long}");

    let mut tired = TwoPoolFatigueState::from_condition(99);
    let mut fit = tired;
    for _ in 0..9 {
        tired.apply_interval(5.0, 22.0, true, 4, 12);
        fit.apply_interval(5.0, 22.0, true, 19, 12);
    }
    assert!(fit.aerobic > tired.aerobic + 0.3,
        "higher stamina should reduce aerobic depletion, low={} high={}", tired.aerobic, fit.aerobic);
    assert!(fit.anaerobic > tired.anaerobic,
        "higher stamina should reduce sprint energy drain");
}

#[test]
fn big_matches_activates_only_in_high_importance_fixtures() {
    use crate::v4::psychology::compute_prematch_form_multiplier;
    let base = PlayerAttributes54::uniform(12).unwrap();
    let low = modify_one_field(base, "big_matches", 2);
    let high = modify_one_field(base, "big_matches", 19);
    let mut rng_low = DeterministicRng::new(911);
    let mut rng_high = DeterministicRng::new(911);
    assert_eq!(
        compute_prematch_form_multiplier(&low, 40, &mut rng_low),
        compute_prematch_form_multiplier(&high, 40, &mut rng_high),
        "big_matches should not affect ordinary fixtures"
    );
    let mut rng_low = DeterministicRng::new(911);
    let mut rng_high = DeterministicRng::new(911);
    assert!(
        compute_prematch_form_multiplier(&high, 95, &mut rng_high)
            > compute_prematch_form_multiplier(&low, 95, &mut rng_low),
        "big_matches must affect high-importance fixtures in the intended direction"
    );
}

#[test]
fn paired_match_core_attribute_changes_real_event_stream_not_only_input_hash() {
    // Reflexes are verified in an isolated shot-stopping test; ordinary matches may
    // produce too few shots on target to detect that rarer outcome reliably.
    for name in ["passing", "first_touch", "finishing", "dribbling"] {
        let mut actual_behavior_changes = 0;
        for index in 0..20i64 {
            let seed = 81001 + index * 109;
            let low = simulate_v4(paired_fixture(seed, name, 4)).unwrap();
            let high = simulate_v4(paired_fixture(seed, name, 19)).unwrap();
            validate_summary(&low.summary).unwrap();
            validate_summary(&high.summary).unwrap();
            // Timeline fingerprint is based on match EVENTS, not the attribute
            // snapshot fingerprint. This checks actual observable consequences.
            if low.event_timeline_fingerprint != high.event_timeline_fingerprint {
                actual_behavior_changes += 1;
            }
        }
        assert!(actual_behavior_changes > 0,
            "{name} affected the input hash but no football event in 20 paired-seed matches");
    }
}

/// A diagnostic matrix, not a false claim that all 50 attributes are proven.
/// Match-level events can be insensitive if a situation never activates
/// (penalties, corners, injuries, etc.). Such rows are explicitly marked.
#[test]
#[ignore = "Extended paired-seed audit; run in dedicated V4 impact workflow"]
fn exhaustive_50_attribute_counterfactual_report() {
    let count = std::env::var("V4_IMPACT_SEEDS").ok()
        .and_then(|s| s.parse::<usize>().ok()).unwrap_or(12).clamp(4, 96);
    let baseline = PlayerAttributes54::uniform(12).unwrap();
    let names: Vec<_> = baseline.all_named().into_iter()
        .map(|(name, _)| name)
        .filter(|name| !CAREER_ONLY.contains(name))
        .collect();
    assert_eq!(names.len(), 50);
    let mut rows = Vec::with_capacity(names.len());
    let mut unobserved = Vec::new();
    for name in names {
        let mut changed_events = 0;
        let mut changed_summaries = 0;
        let mut low_goals = 0u32;
        let mut high_goals = 0u32;
        let mut low_shots = 0u32;
        let mut high_shots = 0u32;
        let mut low_pass_attempts = 0u32;
        let mut high_pass_attempts = 0u32;
        let mut low_pass_completed = 0u32;
        let mut high_pass_completed = 0u32;
        for index in 0..count {
            let seed = 90001 + (index as i64) * 2039;
            let before = paired_fixture(seed, name, 4);
            let after = paired_fixture(seed, name, 19);
            let result_low = simulate_v4(before).unwrap();
            let result_high = simulate_v4(after).unwrap();
            validate_summary(&result_low.summary).unwrap();
            validate_summary(&result_high.summary).unwrap();
            changed_events += usize::from(
                result_low.event_timeline_fingerprint != result_high.event_timeline_fingerprint);
            changed_summaries += usize::from(result_low.summary != result_high.summary);
            low_goals += result_low.summary.home.goals as u32;
            high_goals += result_high.summary.home.goals as u32;
            low_shots += result_low.summary.home.shots as u32;
            high_shots += result_high.summary.home.shots as u32;
            low_pass_attempts += result_low.summary.home.passes_attempted as u32;
            high_pass_attempts += result_high.summary.home.passes_attempted as u32;
            low_pass_completed += result_low.summary.home.passes_completed as u32;
            high_pass_completed += result_high.summary.home.passes_completed as u32;
        }
        let observed = changed_events > 0;
        if !observed { unobserved.push(name); }
        rows.push(json!({
            "attribute": name, "cohort": if GOALKEEPING.contains(&name) { "goalkeeper" } else { "outfield" },
            "low_value": 4, "high_value": 19, "paired_seed_count": count,
            "different_match_event_timelines": changed_events,
            "different_match_summaries": changed_summaries,
            "observed_in_this_sample": observed,
            "low_home_goals": low_goals, "high_home_goals": high_goals,
            "low_home_shots": low_shots, "high_home_shots": high_shots,
            "low_home_passes_attempted": low_pass_attempts,
            "high_home_passes_attempted": high_pass_attempts,
            "low_home_passes_completed": low_pass_completed,
            "high_home_passes_completed": high_pass_completed
        }));
    }
    let report = json!({
        "schema_version": 1,
        "purpose": "COUNTERFACTUAL_DIAGNOSTIC_NOT_PROOF_OF_DIRECTION",
        "engine": crate::v4::ENGINE_VERSION_V4,
        "match_attributes_tested": 50, "career_attributes_excluded": CAREER_ONLY,
        "paired_seeds_per_attribute": count,
        "observed_event_changes": 50 - unobserved.len(),
        "no_change_observed_in_sample": unobserved,
        "method": "fixed matched seeds, identical roster/manager/tactics, one named attribute altered across relevant outfield or goalkeeper cohort; compare resulting event timeline fingerprints and team summaries",
        "results": rows
    });
    println!("ATTRIBUTE_IMPACT_AUDIT {}", serde_json::to_string(&report).unwrap());
    write_json_report("V4_IMPACT_REPORT_PATH", &report);
    assert_eq!(report["results"].as_array().unwrap().len(), 50);
}

#[test]
#[ignore = "Longer 90-minute population diagnostic; targets are advisory"]
fn monte_carlo_realism_baseline_report() {
    let count = std::env::var("V4_REALISM_MATCHES").ok()
        .and_then(|s| s.parse::<usize>().ok()).unwrap_or(128).clamp(16, 3000);
    let inputs: Vec<_> = (0..count)
        .map(|i| make_v4_input(104729 + i as i64 * 7907,
            SimulationMode::FastBatch, RulesModeV4::Standard))
        .collect();
    let metrics = run_monte_carlo_v4(inputs).unwrap();
    assert_eq!(metrics.match_count, count);
    assert!(metrics.avg_goals_per_match.is_finite());
    assert!(metrics.avg_shots_per_match.is_finite());
    assert!(metrics.pass_accuracy_pct.is_finite());
    assert!(metrics.home_win_rate_pct + metrics.draw_rate_pct + metrics.away_win_rate_pct > 99.9);
    let report = json!({
        "schema_version": 1,
        "note": "Synthetic fixed teams; descriptive, NOT representative of real English league calibration",
        "sample": {"matches": count, "home_level": 14, "away_level": 12},
        "metrics": metrics,
        "advisory_ranges_not_ci_gates": {
            "goals_per_match": [2.65,2.95],
            "shots_per_match": [23,27],
            "pass_accuracy_pct": [79,84],
            "fouls_per_match": [20,24]
        }
    });
    println!("V4_REALISM_BASELINE {}", serde_json::to_string(&report).unwrap());
    write_json_report("V4_REALISM_REPORT_PATH", &report);
}
