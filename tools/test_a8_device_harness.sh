#!/usr/bin/env bash
# Fake adb only: verifies safety gates and evidence format, not physical-device behavior.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
printf 'candidate-apk' > "$tmp/candidate.apk"
printf 'other-apk' > "$tmp/wrong.apk"
cat > "$tmp/bin/adb" <<'ADB'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == -s && "$2" == fake-device ]]
shift 2
case "$*" in
 get-state) echo "${FAKE_STATE:-device}" ;;
 "shell getprop ro.kernel.qemu") echo "${FAKE_QEMU:-0}" ;;
 "shell pm path "*) echo "package:/data/app/mock/base.apk"; if [[ "${FAKE_SPLIT:-0}" == 1 ]]; then echo "package:/data/app/mock/split.apk"; fi ;;
 "shell getprop "*) echo "fake-value" ;;
 "shell dumpsys "*) echo "fake evidence" ;;
 "pull /data/app/mock/base.apk "*) cp "$FAKE_APK" "$3" ;;
 *) echo "unexpected adb: $*" >&2; exit 1 ;;
esac
ADB
chmod +x "$tmp/bin/adb"
export PATH="$tmp/bin:$PATH" FAKE_APK="$tmp/candidate.apk"
cd "$ROOT"
run(){ bash tools/capture_a8_device_evidence.sh fake-device "$tmp/candidate.apk" "$tmp/$1"; }
expect_failure(){ local rc=0; "$@" || rc=$?; [[ "$rc" != 0 ]]; }
run good
grep -qx MATCH "$tmp/good/apk-identity-status.txt"
python3 - "$tmp/good/scenarios.csv" <<'PY'
import csv,sys
rows=list(csv.DictReader(open(sys.argv[1])))
assert len(rows)==14
assert all(r['status']=='NOT_RUN' for r in rows), rows
PY
expect_failure run good
FAKE_APK="$tmp/wrong.apk" expect_failure run mismatch
[[ ! -f "$tmp/mismatch/apk-identity-status.txt" ]]
FAKE_SPLIT=1 expect_failure run split
FAKE_QEMU=1 expect_failure run emulator
FAKE_STATE=offline expect_failure run offline
echo "PASS A8 harness: matching APK, mismatch, split, emulator, offline, no overwrite, NOT_RUN matrix"
