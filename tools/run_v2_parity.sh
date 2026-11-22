#!/usr/bin/env bash
# Clean-project Rust/Java fixture parity. Java sources were intentionally removed.
# REQUIRE_RUST=1 prevents accidental success when Cargo is missing (used in CI).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 "$ROOT/tools/verify_lab_v2.py"
python3 "$ROOT/tools/verify_parity_fixtures.py"

if ! command -v cargo >/dev/null 2>&1; then
  echo 'RUST TESTS NOT RUN: cargo is unavailable; Java golden fixture validation alone is not parity.' >&2
  if [[ "${REQUIRE_RUST:-0}" == "1" ]]; then
    echo 'FAIL: REQUIRE_RUST=1 requires a working Rust toolchain.' >&2
    exit 2
  fi
  echo 'PARTIAL CHECK ONLY: install Rust and run REQUIRE_RUST=1 bash tools/run_v2_parity.sh'
  exit 0
fi

# The Rust test suite includes comparisons against the saved Java V3 golden
# event stream and match fingerprints, not only individual formula tests.
(cd "$ROOT/rust/football_match_engine" && cargo test --all-targets)
echo 'RUST TEST SUITE: completed; inspect output for Java V3 event parity.'
