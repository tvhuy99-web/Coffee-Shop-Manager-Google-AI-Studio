//! Actual external-crate integration tests: only public contracts and Engine API.
use match_engine::contracts::VersionManifest4;
use match_engine::v4::*;
mod fixture {
    use match_engine::v4::*;
    include!("match_fixture.rs");
}
use fixture::make_v4_input;

#[test]
fn public_api_manifest_checkpoint_and_replay_are_compatible() {
    let manifest=VersionManifest4::current_engine_manifest();
    let parsed:VersionManifest4=serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
    manifest.check_simulation_compatibility(&parsed).unwrap();
    let mut incompatible=parsed;
    incompatible.schemas.checkpoint_schema_version+=1;
    assert!(manifest.check_simulation_compatibility(&incompatible).is_err());
    let input=make_v4_input(4242,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut live=MatchSessionV4::new(input.clone()).unwrap();
    live.start_next_period().unwrap();
    live.advance_steps(128).unwrap();
    let checkpoint=live.export_checkpoint().unwrap();
    let bytes=serde_json::to_vec(&checkpoint).unwrap();
    let mut restored=MatchSessionV4::from_checkpoint(serde_json::from_slice(&bytes).unwrap()).unwrap();
    let a=live.run_to_full_time().unwrap();
    let b=restored.run_to_full_time().unwrap();
    assert_eq!(serde_json::to_vec(&a).unwrap(),serde_json::to_vec(&b).unwrap());
    let mut fast_input=input.clone();
    fast_input.simulation_mode=SimulationMode::FastBatch;
    let fast=simulate_v4(fast_input).unwrap();
    assert_eq!(fast.summary,a.summary);
    assert_eq!(fast.event_count,a.event_count);
    assert_eq!(fast.event_timeline_fingerprint,a.event_timeline_fingerprint);
    assert!(fast.events.is_empty());
    assert!(verify_seed_replay_parity(input).unwrap());
    let mut bad=checkpoint;
    bad.payload.state.players[0].pos.x=f32::INFINITY;
    bad.checksum_hex=compute_checkpoint_checksum(&serde_json::to_string(&bad.payload).unwrap());
    assert!(MatchSessionV4::from_checkpoint(bad).is_err());
}

#[test]
fn public_api_calibration_is_observational() {
    let input=make_v4_input(4243,SimulationMode::FullEvent,RulesModeV4::Standard);
    let before=simulate_v4(input.clone()).unwrap();
    let report=run_monte_carlo_v4(vec![input.clone()]).unwrap();
    assert_eq!(report.match_count,1);
    let after=simulate_v4(input).unwrap();
    assert_eq!(serde_json::to_vec(&before).unwrap(),serde_json::to_vec(&after).unwrap());
}
