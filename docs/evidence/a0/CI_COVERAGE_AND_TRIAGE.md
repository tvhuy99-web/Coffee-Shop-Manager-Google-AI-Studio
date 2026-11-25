# A0 – CI coverage and failure triage (preparation only)

Baseline: `c2c392d8b3ccc57985e87337e8994fd7033328ee` (PI 4.0.28). This is a coverage plan, not an A0 acceptance report. No existing gate is removed before replacement and baseline evidence. A-1 remains the prerequisite for integrating A0 changes.

## Existing workflow coverage and proposed destination

| Original workflow | Current coverage | Destination / preservation obligation |
|---|---|---|
| validate-port.yml | verify_lab_v2, saved Java V3 fixture parity, Rust tests, 329 commentary cases, Gradle tokens | L0 static/resources; L1 Rust; L4 commentary; retain historical V3 evidence and parity until A1 |
| validate-v4-p1.yml | P0/P1 static, Kotlin smoke, Rust all targets | L0 valid structural rules; L1 Rust; L2 commands/checkpoints; L4 actual Kotlin tests; no optional tool skip |
| validate-v4-p2.yml | P2 static, Rust, 50 counterfactual probes, 128 realism, attacking sample | L0/L1; L3 diagnostic reports with original seed/metric semantics; never silently remove |
| validate-des-repair.yml | DES contracts, playback and commentary JVM, Rust tests, 128/1000 Monte Carlo, ARM64 + debug APK | L0/L1/L3/L4; reuse actual reports and build once |
| build-apk.yml | data, Java fixture verification, Rust, native ARM64, stable signed Debug APK and inspection | L0/L1/L4; preserve package/ABI/signature checks; do not expose release signing secrets |
| validate-v4-android.yml | duplicate pre-merge debug build | L4 single build on intended integration branch; preserve independent artifact and signature checks |
| diagnose-v4-des.yml | population 1,384 FastBatch, attacking diagnosis, aggregate/per-seed evidence | L2 exact locked parity; L3 behavioral evidence with raw rows and checksums |
| pi-attribute-audit.yml | isolated mechanisms + 54 attributes, 904 matches | L1 isolated tests; L3 audit (5 groups); distinguish 50 gameplay/4 career; always preserve raw reports |
| pi4-extraction-parity.yml | previous 964e56a3 baseline, 1,396 comparisons, interface tests | Preserve prior historical comparator/evidence; L2 refactor parity against frozen 4.0.28, no golden overwrite |
| ci-runner-recovery-probe.yml | runner smoke and targeted psychology, commands, checkpoint tests | Infrastructure diagnostic separate from required gates; retain focused tests in L1/L2 |

L0–L4 must run without missing-tool skips. Rust dependencies and compiler are locked. L5 remains device acceptance, not asserted by JVM tests. The 1,396 prior comparisons are 1,384 FastBatch + 12 FullEvent; do not double count. Deep golden and per-state continuation are additional evidence.

## Reproduced CI failures

### Rust: [run 45593934894](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45593934894/job/139134958569)
185 passed, 2 failed, 8 ignored.
- `pi4_legacy_api_and_direct_policy_match`: the actual 64-seed equality and RNG assertions precede a stale literal 4.0.27. Baseline version.rs declares 4.0.28 and policy `pi4.0-second-ball-state-value`; `PI_BASELINE_COMMIT` is older PI extraction provenance, not the A-1 refactor baseline. Replace literal test with independent frozen manifest compatibility checks and negative policy/schema/checksum cases; preserve all 64 comparisons.
- `p0_every_goal_is_followed_by_opponent_kickoff_at_centre`: line 1143 unwraps a search for the next Goal/Kickoff, without considering HalfTime/FullTime. This is a hypothesis for the None, not a proven cause. Before changing anything, emit seed, goal side/time/period and subsequent events through the next boundary for seeds 71001..71011; distinguish a legitimate period end from missing restart. Assert opponent kickoff and centre/actor when play resumes in the same period; explicitly require valid boundary if no restart. Include own goals and ensure the negative case of missing kickoff during live play fails. Any actual engine fix requires separate behavior-change review and preserved golden, never quietly rewrite golden.

### P1 static: [run 45606261634](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45606261634/job/139177782602)
P0/P1 static 12/12 PASS, P1-2/3/4 static 17/22. Five failures:

| Failed assertion | Source inspection | Replacement / evidence required |
|---|---|---|
| Same-stoppage multi-person choice | Activity lines 550–620 already use setMultiChoiceItems, chooseV4IncomingPlayers and one SUBSTITUTE_BATCH payload. Submission uses playbackController.submitCommandJson, not old submitSubstitutions call. | Assert actual batch path + runtime test for two changes, duplicate/over-limit rejection, authoritative roster, window counter. Existing Smoke.kt verifies wire batch; Rust tests verify atomic/window semantics; device UI still pending. |
| At most one beat ahead | MatchPlaybackController uses DES millisecond lookahead and presentation frontier, not one beat. | Replace stale source check with production-controller tests verifying no early event/score/break/full time and safe bounded lookahead behavior. Preserve existing behavior per ADR-006; do not add speculative rollback in A. |
| Queued commands gated on presented beat | Controller submitCommandJson accepts at confirmed native frontier; old beat names no longer exist. | Test acceptance/apply timing and presentation ordering, no cue duplication/early score; no claim of command rollback. |
| No continued presentation during command dialog | Activity lines 404–426 pause/resume playbackController for V4, removeCallbacks(v3Tick) for legacy. | Verify actual pause/resume integration, deterministic JVM clock assertions while paused, preserve resume conditions. |
| No silent overwrite of different match | Store lines 165–175 checks existing.record == record, rejects changed records, legacy incompatible and corrupt archives; old string previous == record is obsolete. | Test identical save idempotence, different content rejection, legacy/corrupt rejection, file bytes unchanged. Smoke.kt already checks collisions; strengthen evidence rather than changing valid implementation. |

The evidence so far is source inspection plus failed static checks; actual JVM/UI behavior must still be tested. Neither substantive assertion is assumed to require a production code change merely because its string check failed.

### Missing tools
validate_v4_p1_full.sh currently prints WARNING and skips kotlinc/cargo if absent. A0 must fail hard before running checks if any tool is missing and use cargo --locked, preserving all tests. Add missing-tool negative tests and check a nonzero test propagates. Do not count the static-only 17/22 as behavioral evidence.

## Runner status
The GITHUB_TOKEN-authored lockfile commit generated several pull_request runs with action_required and no jobs, not evidence of failed tests. A user-authored change now triggers the committed-lock rerun. The deep golden branch has queued jobs. No A0 implementation or merge is authorized by this document alone; transition only once A-1 passes.
