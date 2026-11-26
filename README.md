# PingFootball 4.0

Football simulation foundation: a deterministic Rust DES match engine and Android playback, commentary and audio. [Technical specification and development roadmap](docs/FOOTBALL_SIMULATION_4_SPEC.md).

## Build and test

- Rust: `cd rust && cargo test --workspace --locked --all-targets`.
- Android ARM64: `bash tools/build_lab_rust_android.sh`, then `./gradlew :app:assembleDebug`.
- Commentary: `bash tools/run_commentary_regression.sh`.
- Use the pinned toolchain and locked dependencies. CI checks contracts, Rust, golden parity, attribute audit, Kotlin and Android APK; device acceptance is separate.

## Data and presentation

The `england_v2` assets are development data with simulated rosters, attributes, schedules, contracts and staff; they are not verified current real-world football records. See asset manifests for provenance. Commentary must reflect committed Rust events and use neutral wording or omit cues when evidence is absent. Audio license information is in [AUDIO_LICENSES.md](app/src/main/assets/AUDIO_LICENSES.md).

## Development

Keep the simulation authoritative in Rust. Separate behavior changes from refactoring; do not alter deterministic replay, versions, fixtures or test expectations merely to make checks pass. Begin gameplay development with Physics, Rules and Condition. No production release is implied by the foundation.
