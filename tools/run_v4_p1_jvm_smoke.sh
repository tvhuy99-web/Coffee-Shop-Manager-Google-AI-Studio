#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if ! command -v kotlinc >/dev/null 2>&1; then
  echo 'ERROR: requires kotlinc' >&2
  exit 2
fi
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
K="$ROOT/app/src/main/java/pl/mkrstudio/truefootball3/lab"
S="$ROOT/tests/p1_kotlin"
kotlinc "$S/android/content/Context.kt" "$S/android/util/AtomicFile.kt" \
  "$S/org/json/JSONStub.kt" "$S/pl/mkrstudio/truefootball3/lab/Deps.kt" \
  "$K/LabGlobalSeedReplayStore.kt" "$K/LabRustLiveSession.kt" "$S/Smoke.kt" \
  -include-runtime -d "$OUT/smoke.jar"
java -jar "$OUT/smoke.jar"
