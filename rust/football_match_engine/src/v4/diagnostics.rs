//! Compatibility re-export: diagnostics accumulation is owned by match_telemetry.
pub use match_telemetry::*;
#[cfg(test)]
use crate::v4::{events::{EventMetadataV4,EventTypeV4},spatial::{TeamSideV4,Vec2,Vec3}};

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
