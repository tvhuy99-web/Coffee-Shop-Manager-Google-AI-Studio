#!/usr/bin/env python3
"""Check integrity of stored Java V3 golden data in the *clean* Rust/Kotlin port.

The original Java engine is intentionally not in this project. Checking that
fixtures are intact is NOT the same thing as executing Java or passing Rust parity.
The actual parity test is `cargo test` (parity_full_match.rs et al.).
"""

import csv
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "docs/archive/legacy/tests/parity"


def fail(reason):
    print(f"GOLDEN FIXTURE ERROR: {reason}", file=sys.stderr)
    sys.exit(1)


def rows(name):
    with (GOLDEN / name).open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def main():
    manifest = json.loads((GOLDEN / "java_event_trace_manifest.json").read_text(encoding="utf-8"))
    data = (GOLDEN / "java_event_trace_golden.csv").read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != manifest["trace_sha256"]:
        fail(f"event trace hash mismatch: {digest}")

    traces = rows("java_event_trace_golden.csv")
    matches = rows("java_full_match_golden.csv")
    expected_counts = {str(k): int(v) for k, v in manifest["seed_event_counts"].items()}
    actual_counts = Counter(row["seed"] for row in traces)
    if actual_counts != Counter(expected_counts):
        fail(f"seed event count mismatch: expected={expected_counts}, got={dict(actual_counts)}")
    if len(traces) != 7044 or len(matches) != len(expected_counts) or len(matches) != 3:
        fail(f"unexpected number of golden rows: trace={len(traces)}, matches={len(matches)}")

    seen_matches = set()
    for match in matches:
        seed = match["seed"]
        if seed not in expected_counts or seed in seen_matches:
            fail(f"unexpected or duplicate match seed: {seed}")
        seen_matches.add(seed)
        if int(match["event_count"]) != expected_counts[seed]:
            fail(f"match row event count disagrees with trace: seed {seed}")
        if int(match["retained_events"]) != expected_counts[seed]:
            fail(f"match row retained_events disagrees with trace: seed {seed}")

    sequences = Counter()
    last_second = {seed: -1 for seed in expected_counts}
    for ev in traces:
        seed = ev["seed"]
        sequences[seed] += 1
        if int(ev["sequence"]) != sequences[seed]:
            fail(f"nonconsecutive event sequence for seed {seed}: {ev['sequence']}")
        second = int(ev["game_second"])
        if second < last_second[seed]:
            fail(f"event time decreased for seed {seed}")
        last_second[seed] = second

    for name in ["java_reference_expected.csv", "java_contextual_expected.csv", "java_phase2_expected.csv"]:
        if not (GOLDEN / name).is_file() or not (GOLDEN / name).read_text(encoding="utf-8").strip():
            fail(f"missing or empty fixture: {name}")

    print(f"GOLDEN FIXTURES OK: {len(matches)} saved Java matches, {len(traces)} events, SHA-256 verified")
    print("NOTE: This does NOT run the original Java source. Rust parity requires cargo test.")


if __name__ == "__main__":
    main()
