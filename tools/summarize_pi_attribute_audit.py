#!/usr/bin/env python3
"""Descriptive paired contrasts; no realism thresholds or claims of significance."""
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path

def metrics(row):
    s = row["summary"]
    h, a = s["home"], s["away"]
    players = [p for p in s["players"] if p["side"] == "HOME"]
    # Use the fixture's canonical IDs as a fallback to preserve compatibility
    # with side serialization; these are synthetic names, not public players.
    if not players:
        players = [p for p in s["players"] if p["playerId"].startswith("H")]
    outfield = [p for p in players if p["playerId"] != "H00"]
    keeper = next(p for p in players if p["playerId"] == "H00")
    return {
        "goalsFor": h["goals"], "goalsAgainst": a["goals"],
        "shotsFor": h["shots"], "shotsAgainst": a["shots"],
        "xgFor": h["xg"], "xgAgainst": a["xg"],
        "passesAttempted": h["passesAttempted"], "passesCompleted": h["passesCompleted"],
        "passAccuracyPct": 100*h["passesCompleted"]/h["passesAttempted"] if h["passesAttempted"] else None,
        "opponentPassAccuracyPct": 100*a["passesCompleted"]/a["passesAttempted"] if a["passesAttempted"] else None,
        "fouls": h["fouls"], "yellowCards": h["yellowCards"], "redCards": h["redCards"],
        "corners": h["corners"], "offsides": h["offsides"], "saves": h["saves"],
        "outfieldDistanceM": sum(p["distanceM"] for p in outfield),
        "outfieldAerobic": statistics.mean(p["finalAerobic"] for p in outfield),
        "outfieldAnaerobic": statistics.mean(p["finalAnaerobic"] for p in outfield),
        "keeperDistanceM": keeper["distanceM"], "keeperAerobic": keeper["finalAerobic"],
        "injuredPlayers": sum(p.get("injury") is not None for p in players),
        "homePossessionPct": h["possessionPct"],
        "eventCount": row["eventCount"],
    }

def describe(values):
    return {"n":len(values),"mean":statistics.mean(values) if values else None,
            "sampleSd":statistics.stdev(values) if len(values)>1 else None,
            "positive":sum(x>0 for x in values),"negative":sum(x<0 for x in values),
            "zero":sum(x==0 for x in values)}

def main():
    data=json.loads(Path(sys.argv[1]).read_text())
    baseline={r["seed"]:metrics(r) for r in data["baseline"]}
    by_name=defaultdict(list)
    for r in data["pairs"]:
        by_name[r["attribute"]].append(r)
    assert len(baseline)==len(data["seeds"])==8
    assert data["simulations"]==len(baseline)+2*len(data["pairs"])
    expected={"technical":13,"mental":14,"physical":8,"goalkeeping":9,"hidden":10}
    assert len(by_name)==expected[data["group"]]
    results=[]
    for name,rows in sorted(by_name.items()):
        assert {r["seed"] for r in rows}==set(baseline) and len(rows)==8
        levels={5:[],12:[],18:[]}
        for row in rows:
            levels[5].append(metrics(row["low"]))
            levels[12].append(baseline[row["seed"]])
            levels[18].append(metrics(row["high"]))
        measures={}
        for key in levels[12][0]:
            raw={str(l):[v[key] for v in levels[l]] for l in levels}
            paired={}
            for a,b in [(18,5),(12,5),(18,12)]:
                paired[f"{a}minus{b}"]=describe([x-y for x,y in zip(raw[str(a)],raw[str(b)]) if x is not None and y is not None])
            measures[key]={"levels":{l:describe([v for v in vs if v is not None]) for l,vs in raw.items()},"paired":paired}
        changed=sum(not r["lowEqualsBaseline"] or not r["highEqualsBaseline"] for r in rows)
        play_changed=sum(not r["lowPlayEqualsBaseline"] or not r["highPlayEqualsBaseline"] for r in rows)
        item={"attribute":name,"careerHook":rows[0]["careerHook"],"changedSeeds":changed,"playChangedSeeds":play_changed,"metrics":measures}
        results.append(item)
        print("PI_ATTRIBUTE_CONTRAST",json.dumps({
            "attribute":name,"careerHook":item["careerHook"],"changedSeeds":changed,"playChangedSeeds":play_changed,
            "highMinusLow":{k:measures[k]["paired"]["18minus5"]["mean"] for k in ("goalsFor","goalsAgainst","passAccuracyPct","fouls","outfieldDistanceM","keeperDistanceM")}
        },sort_keys=True))
    Path(sys.argv[2]).write_text(json.dumps({
        "group":data["group"],"piVersion":data["piVersion"],"simulations":data["simulations"],
        "careerDifferences":data["careerDifferences"],"careerCarryoverDifferences":data["careerCarryoverDifferences"],"limitations":data["limitations"],
        "attributes":results
    },indent=2))
if __name__=="__main__":
    main()
