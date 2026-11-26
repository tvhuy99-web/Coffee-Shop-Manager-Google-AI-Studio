#!/usr/bin/env python3
"""Validate and summarize paired B1.2 adapter intervals, not full-match candidates."""
import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from statistics import mean, median


def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value, name, nonnegative=True):
    require(type(value) in (int, float) and math.isfinite(value), f"invalid {name}")
    require(not nonnegative or value >= 0, f"negative {name}")
    return value


def stats(values):
    ordered = sorted(values)
    require(bool(ordered), "empty distribution")
    return {
        "n": len(ordered), "min": ordered[0], "mean": mean(ordered),
        "p50": median(ordered), "p90": ordered[math.ceil(0.9 * len(ordered)) - 1],
        "max": ordered[-1],
    }


def summarize(report, minimum):
    require(report["schema"] == 1, "schema mismatch")
    require(report["scope"] == "reference_full_matches_plus_paired_live_intervals", "wrong scope")
    require(report["candidate_full_matches"] == 0, "shadow report cannot certify candidate matches")
    rows = report["rows"]
    require(len(rows) >= minimum, "too few paired intervals")
    require(report["paired_intervals"] == report["reference_full_matches"] == len(rows), "count mismatch")
    require(report["failures"] == 0, "adapter failures")
    seen = set()
    cohorts = defaultdict(int)
    groups = defaultdict(lambda: defaultdict(list))
    times = defaultdict(list)
    baseline = defaultdict(lambda: defaultdict(list))
    outside = 0
    for row in rows:
        seed = row["seed"]
        require(type(seed) is int and seed not in seen, "duplicate/invalid seed")
        seen.add(seed)
        cohort = row["cohort"]
        require(cohort in {"reference", "balanced", "strong_away", "high_press"}, "unknown cohort")
        cohorts[cohort] += 1
        require(row["failure"] is None, f"candidate error seed {seed}")
        duration = row["interval_ms"]
        require(duration in (250, 500, 1000, 2000), "unexpected interval")
        require(row["sample_ms"] >= row["requested_ms"] >= 0, "snapshot before target")
        for key in ("legacy_adapter_us", "candidate_adapter_us", "baseline_session_wall_ms_including_probe"):
            times[key].append(number(row[key], key))
        require(row["baseline_timeline_fingerprint"] is not None, "missing fingerprint")
        players = row["players"]
        require(0 < len(players) <= 22 and len({p["id"] for p in players}) == len(players), "invalid player rows")
        for p in players:
            role = "GK" if p["goalkeeper"] else p["slot"]
            legacy = number(p["legacy_distance_m"], "legacy distance")
            candidate = number(p["candidate_distance_m"], "candidate distance")
            v = number(p["candidate_terminal_speed_mps"], "candidate speed")
            proxy = number(p["legacy_terminal_speed_proxy_mps"], "legacy speed proxy")
            load = number(p["acceleration_load_mps"], "acceleration load")
            number(p["initial_speed_mps"], "initial speed")
            for energy in ("aerobic", "anaerobic"):
                initial = number(p[energy], energy)
                delta = number(p["candidate_" + energy + "_delta"], energy + " delta", False)
                require(0 <= initial <= 100 and -1e-4 <= initial + delta <= 100.0001, "energy domain")
            require(type(p["candidate_outside_pitch"]) is bool, "boundary flag type")
            outside += int(p["candidate_outside_pitch"])
            for group in ("all", f"cohort:{cohort}", f"role:{role}", f"interval_ms:{duration}"):
                g = groups[group]
                g["legacy_distance_m"].append(legacy)
                g["candidate_distance_m"].append(candidate)
                g["paired_distance_delta_m"].append(candidate - legacy)
                g["legacy_average_speed_proxy_mps"].append(proxy)
                g["candidate_terminal_speed_mps"].append(v)
                g["candidate_acceleration_load_mps"].append(load)
        bplayers = row["baseline_players"]
        require(len(bplayers) == 22 and len({p["id"] for p in bplayers}) == 22, "reference roster mismatch")
        for p in bplayers:
            role = "GK" if p["goalkeeper"] else p["slot"]
            for group in (f"role:{role}", f"cohort:{cohort}"):
                baseline[group]["recorded_distance_m"].append(number(p["distance_m"], "baseline distance"))
                baseline[group]["legacy_interval_sprint_counter"].append(number(p["legacy_sprint_counter"], "legacy sprint counter"))
    if minimum >= 1000:
        require(len(cohorts) == 4 and min(cohorts.values()) >= 250, "missing cohort coverage")
    return {
        "schema": 1, "scope": report["scope"], "candidate_full_matches": 0,
        "paired_intervals": len(rows), "reference_full_matches": len(rows),
        "cohorts": dict(cohorts), "candidate_outside_pitch_endpoints": outside,
        "adapter_timings_us_and_reference_wall_ms": {k: stats(v) for k, v in times.items()},
        "paired_player_interval_distributions": {g: {k: stats(v) for k, v in m.items()} for g, m in groups.items()},
        "reference_full_match_recorded_counters": {g: {k: stats(v) for k, v in m.items()} for g, m in baseline.items()},
        "production_acceptance": "BLOCKED: candidate full-match integration and paired evidence absent",
        "notes": [
            "No match-distance extrapolation from interval samples.",
            "Reference counters include legacy distance estimation and interval sprint counts; no exposure adjustment.",
            "Timings are host adapter costs, not whole-engine speed comparisons.",
            "Boundary excursions are diagnostic, not a calibration target.",
        ],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("summary", type=Path)
    parser.add_argument("--minimum", type=int, default=1000)
    args = parser.parse_args()
    require(args.minimum > 0, "invalid minimum")
    result = summarize(json.loads(args.report.read_text()), args.minimum)
    args.summary.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(f"B1.2 shadow evidence validated: {result['paired_intervals']} intervals, "
          f"{result['reference_full_matches']} reference matches, 0 candidate full matches")


if __name__ == "__main__":
    main()
