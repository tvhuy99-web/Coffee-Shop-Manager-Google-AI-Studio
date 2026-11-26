# A3–A5 incremental extraction

The four-package checkpoint d17062a passed all 19 L0–L4 jobs in run 45625297758. The subsequent candidate creates the full ten named production packages. No external registry dependency change; CI checks original lock graph and exact golden parity.

- match_contracts: pure shared types, event fingerprints, rule vocabulary, units, port signatures.
- player_intelligence: existing intent, utility, timing, support, defensive and off-ball policies. Complex on-ball, tackle/pass forecast and GK compatibility code still in engine.
- coach_intelligence: read-only tactical proposal. Substitution selection/application and command scheduling still require separation.
- match_physics: original trajectory/contact/recovery code. Execution involving world state still in engine.
- player_condition: fatigue, match-day form, post-match morale; remaining psychology integration in engine.
- match_tactics: instruction propagation, blending, press multiplier; shape and phase integration remains engine-side.
- match_rules: original ruleset/timing/restart logic. Rules DTOs and legal application still require further separation.
- match_telemetry: original diagnostics accumulation. Result envelope, stats audit and ratings still in engine.
- match_engine: existing engine directory retained temporarily to keep fixture injection/build paths stable; package now match_engine (rlib only).
- football_match_engine_jni: actual JNI bridge, cdylib named football_match_engine, depends only on engine/contracts internally.

CI enforces explicit local DAG. A3/A4 ownership, full DTO isolation, strict formatting, physical crate directory for match_engine, and integration harness migration are outstanding. Candidate not claimed accepted until all gates green. No behavior version bump, no main merge.

## Final Phase A structural pass
Session transition method groups are mechanically extracted into `session_parts` while retaining the same Engine module/private state, exact method bodies, call ordering and checkpoint format. White-box integration sources live in `rust/tests/engine` using explicit `#[path]` modules (test paths and private access intentionally preserved); public calibration remains a production API. Attribute wiring scans the specified runtime source inventory and session parts instead of compile-time source includes. Static source gates expand implementation includes. New L0 policy boundary check rejects direct engine state/input/RNG references in the leaf PI and Coach crates. Existing compatibility reexports and Kotlin namespaces remain unchanged. Pending CI, no behavior version bump.
