// Shared source fixture: copied verbatim into the pinned baseline checkout by CI.
// Intentionally uses the stable V4 interface rather than PI-specific names.
#[test]
#[ignore = "CI writes full-event parity fixtures"]
fn pi_extraction_full_event_evidence() {
    let mut rows = Vec::new();
    for seed in 92_000i64..92_012 {
        let result = simulate_v4(make_v4_input(seed, SimulationMode::FullEvent, RulesModeV4::Standard))
            .expect("full-event fixture must finish");
        rows.push(serde_json::to_value(result).expect("serialize full result"));
    }
    std::fs::write(std::env::var("PI_FULL_REPORT_PATH").expect("PI_FULL_REPORT_PATH"),
        serde_json::to_string(&rows).unwrap()).expect("write full-event fixtures");
    println!("PI_FULL_EVENT_EVIDENCE {} matches", rows.len());
}
