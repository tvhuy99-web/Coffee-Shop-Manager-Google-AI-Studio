#!/usr/bin/env python3
"""Strict streaming golden comparison; report the first differing field without tolerance."""
import argparse
import copy
import hashlib
import itertools
import json
from decimal import Decimal
from pathlib import Path

def parse(line):
    return json.loads(line, parse_float=Decimal)

def first_diff(a, b, path="$"):
    if type(a) is not type(b):
        return path, a, b
    if isinstance(a, dict):
        for k in sorted(a.keys() | b.keys()):
            p = f"{path}.{k}"
            if k not in a or k not in b:
                return p, a.get(k, "<missing>"), b.get(k, "<missing>")
            d = first_diff(a[k], b[k], p)
            if d:
                return d
    elif isinstance(a, list):
        if len(a) != len(b):
            return path + ".length", len(a), len(b)
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_diff(x, y, f"{path}[{i}]")
            if d:
                return d
    elif isinstance(a, Decimal):
        if a.as_tuple() != b.as_tuple():
            return path, a, b
    elif a != b:
        return path, a, b
    return None

def digest(v):
    return hashlib.sha256(json.dumps(v, sort_keys=True, default=str).encode()).hexdigest()

def context(row, d):
    checkpoint = row.get("terminal", {})
    payload = checkpoint.get("payload", {})
    path = d[0]
    # When a retained event differs, identify that event rather than just final clock.
    event = None
    if path.startswith("$.result.events["):
        try:
            event = row["result"]["events"][int(path.split("[", 1)[1].split("]", 1)[0])]
        except (IndexError, KeyError, ValueError):
            pass
    return {
        "scenario": row.get("scenario"), "seed": row.get("seed"),
        "fieldPath": path, "expected": repr(d[1])[:400], "actual": repr(d[2])[:400],
        "eventSequence": event.get("sequence") if event else payload.get("nextEventSequence"),
        "simTimeMs": event.get("simTimeMs") if event else checkpoint.get("simTimeMs"),
        "rngDigest": digest(payload.get("rng")),
        "pendingAction": repr(payload.get("pendingFlight") or payload.get("pendingDecision") or payload.get("pendingCarry"))[:400],
        "module": "unclassified; investigate first differing field",
    }

def check_same(a, b):
    # Prefer result/event divergence over snapshot checksum metadata.
    keys = ["result", "externalCommands", "initial", "markers", "terminal"]
    for key in keys:
        if key not in a and key not in b:
            continue
        if key not in a or key not in b:
            return "$." + key, a.get(key, "<missing>"), b.get(key, "<missing>")
        if key in ("initial", "terminal") and isinstance(a[key], dict) and isinstance(b[key], dict):
            d = first_diff(a[key].get("payload"), b[key].get("payload"), "$." + key + ".payload")
            if d:
                return d
        d = first_diff(a[key], b[key], "$." + key)
        if d:
            return d
    return first_diff(a, b)

def validate(row):
    for k in ("initial", "terminal"):
        ck = row[k]
        assert ck["seed"] == row["seed"]
        for field in ("input", "state", "scheduler", "rng", "stats", "appliedCommands"):
            assert field in ck["payload"], (row["seed"], k, field)
        assert ck["checksumHex"], (row["seed"], k)
    assert row["result"]["ok"] and row["result"]["seed"] == row["seed"]
    assert isinstance(row["externalCommands"], list)

def main():
    p = argparse.ArgumentParser()
    p.add_argument("first", type=Path)
    p.add_argument("second", type=Path)
    p.add_argument("--scenario", required=True)
    p.add_argument("--expected", type=int, required=True)
    p.add_argument("--reference", type=Path)
    args = p.parse_args()
    ref = None
    if args.reference:
        doc = parse(args.reference.read_text())
        ref = doc if args.scenario == "full_event" else doc["matches"]
        assert len(ref) == args.expected
    seen = set()
    markers = set()
    first_row = None
    with args.first.open() as f, args.second.open() as g:
        for line_a, line_b in itertools.zip_longest(f, g):
            assert line_a is not None and line_b is not None, "different number of rows"
            a, b = parse(line_a), parse(line_b)
            validate(a)
            validate(b)
            assert a["scenario"] == b["scenario"] == args.scenario
            assert a["seed"] not in seen
            i = len(seen)
            seen.add(a["seed"])
            if first_row is None:
                first_row = a
            d = check_same(a, b)
            if d:
                print(json.dumps(context(a, d), ensure_ascii=False))
                raise SystemExit("A1_DEEP_DIVERGENCE")
            if ref is not None:
                old = ref[i]
                assert old["seed"] == a["seed"] and old["ok"]
                if args.scenario == "full_event":
                    d = first_diff(old, a["result"], "$.result")
                else:
                    old_view = {k: old[k] for k in ("eventCount", "fingerprint", "summary", "diagnostics")}
                    new_view = {k: a["result"][k] for k in ("eventCount", "summary", "diagnostics")}
                    new_view["fingerprint"] = a["result"]["eventTimelineFingerprint"]
                    d = first_diff(old_view, new_view, "$.priorPopulation")
                if d:
                    print(json.dumps(context(a, d), ensure_ascii=False))
                    raise SystemExit("A1_PRIOR_GOLDEN_DIVERGENCE")
            for m in a.get("markers", []):
                markers.update(m["labels"])
    assert first_row is not None
    if args.scenario == "checkpoint_markers":
        required = {"live","pass_flight","tackle_loose","goal_celebration","pending_tactics","halftime","period_end"}
        assert markers == required, (markers, required)
    else:
        assert len(seen) == args.expected
        expected_seeds = {92_000+i if args.scenario=="full_event" else 104_729+i*7907 for i in range(args.expected)}
        assert seen == expected_seeds
    # Negative controls on actual captured checkpoint structures, not invented gameplay.
    for field in ("state", "rng", "scheduler", "appliedCommands"):
        changed = copy.deepcopy(first_row)
        payload = changed["terminal"]["payload"]
        if isinstance(payload[field], dict):
            payload[field]["__negative_control__"] = True
        elif isinstance(payload[field], list):
            payload[field].append({"__negative_control__": True})
        else:
            payload[field] = None
        d = check_same(first_row, changed)
        assert d and d[0].startswith("$.terminal.payload." + field), d
    print(json.dumps({"marker":"A1_DEEP_PARITY","scenario":args.scenario,"rows":len(seen),
                      "mismatches":0,"coveredMarkers":sorted(markers),"negativeControls":4,
                      "firstSha256":hashlib.sha256(args.first.read_bytes()).hexdigest(),
                      "secondSha256":hashlib.sha256(args.second.read_bytes()).hexdigest()}, sort_keys=True))
if __name__ == "__main__":
    main()
