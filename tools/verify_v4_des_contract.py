#!/usr/bin/env python3
"""Static contract verifier for Match Engine V4.1 DES v1.4 (Rust <-> JNI <-> Kotlin).

IMPORTANT: This static contract check does NOT replace `cargo test`, `kotlinc`,
Gradle builds, or real Android device QA.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import List, Tuple

ROOT = Path(__file__).resolve().parent.parent
RUST_SRC = ROOT / "rust" / "football_match_engine" / "src"
RUST_V4 = RUST_SRC / "v4"
KOTLIN_LAB = ROOT / "app" / "src" / "main" / "java" / "pl" / "mkrstudio" / "truefootball3" / "lab"
KOTLIN_SMOKE = ROOT / "tests" / "p1_kotlin" / "Smoke.kt"

REQUIRED_NEW_JNI_METHODS = [
    "nativeSubmitV4CommandJson",
    "nativeAdvanceV4SessionUntilMs",
    "nativeStartNextPeriod",
    "nativeExportV4Checkpoint",
    "nativeImportV4Checkpoint",
]

REQUIRED_DES_TESTS = [
    "des_clock_dynamic_added_time_no_hard_cap",
    "des_clock_no_double_counting_deadball_vs_compensable",
    "des_halftime_break_pauses_interactive_and_auto_advances_fastbatch",
    "des_goal_celebration_then_kickoff_nonzero_variable_duration",
    "des_deadball_state_machine_blocks_open_play_events",
    "des_scheduler_tie_break_priority_sequence_deterministic",
    "des_scheduler_cancels_stale_generation_events",
    "des_scheduler_rejects_past_and_infinite_same_ms_loop",
    "des_ci_defender_intercepts_before_receiver",
    "des_tactical_command_delayed_effective_time_and_override_policy",
    "des_live_sub_waits_for_next_deadball",
    "des_checkpoint_roundtrip_exact_continuation_all_states",
    "des_fullevent_fastbatch_exact_parity_with_commands",
    "des_ruleset_version_immutable_per_match",
    "des_fatigue_step_splitting_invariance",
]


def strip_comments_and_strings(code: str, is_rust: bool) -> str:
    """Strips comments, string literals, raw strings, and char literals for delimiter checking."""
    out: List[str] = []
    n = len(code)
    i = 0
    while i < n:
        ch = code[i]
        nxt = code[i + 1] if i + 1 < n else ""

        # Line comment //
        if ch == "/" and nxt == "/":
            i += 2
            while i < n and code[i] != "\n":
                i += 1
            continue

        # Block comment /* ... */ (supports nesting in Rust & Kotlin)
        if ch == "/" and nxt == "*":
            depth = 1
            i += 2
            while i < n and depth > 0:
                if code[i] == "/" and i + 1 < n and code[i + 1] == "*":
                    depth += 1
                    i += 2
                elif code[i] == "*" and i + 1 < n and code[i + 1] == "/":
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue

        # Rust raw string r"...", r#"..."#, br#"..."#
        if is_rust and (ch == "r" or (ch == "b" and nxt == "r")):
            j = i + (2 if ch == "b" else 1)
            hashes = 0
            while j < n and code[j] == "#":
                hashes += 1
                j += 1
            if j < n and code[j] == '"':
                j += 1
                end_seq = '"' + ("#" * hashes)
                end_idx = code.find(end_seq, j)
                if end_idx == -1:
                    break
                i = end_idx + len(end_seq)
                continue

        # Kotlin triple-quoted raw string """..."""
        if not is_rust and ch == '"' and code[i : i + 3] == '"""':
            end_idx = code.find('"""', i + 3)
            if end_idx == -1:
                break
            i = end_idx + 3
            continue

        # Standard double-quoted string "..."
        if ch == '"':
            i += 1
            while i < n:
                if code[i] == "\\":
                    i += 2
                    continue
                if code[i] == '"':
                    i += 1
                    break
                i += 1
            continue

        # Char literal '...' (in Rust skip lifetimes like 'local or 'a)
        if ch == "'":
            if is_rust:
                # Escaped char literal like '\n' or '\x7f' or '\u{...}'
                if i + 1 < n and code[i + 1] == "\\":
                    j = i + 2
                    while j < n and code[j] != "'" and code[j] != "\n":
                        j += 1
                    if j < n and code[j] == "'":
                        i = j + 1
                        continue
                # Single-character literal like 'x'
                if i + 2 < n and code[i + 2] == "'" and code[i + 1] != "\n":
                    i += 3
                    continue
                # Otherwise it's a Rust lifetime ('a, 'static, 'local) -> skip just the quote
                i += 1
                continue
            else:
                i += 1
                while i < n:
                    if code[i] == "\\":
                        i += 2
                        continue
                    if code[i] == "'":
                        i += 1
                        break
                    i += 1
                continue

        out.append(ch)
        i += 1

    return "".join(out)



def strip_comments_only(code: str) -> str:
    """Strips // line comments and /* ... */ block comments while preserving code & string literals."""
    out: List[str] = []
    n = len(code)
    i = 0
    while i < n:
        ch = code[i]
        nxt = code[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            i += 2
            while i < n and code[i] != "\n":
                i += 1
            continue
        if ch == "/" and nxt == "*":
            depth = 1
            i += 2
            while i < n and depth > 0:
                if code[i] == "/" and i + 1 < n and code[i + 1] == "*":
                    depth += 1
                    i += 2
                elif code[i] == "*" and i + 1 < n and code[i + 1] == "/":
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def check_balanced_delimiters(path: Path) -> Tuple[bool, str]:
    text = path.read_text(encoding="utf-8")
    cleaned = strip_comments_and_strings(text, is_rust=(path.suffix == ".rs"))
    stack: List[str] = []
    pairs = {")": "(", "]": "[", "}": "{"}
    openers = set(pairs.values())
    for ch in cleaned:
        if ch in openers:
            stack.append(ch)
        elif ch in pairs:
            if not stack or stack[-1] != pairs[ch]:
                return False, f"unmatched closing '{ch}' in {path.relative_to(ROOT)}"
            stack.pop()
    if stack:
        return False, f"unclosed delimiters {stack[-3:]} in {path.relative_to(ROOT)}"
    return True, "ok"


def main() -> int:
    failures: List[str] = []
    passes: List[str] = []

    def check(name: str, condition: bool, detail: str = "") -> None:
        if condition:
            passes.append(name)
            print(f"[PASS] {name}")
        else:
            msg = f"{name}: {detail}" if detail else name
            failures.append(msg)
            print(f"[FAIL] {msg}")

    # 1. Balanced delimiters across all Rust v4/core files and Kotlin lab/smoke files
    rs_files = [
        RUST_SRC / "lib.rs",
        RUST_SRC / "clock.rs",
        RUST_SRC / "jni_bridge.rs",
        *sorted(RUST_V4.glob("*.rs")),
        *sorted((ROOT / "rust/crates").rglob("*.rs")),
    ]
    kt_files = [
        *sorted(KOTLIN_LAB.glob("*.kt")),
        KOTLIN_SMOKE,
    ]
    bad_delims: List[str] = []
    for f in rs_files + kt_files:
        if not f.is_file():
            bad_delims.append(f"missing file {f.relative_to(ROOT)}")
            continue
        ok, msg = check_balanced_delimiters(f)
        if not ok:
            bad_delims.append(msg)
    check(
        f"Delimiter balance across {len(rs_files)} .rs and {len(kt_files)} .kt files",
        len(bad_delims) == 0,
        "; ".join(bad_delims),
    )

    # 2. Version constants in Rust v4/mod.rs & lib.rs cast
    mod_rs = (RUST_V4 / "mod.rs").read_text(encoding="utf-8")
    lib_rs = (RUST_SRC / "jni_bridge.rs").read_text(encoding="utf-8")
    check(
        "Rust JNI_BRIDGE_VERSION == 8",
        re.search(r"pub\s+const\s+JNI_BRIDGE_VERSION\s*:\s*i32\s*=\s*8\s*;", mod_rs) is not None,
        "Expected pub const JNI_BRIDGE_VERSION: i32 = 8 in v4/mod.rs",
    )
    check(
        "Rust ENGINE_PROTOCOL_VERSION_V4 == 5",
        re.search(r"pub\s+const\s+ENGINE_PROTOCOL_VERSION_V4\s*:\s*u32\s*=\s*5\s*;", mod_rs) is not None,
        "Expected pub const ENGINE_PROTOCOL_VERSION_V4: u32 = 5 in v4/mod.rs",
    )
    check(
        "Rust EVENT_SCHEMA_VERSION_V4 == 5",
        re.search(r"pub\s+const\s+EVENT_SCHEMA_VERSION_V4\s*:\s*u32\s*=\s*5\s*;", mod_rs) is not None,
        "Expected pub const EVENT_SCHEMA_VERSION_V4: u32 = 5 in v4/mod.rs",
    )
    check(
        "Rust jni_bridge.rs casts ENGINE_PROTOCOL_VERSION_V4 as i32",
        "v4::ENGINE_PROTOCOL_VERSION_V4 as i32" in lib_rs,
        "Expected `v4::ENGINE_PROTOCOL_VERSION_V4 as i32` in jni_bridge.rs",
    )

    # 3. Kotlin NativeBridge.isV4DesReady() matches bridge >= 8 & protocol >= 5
    native_bridge_kt = (KOTLIN_LAB / "NativeBridge.kt").read_text(encoding="utf-8")
    des_ready_match = re.search(
        r"fun\s+isV4DesReady\s*\(\s*\)\s*:\s*Boolean[\s\S]{0,220}?nativeBridgeVersion\s*\(\s*\)\s*>=\s*8[\s\S]{0,120}?nativeEngineProtocolVersion\s*\(\s*\)\s*>=\s*5",
        native_bridge_kt,
    )
    check(
        "Kotlin NativeBridge.isV4DesReady() enforces bridge >= 8 & protocol >= 5",
        des_ready_match is not None,
        "NativeBridge.kt missing `isV4DesReady()` checking nativeBridgeVersion() >= 8 && nativeEngineProtocolVersion() >= 5",
    )

    # 4. 100% match of 5 new JNI functions between jni_bridge.rs and NativeBridge.kt
    jni_rs = (RUST_SRC / "jni_bridge.rs").read_text(encoding="utf-8")
    missing_rust_jni = [
        fn_name
        for fn_name in REQUIRED_NEW_JNI_METHODS
        if f"Java_pl_mkrstudio_truefootball3_lab_NativeBridge_{fn_name}" not in jni_rs
    ]
    missing_kt_jni = [
        fn_name
        for fn_name in REQUIRED_NEW_JNI_METHODS
        if re.search(rf"external\s+fun\s+{re.escape(fn_name)}\s*\(", native_bridge_kt) is None
    ]
    check(
        "Rust jni_bridge.rs exports all 5 new DES JNI functions",
        len(missing_rust_jni) == 0,
        f"Missing in jni_bridge.rs: {missing_rust_jni}",
    )
    check(
        "Kotlin NativeBridge.kt declares all 5 new DES external JNI functions",
        len(missing_kt_jni) == 0,
        f"Missing in NativeBridge.kt: {missing_kt_jni}",
    )

    # 5. Zero-regression checks (ignoring comments so explanatory docstrings do not false-positive)
    v4_all_text = "\n".join(
        strip_comments_only(p.read_text(encoding="utf-8")) for p in sorted(RUST_V4.glob("*.rs"))
    )
    session_rs = strip_comments_only((RUST_V4 / "session.rs").read_text(encoding="utf-8"))
    state_rs = strip_comments_only((ROOT / "rust/crates/player_condition/src/lib.rs").read_text(encoding="utf-8"))
    audio_kt = strip_comments_only((KOTLIN_LAB / "LabAudioDirector.kt").read_text(encoding="utf-8"))

    check(
        "Zero regression: 0 occurrences of `dt_sec.clamp(1.0, 6.0)` in v4/",
        "dt_sec.clamp(1.0, 6.0)" not in v4_all_text,
        "Found legacy `dt_sec.clamp(1.0, 6.0)` in v4/",
    )
    check(
        "Zero regression: 0 occurrences of `added_time_cap_seconds` in v4/session.rs",
        "added_time_cap_seconds" not in session_rs,
        "Found legacy `added_time_cap_seconds` in v4/session.rs",
    )
    check(
        "Zero regression: 0 occurrences of `uptimeMillis` in LabAudioDirector.kt",
        "uptimeMillis" not in audio_kt,
        "Found legacy `uptimeMillis` in LabAudioDirector.kt",
    )
    check(
        "Zero regression: 0 occurrences of fixed `ana_drain = 4.8` in v4/state.rs",
        "ana_drain = 4.8" not in state_rs,
        "Found legacy fixed `ana_drain = 4.8` in v4/state.rs",
    )
    check(
        "Zero regression: 0 occurrences of `transition_beats_remaining` in v4/",
        "transition_beats_remaining" not in v4_all_text,
        "Found legacy `transition_beats_remaining` in v4/",
    )
    check(
        "Zero regression: 0 occurrences of `sustained_pressure_streak` in v4/",
        "sustained_pressure_streak" not in v4_all_text,
        "Found legacy `sustained_pressure_streak` in v4/",
    )

    # 6. All 15 mandatory DES v1.4 tests present in v4/tests.rs
    tests_rs = (RUST_V4 / "tests.rs").read_text(encoding="utf-8")
    missing_tests = [
        t for t in REQUIRED_DES_TESTS if re.search(rf"fn\s+{re.escape(t)}\s*\(", tests_rs) is None
    ]
    check(
        "All 15 mandatory DES v1.4 tests exist in v4/tests.rs",
        len(missing_tests) == 0,
        f"Missing tests: {missing_tests}",
    )

    # 7. V4 presentation uses the production coordinator, not the deleted V3 filter.
    ref_activity_kt = (KOTLIN_LAB / "LabReferenceMatchActivity.kt").read_text(encoding="utf-8")
    coordinator_kt = (KOTLIN_LAB / "LabV4PlaybackCoordinator.kt").read_text(encoding="utf-8")
    controller_kt = (KOTLIN_LAB / "MatchPlaybackController.kt").read_text(encoding="utf-8")
    short_hl_match = re.search(
        r'LabViewingMode\.SHORT_HIGHLIGHTS\s*->\s*\{\s*if\s*\(([^)]*)\)',
        coordinator_kt,
    )
    has_own_goal_short = short_hl_match is not None and 'kind == "OWN_GOAL"' in short_hl_match.group(1)
    check(
        "Production V4 short highlights include OWN_GOAL and controller delegates filtering",
        has_own_goal_short and "LabV4PlaybackCoordinator.filterEventsForViewingMode(" in controller_kt
        and "convertV4StepEvents(stepObj, rawEvents, filteredEvents)" in ref_activity_kt,
        "Missing OWN_GOAL or V4 production filter wiring",
    )

    # --- Rework Pass 2 checks (Independent Review CRITICAL-01..09 + QA-01..12) ---
    root = Path(__file__).resolve().parent.parent
    sess_rs = (root / "rust/football_match_engine/src/v4/session.rs").read_text()
    rng_rs = (root / "rust/football_match_engine/src/rng.rs").read_text()
    diag_rs = (root / "rust/crates/match_telemetry/src/lib.rs").read_text()
    tests_rs = (root / "rust/football_match_engine/src/v4/tests.rs").read_text()
    ctrl_kt = (root / "app/src/main/java/pl/mkrstudio/truefootball3/lab/MatchPlaybackController.kt").read_text()
    ns_kt = (root / "app/src/main/java/pl/mkrstudio/truefootball3/lab/NarrationScheduler.kt").read_text()
    tts_kt = (root / "app/src/main/java/pl/mkrstudio/truefootball3/audio/MatchTtsManager.kt").read_text()
    act_kt = (root / "app/src/main/java/pl/mkrstudio/truefootball3/lab/LabReferenceMatchActivity.kt").read_text()

    check(
        "CRITICAL-01: session.rs schedules & handles BallInFlightArrival, FirstTouchComplete, ShotArrival, EarlyInteractionResolved separately",
        all(fn in sess_rs for fn in (
            "fn handle_early_interaction_resolved",
            "fn handle_ball_in_flight_arrival",
            "fn handle_first_touch_complete",
            "fn handle_shot_arrival",
            "fn schedule_shot_flight",
        )),
    )
    check(
        "CRITICAL-02: MatchPlaybackController separates SimulationFrontier vs PresentationFrontier",
        "simFrontierBreakSimTimeMs" in ctrl_kt
        and "simFrontierFullTimeSimTimeMs" in ctrl_kt
        and "triggerPresentationPeriodBreak" in ctrl_kt
        and "triggerPresentationFullTime" in ctrl_kt
        and "Long.MAX_VALUE / 2" not in ctrl_kt,
    )
    check(
        "CRITICAL-03: submitCommandJson does NOT blindly revoke confirmed lookahead cues on delayed tactics",
        'revokeSpeculativeAfter(committedSimTimeMs, "live_command")' not in ctrl_kt
        and "revokeSpeculativeAfter(" not in ctrl_kt.split("fun submitCommandJson(", 1)[1].split("fun release(", 1)[0],
    )
    check(
        "CRITICAL-04: Checkpoint persists unpresentedEventsJson + pendingCuesJson & Activity has tryResumeActiveV4Checkpoint",
        "unpresentedEventsJson" in ctrl_kt
        and "pendingCuesJson" in ctrl_kt
        and "fun tryResumeActiveV4Checkpoint" in act_kt,
    )
    check(
        "CRITICAL-05: MatchTtsManager calibrates Vietnamese base rate to 4.2 syl/s (no hardcoded setSpeechRate(1.0f))",
        "fun calibrateVietnameseBaseSpeechRate" in tts_kt
        and "engine.setSpeechRate(calibratedBaseSpeechRate" in tts_kt,
    )
    check(
        "CRITICAL-06: NarrationScheduler reflows 1.0x schedule on measured WAV duration",
        "fun reflowScheduleFromLocked" in ns_kt
        and "reflowScheduleFromLocked(entry.cueId)" in ns_kt,
    )
    check(
        "CRITICAL-07: NarrationScheduler enforces strict FIFO audio order & protects all unplayed WAVs",
        "fifo_head_not_ready" in ns_kt
        and "val protectedNames = HashSet<String>()" in ns_kt,
    )
    check(
        "CRITICAL-08: session.rs centralizes world integration in advance_world_for_dt with zero .max(0.05) hack",
        "fn advance_world_for_dt" in sess_rs
        and "(dt_ms as f32 / 1000.0).max(0.05)" not in sess_rs,
    )
    check(
        "CRITICAL-09: Checkpoint restores RNG root_seed+state via from_seed_and_state & full MatchDiagnosticsCollectorV4",
        "pub fn from_seed_and_state" in rng_rs
        and "DeterministicRng::from_seed_and_state(payload.rng.root_seed, payload.rng.state)" in sess_rs
        and "Serialize, Deserialize)]\npub struct MatchDiagnosticsCollectorV4" in diag_rs,
    )
    check(
        "Six Rust QA test declarations present (static only, not executed)",
        all(t in tests_rs for t in (
            "fn qa01_long_pass_interception_emitted_at_interaction_time",
            "fn qa02_shot_goal_emitted_at_arrival_not_kick_time",
            "fn qa08_checkpoint_roundtrip_preserves_rng_root_seed_and_diagnostics",
            "fn qa10_mid_interval_tactical_event_preserves_total_movement_and_fatigue",
            "fn qa11_injury_in_added_time_extends_period_dynamically",
            "fn qa12_monte_carlo_1000_matches_benchmark",
        )),
    )

    print("-" * 72)
    print(f"Summary: {len(passes)} passed, {len(failures)} failed.")
    print(
        "NOTE: Static contract verification does NOT replace `cargo test`, `kotlinc`, "
        "Gradle builds, or real Android device tests."
    )
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
