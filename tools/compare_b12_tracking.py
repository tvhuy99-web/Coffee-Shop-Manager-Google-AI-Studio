#!/usr/bin/env python3
"""Descriptive like-for-like 5 Hz comparison; not a gameplay acceptance gate."""
import json
import math
import statistics
import sys
from pathlib import Path

from verify_b12_experiment_report import BEHAVIOR

def describe(values):
    a=sorted(values)
    assert a and all(math.isfinite(v) and v>=0 for v in a)
    return {"n":len(a),"min":a[0],"median":statistics.median(a),
            "p90":a[math.ceil(.9*len(a))-1],"max":a[-1]}

def summarize(rows):
    # Same duration condition for both populations; no missing-data imputation.
    eligible=[p for p in rows if p["observed_s"]>=5400]
    assert eligible, "no 90-minute observations"
    result={"all_actor_records":len(rows),"eligible_actor_records":len(eligible),
            "filtered_jumps":sum(p["outlier_intervals"] for p in rows),
            "distance_km_per_90":describe([p["distance_m"]*5.4/p["observed_s"] for p in eligible])}
    for metric in ("speed_mps","vector_acceleration_mps2","absolute_tangential_acceleration_mps2"):
        for q in ("p50","p95","p99","max"):
            result[metric+"_"+q]=describe([p[metric][q] for p in eligible if p[metric]["n"]])
    return result

def main():
    ref,candidate,out=map(Path,sys.argv[1:4])
    r=json.loads(ref.read_text())
    c=json.loads(candidate.read_text())
    assert r["sampling_hz"]==c["sampling_hz"]==5
    assert r["speed_outlier_mps"]==c["speed_outlier_mps"]==12
    assert c["behavior_id"]==BEHAVIOR and len(c["matches"])==8
    assert len({m["seed"] for m in c["matches"]})==8
    reference=[p for g in r["games"] for p in g["players"]]
    observed=[p for g in c["matches"] for p in g["players"]]
    report={"schema":1,"source_commit":r["source_commit"],"behavior_id":BEHAVIOR,
            "reference":summarize(reference),"candidate":summarize(observed),
            "candidate_outfield":summarize([p for p in observed if not p["is_goalkeeper"]]),
            "candidate_keeper":summarize([p for p in observed if p["is_goalkeeper"]]),
            "acceptance":"DESCRIPTIVE_ONLY",
            "limitations":["Per-actor quantiles, not pooled frame quantiles.",
                           "Reference anonymized; no comparable role/skill strata.",
                           "Both include stoppages; exclude gaps and >12 m/s jumps.",
                           "Period reset and sample phase may differ; no inferred sprint episode.",
                           "Eight synthetic games and two real games are not sufficient for a universal fit."]}
    if len(sys.argv)==5:
        before=json.loads(Path(sys.argv[4]).read_text())
        assert before["behavior_id"]=="b1.2-batch2-specialist-shots"
        assert before["sampling_hz"]==5 and before["speed_outlier_mps"]==12
        assert [(m["seed"],m["cohort"]) for m in before["matches"]]==[(m["seed"],m["cohort"]) for m in c["matches"]]
        rows=[p for m in before["matches"] for p in m["players"]]
        report["before_effort"]=summarize(rows)
        report["before_effort_outfield"]=summarize([p for p in rows if not p["is_goalkeeper"]])
        report["before_effort_keeper"]=summarize([p for p in rows if p["is_goalkeeper"]])
        report["before_effort_behavior_id"]=before["behavior_id"]
        report["before_effort_sha"]="7426c802b44faeff6fbadc9cc27ed7e872984daf"
    out.write_text(json.dumps(report,indent=2,allow_nan=False))
    print(json.dumps(report,indent=2))

if __name__=="__main__":
    main()
