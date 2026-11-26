#!/usr/bin/env python3
"""Unit tests of report aggregation only; never evidence of simulated matches."""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

import verify_b12_experiment_report as report


class ReportValidatorTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)

    def write_population(self, corrupt=None):
        players = [{"id": f"p{i}", "slot": "CF", "goalkeeper": i in (0, 11),
                    "distance_m": 9000, "aerobic": 55, "anaerobic": 80,
                    "sprint_counter": 12, "high_intensity_counter": 25}
                   for i in range(22)]
        for shard, cohort in enumerate(report.COHORTS):
            rows = []
            for i in range(250):
                a = dict.fromkeys(report.FIELDS, 0)
                b = dict.fromkeys(report.FIELDS, 0)
                a.update(ok=True, behavior_id="synthetic-legacy", fingerprint="old",
                         time_ms=5400000, live_ms=3600000, dead_ms=1800000,
                         home_goals=1, away_goals=2, passes=10, completed_passes=5,
                         shots_on_target=4, saves=1, players=players,
                         counter_semantics="legacy_interval_counts")
                b.update(ok=True, behavior_id=report.BEHAVIOR, fingerprint="new",
                         time_ms=5400000, live_ms=3600000, dead_ms=1800000,
                         home_goals=3, away_goals=1, passes=20, completed_passes=15,
                         shots_on_target=5, saves=1, players=players,
                         shot_probe_on_target=4, shot_probe_no_reach=2,
                         shot_probe_reachable_goal=1, shot_probe_keeper_segment_m=8,
                         shot_probe_keeper_depth_m=12,
                         counter_semantics="upward_speed_crossings_4_2_and_7_mps")
                if shard == 0 and i == 0 and corrupt:
                    corrupt(a, b)
                rows.append({"seed": 104729 + (4*i+shard)*7907,
                             "cohort": cohort, "legacy": a, "experiment": b})
            data = {"schema": 1, "scope": "hybrid_experiment_full_match_pairs",
                    "behavior_id": report.BEHAVIOR, "shard": shard,
                    "attempted_pairs": 250, "failed_pairs": 0, "rows": rows}
            (self.path / f"b12-exp-{shard}.json").write_text(json.dumps(data))

    def test_pooled_rates_and_total_goal_distribution(self):
        self.write_population()
        with contextlib.redirect_stdout(io.StringIO()):
            report.main(self.path)
        data = json.loads((self.path / "b12-experiment-summary.json").read_text())
        self.assertEqual(data["successful_pairs"], 1000)
        self.assertEqual(data["changed_fingerprints_in_successful_pairs"], 1000)
        self.assertEqual(data["failed_pairs"], 0)
        self.assertTrue(data["production_acceptance"].startswith("BLOCKED"))
        for group in ["all", *report.COHORTS]:
            d = data["cohort_distributions"][group]
            self.assertEqual(d["legacy_total_goals"]["p50"], 3)
            self.assertEqual(d["experiment_total_goals"]["p50"], 4)
            self.assertEqual(d["delta_total_goals"]["mean"], 1)
            rates = data["pooled_rates"][group]
            self.assertEqual(rates["legacy_pass_completion"], .5)
            self.assertEqual(rates["experiment_pass_completion"], .75)
            self.assertIsNone(rates["legacy_keeper_probe_no_reach_share"])
            self.assertEqual(rates["experiment_keeper_probe_no_reach_share"], .5)
            self.assertEqual(rates["experiment_keeper_probe_mean_segment_m"], 2)

    def test_rejects_impossible_probe_counts(self):
        self.write_population(lambda a, b: b.update(shot_probe_no_reach=5))
        with self.assertRaisesRegex(ValueError, "keeper probe counts"):
            report.main(self.path)

    def test_rejects_legacy_probe_measurement(self):
        self.write_population(lambda a, b: a.update(shot_probe_on_target=1))
        with self.assertRaisesRegex(ValueError, "legacy probe unexpectedly measured"):
            report.main(self.path)


if __name__ == "__main__":
    unittest.main()
