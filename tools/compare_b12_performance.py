#!/usr/bin/env python3
"""Same-runner frozen-reference report comparison; no gameplay tuning."""
import json
import math
import sys
from pathlib import Path
from statistics import mean, median

def compare(reference, candidate):
    for key in ("schema", "scope", "behavior_id", "shard", "attempted_pairs"):
        if reference[key] != candidate[key]:
            raise ValueError(f"reference mismatch: {key}")
    if reference["failed_pairs"] or candidate["failed_pairs"]:
        raise ValueError("failed pairs")
    if len(reference["rows"]) != len(candidate["rows"]) or len(candidate["rows"]) != candidate["attempted_pairs"]:
        raise ValueError("population mismatch")
    metrics = {}
    for mode in ("legacy", "experiment"):
        before, after = [], []
        for a, b in zip(reference["rows"], candidate["rows"]):
            if (a["seed"], a["cohort"]) != (b["seed"], b["cohort"]):
                raise ValueError("seed mismatch")
            x, y = dict(a[mode]), dict(b[mode])
            if x["ok"] is not True or y["ok"] is not True:
                raise ValueError("failed match")
            t0, t1 = x.pop("wall_ms"), y.pop("wall_ms")
            for report in (x, y):
                report.pop("shot_probe_ms")
            if x != y:
                fields = sorted(k for k in set(x) | set(y) if x.get(k) != y.get(k))
                raise ValueError(f"behavior parity failure {mode} seed={a['seed']} fields={fields}")
            if not all(type(t) in (int, float) and math.isfinite(t) and t > 0 for t in (t0,t1)):
                raise ValueError("invalid timing")
            before.append(t0)
            after.append(t1)
        metrics[mode] = {"reference_mean_ms":mean(before), "candidate_mean_ms":mean(after),
                         "reference_median_ms":median(before), "candidate_median_ms":median(after),
                         "speedup_ratio":mean(before)/mean(after)}
    return {"reference_sha":"411f580c04f4b840a70bdd026c5d91f1c7f9fa69",
            "behavior_id":candidate["behavior_id"],"shard":candidate["shard"],
            "pairs":candidate["attempted_pairs"],"exact_report_parity_excluding_timing":True,
            "metrics":metrics,"note":"Sequential same-runner timing, not controlled CPU benchmark."}

if __name__ == "__main__":
    a,b,out=map(Path,sys.argv[1:])
    result=compare(json.loads(a.read_text()),json.loads(b.read_text()))
    out.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result,indent=2))
