#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$ROOT/tools/verify_v4_p0_p1_patch.py"
python3 "$ROOT/tools/verify_v4_p1_234.py"
if command -v kotlinc >/dev/null 2>&1; then
  bash "$ROOT/tools/run_v4_p1_jvm_smoke.sh"
else
  echo 'WARNING: kotlinc not found; Kotlin snapshot smoke tests skipped' >&2
fi
if command -v cargo >/dev/null 2>&1; then
  ( cd "$ROOT/rust/football_match_engine" && cargo test --all-targets )
else
  echo 'WARNING: cargo not found; Rust compile and tests skipped' >&2
fi
