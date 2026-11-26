# A8 real-device acceptance harness
Status: BLOCKED until a real target Android device and approved ADR-009 thresholds are available. Run tools/capture_a8_device_evidence.sh SERIAL APK OUTPUT_DIR after installing the exact candidate. It records APK hash, source commit, model/ABI/OS, installed package metadata, battery, thermal, memory and frames, and creates a NOT_RUN scenario matrix. No installation or destructive operation is performed. Repeat captures after each scenario in a distinct folder; verify installed APK hash independently before marking PASS. Manual checks remain: force-stop during writes/TTS, audio focus/call, latency/cue drift, replay continuation and exact score. Linux host timings from A4 are not Android measurements.

The collector now rejects emulators, split installations and an installed base APK whose SHA-256 differs from the candidate. It captures audio focus/process information and does not mark scenarios PASS. Fake-ADB tests only validate the collector's gates; they are not hardware acceptance.

## Handoff
Download `football-match-lab-v1.0-stable-debug` from a successful final L0-L4 run; it includes APK, SHA-256, commit, package, signature, JNI exports. Install only the verified debug APK (`pl.mkrstudio.truefootball3.debug`); keep existing release app/data separate. Host reports are in `l1-rust` and the automated acceptance record in `phase-a-automated-acceptance`. Neither artifact approves release or establishes hardware performance. The `/usr/bin/time -v` host result measures the entire cargo test command including build if any, not the Android app or isolated native peak RSS.

## Frozen phone-test candidate
Use [Vietnamese phone checklist](PHONE_CHECKLIST_VI.md), commit e540c51, run 45747583976, APK artifact 15083733591. Actual APK hash matches f84c13f. Manual feedback is explicitly separate from L5; a single successful match or fake-adb is not sufficient.

## Current status — supersedes earlier BLOCKED status for project acceptance
2026-11-26: **ACCEPTED_BY_OWNER**. See [owner acceptance and reported device results](OWNER_ACCEPTANCE_VI.md). The owner requested acceptance after phone testing; formal instrumented L5 remains NOT_FULLY_VERIFIED and individual unreported scenarios remain NOT_RUN. No claim of independently checked installed hash, release approval, or complete device thresholds. Retain the harness for future use; do not require additional device checks for the owner-accepted A8 closure.
