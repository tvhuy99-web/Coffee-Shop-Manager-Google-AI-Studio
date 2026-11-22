use crate::{
    input::*,
    rules::MatchRules,
    session::{UnifiedMatchEngine, ENGINE_VERSION},
    snapshot::PlayerAttributesSnapshot,
};

const GOLDEN_MATCHES: &str = include_str!("../../../tests/parity/java_full_match_golden.csv");
const GOLDEN_TRACE: &str = include_str!("../../../tests/parity/java_event_trace_golden.csv");

fn cl(x: i32) -> u8 {
    x.clamp(1, 99) as u8
}

fn attrs(base: i32, salt: i32) -> PlayerAttributesSnapshot {
    let gk = if salt == 0 { (base + 18).min(99) as u8 } else { (8 + (salt % 6)) as u8 };
    PlayerAttributesSnapshot::new([
        gk,
        cl(base + (salt % 7) - 3),
        cl(base + ((salt * 3) % 9) - 4),
        cl(base + ((salt * 5) % 11) - 5),
        cl(base + ((salt * 2) % 9) - 4),
        cl(base + ((salt * 4) % 11) - 5),
        cl(base + ((salt * 6) % 13) - 6),
        cl(base + ((salt * 7) % 13) - 6),
        cl(base + ((salt * 8) % 13) - 6),
        cl(base + ((salt * 9) % 11) - 5),
        cl(base + ((salt * 10) % 13) - 6),
        cl(base + ((salt * 11) % 11) - 5),
        cl(base + ((salt * 12) % 13) - 6),
        cl(base + ((salt * 13) % 11) - 5),
        cl(base + ((salt * 14) % 13) - 6),
        cl(base + ((salt * 15) % 13) - 6),
        cl(base + ((salt * 16) % 13) - 6),
        cl(base + ((salt * 17) % 11) - 5),
        cl(base + ((salt * 18) % 13) - 6),
        cl(base + ((salt * 19) % 11) - 5),
    ])
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn build_core_team(
    team_id: &str,
    prefix: &str,
    base: i32,
    pressing: i32,
    tempo: i32,
    line_height: i32,
    width: i32,
    aggression: i32,
    transition_risk: i32,
    build_up_risk: i32,
) -> TeamMatchSnapshot {
    let slots = ["GK", "LB", "LCB", "RCB", "RB", "DM", "LCM", "RCM", "LW", "RW", "CF"];
    let out_pos = [
        (50, 5), (15, 20), (38, 18), (62, 18), (85, 20),
        (50, 38), (35, 50), (65, 50), (15, 70), (85, 70), (50, 78),
    ];
    let in_pos = [
        (50, 8), (15, 38), (38, 28), (62, 28), (85, 38),
        (50, 48), (35, 60), (65, 60), (15, 84), (85, 84), (50, 88),
    ];
    let mut lineup = Vec::with_capacity(11);
    let mut bench = Vec::with_capacity(5);
    let mut assignments = Vec::with_capacity(11);
    let mut tactical_slots = Vec::with_capacity(11);
    let mut slot_instructions = Vec::with_capacity(11);

    for i in 0..11 {
        let id = format!("{}{:02}", prefix, i);
        let ii = i as i32;
        lineup.push(
            PlayerMatchSnapshot::new(
                &id,
                &id,
                attrs(base, ii),
                98 - (ii % 3),
                65 + (ii % 10),
                55,
                21 + (ii % 11),
                false,
                true,
                "",
                "",
            )
            .unwrap(),
        );
        assignments.push(LineupAssignmentSnapshot::new(&id, slots[i]).unwrap());
        tactical_slots.push(TacticalSlotSnapshot::new(slots[i], out_pos[i].0, out_pos[i].1, in_pos[i].0, in_pos[i].1).unwrap());
        let fm = if i == 0 { 20 } else if i <= 4 { 48 } else if i <= 7 { 58 } else { 72 };
        let pr = if i == 0 { 25 } else if i <= 4 { 42 } else if i <= 7 { 55 } else { 68 };
        slot_instructions.push(SlotInstructionsSnapshot::new(slots[i], fm, pr, 55, 55, 55, 50, 35, 60).unwrap());
    }
    for i in 11..16 {
        let id = format!("{}{:02}", prefix, i);
        bench.push(
            PlayerMatchSnapshot::new(&id, &id, attrs(base - 4, i as i32), 95, 60, 50, 23, false, true, "", "")
                .unwrap(),
        );
    }
    let ti = TeamInstructionsSnapshot::new(
        pressing, tempo, line_height, width, 10, false, aggression, transition_risk, build_up_risk, 50,
    )
    .unwrap();
    TeamMatchSnapshot::new(
        team_id,
        team_id,
        lineup,
        bench,
        assignments,
        format!("{}00", prefix),
        "433-v3",
        tactical_slots,
        ti,
        slot_instructions,
        SetPiecePlanSnapshot::empty(),
        TeamCollectiveSnapshot::new(62, 60, 58).unwrap(),
    )
    .unwrap()
}

fn core_rules_input(seed: i64) -> MatchInput {
    let home = build_core_team("HOME", "H", 74, 55, 55, 55, 55, 55, 55, 55);
    let away = build_core_team("AWAY", "A", 74, 50, 50, 50, 50, 50, 50, 50);
    MatchInput::new(
        format!("CORE-{}", seed),
        seed,
        ENGINE_VERSION,
        home,
        away,
        MatchRules::standard(),
        false,
        "CORE_TEST",
        LeagueObjectiveSnapshot::unavailable(),
        LeagueObjectiveSnapshot::unavailable(),
        20,
    )
    .unwrap()
}

#[test]
fn golden_three_matches_and_event_trace_parity() {
    let mut expected_trace_lines = GOLDEN_TRACE.lines().skip(1);
    for line in GOLDEN_MATCHES.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        let seed: i64 = cols[0].parse().unwrap();
        let exp_home_goals: i32 = cols[1].parse().unwrap();
        let exp_away_goals: i32 = cols[2].parse().unwrap();
        let exp_final_sec: i32 = cols[3].parse().unwrap();
        let exp_event_count: i32 = cols[4].parse().unwrap();
        let exp_timeline_fp: i64 = cols[5].parse().unwrap();
        let exp_match_fp: i64 = cols[6].parse().unwrap();

        let result = UnifiedMatchEngine::simulate(core_rules_input(seed)).unwrap();

        for ev in &result.events {
            let expected_line = expected_trace_lines
                .next()
                .unwrap_or_else(|| panic!("Rust emitted extra event at seed {} seq {}", seed, ev.sequence));
            let actual_line = format!(
                "{},{},{},{},{},{}",
                seed,
                ev.sequence,
                ev.game_second,
                ev.kind.as_str(),
                ev.side.map(|s| s.as_str()).unwrap_or("NONE"),
                ev.fingerprint() as i64
            );
            assert_eq!(
                actual_line, expected_line,
                "First event divergence at seed {} seq {} (actor={:?}, target={:?}, ball=({},{}), meta={:?})",
                seed, ev.sequence, ev.actor, ev.target, ev.ball_x, ev.ball_y, ev.metadata
            );
        }

        assert_eq!(result.home_goals, exp_home_goals, "home_goals mismatch for seed {}", seed);
        assert_eq!(result.away_goals, exp_away_goals, "away_goals mismatch for seed {}", seed);
        assert_eq!(result.final_game_second, exp_final_sec, "final_game_second mismatch for seed {}", seed);
        assert_eq!(result.event_count, exp_event_count, "event_count mismatch for seed {}", seed);
        assert_eq!(
            result.event_timeline_fingerprint as i64, exp_timeline_fp,
            "event_timeline_fingerprint mismatch for seed {}",
            seed
        );
        assert_eq!(
            result.fingerprint as i64, exp_match_fp,
            "match fingerprint mismatch for seed {}",
            seed
        );
    }
    assert!(expected_trace_lines.next().is_none(), "Java trace had extra trailing events");
}
