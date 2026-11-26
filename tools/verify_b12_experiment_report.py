#!/usr/bin/env python3
"""Aggregate experimental hybrid full-match pairs; never certify the final solver."""
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from statistics import mean, median

BEHAVIOR = "b1.2-measured-workload-exp3"
COHORTS = ["reference", "balanced", "strong_away", "high_press"]
FIELDS = ["home_goals", "away_goals", "shots", "passes", "completed_passes",
          "fouls", "live_ms", "dead_ms", "events", "wall_ms"]


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


def main(directory):
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
        require(report["attempted_pairs"] == len(report["rows"]) == 250, "wrong shard size")
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
            for field in FIELDS:
                old, new = number(a[field], field), number(b[field], field)
                for group in ("all", cohort):
                    cohorts[group]["legacy_" + field].append(old)
                    cohorts[group]["experiment_" + field].append(new)
                    cohorts[group]["delta_" + field].append(new - old)
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
    require(len(seeds) == 1000 and successful + len(failures) == 1000, "population count mismatch")
    summary = {
        "schema": 1, "scope": "hybrid_experiment_full_match_pairs", "behavior_id": BEHAVIOR,
        "attempted_pairs": 1000, "successful_pairs": successful,
        "failed_pairs": len(failures), "changed_fingerprints_in_successful_pairs": changed,
        "failure_rows": failures,
        "cohort_distributions": {g: {k: stats(v) for k, v in m.items()} for g, m in cohorts.items()},
        "role_distributions": {g: {k: stats(v) for k, v in m.items()} for g, m in roles.items()},
        "production_acceptance": "BLOCKED: remaining execution paths, rule boundaries, perception, sprint episode definition and tracking calibration incomplete",
        "notes": ["Exp3 adds role/distance speed requests and measured motion workload to Exp2 paths; remaining execution paths and perception are legacy.",
                  "Distributions are conditional on successful pairs; failures are retained.",
                  "Legacy interval counters and experiment threshold crossing counts are reported separately; neither is a validated sprint episode metric.",
                  "No outcome target or threshold is used to tune gameplay."],
    }
    (directory / "b12-experiment-summary.json").write_text(
        json.dumps(summary, indent=2, allow_nan=False) + "\n")
    print(f"B1.2 hybrid full-match experiment: {successful}/1000 successful pairs; "
          f"{len(failures)} failed; {changed} changed fingerprints")
    require(not failures, "experimental pairs failed; inspect summary")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
