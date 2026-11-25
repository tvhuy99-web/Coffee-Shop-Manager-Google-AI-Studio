// A-1 test-only companion to deep_golden_fixture.rs.
// The earlier FastBatch golden contains a fingerprint rather than retained events.
// Replay its exact normalized input in FullEvent and retain every gameplay event.
// Cross-mode parity must hold; a differing result/state is a real red gate.
#[test]
#[ignore = "CI captures every event of the frozen FastBatch population in FullEvent"]
fn a1_full_event_population_trace() {
    use std::io::{BufRead, BufReader, BufWriter, Write};
    use serde_json::{json, Value};
    let input_path = std::env::var("A1_DEEP_REFERENCE").expect("deep golden reference");
    let output_path = std::env::var("A1_OUTPUT").expect("output");
    let cohort = std::env::var("A1_SCENARIO").expect("cohort");
    let expected: usize = std::env::var("A1_COUNT").unwrap().parse().unwrap();
    let reader = BufReader::new(std::fs::File::open(input_path).unwrap());
    let mut out = BufWriter::new(std::fs::File::create(output_path).unwrap());
    let mut count = 0;
    for line in reader.lines() {
        let old: Value = serde_json::from_str(&line.unwrap()).unwrap();
        assert_eq!(old["scenario"].as_str(), Some(cohort.as_str()));
        let seed = old["seed"].as_i64().unwrap();
        let mut input: MatchInputV4 = serde_json::from_value(old["initial"]["payload"]["input"].clone()).unwrap();
        assert_eq!(input.seed, seed);
        assert_eq!(input.simulation_mode, SimulationMode::FastBatch);
        input.simulation_mode = SimulationMode::FullEvent;
        let mut session = MatchSessionV4::new(input).unwrap();
        let result = session.run_to_full_time().unwrap();
        let actual = serde_json::to_value(&result).unwrap();
        let previous = &old["result"];
        for key in ["ok","seed","eventCount","eventTimelineFingerprint","summary","diagnostics","commandTimeline"] {
            assert_eq!(actual.get(key), previous.get(key), "cross-mode divergence cohort={cohort} seed={seed} result field={key}");
        }
        assert_eq!(result.events.len(), result.event_count as usize, "missing event seed={seed}");
        let checkpoint = session.export_checkpoint().unwrap();
        crate::v4::verify_checkpoint(&checkpoint).unwrap();
        let new_terminal = serde_json::to_value(&checkpoint).unwrap();
        let p = &new_terminal["payload"];
        let q = &old["terminal"]["payload"];
        for key in ["state","rng","scheduler","appliedCommands","pendingSubstitutions","pendingTacticalCommands","pendingFlight","pendingDecision","pendingCarry","tackleReadyAt","nextEventSequence","halfStage","piPolicyId"] {
            assert_eq!(p.get(key), q.get(key), "cross-mode divergence cohort={cohort} seed={seed} terminal field={key}");
        }
        // The original checkpoints and normalized inputs remain preserved in deep evidence;
        // don't duplicate the entire retained event vector in this companion trace.
        let row = json!({
            "scenario": cohort, "seed": seed,
            "fastBatchInputFingerprint": previous["inputFingerprint"],
            "fullEventInputFingerprint": actual["inputFingerprint"],
            "eventTimelineFingerprint": actual["eventTimelineFingerprint"],
            "eventCount": result.event_count, "events": result.events,
            "fastBatchTerminalChecksum": old["terminal"]["checksumHex"],
            "fullEventTerminalChecksum": checkpoint.checksum_hex,
        });
        serde_json::to_writer(&mut out, &row).unwrap();
        writeln!(out).unwrap();
        count += 1;
    }
    out.flush().unwrap();
    assert_eq!(count, expected);
    println!("A1_ALL_EVENT_TRACE {cohort} {count}");
}
