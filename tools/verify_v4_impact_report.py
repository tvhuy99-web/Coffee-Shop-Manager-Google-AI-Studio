#!/usr/bin/env python3
"""Fail malformed V4 impact/realism evidence; do not mislabel exploratory observations as proof."""
import json
import math
import sys
from pathlib import Path

CAREER = {"professionalism", "ambition", "adaptability", "loyalty"}
EXPECTED = {
    "technical": "finishing passing dribbling technique first_touch crossing heading long_shots tackling marking free_kicks penalties corners",
    "mental": "vision decisions composure concentration off_the_ball positioning anticipation teamwork determination work_rate leadership flair aggression bravery",
    "physical": "pace acceleration strength stamina balance agility jumping_reach natural_fitness",
    "goalkeeping": "reflexes handling one_on_ones command_of_area aerial_reach kicking throwing rushing_out punching",
    "hidden": "professionalism ambition consistency big_matches adaptability loyalty pressure injury_proneness temperament sportsmanship",
}
ALL_NAMES = {name for values in EXPECTED.values() for name in values.split()}
IN_MATCH = ALL_NAMES - CAREER


def require(ok, message):
    if not ok:
        raise SystemExit(message)


def check_impact(path: Path):
    audit = json.loads(path.read_text(encoding="utf-8"))
    require(audit.get("schema_version") == 1, "impact schema must be v1")
    require(audit.get("match_attributes_tested") == 50, "impact audit must cover 50 match attributes")
    require(set(audit.get("career_attributes_excluded", [])) == CAREER, "wrong career exclusion list")
    rows = audit["results"]
    require(isinstance(rows, list) and len(rows) == 50, "require 50 diagnostic rows")
    names = [row["attribute"] for row in rows]
    require(set(names) == IN_MATCH and len(set(names)) == 50, "missing or duplicated attribute rows")
    seeds = audit["paired_seeds_per_attribute"]
    require(isinstance(seeds, int) and 4 <= seeds <= 96, "invalid paired seed count")
    observed = 0
    for row in rows:
        name = row["attribute"]
        require(row["low_value"] == 4 and row["high_value"] == 19, f"{name}: wrong test contrast")
        require(row["paired_seed_count"] == seeds, f"{name}: incomplete paired seed sample")
        require(row["cohort"] == ("goalkeeper" if name in EXPECTED["goalkeeping"].split() else "outfield"),
                f"{name}: wrong player cohort")
        events = row["different_match_event_timelines"]
        summaries = row["different_match_summaries"]
        require(type(events) is int and 0 <= events <= seeds, f"{name}: invalid event differences")
        require(type(summaries) is int and 0 <= summaries <= seeds, f"{name}: invalid summary differences")
        require(row["observed_in_this_sample"] is (events > 0), f"{name}: observation flag disagrees")
        for k in ("low_home_goals", "high_home_goals", "low_home_shots", "high_home_shots",
                  "low_home_passes_attempted", "high_home_passes_attempted",
                  "low_home_passes_completed", "high_home_passes_completed"):
            require(type(row[k]) is int and row[k] >= 0, f"{name}: invalid {k}")
        observed += int(events > 0)
    require(audit["observed_event_changes"] == observed, "observed count mismatch")
    unseen = set(audit["no_change_observed_in_sample"])
    require(unseen == {r["attribute"] for r in rows if not r["observed_in_this_sample"]},
            "unobserved list disagrees with rows")
    print(f"V4 impact evidence valid: 50/50 attributes probed, {seeds} paired seeds each, "
          f"{observed} with observed event differences, {len(unseen)} NOT observed.")
    if unseen:
        print("Diagnostic gaps requiring specialized scenarios:", ", ".join(sorted(unseen)))


def check_realism(path: Path):
    report = json.loads(path.read_text(encoding="utf-8"))
    require(report.get("schema_version") == 1, "realism schema must be v1")
    sample = report["sample"]
    metrics = report["metrics"]
    require(16 <= sample["matches"] <= 3000, "realism sample invalid")
    require(metrics["matchCount"] == sample["matches"], "Monte Carlo count mismatch")
    for key in ("avgGoalsPerMatch", "avgShotsPerMatch", "avgShotsBlockedPerMatch",
                "avgShotsOffTargetPerMatch", "avgSavesPerMatch", "avgOwnGoalsPerMatch",
                "passAccuracyPct", "avgFoulsPerMatch", "avgYellowCardsPerMatch",
                "avgRedCardsPerMatch", "homeWinRatePct", "drawRatePct", "awayWinRatePct",
                "matchesPerSec",
                "avgPossessionsPerMatch", "avgFinalThirdPossessionsPerMatch",
                "avgPenaltyAreaPossessionsPerMatch", "avgPenaltyAreaWithoutShotPerMatch",
                "avgShotPossessionsPerMatch", "avgShotsFromBoxPerMatch",
                "avgPassInterceptionsPerMatch", "avgPassMisplacementsPerMatch",
                "avgPassOutOfPlayPerMatch", "avgFirstTouchErrorsPerMatch",
                "avgDribbleTacklesPerMatch", "avgUnclassifiedTurnoversPerMatch"):
        require(isinstance(metrics[key], (int, float)) and math.isfinite(metrics[key]) and metrics[key] >= 0,
                f"invalid aggregate metric: {key}")
    outcomes = sum(metrics[key] for key in ("homeWinRatePct", "drawRatePct", "awayWinRatePct"))
    require(abs(outcomes - 100.0) < 0.1, "home/draw/away proportions must sum to 100%")
    require(0 <= metrics["passAccuracyPct"] <= 100, "invalid pass accuracy percentage")
    require(metrics["avgShotsBlockedPerMatch"] + metrics["avgShotsOffTargetPerMatch"]
            <= metrics["avgShotsPerMatch"] + 1e-4, "shot breakdown impossible")
    eps = 1e-4
    require(metrics["avgFinalThirdPossessionsPerMatch"] <= metrics["avgPossessionsPerMatch"] + eps,
            "final-third entries exceed total possessions")
    require(metrics["avgPenaltyAreaPossessionsPerMatch"] <= metrics["avgFinalThirdPossessionsPerMatch"] + eps,
            "penalty-area entries exceed final-third entries")
    require(metrics["avgPenaltyAreaWithoutShotPerMatch"] <= metrics["avgPenaltyAreaPossessionsPerMatch"] + eps,
            "box-without-shot exceeds box possessions")
    require(metrics["avgShotPossessionsPerMatch"] <= metrics["avgPossessionsPerMatch"] + eps,
            "shot possessions exceed total possessions")
    require(metrics["avgShotsFromBoxPerMatch"] <= metrics["avgShotsPerMatch"] + eps,
            "shots from box exceed total shots")
    print(f"V4 realism evidence valid: {sample['matches']} synthetic 90-minute fixtures; "
          f"mean goals {metrics['avgGoalsPerMatch']:.2f}, shots {metrics['avgShotsPerMatch']:.2f}, "
          f"pass accuracy {metrics['passAccuracyPct']:.1f}%. Advisory, not calibrated.")
    print(f"V4 attack funnel/match: possessions {metrics['avgPossessionsPerMatch']:.2f}, "
          f"final third {metrics['avgFinalThirdPossessionsPerMatch']:.2f}, "
          f"box {metrics['avgPenaltyAreaPossessionsPerMatch']:.2f}, "
          f"box-without-shot {metrics['avgPenaltyAreaWithoutShotPerMatch']:.2f}, "
          f"shot possessions {metrics['avgShotPossessionsPerMatch']:.2f}.")
    print(f"V4 possession losses/match: intercepted passes {metrics['avgPassInterceptionsPerMatch']:.2f}, "
          f"misplaced passes {metrics['avgPassMisplacementsPerMatch']:.2f}, "
          f"first touches {metrics['avgFirstTouchErrorsPerMatch']:.2f}, "
          f"dribble tackles {metrics['avgDribbleTacklesPerMatch']:.2f}, "
          f"unclassified {metrics['avgUnclassifiedTurnoversPerMatch']:.2f}.")


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: verify_v4_impact_report.py ATTRIBUTE_REPORT.json REALISM_REPORT.json")
    check_impact(Path(sys.argv[1]))
    check_realism(Path(sys.argv[2]))


if __name__ == "__main__":
    main()
