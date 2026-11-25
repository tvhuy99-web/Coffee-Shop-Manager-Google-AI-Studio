#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
K="$ROOT/app/src/main/java/pl/mkrstudio/truefootball3/lab"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
kotlinc "$ROOT"/tests/des_playback_jvm/*.kt "$K/LabModels.kt" "$K/MatchCheckpointStore.kt" "$K/MatchPlaybackController.kt" "$K/NarrationScheduler.kt" "$K/LabV4PlaybackCoordinator.kt" -include-runtime -d "$OUT/des-tests.jar"
java -jar "$OUT/des-tests.jar"
