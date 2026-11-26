pub(super) fn make_v4_team_with_attrs<F>(prefix: &str, team_id: &str, mut attr_fn: F) -> TeamMatchSnapshotV4
where
    F: FnMut(usize) -> PlayerAttributes54,
{
    let slots = [
        ("GK", 50, 5, 50, 8, "GK"),
        ("LB", 15, 20, 15, 38, "LB"),
        ("LCB", 38, 18, 38, 28, "CB"),
        ("RCB", 62, 18, 62, 28, "CB"),
        ("RB", 85, 20, 85, 38, "RB"),
        ("DM", 50, 38, 50, 48, "DM"),
        ("LCM", 35, 50, 35, 60, "CM"),
        ("RCM", 65, 50, 65, 60, "CM"),
        ("LW", 15, 70, 15, 84, "LW"),
        ("RW", 85, 70, 85, 84, "RW"),
        ("CF", 50, 78, 50, 88, "CF"),
    ];
    let mut lineup = Vec::with_capacity(11);
    let mut assignments = Vec::with_capacity(11);
    let mut tactical_slots = Vec::with_capacity(11);
    let mut slot_instructions = Vec::with_capacity(11);

    for (i, &(sid, ox, oy, ix, iy, pos)) in slots.iter().enumerate() {
        let pid = format!("{}{:02}", prefix, i);
        lineup.push(PlayerMatchSnapshotV4 {
            player_id: pid.clone(),
            display_name: pid.clone(),
            attributes: attr_fn(i),
            condition: 98,
            morale: 70,
            form: 65,
            age: 24,
            injured: false,
            eligible: true,
            primary_position: pos.into(),
            secondary_position: String::new(),
            preferred_foot: PreferredFoot::Right,
            weak_foot: 3,
            height_cm: 181,
        });
        assignments.push(LineupAssignmentV4 {
            player_id: pid,
            slot_id: sid.into(),
        });
        tactical_slots.push(TacticalSlotV4 {
            slot_id: sid.into(),
            out_x: ox,
            out_y: oy,
            in_x: ix,
            in_y: iy,
        });
        slot_instructions.push(SlotInstructionsV4 {
            slot_id: sid.into(),
            forward_movement: 55,
            passing_risk: 55,
            crossing_tendency: 50,
            dribbling_tendency: 50,
            defensive_aggression: 55,
            support_distance: 50,
            roaming: 35,
            shooting_tendency: 55,
        });
    }

    TeamMatchSnapshotV4 {
        team_id: team_id.into(),
        display_name: team_id.into(),
        shape_id: "433-v4".into(),
        designated_goalkeeper_id: format!("{}00", prefix),
        lineup,
        bench: Vec::new(),
        assignments,
        tactical_slots,
        team_instructions: TeamInstructionsV4::default(),
        slot_instructions,
        set_piece_plan: SetPiecePlanV4::default(),
        collective_state: TeamCollectiveV4::default(),
    }
}

pub(super) fn make_v4_team(prefix: &str, team_id: &str, attr_level: u8) -> TeamMatchSnapshotV4 {
    make_v4_team_with_attrs(prefix, team_id, |_| {
        PlayerAttributes54::uniform(attr_level).unwrap()
    })
}

pub(super) fn make_v4_input_teams(
    seed: i64,
    mode: SimulationMode,
    rules: RulesModeV4,
    home: TeamMatchSnapshotV4,
    away: TeamMatchSnapshotV4,
) -> MatchInputV4 {
    MatchInputV4 {
        protocol_version: ENGINE_PROTOCOL_VERSION_V4 as i32,
        match_id: format!("V4-TEST-{}", seed),
        seed,
        engine_version: ENGINE_VERSION_V4.into(),
        neutral_ground: false,
        competition_context: "ENG_1".into(),
        rules_mode: rules,
        ruleset: RulesetConfig::from_rules_mode(rules),
        timing_model: TimingModelConfig::des_v1_default(),
        simulation_mode: mode,
        simulated_action_minutes: None,
        ai_home_manager: false,
        ai_away_manager: true,
        match_environment: MatchEnvironment::default(),
        referee_profile: RefereeProfile::default(),
        home_team: home,
        away_team: away,
    }
}

pub(super) fn make_v4_input(seed: i64, mode: SimulationMode, rules: RulesModeV4) -> MatchInputV4 {
    make_v4_input_teams(
        seed,
        mode,
        rules,
        make_v4_team("H", "HOME_V4", 14),
        make_v4_team("A", "AWAY_V4", 12),
    )
}

