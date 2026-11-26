#!/usr/bin/env bash
set -euo pipefail
# Check tools before any test; missing prerequisites are failures, not skipped gates.
for tool in cargo kotlinc java python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "ERROR: required tool missing: $tool" >&2
    exit 2
  fi
done
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$ROOT/tools/verify_v4_p0_p1_patch.py"
python3 "$ROOT/tools/verify_v4_p1_234.py"
bash "$ROOT/tools/run_v4_p1_jvm_smoke.sh"
bash "$ROOT/tools/run_des_playback_jvm.sh"
( cd "$ROOT/rust/football_match_engine" && cargo test --locked --all-targets )
