#!/usr/bin/env python3
"""Aggregate experimental hybrid full-match pairs; never certify the final solver."""
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from statistics import mean, median

BEHAVIOR = "b1.2-batch2-set-piece-delivery"
COHORTS = ["reference", "balanced", "strong_away", "high_press"]
FIELDS = ["home_goals", "away_goals", "shots", "passes", "completed_passes",
          "fouls", "live_ms", "dead_ms", "events", "wall_ms",
          "pass_receiver_unreached", "pass_execution_errors", "pass_intercepted",
          "pass_out_of_play", "first_touch_errors", "pass_contacts_loose",
          "pass_risk_pairs", "pass_risk_initial_sum", "pass_risk_release_sum", "reaim_choices",
          "possessions", "final_third_possessions", "penalty_area_possessions",
          "possessions_with_shot", "shots_from_box", "shots_blocked", "shots_on_target",
          "shots_off_target", "shot_xg", "saves", "carry_starts", "carry_contacts",
          "carry_turnovers", "tackle_opportunities", "tackle_commit_choices",
          "shot_probe_on_target", "shot_probe_no_reach", "shot_probe_reachable_goal", "shot_probe_keeper_segment_m", "shot_probe_keeper_depth_m", "shot_probe_keeper_speed_mps", "shot_probe_ms"]


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def number(value, name):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0,
            f"invalid {name}: {value!r}")
    return value


def stats(values):
    ordered = sorted(values)
    return {"n": len(ordered), "mean": mean(ordered), "p50": median(ordered),
            "p90": ordered[math.ceil(len(ordered) * .9) - 1],
            "min": ordered[0], "max": ordered[-1]}


def main(directory, pairs_per_shard=250):
    require(pairs_per_shard in (12, 50, 250), "invalid evidence tier")
    population = pairs_per_shard * len(COHORTS)
    tier = {12: "smoke", 50: "integration", 250: "full"}[pairs_per_shard]
    cohorts = defaultdict(lambda: defaultdict(list))
    roles = defaultdict(lambda: defaultdict(list))
    failures = []
    seeds = set()
    successful = 0
    changed = 0
    for shard, cohort in enumerate(COHORTS):
        report = json.loads((directory / f"b12-exp-{shard}.json").read_text())
        require(report["schema"] == 1 and report["scope"] == "hybrid_experiment_full_match_pairs",
                "wrong report schema/scope")
        require(report["behavior_id"] == BEHAVIOR and report["shard"] == shard,
                "wrong experiment identity")
        require(report["attempted_pairs"] == len(report["rows"]) == pairs_per_shard, "wrong shard size")
        shard_failed = 0
        for i, row in enumerate(report["rows"]):
            seed = row["seed"]
            require(seed == 104729 + (4 * i + shard) * 7907 and seed not in seeds
                    and row["cohort"] == cohort, "seed/cohort mismatch")
            seeds.add(seed)
            a, b = row["legacy"], row["experiment"]
            if a["ok"] is not True or b["ok"] is not True:
                shard_failed += 1
                failures.append({"seed": seed, "cohort": cohort,
                                 "legacy_error": None if a["ok"] else a,
                                 "experiment_error": None if b["ok"] else b})
                continue
            successful += 1
            require(b["behavior_id"] == BEHAVIOR and a["behavior_id"] != BEHAVIOR,
                    "invalid behavior identity")
            require(bool(a["fingerprint"]) and bool(b["fingerprint"]), "missing fingerprint")
            changed += int(a["fingerprint"] != b["fingerprint"])
            for run in (a, b):
                require(number(run["time_ms"], "time") > 0, "empty match")
                require(run["live_ms"] + run["dead_ms"] == run["time_ms"], "clock mismatch")
                require(run["completed_passes"] <= run["passes"], "pass count mismatch")
            require(b["shot_probe_no_reach"] + b["shot_probe_reachable_goal"] <= b["shot_probe_on_target"],
                    "keeper probe counts exceed observed on-target launches")
            for field in FIELDS:
                old, new = number(a[field], field), number(b[field], field)
                if field.startswith("shot_probe_"):
                    require(old == 0, f"legacy probe unexpectedly measured: {field}")
                for group in ("all", cohort):
                    cohorts[group]["legacy_" + field].append(old)
                    cohorts[group]["experiment_" + field].append(new)
                    cohorts[group]["delta_" + field].append(new - old)
            # Total goals must be paired before calculating quantiles; summing
            # per-team medians/percentiles would not give the match distribution.
            old_goals = a["home_goals"] + a["away_goals"]
            new_goals = b["home_goals"] + b["away_goals"]
            for group in ("all", cohort):
                cohorts[group]["legacy_total_goals"].append(old_goals)
                cohorts[group]["experiment_total_goals"].append(new_goals)
                cohorts[group]["delta_total_goals"].append(new_goals - old_goals)
            ap, bp = {p["id"]: p for p in a["players"]}, {p["id"]: p for p in b["players"]}
            require(len(ap) == len(bp) == 22 and ap.keys() == bp.keys(), "roster mismatch")
            for pid in ap:
                old, new = ap[pid], bp[pid]
                group = "GK" if old["goalkeeper"] else old["slot"]
                require(a["counter_semantics"] == "legacy_interval_counts" and b["counter_semantics"] == "upward_speed_crossings_4_2_and_7_mps", "counter semantics mismatch")
                for key in ("sprint_counter", "high_intensity_counter"):
                    roles[group]["legacy_interval_" + key].append(number(old[key], key))
                    roles[group]["experiment_upward_crossing_" + key].append(number(new[key], key))
                for field in ("distance_m", "aerobic", "anaerobic"):
                    x, y = number(old[field], field), number(new[field], field)
                    if field in ("aerobic", "anaerobic"):
                        require(x <= 100 and y <= 100, "energy outside domain")
                    roles[group]["legacy_" + field].append(x)
                    roles[group]["experiment_" + field].append(y)
                    roles[group]["delta_" + field].append(y - x)
        require(shard_failed == report["failed_pairs"], "failure count mismatch")
    require(len(seeds) == population and successful + len(failures) == population, "population count mismatch")
    summary = {
        "schema": 1, "scope": "hybrid_experiment_full_match_pairs", "behavior_id": BEHAVIOR,
        "evidence_tier": tier, "attempted_pairs": population, "successful_pairs": successful,
        "failed_pairs": len(failures), "changed_fingerprints_in_successful_pairs": changed,
        "failure_rows": failures,
        "cohort_distributions": {g: {k: stats(v) for k, v in m.items()} for g, m in cohorts.items()},
        "role_distributions": {g: {k: stats(v) for k, v in m.items()} for g, m in roles.items()},
        "production_acceptance": "BLOCKED: remaining execution paths, rule boundaries, non-pass perception, sprint episode definition, performance and tracking calibration incomplete",
        "notes": ["Batch1 shares complete integer-ms contact windows and orders nominal shot-block and keeper opportunities before commitment. Shot, block and save probability formulas and reach priors are unchanged; experimental RNG draw order changes. Off-ball targets persist across clock-only events. Batch2 physically approaches ordinary restart/kickoff positions, schedules clearance flight and penalty loose recovery. Corner and non-direct deliveries use the shared physical race; direct close free kick and penalty execution remain provisional. Probe counts are experimental open-play scheduled shot launches only; legacy probe fields are zero/not measured.",
                  "Keeper segment/depth/speed fields are sums over probe launches, divide by total probe count; diagnostic probe time is excluded from wall_ms. Scheduled shots may be cancelled before commit, so probe counts are not a subset of committed shot totals.",
                  "Pooled rates are descriptive. Goals and saves may include restarts/own goals; ratios are not a disjoint shot-outcome partition.",
                  "Distributions are conditional on successful pairs; failures are retained.",
                  "Legacy interval counters and experiment threshold crossing counts are reported separately; neither is a validated sprint episode metric.",
                  "Pass diagnostic counters retain their existing definitions; not disjoint causes and not forced to sum to pass attempts.",
                  "No outcome target or threshold is used to tune gameplay."],
    }
    # Derive rates from pooled exposure, not the mean of match-level ratios.
    # None means unmeasured/no denominator, never a fabricated zero rate.
    def ratio(numerator, denominator):
        return numerator / denominator if denominator else None
    rates = {}
    for group, values in cohorts.items():
        row = {}
        for prefix in ("legacy_", "experiment_"):
            def total(field):
                return sum(values[prefix + field])
            row[prefix + "pass_completion"] = ratio(total("completed_passes"), total("passes"))
            row[prefix + "shots_per_possession"] = ratio(total("shots"), total("possessions"))
            row[prefix + "shots_on_target_per_shot"] = ratio(total("shots_on_target"), total("shots"))
            row[prefix + "goals_per_on_target"] = ratio(total("home_goals") + total("away_goals"), total("shots_on_target"))
            row[prefix + "saves_per_on_target"] = ratio(total("saves"), total("shots_on_target"))
            row[prefix + "keeper_probe_no_reach_share"] = ratio(total("shot_probe_no_reach"), total("shot_probe_on_target"))
            row[prefix + "keeper_probe_reachable_goal_share"] = ratio(total("shot_probe_reachable_goal"), total("shot_probe_on_target"))
            row[prefix + "keeper_probe_mean_segment_m"] = ratio(total("shot_probe_keeper_segment_m"), total("shot_probe_on_target"))
            row[prefix + "keeper_probe_mean_depth_m"] = ratio(total("shot_probe_keeper_depth_m"), total("shot_probe_on_target"))
        rates[group] = row
    summary["pooled_rates"] = rates
    (directory / "b12-experiment-summary.json").write_text(
        json.dumps(summary, indent=2, allow_nan=False) + "\n")
    print(f"B1.2 hybrid full-match experiment: {successful}/{population} successful pairs; "
          f"{len(failures)} failed; {changed} changed fingerprints")
    require(not failures, "experimental pairs failed; inspect summary")


if __name__ == "__main__":
    main(Path(sys.argv[1]), int(sys.argv[2]) if len(sys.argv)>2 else 250)
