// Test-only baseline evidence fixture. Included in v4/tests.rs by the A-1 CI
// harness on the exact baseline checkout. Never included in production builds.
#[test]
#[ignore = "A-1 writes full per-seed snapshots under the locked toolchain"]
fn a1_deep_terminal_golden() {
    use crate::v4::checkpoint::{verify_checkpoint, MatchCheckpointV4};
    use std::io::{BufWriter, Write};
    let scenario = std::env::var("A1_SCENARIO").expect("A1_SCENARIO");
    let count: usize = std::env::var("A1_COUNT").expect("A1_COUNT").parse().unwrap();
    let expected = match scenario.as_str() {
        "baseline" => 1000, "balanced" => 128,
        "high_press" | "low_block" | "strong_home" | "strong_away" => 64,
        "full_event" => 12, _ => panic!("unknown cohort {scenario}"),
    };
    assert_eq!(count, expected);
    let path = std::env::var("A1_OUTPUT").expect("A1_OUTPUT");
    let mut out = BufWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..count {
        let seed = if scenario == "full_event" { 92_000 + i as i64 } else { 104_729 + i as i64 * 7907 };
        let input = if scenario == "full_event" {
            make_v4_input(seed, SimulationMode::FullEvent, RulesModeV4::Standard)
        } else {
            let (hl, al) = match scenario.as_str() {
                "baseline" => (14, 12),
                "balanced" | "high_press" | "low_block" => (13, 13),
                "strong_home" => (16, 9), "strong_away" => (9, 16),
                _ => unreachable!(),
            };
            let mut h = make_v4_team("H", "HOME", hl);
            let a = make_v4_team("A", "AWAY", al);
            if scenario == "high_press" {
                h.team_instructions.pressing_intensity = 86;
                h.team_instructions.line_height = 76;
            } else if scenario == "low_block" {
                h.team_instructions.pressing_intensity = 28;
                h.team_instructions.line_height = 30;
            }
            let mut input = make_v4_input_teams(seed, SimulationMode::FastBatch, RulesModeV4::Standard, h, a);
            if scenario != "baseline" {
                input.ai_home_manager = false;
                input.ai_away_manager = false;
                input.neutral_ground = true;
            }
            input
        };
        let mut session = MatchSessionV4::new(input).unwrap();
        let initial = session.export_checkpoint().unwrap();
        verify_checkpoint(&initial).unwrap();
        let result = session.run_to_full_time().unwrap();
        assert!(result.ok);
        result.diagnostics.as_ref().expect("diagnostics").validate_basic().unwrap();
        let terminal = session.export_checkpoint().unwrap();
        let decoded: MatchCheckpointV4 = serde_json::from_str(&serde_json::to_string(&terminal).unwrap()).unwrap();
        verify_checkpoint(&decoded).unwrap();
        assert_eq!(terminal.seed, seed);
        let row = serde_json::json!({
            "scenario": scenario, "seed": seed, "externalCommands": [],
            "initial": initial, "result": result, "terminal": terminal,
        });
        serde_json::to_writer(&mut out, &row).unwrap();
        writeln!(out).unwrap();
    }
    out.flush().unwrap();
    println!("A1_DEEP_TERMINAL {scenario} {count}");
}

#[test]
#[ignore = "A-1 exports and restores named checkpoint states"]
fn a1_deep_checkpoint_markers() {
    use crate::v4::checkpoint::{verify_checkpoint, FlightActionV4, MatchCheckpointV4};
    use crate::v4::events::SessionStageV4;
    use std::collections::BTreeSet;
    use std::io::{BufWriter, Write};
    let wanted = ["live", "pass_flight", "tackle_loose", "goal_celebration", "pending_tactics", "halftime", "period_end"];
    let mut covered = BTreeSet::new();
    let path = std::env::var("A1_OUTPUT").expect("A1_OUTPUT");
    let mut out = BufWriter::new(std::fs::File::create(path).unwrap());
    // Fixed seed search, never adjust gameplay to manufacture a missing state.
    for seed in [14012i64, 14013].into_iter().chain(92_000..92_012) {
        let mut input = make_v4_input(seed, SimulationMode::FullEvent, RulesModeV4::Standard);
        input.ai_home_manager = false;
        let mut bench = input.home_team.lineup[8].clone();
        bench.player_id = "A1-SUB-0".into();
        input.home_team.bench.push(bench);
        let mut a = MatchSessionV4::new(input.clone()).unwrap();
        let mut b = MatchSessionV4::new(input).unwrap();
        let initial = a.export_checkpoint().unwrap();
        verify_checkpoint(&initial).unwrap();
        let mut markers = Vec::new();
        let mut commands = Vec::new();
        let mut submitted = false;
        for _ in 0..100_000 {
            let sa = a.advance_steps(1).unwrap();
            let sb = b.advance_steps(1).unwrap();
            assert_eq!(serde_json::to_string(&sa).unwrap(), serde_json::to_string(&sb).unwrap(),
                "step divergence seed={seed}, time={}", sa.sim_time_ms);
            if !submitted && sa.completed_beats >= 35 && sa.dead_ball_state == "LIVE_PLAY" && sa.stage == SessionStageV4::InHalf {
                let mut ti = TeamInstructionsV4::default();
                ti.pressing_intensity = 77;
                let cmds = [
                    MatchCommandV4::UpdateTactics { side: TeamSideV4::Home, instructions: ti },
                    MatchCommandV4::Substitute { side: TeamSideV4::Home, player_out_id: "H08".into(), player_in_id: "A1-SUB-0".into() },
                ];
                for cmd in cmds {
                    a.submit_command(cmd.clone()).unwrap();
                    b.submit_command(cmd.clone()).unwrap();
                    commands.push(serde_json::json!({"step":sa.completed_beats,"simTimeMs":sa.sim_time_ms,"command":cmd}));
                }
                submitted = true;
            }
            let inspect = (sa.dead_ball_state == "LIVE_PLAY" && (!covered.contains("live") || !covered.contains("pass_flight") || !covered.contains("tackle_loose") || (submitted && !covered.contains("pending_tactics"))))
                || (sa.dead_ball_state.starts_with("GoalCelebration::") && !covered.contains("goal_celebration"))
                || (sa.stage == SessionStageV4::HalfTimeBreak && !covered.contains("halftime"))
                || (sa.is_full_time && !covered.contains("period_end"));
            if inspect {
                let checkpoint = a.export_checkpoint().unwrap();
                let mut labels = Vec::new();
                if sa.stage == SessionStageV4::InHalf && sa.dead_ball_state == "LIVE_PLAY" { labels.push("live"); }
                if let Some(flight) = &checkpoint.payload.pending_flight {
                    match &flight.action {
                        FlightActionV4::Pass { .. } => labels.push("pass_flight"),
                        FlightActionV4::LooseRecovery { cause, .. } if cause == "TACKLE_CONTACT_LOOSE" => labels.push("tackle_loose"),
                        _ => {}
                    }
                }
                if sa.dead_ball_state.starts_with("GoalCelebration::") { labels.push("goal_celebration"); }
                if checkpoint.payload.pending_tactical_commands.iter().any(|c| c.actual_effective_sim_time_ms.is_none()) { labels.push("pending_tactics"); }
                if sa.stage == SessionStageV4::HalfTimeBreak { labels.push("halftime"); }
                if sa.is_full_time { labels.push("period_end"); }
                labels.retain(|label| !covered.contains(label));
                if !labels.is_empty() {
                    let raw = serde_json::to_string(&checkpoint).unwrap();
                    assert_eq!(raw, serde_json::to_string(&b.export_checkpoint().unwrap()).unwrap(),
                        "checkpoint divergence seed={seed}, labels={labels:?}");
                    let parsed: MatchCheckpointV4 = serde_json::from_str(&raw).unwrap();
                    verify_checkpoint(&parsed).unwrap();
                    b = MatchSessionV4::from_checkpoint(parsed).unwrap();
                    markers.push(serde_json::json!({"labels":labels,"checkpoint":checkpoint}));
                    for label in labels { covered.insert(label); }
                }
            }
            if sa.is_full_time { break; }
            if sa.stage == SessionStageV4::HalfTimeBreak || sa.stage == SessionStageV4::ExtraTimeBreak {
                let pa = a.start_next_period().unwrap();
                let pb = b.start_next_period().unwrap();
                assert_eq!(serde_json::to_string(&pa).unwrap(), serde_json::to_string(&pb).unwrap());
            }
        }
        assert!(a.is_full_time() && b.is_full_time(), "watchdog seed={seed}");
        assert!(submitted, "commands never submitted seed={seed}");
        let ra = a.run_to_full_time().unwrap();
        let rb = b.run_to_full_time().unwrap();
        assert_eq!(serde_json::to_string(&ra).unwrap(), serde_json::to_string(&rb).unwrap(),
            "restored result divergence seed={seed}");
        let ca = a.export_checkpoint().unwrap();
        let cb = b.export_checkpoint().unwrap();
        assert_eq!(serde_json::to_string(&ca).unwrap(), serde_json::to_string(&cb).unwrap(),
            "restored terminal checkpoint divergence seed={seed}");
        verify_checkpoint(&ca).unwrap();
        let row = serde_json::json!({"scenario":"checkpoint_markers","seed":seed,"externalCommands":commands,
            "initial":initial,"markers":markers,"result":ra,"terminal":ca});
        serde_json::to_writer(&mut out, &row).unwrap();
        writeln!(out).unwrap();
        if wanted.iter().all(|label| covered.contains(label)) { break; }
    }
    out.flush().unwrap();
    assert!(wanted.iter().all(|label| covered.contains(label)), "missing checkpoint marker: covered={covered:?}");
    println!("A1_DEEP_MARKERS {covered:?}");
}
