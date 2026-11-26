#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if ! command -v kotlinc >/dev/null 2>&1; then
  echo 'ERROR: Kotlin CLI (kotlinc) is required for this regression suite' >&2
  exit 2
fi
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
MAIN="$ROOT/app/src/main/java/pl/mkrstudio/truefootball3/lab"
TEST="$ROOT/tests/commentary"
kotlinc \
  "$TEST/stubs/android/content/Context.kt" \
  "$TEST/stubs/org/json/JSONObject.kt" \
  "$TEST/stubs/LabEnums.kt" \
  "$MAIN/match_commentary/LabNarrativeTracker.kt" \
  "$MAIN/match_commentary/LabCommentaryTemplatePolicy.kt" \
  "$MAIN/match_commentary/LabCommentaryEngine.kt" \
  "$TEST/CommentaryRegression.kt" \
  -include-runtime -d "$OUT/test.jar"
java -jar "$OUT/test.jar" "$ROOT/app/src/main/res"
