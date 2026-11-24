# Player Intelligence 4.0 — architecture contract and extraction

Status: stages 5.0–5.1 implementation; stage 5.2 requires CI evidence. Baseline `964e56a355637ef9753cdf89cc58a696ad064ca0` ([population evidence](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45351274681)).
Module identity: PI 4.0.0, policy `pi4.0-legacy-equivalent`. This is a source-level module boundary, not a new executable or crate. No new input/checkpoint fields or wire/version changes are introduced by this behavior-preserving extraction.

## Ownership
- PI returns action intentions, decision timing intentions, off-ball movement targets, and GK distribution choices. It receives read-only observations and the existing seeded RNG.
- DES owns event scheduling, cancellation, ordering, execution times and checkpoint/replay.
- Physics owns movement integration, velocity, ball flight, contact/control, outcomes and energy. Rules own legality and score attribution. Coach AI owns team strategy, substitutions and tactical commands.
- No policy function may write match state, resolve a goal, assign ball ownership or apply fatigue; no global clock, new random stream or match-statistics target feedback.
- FullEvent/FastBatch use the same policy. Read-only shared geometry, attributes, tactical context and timing utilities remain engine services during this extraction. Current decision entry point still takes immutable V4 world/input references; this is a migration adapter, not yet a restricted perception DTO. MovementObservation is already a value/borrow-based boundary. No claim of limited information or separate crate until explicitly implemented and tested.

## 5.1 exact extraction scope
- `intent.rs`: existing pass/action/plan types, unchanged discriminants and serialization.
- `decision.rs`: existing candidate generation, utility and decision formulas, unchanged.
- `timing.rs`: existing one-touch policy, unchanged; actual scheduling remains legacy.
- `off_ball.rs`: existing target selection for carrier, goalkeeper, attacking and defending players; movement integration and iteration order remain in V4.
- `goalkeeper.rs`: existing read-only distribution choice, not physical save/claim resolution.
- `version.rs`: separate module and policy identity. Reexports preserve `v4::perception` and `v4::goalkeeper::choose_gk_distribution`. No parallel policy implementation.

Legacy behaviors deliberately retained: discarded decision delay, old one-touch integration, dribble/hold execution, static spatial pressure/lane services, tactical anchoring and RNG coupling. Fix these in separately reviewed behavior commits after parity. Do not add empty speculative modules for prediction, memory, personality, pressing, etc. New components need defined input/output and scenario tests.

## 5.2 verification
Compile the pinned baseline and candidate with the same runner/toolchain in separate target directories. Run the existing 1000 baseline + 128 balanced + 4 x 64 tactical/strength fixtures (1384) and 12 identical FullEvent fixtures (1396 total comparisons). Compare every per-seed result exactly: timeline fingerprint, count, summary including players, diagnostics and, for FullEvent, the complete serialized result/event list. Exclude only runtime wall-clock measurement in the population headline; no numerical tolerances or rounding. Report first divergent seed and fields; fail on any difference or missing match. Independent API and existing scenario tests remain. The frozen-baseline workflow triggers on its introduction/manual dispatch, not every future intentionally changed policy.

## Future policy versions
5.3: integrate action timing into DES and remove duplicate movement/fatigue; then restrict observations and introduce explicit on-ball/off-ball/defensive behavior, shared utility, and traceable decisions. Future behavior changes require policy identity in input/replay/checkpoint metadata and compatibility rules before a release; no silent replay migration. Existing QA-10, GK isolation and Android build issues are tracked separately, not fixed or hidden here.
