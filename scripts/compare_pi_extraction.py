#!/usr/bin/env python3
"""Exact refactor parity, never a tolerance/realism gate."""
import argparse
import json
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("before", type=Path)
p.add_argument("after", type=Path)
p.add_argument("--full", action="store_true")
p.add_argument("--expected", type=int, required=True)
args = p.parse_args()
old = json.loads(args.before.read_text())
new = json.loads(args.after.read_text())
if not args.full:
    assert old["scenario"] == new["scenario"]
    for report in (old, new):
        h = report["headline"]
        assert h["attemptedMatches"] == args.expected
        assert h["completedMatches"] == args.expected
        assert h["failedMatches"] == h["invalidDiagnostics"] == 0
    old, new = old["matches"], new["matches"]

assert len(old) == len(new) == args.expected
mismatches = []
for a, b in zip(old, new):
    assert a["ok"] and b["ok"]
    if a != b:
        keys = sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))
        mismatches.append({"seedBefore": a.get("seed"), "seedAfter": b.get("seed"), "differentFields": keys})
print("PI_EXTRACTION_PARITY", json.dumps({
    "mode": "FullEvent" if args.full else "FastBatch",
    "matches": args.expected, "identical": args.expected - len(mismatches),
    "mismatches": len(mismatches), "firstDifferences": mismatches[:12],
}, sort_keys=True))
if mismatches:
    raise SystemExit("Behavior changed during PI extraction")
