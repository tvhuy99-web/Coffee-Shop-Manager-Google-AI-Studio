#!/usr/bin/env bash
# Collect reproducible real-device evidence. Never marks a device scenario PASS.
set -euo pipefail
usage(){ echo "usage: $0 SERIAL APK OUTPUT_DIR [PACKAGE]"; exit 2; }
[[ $# -ge 3 && $# -le 4 ]] || usage
serial="$1"; apk="$2"; out="$3"; pkg="${4:-pl.mkrstudio.truefootball3.debug}"
command -v adb >/dev/null || { echo "BLOCKED: adb unavailable" >&2; exit 3; }
[[ -f "$apk" ]] || { echo "BLOCKED: APK missing" >&2; exit 3; }
[[ ! -e "$out" ]] || { echo "Refusing to overwrite evidence: $out" >&2; exit 2; }
[[ "$(adb -s "$serial" get-state)" == device ]] || { echo "BLOCKED: real device not available" >&2; exit 3; }
adb_cmd(){ adb -s "$serial" "$@"; }
[[ "$(adb_cmd shell getprop ro.kernel.qemu | tr -d '\r')" != 1 ]] || { echo "BLOCKED: emulator is not a real target device" >&2; exit 3; }
mkdir -p "$out"
sha256sum "$apk" > "$out/apk.sha256"
git rev-parse HEAD > "$out/source-commit.txt"
adb_cmd shell getprop ro.product.model > "$out/model.txt"
adb_cmd shell getprop ro.build.version.release > "$out/android-version.txt"
adb_cmd shell getprop ro.product.cpu.abi > "$out/abi.txt"
adb_cmd shell dumpsys package "$pkg" > "$out/installed-package.txt"
adb_cmd shell dumpsys battery > "$out/battery-before.txt"
adb_cmd shell dumpsys thermalservice > "$out/thermal-before.txt"
adb_cmd shell dumpsys meminfo "$pkg" > "$out/memory-before.txt"
adb_cmd shell dumpsys gfxinfo "$pkg" > "$out/frames-before.txt"
adb_cmd shell pm path "$pkg" > "$out/installed-apk-path.txt"
mapfile -t installed < <(tr -d '\r' < "$out/installed-apk-path.txt" | sed -n 's/^package://p')
[[ ${#installed[@]} -eq 1 ]] || { echo "BLOCKED: expected one monolithic installed APK" >&2; exit 4; }
[[ "${installed[0]}" == */base.apk ]] || { echo "BLOCKED: installed base APK path not found" >&2; exit 4; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
adb_cmd pull "${installed[0]}" "$tmp/installed.apk" > "$out/adb-pull.txt"
expected="$(sha256sum "$apk" | cut -d' ' -f1)"
actual="$(sha256sum "$tmp/installed.apk" | cut -d' ' -f1)"
printf '%s\n' "$actual" > "$out/installed-apk.sha256"
[[ "$actual" == "$expected" ]] || { echo "BLOCKED: installed APK SHA-256 differs from candidate" >&2; exit 4; }
printf 'MATCH\n' > "$out/apk-identity-status.txt"
adb_cmd shell dumpsys audio > "$out/audio-focus-before.txt"
adb_cmd shell dumpsys activity processes > "$out/processes-before.txt"
echo "Evidence captured; no device test has been marked PASS. Record scenario outcomes manually."
cat > "$out/scenarios.csv" <<'CSV'
scenario,rate,seed,event_fingerprint,cue_fingerprint,drift_p95_ms,backlog_max,elapsed_ms,status,evidence
full_match,1.0,,,,,,,NOT_RUN,
full_match,2.0,,,,,,,NOT_RUN,
full_match,3.0,,,,,,,NOT_RUN,
rate_switch_pause_resume,,,,,,,,NOT_RUN,
restore_live,,,,,,,,NOT_RUN,
restore_goal_celebration,,,,,,,,NOT_RUN,
restore_corner_free_kick,,,,,,,,NOT_RUN,
restore_halftime,,,,,,,,NOT_RUN,
restore_pending_command,,,,,,,,NOT_RUN,
restore_tts_or_write,,,,,,,,NOT_RUN,
background_lock_audio_focus,,,,,,,,NOT_RUN,
low_memory_jni_tts_error,,,,,,,,NOT_RUN,
substitution_duplicate_limits,,,,,,,,NOT_RUN,
extra_time_shootout,,,,,,,,NOT_RUN,
CSV
