#!/usr/bin/env python3
"""Static validation of the slim Kotlin + Rust Match Lab project.

No Android SDK or Rust compiler is needed for these *static* checks.
They do not prove the Android app or Rust Match Engine can be built or run.
"""

import collections
import hashlib
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "app/src/main"
RUST = ROOT / "rust/football_match_engine/src"
KOTLIN = APP / "java/pl/mkrstudio/truefootball3/lab"
passed = 0
failed = 0


def check(condition, title):
    global passed, failed
    if condition:
        passed += 1
        print(f"PASS {title}")
    else:
        failed += 1
        print(f"FAIL {title}")


check(not list((ROOT / "app/src").rglob("*.java")), "Android app has no Java source files")
check(len(list((APP / "java").rglob("*.kt"))) >= 20, "Kotlin application sources present")
for module in ["clock", "fingerprints", "rng", "lib", "jni_bridge",
               "contracts/mod", "contracts/units", "contracts/ports", "contracts/version",
               "contracts/attributes", "contracts/error", "contracts/input", "contracts/events", "contracts/spatial",
               "v4/session", "v4/rules", "v4/state", "player_intelligence/mod"]:
    check((RUST / f"{module}.rs").is_file(), f"Rust V4 module {module}.rs")

assets = APP / "assets"
check(not (assets / "competition_data_current").exists(), "no legacy competition_data_current")
check(not (assets / "competition_data_2025").exists(), "no legacy competition_data_2025")

# Phase 2: 232 England-pyramid clubs, 11 development divisions, 54 fields.
england_root = assets / "england_v2"
check((england_root / "manifest.json").is_file(), "England V2 manifest present")
check((england_root / "clubs.json").is_file(), "England V2 club catalogue present")
check((england_root / "competition_rules.json").is_file(), "England V2 competitions and career rules present")
check(not (assets / "match_lab/epl20_original.json").exists(), "obsolete EPL20 asset deleted")
if (england_root / "manifest.json").is_file():
    data = json.loads((england_root / "manifest.json").read_text(encoding="utf-8"))
    count = data.get("counts", {})
    check(count.get("clubs") == 232, "232 England-pyramid clubs")
    check(count.get("divisions") == 11, "11 league and development groups")
    check(count.get("players") == 5640, "5,640 first-team profiles")
    check(count.get("fixtures") == 4780, "4,780 simulated fixtures")
    check(count.get("staff") == 4640, "4,640 simulated staff profiles")
    check(count.get("youth") == 2784, "2,784 youth-player profiles")

manifest = ROOT / "AUDIO_SHA256_MANIFEST.txt"
check(manifest.is_file(), "OGG checksum manifest present")
if manifest.is_file():
    entries = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            parts = line.split()
            if len(parts) != 2:
                check(False, f"invalid checksum manifest entry: {line[:80]}")
                continue
            entries.append(parts)
    check(len(entries) == 48, f"exactly 48 checksum entries ({len(entries)})")
    file_names = [Path(name).name for _, name in entries]
    check(len(file_names) == len(set(file_names)), "no duplicate OGG names in manifest")
    for expected, name in entries:
        p = assets / "mod_audio/core" / Path(name).name
        check(p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest() == expected,
              f"OGG checksum: {p.name}")

# Check resource/variable consistency without referring to the deleted Java enum.
commentary = {}
for locale in ("values", "values-vi"):
    strings = {}
    for filename in ("commentary_v2.xml", "commentary_v3.xml"):
        path = APP / "res" / locale / filename
        check(path.is_file(), f"{locale}/{filename} present")
        if not path.is_file():
            continue
        for node in ET.parse(path).getroot().findall("string"):
            key = node.get("name")
            if key in strings:
                check(False, f"duplicate commentary ID {locale}/{key}")
            strings[key] = "".join(node.itertext())
    check(len(strings) == 895, f"{locale} has 895 commentary templates ({len(strings)})")
    commentary[locale] = strings

renderer = KOTLIN / "LabCommentaryEngine.kt"
check(renderer.is_file(), "Kotlin commentary renderer present")
known_variables = set()
if renderer.is_file():
    text = renderer.read_text(encoding="utf-8")
    match = re.search(r'val values = mapOf\((.*?)\n\s*\)', text, re.DOTALL)
    if match:
        known_variables = set(re.findall(r'"([A-Z][A-Z0-9_]*)"\s+to\s+', match.group(1)))
check(len(known_variables) >= 20, f"Kotlin formatter defines known variables ({len(known_variables)})")

if all(commentary.values()):
    en, vi = commentary["values"], commentary["values-vi"]
    check(en.keys() == vi.keys(), "commentary IDs identical across languages")
    tokens = re.compile(r'\{([A-Z][A-Z0-9_]*)\}')
    unknown = set()
    mismatches = []
    legacy = []
    malformed = []
    for key in en.keys() | vi.keys():
        profiles = []
        for locale, mapping in (("values", en), ("values-vi", vi)):
            value = mapping.get(key, "")
            names = tokens.findall(value)
            profiles.append(collections.Counter(names))
            unknown.update(set(names) - known_variables)
            if re.search(r'%(?:[0-9]+\$)?[a-zA-Z]', value):
                legacy.append((locale, key))
            if '{' in tokens.sub('', value) or '}' in tokens.sub('', value):
                malformed.append((locale, key))
        if profiles[0] != profiles[1]:
            mismatches.append(key)
    check(not unknown and "SCORE" not in unknown, f"no unknown resource tokens ({sorted(unknown)})")
    check(not mismatches, f"same token counts in translations (mismatches {len(mismatches)})")
    check(not malformed, f"no malformed resource placeholders ({len(malformed)})")
    check(not legacy, f"no legacy printf placeholders ({len(legacy)})")

main = (KOTLIN / "LabMainActivity.kt").read_text(encoding="utf-8")
gradle = (ROOT / "app/build.gradle").read_text(encoding="utf-8")
runner = (ROOT / "tools/validate_v4_p1_full.sh").read_text(encoding="utf-8")
check("summary.replace(\"\\n\", \" | \")" in main, "Kotlin calibration logger uses escaped newline")
check('summary.replace("\n", " | ")' not in main, "no multiline Kotlin string at prior defect")
check("TtsSettingsActivity::class.java" in main and "SoundSettingsActivity::class.java" in main,
      "separate TTS and sound settings screens")
check("LabReferenceMatchActivity::class.java" in main, "match screen wired")
check("showLogs()" in main and "LabLogStore" in main, "lab logging screen wired")
check("LabCommentaryEngine.kt" in gradle and "CommentaryVariable.java" not in gradle,
      "Gradle commentary validator references Kotlin, not deleted Java")
check("validateCommentaryTokens" in gradle and "dependsOn(tasks.named(\"validateCommentaryTokens\"))" in gradle,
      "commentary validation still runs as a preBuild dependency")
check("run_match_engine_behavior_tests.sh" not in runner, "parity runner does not call deleted script")
check("cargo test --locked --all-targets" in runner, "V4 runner executes locked Rust tests")
check((ROOT / "tools/build_lab_rust_android.sh").is_file(), "Android Rust build script provided")
check((ROOT / "gradle/wrapper/gradle-wrapper.jar").is_file(), "Gradle wrapper JAR included")
check((ROOT / "gradlew").is_file(), "Gradle launcher included")
print(f"CHECKS: {passed + failed}; passed={passed}; failed={failed}")
print("STATIC CHECKS ONLY: does not imply Kotlin/Android/Rust compiled or ran.")
sys.exit(0 if failed == 0 else 1)
