use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite, PlayerAttributes54};
use crate::v4::error::V4Error;
use crate::v4::events::EventTypeV4;
use crate::v4::goalkeeper::{resolve_gk_shot_reaction, GkShotOutcome};
use crate::v4::input::{
    LineupAssignmentV4, MatchEnvironment, MatchInputV4, PlayerMatchSnapshotV4, PreferredFoot,
    RefereeProfile, RulesModeV4, SetPiecePlanV4, SimulationMode, SlotInstructionsV4,
    TacticalSlotV4, TeamCollectiveV4, TeamInstructionsV4, TeamMatchSnapshotV4, WeatherCondition,
};
use crate::v4::session::{simulate_batch_fast_v4, simulate_v4};
use crate::v4::spatial::{
    attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box,
    is_in_attacking_six_yard_box, point_to_segment_distance_m, TeamSideV4, Vec2, Vec3,
    PITCH_LENGTH_M, PITCH_WIDTH_M,
};
use crate::v4::{ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, REGULATION_SECONDS_V4};

fn make_v4_team_with_attrs<F>(prefix: &str, team_id: &str, mut attr_fn: F) -> TeamMatchSnapshotV4
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

fn make_v4_team(prefix: &str, team_id: &str, attr_level: u8) -> TeamMatchSnapshotV4 {
    make_v4_team_with_attrs(prefix, team_id, |_| {
        PlayerAttributes54::uniform(attr_level).unwrap()
    })
}

fn make_v4_input_teams(
    seed: i64,
    mode: SimulationMode,
    rules: RulesModeV4,
    home: TeamMatchSnapshotV4,
    away: TeamMatchSnapshotV4,
) -> MatchInputV4 {
    MatchInputV4 {
        protocol_version: ENGINE_PROTOCOL_VERSION_V4,
        match_id: format!("V4-TEST-{}", seed),
        seed,
        engine_version: ENGINE_VERSION_V4.into(),
        neutral_ground: false,
        competition_context: "ENG_1".into(),
        rules_mode: rules,
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

fn make_v4_input(seed: i64, mode: SimulationMode, rules: RulesModeV4) -> MatchInputV4 {
    make_v4_input_teams(
        seed,
        mode,
        rules,
        make_v4_team("H", "HOME_V4", 14),
        make_v4_team("A", "AWAY_V4", 12),
    )
}

#[test]
fn attributes_54_validation_and_nonlinear_curve() {
    assert!(PlayerAttributes54::uniform(1).is_ok());
    assert!(PlayerAttributes54::uniform(20).is_ok());
    assert!(matches!(
        PlayerAttributes54::uniform(0),
        Err(V4Error::InvalidAttribute { .. })
    ));
    assert!(matches!(
        PlayerAttributes54::uniform(21),
        Err(V4Error::InvalidAttribute { .. })
    ));

    for v in 1..20 {
        assert!(normalize_attr(v) < normalize_attr(v + 1));
        assert!(normalize_attr_elite(v) < normalize_attr_elite(v + 1));
    }
    let elite_gap = normalize_attr_elite(20) - normalize_attr_elite(16);
    let std_gap = normalize_attr(20) - normalize_attr(16);
    assert!(elite_gap > std_gap);
}

#[test]
fn attributes_deserialize_england_v2_json_54_keys() {
    let raw = r#"{
        "technical": {
            "finishing": 15, "passing": 14, "dribbling": 16, "technique": 15,
            "first_touch": 15, "crossing": 12, "heading": 11, "long_shots": 13,
            "tackling": 7, "marking": 6, "free_kicks": 12, "penalties": 14, "corners": 11
        },
        "mental": {
            "vision": 15, "decisions": 14, "composure": 16, "concentration": 13,
            "off_the_ball": 17, "positioning": 8, "anticipation": 15, "teamwork": 13,
            "determination": 15, "work_rate": 14, "leadership": 11, "flair": 16,
            "aggression": 9, "bravery": 10
        },
        "physical": {
            "pace": 17, "acceleration": 18, "strength": 12, "stamina": 15,
            "balance": 14, "agility": 16, "jumping_reach": 11, "natural_fitness": 15
        },
        "goalkeeping": {
            "reflexes": 3, "handling": 2, "one_on_ones": 2, "command_of_area": 2,
            "aerial_reach": 3, "kicking": 5, "throwing": 4, "rushing_out": 2, "punching": 2
        },
        "hidden": {
            "professionalism": 14, "ambition": 16, "consistency": 15, "big_matches": 14,
            "adaptability": 13, "loyalty": 12, "pressure": 15, "injury_proneness": 7,
            "temperament": 13, "sportsmanship": 12
        }
    }"#;
    let parsed: PlayerAttributes54 = serde_json::from_str(raw).unwrap();
    assert!(parsed.validate().is_ok());
    assert_eq!(parsed.all_named().len(), 54);
}

#[test]
fn spatial_105x68_and_ball_height_math() {
    let home_box = Vec2::new(96.0, 34.0);
    assert!(is_in_attacking_penalty_box(TeamSideV4::Home, home_box));
    assert!(!is_in_attacking_penalty_box(TeamSideV4::Away, home_box));
    assert!((distance_to_attacking_goal(TeamSideV4::Home, home_box) - 9.0).abs() < 1e-4);

    let away_six = Vec2::new(4.0, 34.0);
    assert!(is_in_attacking_six_yard_box(TeamSideV4::Away, away_six));
    assert!((attacking_progress_m(TeamSideV4::Away, away_six) - 101.0).abs() < 1e-4);

    let slot_center = Vec2::from_normalized_slot(TeamSideV4::Home, 50.0, 50.0);
    assert!((slot_center.x - PITCH_LENGTH_M * 0.5).abs() < 1e-4);
    assert!((slot_center.y - PITCH_WIDTH_M * 0.5).abs() < 1e-4);

    let d = point_to_segment_distance_m(
        Vec2::new(50.0, 40.0),
        Vec2::new(40.0, 34.0),
        Vec2::new(60.0, 34.0),
    );
    assert!((d - 6.0).abs() < 1e-4);

    let aerial = Vec3::new(92.0, 34.0, 2.2);
    assert!(aerial.z > 2.0);
}

#[test]
fn rejects_compressed_20_minute_mode() {
    let mut input = make_v4_input(42, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.simulated_action_minutes = Some(20);
    assert!(matches!(
        simulate_v4(input),
        Err(V4Error::UnsupportedCompressionMode)
    ));
}

#[test]
fn full_90_minute_determinism_invariants_and_fast_batch_equivalence() {
    let input_full = make_v4_input(20260417, SimulationMode::FullEvent, RulesModeV4::Standard);
    let r1 = simulate_v4(input_full.clone()).unwrap();
    let r2 = simulate_v4(input_full.clone()).unwrap();

    assert!(r1.summary.final_game_second >= REGULATION_SECONDS_V4);
    assert_eq!(r1.event_timeline_fingerprint, r2.event_timeline_fingerprint);
    assert_eq!(r1.event_count, r2.event_count);
    assert_eq!(r1.summary, r2.summary);
    assert_eq!(r1.events.len() as u32, r1.event_count);

    for ev in &r1.events {
        assert!(ev.ball_pos.x >= 0.0 && ev.ball_pos.x <= PITCH_LENGTH_M);
        assert!(ev.ball_pos.y >= 0.0 && ev.ball_pos.y <= PITCH_WIDTH_M);
        assert!(ev.ball_pos.z >= 0.0);
    }

    let mut input_fast = input_full;
    input_fast.simulation_mode = SimulationMode::FastBatch;
    let rf = simulate_v4(input_fast).unwrap();
    assert!(rf.events.is_empty());
    assert_eq!(rf.event_count, r1.event_count);
    assert_eq!(rf.event_timeline_fingerprint, r1.event_timeline_fingerprint);
    assert_eq!(rf.summary, r1.summary);
}

#[test]
fn parallel_rayon_batch_matches_sequential_deterministically() {
    let inputs: Vec<MatchInputV4> = (1..=16)
        .map(|s| make_v4_input(s * 97, SimulationMode::FastBatch, RulesModeV4::Standard))
        .collect();
    let batch = simulate_batch_fast_v4(inputs.clone());
    assert_eq!(batch.len(), inputs.len());
    for (inp, res) in inputs.into_iter().zip(batch.into_iter()) {
        let par_res = res.unwrap();
        let seq_res = simulate_v4(inp).unwrap();
        assert_eq!(
            par_res.event_timeline_fingerprint,
            seq_res.event_timeline_fingerprint
        );
        assert_eq!(par_res.summary, seq_res.summary);
    }
}

// ============================================================================
// Stage B & Stage C Attribute Impact Tests
// ============================================================================

#[test]
fn stage_b_passing_attribute_impact_test() {
    let mut low_comp = 0u32;
    let mut low_att = 0u32;
    let mut high_comp = 0u32;
    let mut high_att = 0u32;

    for seed in 1..=20i64 {
        let home_low = make_v4_team_with_attrs("H", "HOME_LOW_PASS", |_| {
            let mut a = PlayerAttributes54::uniform(10).unwrap();
            a.technical.passing = 5;
            a
        });
        let home_high = make_v4_team_with_attrs("H", "HOME_HIGH_PASS", |_| {
            let mut a = PlayerAttributes54::uniform(10).unwrap();
            a.technical.passing = 18;
            a
        });
        let away = make_v4_team("A", "AWAY_BASE", 10);

        let r_low = simulate_v4(make_v4_input_teams(
            seed * 131,
            SimulationMode::FastBatch,
            RulesModeV4::Standard,
            home_low,
            away.clone(),
        ))
        .unwrap();
        let r_high = simulate_v4(make_v4_input_teams(
            seed * 131,
            SimulationMode::FastBatch,
            RulesModeV4::Standard,
            home_high,
            away,
        ))
        .unwrap();

        low_comp += r_low.summary.home.passes_completed as u32;
        low_att += r_low.summary.home.passes_attempted as u32;
        high_comp += r_high.summary.home.passes_completed as u32;
        high_att += r_high.summary.home.passes_attempted as u32;
    }

    let low_pct = (low_comp as f32) / (low_att.max(1) as f32);
    let high_pct = (high_comp as f32) / (high_att.max(1) as f32);
    assert!(
        high_pct > low_pct + 0.04,
        "Expected passing=18 ({high_pct:.3}) to beat passing=5 ({low_pct:.3}) by >4%"
    );
}

#[test]
fn stage_b_first_touch_reduces_miscontrol_errors() {
    let mut low_errors = 0u32;
    let mut high_errors = 0u32;

    for seed in 1..=20i64 {
        let home_low = make_v4_team_with_attrs("H", "HOME_LOW_FT", |_| {
            let mut a = PlayerAttributes54::uniform(11).unwrap();
            a.technical.first_touch = 4;
            a
        });
        let home_high = make_v4_team_with_attrs("H", "HOME_HIGH_FT", |_| {
            let mut a = PlayerAttributes54::uniform(11).unwrap();
            a.technical.first_touch = 19;
            a
        });
        let away = make_v4_team("A", "AWAY_BASE", 12);

        let r_low = simulate_v4(make_v4_input_teams(
            seed * 257,
            SimulationMode::FullEvent,
            RulesModeV4::Standard,
            home_low,
            away.clone(),
        ))
        .unwrap();
        let r_high = simulate_v4(make_v4_input_teams(
            seed * 257,
            SimulationMode::FullEvent,
            RulesModeV4::Standard,
            home_high,
            away,
        ))
        .unwrap();

        low_errors += r_low
            .events
            .iter()
            .filter(|e| {
                e.kind == EventTypeV4::FirstTouch
                    && e.side == Some(TeamSideV4::Home)
                    && e.metadata.outcome.as_deref() == Some("HEAVY_TOUCH_LOST")
            })
            .count() as u32;
        high_errors += r_high
            .events
            .iter()
            .filter(|e| {
                e.kind == EventTypeV4::FirstTouch
                    && e.side == Some(TeamSideV4::Home)
                    && e.metadata.outcome.as_deref() == Some("HEAVY_TOUCH_LOST")
            })
            .count() as u32;
    }

    assert!(
        low_errors > high_errors,
        "Expected first_touch=4 errors ({low_errors}) > first_touch=19 errors ({high_errors})"
    );
}

#[test]
fn stage_c_gk_handling_reduces_rebounds_in_rain() {
    let mut input = make_v4_input(101, SimulationMode::FastBatch, RulesModeV4::Standard);
    input.match_environment.weather = WeatherCondition::HeavyRain;

    let mut gk_low = PlayerAttributes54::uniform(12).unwrap();
    gk_low.goalkeeping.reflexes = 15;
    gk_low.goalkeeping.handling = 4;

    let mut gk_high = PlayerAttributes54::uniform(12).unwrap();
    gk_high.goalkeeping.reflexes = 15;
    gk_high.goalkeeping.handling = 19;

    let mut low_held = 0u32;
    let mut low_rebound = 0u32;
    let mut high_held = 0u32;
    let mut high_rebound = 0u32;

    let mut rng_l = DeterministicRng::new(7777);
    let mut rng_h = DeterministicRng::new(7777);

    for _ in 0..400 {
        match resolve_gk_shot_reaction(&gk_low, 1.0, 90.0, 0.22, 13.0, false, false, false, &input, &mut rng_l) {
            GkShotOutcome::SavedHeld => low_held += 1,
            GkShotOutcome::SavedParriedRebound => low_rebound += 1,
            _ => {}
        }
        match resolve_gk_shot_reaction(&gk_high, 1.0, 90.0, 0.22, 13.0, false, false, false, &input, &mut rng_h) {
            GkShotOutcome::SavedHeld => high_held += 1,
            GkShotOutcome::SavedParriedRebound => high_rebound += 1,
            _ => {}
        }
    }

    assert!(
        high_held > low_held && low_rebound > high_rebound,
        "Expected handling=19 (held={high_held}, reb={high_rebound}) to hold cleaner than handling=4 (held={low_held}, reb={low_rebound})"
    );
}

#[test]
fn stage_c_gk_reflexes_reduces_goals_conceded() {
    let input = make_v4_input(202, SimulationMode::FastBatch, RulesModeV4::Standard);
    let mut gk_low = PlayerAttributes54::uniform(12).unwrap();
    gk_low.goalkeeping.reflexes = 4;
    let mut gk_high = PlayerAttributes54::uniform(12).unwrap();
    gk_high.goalkeeping.reflexes = 19;

    let mut low_conceded = 0u32;
    let mut high_conceded = 0u32;
    let mut rng_l = DeterministicRng::new(9999);
    let mut rng_h = DeterministicRng::new(9999);

    for _ in 0..400 {
        if resolve_gk_shot_reaction(&gk_low, 1.0, 90.0, 0.28, 9.0, false, false, false, &input, &mut rng_l)
            == GkShotOutcome::GoalConceded
        {
            low_conceded += 1;
        }
        if resolve_gk_shot_reaction(&gk_high, 1.0, 90.0, 0.28, 9.0, false, false, false, &input, &mut rng_h)
            == GkShotOutcome::GoalConceded
        {
            high_conceded += 1;
        }
    }

    assert!(
        low_conceded > high_conceded + 20,
        "Expected reflexes=4 conceded ({low_conceded}) > reflexes=19 conceded ({high_conceded}) by >20"
    );
}

#[test]
fn stage_c_finishing_boosts_team_goal_conversion() {
    let mut low_goals = 0u32;
    let mut high_goals = 0u32;

    for seed in 1..=24i64 {
        let home_low = make_v4_team_with_attrs("H", "HOME_LOW_FIN", |_| {
            let mut a = PlayerAttributes54::uniform(12).unwrap();
            a.technical.finishing = 4;
            a.technical.long_shots = 4;
            a
        });
        let home_high = make_v4_team_with_attrs("H", "HOME_HIGH_FIN", |_| {
            let mut a = PlayerAttributes54::uniform(12).unwrap();
            a.technical.finishing = 19;
            a.technical.long_shots = 18;
            a
        });
        let away = make_v4_team("A", "AWAY_BASE", 11);

        let rl = simulate_v4(make_v4_input_teams(
            seed * 521,
            SimulationMode::FastBatch,
            RulesModeV4::Standard,
            home_low,
            away.clone(),
        ))
        .unwrap();
        let rh = simulate_v4(make_v4_input_teams(
            seed * 521,
            SimulationMode::FastBatch,
            RulesModeV4::Standard,
            home_high,
            away,
        ))
        .unwrap();

        low_goals += rl.summary.home.goals as u32;
        high_goals += rh.summary.home.goals as u32;
    }

    assert!(
        high_goals > low_goals,
        "Expected finishing=19 goals ({high_goals}) > finishing=4 goals ({low_goals})"
    );
}

// ============================================================================
// Stage D Tactical Identity & Adaptive AI Manager Tests
// ============================================================================

#[test]
fn stage_d_high_press_vs_low_block_measurable_tradeoff() {
    let mut press_aero_sum = 0.0f32;
    let mut low_aero_sum = 0.0f32;
    let mut high_line_flag_seen = false;

    for seed in 1..=12i64 {
        let mut home_press = make_v4_team("H", "HOME_PRESS", 12);
        home_press.team_instructions.pressing_intensity = 86;
        home_press.team_instructions.line_height = 76;

        let mut home_low = make_v4_team("H", "HOME_LOW", 12);
        home_low.team_instructions.pressing_intensity = 28;
        home_low.team_instructions.line_height = 30;

        let away = make_v4_team("A", "AWAY_NEUTRAL", 12);

        let mut inp_press = make_v4_input_teams(
            seed * 613,
            SimulationMode::FullEvent,
            RulesModeV4::Standard,
            home_press,
            away.clone(),
        );
        inp_press.ai_home_manager = false;
        inp_press.ai_away_manager = false;

        let mut inp_low = make_v4_input_teams(
            seed * 613,
            SimulationMode::FastBatch,
            RulesModeV4::Standard,
            home_low,
            away,
        );
        inp_low.ai_home_manager = false;
        inp_low.ai_away_manager = false;

        let rp = simulate_v4(inp_press).unwrap();
        let rl = simulate_v4(inp_low).unwrap();

        if rp.events.iter().any(|e| e.metadata.high_defensive_line == Some(true)) {
            high_line_flag_seen = true;
        }

        press_aero_sum += rp
            .summary
            .players
            .iter()
            .filter(|p| p.side == TeamSideV4::Home)
            .map(|p| p.final_aerobic)
            .sum::<f32>();
        low_aero_sum += rl
            .summary
            .players
            .iter()
            .filter(|p| p.side == TeamSideV4::Home)
            .map(|p| p.final_aerobic)
            .sum::<f32>();
    }

    assert!(high_line_flag_seen, "Expected highDefensiveLine metadata flag under high line");
    assert!(
        press_aero_sum < low_aero_sum,
        "Expected High Press to drain more stamina ({press_aero_sum:.1}) than Low Block ({low_aero_sum:.1})"
    );
}

#[test]
fn stage_d_ai_manager_executes_substitutions_deterministically() {
    let mut home = make_v4_team("H", "HOME_SUBS", 12);
    for p in &mut home.lineup {
        p.condition = 72; // ensures fatigue drops below AI sub threshold in second half
    }
    for b in 0..5 {
        home.bench.push(PlayerMatchSnapshotV4 {
            player_id: format!("HB{:02}", b),
            display_name: format!("Home Sub {}", b),
            attributes: PlayerAttributes54::uniform(12).unwrap(),
            condition: 98,
            morale: 70,
            form: 65,
            age: 22,
            injured: false,
            eligible: true,
            primary_position: "CM".into(),
            secondary_position: "CF".into(),
            preferred_foot: PreferredFoot::Right,
            weak_foot: 3,
            height_cm: 180,
        });
    }
    let away = make_v4_team("A", "AWAY_SUBS", 12);

    let mut inp_full = make_v4_input_teams(
        424242,
        SimulationMode::FullEvent,
        RulesModeV4::Standard,
        home,
        away,
    );
    inp_full.ai_home_manager = true;

    let rf = simulate_v4(inp_full.clone()).unwrap();
    let sub_count = rf
        .events
        .iter()
        .filter(|e| e.kind == EventTypeV4::Substitution && e.side == Some(TeamSideV4::Home))
        .count();
    assert!(
        (1..=5).contains(&sub_count),
        "Expected AI manager to make 1..=5 substitutions, got {sub_count}"
    );

    let mut inp_fast = inp_full;
    inp_fast.simulation_mode = SimulationMode::FastBatch;
    let rfast = simulate_v4(inp_fast).unwrap();
    assert_eq!(rf.event_timeline_fingerprint, rfast.event_timeline_fingerprint);
    assert_eq!(rf.summary, rfast.summary);
}

// ============================================================================
// Stage E Psychology, Two-Pool Fatigue Recovery & Career Carryover Tests
// ============================================================================

#[test]
fn stage_e_consistency_narrows_variance_and_big_matches_activates() {
    use crate::v4::psychology::compute_prematch_form_multiplier;

    let mut low_cons = PlayerAttributes54::uniform(12).unwrap();
    low_cons.hidden.consistency = 3;
    let mut high_cons = PlayerAttributes54::uniform(12).unwrap();
    high_cons.hidden.consistency = 19;

    let mut rng_l = DeterministicRng::new(12345);
    let mut rng_h = DeterministicRng::new(12345);
    let mut low_spread = 0.0f32;
    let mut high_spread = 0.0f32;

    for _ in 0..200 {
        low_spread += (compute_prematch_form_multiplier(&low_cons, 50, &mut rng_l) - 1.0).abs();
        high_spread += (compute_prematch_form_multiplier(&high_cons, 50, &mut rng_h) - 1.0).abs();
    }
    assert!(
        low_spread > high_spread * 2.0,
        "Expected low consistency variance ({low_spread:.3}) > 2x high consistency ({high_spread:.3})"
    );

    let mut big_star = PlayerAttributes54::uniform(12).unwrap();
    big_star.hidden.big_matches = 20;
    big_star.hidden.consistency = 20;
    let mut big_shy = PlayerAttributes54::uniform(12).unwrap();
    big_shy.hidden.big_matches = 2;
    big_shy.hidden.consistency = 20;

    let mut rng_s = DeterministicRng::new(555);
    let mut rng_w = DeterministicRng::new(555);
    let star_m = compute_prematch_form_multiplier(&big_star, 95, &mut rng_s);
    let shy_m = compute_prematch_form_multiplier(&big_shy, 95, &mut rng_w);
    assert!(star_m > shy_m + 0.08, "Expected big_matches=20 ({star_m:.3}) > big_matches=2 ({shy_m:.3}) in final");
}

#[test]
fn stage_e_natural_fitness_speeds_anaerobic_recovery_and_carryover_is_valid() {
    use crate::v4::state::TwoPoolFatigueState;

    let mut low_nat = TwoPoolFatigueState::from_condition(95);
    let mut high_nat = TwoPoolFatigueState::from_condition(95);

    // Drain anaerobic via high-intensity burst, then recover over 10s
    low_nat.apply_interval(5.0, 25.0, true, 12, 4);
    high_nat.apply_interval(5.0, 25.0, true, 12, 19);
    low_nat.apply_interval(10.0, 12.0, false, 12, 4);
    high_nat.apply_interval(10.0, 12.0, false, 12, 19);
    assert!(
        high_nat.anaerobic > low_nat.anaerobic + 2.0,
        "Expected natural_fitness=19 anaerobic ({:.2}) > natural_fitness=4 ({:.2})",
        high_nat.anaerobic,
        low_nat.anaerobic
    );

    let res = simulate_v4(make_v4_input(909090, SimulationMode::FastBatch, RulesModeV4::Standard)).unwrap();
    assert!(!res.summary.players.is_empty());
    for p in &res.summary.players {
        assert!((5..=100).contains(&p.post_match_condition));
        assert!((-12..=12).contains(&p.morale_delta));
    }
}

// ============================================================================
// Stage F Monte Carlo Balance, FastBatch Throughput & Global Seed-Replay Tests
// ============================================================================

#[test]
fn stage_f_monte_carlo_64_matches_and_seed_replay_parity() {
    use crate::v4::calibration::{run_monte_carlo_v4, verify_seed_replay_parity};

    // 1. Verify Seed-Replay parity (FastBatch == FullEvent)
    let sample = make_v4_input(20261123, SimulationMode::FastBatch, RulesModeV4::Standard);
    assert!(verify_seed_replay_parity(sample).unwrap());

    // 2. Run 64 balanced 90-minute matches in parallel via Rayon
    let inputs: Vec<MatchInputV4> = (1..=64i64)
        .map(|i| {
            make_v4_input_teams(
                i * 1009,
                SimulationMode::FastBatch,
                RulesModeV4::Standard,
                make_v4_team("H", "HOME_EQ", 13),
                make_v4_team("A", "AWAY_EQ", 13),
            )
        })
        .collect();
    let report = run_monte_carlo_v4(inputs).unwrap();
    assert_eq!(report.match_count, 64);
    assert!(report.avg_shots_per_match > 5.0, "low shot generation, report={report:?}");
    assert!(report.pass_accuracy_pct > 55.0 && report.pass_accuracy_pct < 96.0);

    // 3. Stronger team (+5 attr delta across board) beats weaker team decisively
    let mismatched: Vec<MatchInputV4> = (1..=40i64)
        .map(|i| {
            make_v4_input_teams(
                i * 2027,
                SimulationMode::FastBatch,
                RulesModeV4::Standard,
                make_v4_team("H", "STRONG_FC", 16),
                make_v4_team("A", "WEAK_FC", 9),
            )
        })
        .collect();
    let mismatch_report = run_monte_carlo_v4(mismatched).unwrap();
    assert!(
        mismatch_report.home_win_rate_pct > mismatch_report.away_win_rate_pct + 20.0,
        "Expected strong team win rate ({:.1}%) to decisively beat weak team ({:.1}%)",
        mismatch_report.home_win_rate_pct,
        mismatch_report.away_win_rate_pct
    );
}

#[test]
#[ignore]
fn stage_f_monte_carlo_1000_matches_benchmark() {
    use crate::v4::calibration::run_monte_carlo_v4;

    let inputs: Vec<MatchInputV4> = (1..=1000i64)
        .map(|i| {
            make_v4_input_teams(
                i * 7919,
                SimulationMode::FastBatch,
                RulesModeV4::Standard,
                make_v4_team("H", "MC_HOME", 13),
                make_v4_team("A", "MC_AWAY", 13),
            )
        })
        .collect();
    let report = run_monte_carlo_v4(inputs).unwrap();
    println!(
        "V4.0 Monte Carlo 1000 matches: {:.2} goals/m | {:.1} shots/m | {:.1}% pass | {:.1} fouls/m | {:.2} YC | {:.2} RC | {} ms ({:.1} matches/s)",
        report.avg_goals_per_match,
        report.avg_shots_per_match,
        report.pass_accuracy_pct,
        report.avg_fouls_per_match,
        report.avg_yellow_cards_per_match,
        report.avg_red_cards_per_match,
        report.elapsed_ms,
        report.matches_per_sec
    );
    assert_eq!(report.match_count, 1000);
}

// ============================================================================
// Stage A0 Hardening Tests: 4-Half Clock, Strict Validation, Laws, Metadata,
// Single-Attribute Isolation & Interactive Session
// ============================================================================

#[test]
fn stage_a0_clock_second_half_never_shortened_by_first_half_stoppage() {
    let inp = make_v4_input(112233, SimulationMode::FullEvent, RulesModeV4::Knockout);
    let res = simulate_v4(inp).unwrap();
    assert!(
        res.summary.first_half_elapsed_seconds >= 2700,
        "H1 must run >= 2700s, got {}",
        res.summary.first_half_elapsed_seconds
    );
    assert!(
        res.summary.second_half_elapsed_seconds >= 2700,
        "H2 must run >= 2700s regardless of H1 stoppage, got {}",
        res.summary.second_half_elapsed_seconds
    );
    if res.summary.went_to_extra_time {
        assert!(res.summary.extra_time_first_elapsed_seconds >= 900);
        assert!(res.summary.extra_time_second_elapsed_seconds >= 900);
    }
}

#[test]
fn stage_a0_strict_input_validation_guards() {
    // 1. Injured starter rejected
    let mut bad_inj = make_v4_input(1, SimulationMode::FastBatch, RulesModeV4::Standard);
    bad_inj.home_team.lineup[3].injured = true;
    assert!(bad_inj.validate().is_err());

    // 2. Duplicate player_id across Home and Away rejected
    let mut bad_cross = make_v4_input(2, SimulationMode::FastBatch, RulesModeV4::Standard);
    let dup_id = bad_cross.home_team.lineup[1].player_id.clone();
    bad_cross.away_team.lineup[1].player_id = dup_id.clone();
    bad_cross.away_team.assignments[1].player_id = dup_id;
    assert!(bad_cross.validate().is_err());

    // 3. Duplicate slot_id in assignments rejected
    let mut bad_slot = make_v4_input(3, SimulationMode::FastBatch, RulesModeV4::Standard);
    bad_slot.home_team.assignments[2].slot_id = bad_slot.home_team.assignments[1].slot_id.clone();
    assert!(bad_slot.validate().is_err());

    // 4. Input fingerprint changes when bench or weather changes
    let mut inp_a = make_v4_input(4, SimulationMode::FastBatch, RulesModeV4::Standard);
    let fp_a = inp_a.input_fingerprint();
    inp_a.match_environment.weather = WeatherCondition::HeavyRain;
    assert_ne!(fp_a, inp_a.input_fingerprint());
}

#[test]
fn stage_a0_offside_law_and_kotlin_metadata_contract() {
    use crate::v4::ball_play::is_receiver_offside;
    use crate::v4::events::EventMetadataV4;
    use crate::v4::state::MatchStateV4;

    let inp = make_v4_input(55, SimulationMode::FastBatch, RulesModeV4::Standard);
    let mut st = MatchStateV4::from_input(&inp);

    // Place Away GK at x=100, second-last defender at x=80, all others deeper toward midfield
    for p in &mut st.players {
        if p.side == TeamSideV4::Away {
            p.pos = if p.is_goalkeeper {
                Vec2::new(100.0, 34.0)
            } else {
                Vec2::new(80.0, 34.0)
            };
        }
    }
    // Own half exemption (x = 45 <= 52.5)
    assert!(!is_receiver_offside(&st, TeamSideV4::Home, Vec2::new(40.0, 34.0), Vec2::new(45.0, 34.0)));
    // Behind the ball exemption
    assert!(!is_receiver_offside(&st, TeamSideV4::Home, Vec2::new(85.0, 34.0), Vec2::new(83.0, 34.0)));
    // Ahead of ball AND ahead of second-last opponent (x=80) -> OFFSIDE
    assert!(is_receiver_offside(&st, TeamSideV4::Home, Vec2::new(72.0, 34.0), Vec2::new(84.0, 34.0)));

    // Verify Kotlin commentary metadata JSON contract (`distance`, integer `progressGain`)
    let meta = EventMetadataV4 {
        distance_m: Some(18.5),
        progress_gain: Some(14),
        ..EventMetadataV4::default()
    };
    let serialized = serde_json::to_string(&meta).unwrap();
    assert!(serialized.contains("\"distance\":18.5"), "Expected 'distance' key, got {serialized}");
    assert!(!serialized.contains("\"distanceM\""), "Must not serialize as 'distanceM'");
    assert!(serialized.contains("\"progressGain\":14"), "Expected integer 'progressGain', got {serialized}");
}

#[test]
fn stage_a0_isolated_single_attribute_and_no_unintended_gk_side_effect() {
    // Isolate ONLY `handling` (4 vs 19) on GK and verify GK distance_m is unaffected
    let home_h4 = make_v4_team_with_attrs("H", "GK_H4", |i| {
        let mut a = PlayerAttributes54::uniform(12).unwrap();
        if i == 0 {
            a.goalkeeping.handling = 4;
        }
        a
    });
    let home_h19 = make_v4_team_with_attrs("H", "GK_H19", |i| {
        let mut a = PlayerAttributes54::uniform(12).unwrap();
        if i == 0 {
            a.goalkeeping.handling = 19;
        }
        a
    });
    let away = make_v4_team("A", "AWAY_ISO", 12);

    let r4 = simulate_v4(make_v4_input_teams(7001, SimulationMode::FastBatch, RulesModeV4::Standard, home_h4, away.clone())).unwrap();
    let r19 = simulate_v4(make_v4_input_teams(7001, SimulationMode::FastBatch, RulesModeV4::Standard, home_h19, away)).unwrap();

    let d4 = r4.summary.players[0].distance_m;
    let d19 = r19.summary.players[0].distance_m;
    assert!(
        (d4 - d19).abs() < 450.0,
        "Changing GK handling alone should not drastically alter GK running distance ({d4} vs {d19})"
    );
}

#[test]
fn stage_a0_interactive_session_step_advance_and_live_commands() {
    use crate::v4::session::{MatchCommandV4, MatchSessionV4};

    let mut home = make_v4_team("H", "HOME_INT", 13);
    home.bench.push(PlayerMatchSnapshotV4 {
        player_id: "HB00".into(),
        display_name: "Bench Sub 0".into(),
        attributes: PlayerAttributes54::uniform(14).unwrap(),
        condition: 100,
        morale: 75,
        form: 70,
        age: 21,
        injured: false,
        eligible: true,
        primary_position: "CF".into(),
        secondary_position: "LW".into(),
        preferred_foot: PreferredFoot::Right,
        weak_foot: 4,
        height_cm: 182,
    });
    let away = make_v4_team("A", "AWAY_INT", 13);
    let inp = make_v4_input_teams(88888, SimulationMode::FullEvent, RulesModeV4::Standard, home, away);

    let mut sess = MatchSessionV4::new(inp).unwrap();
    let step1 = sess.advance_steps(10).unwrap();
    assert!(step1.ok && !step1.is_full_time && !step1.new_events.is_empty());

    let mut new_ti = TeamInstructionsV4::default();
    new_ti.pressing_intensity = 82;
    new_ti.line_height = 75;
    assert!(sess
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: new_ti,
        })
        .is_ok());

    assert!(sess
        .submit_command(MatchCommandV4::Substitute {
            side: TeamSideV4::Home,
            player_out_id: "H10".into(),
            player_in_id: "HB00".into(),
        })
        .is_ok());

    let final_res = sess.run_to_full_time().unwrap();
    assert!(final_res.ok);
    assert!(final_res.events.iter().any(|e| e.kind == EventTypeV4::Substitution && e.actor.as_deref() == Some("HB00")));
}

// P0/P1 safety and Law 10/11 regression tests added for the hardening patch.
#[test]
fn p0_offside_position_is_not_automatically_an_offence() {
    use crate::v4::ball_play::{execute_pass_and_first_touch, is_receiver_offside};
    use crate::v4::movement::evaluate_local_context;
    use crate::v4::perception::PassKindV4;
    use crate::v4::state::MatchStateV4;

    let input = make_v4_input(4001, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut received_offside = false;
    let mut misplaced_without_offside = false;
    for seed in 4001..4141 {
        let mut state = MatchStateV4::from_input(&input);
        let passer_idx = state.players.iter().position(|p|
            p.side == TeamSideV4::Home && p.player_id == "H07").unwrap();
        let receiver_idx = state.players.iter().position(|p|
            p.side == TeamSideV4::Home && p.player_id == "H10").unwrap();
        for player in &mut state.players {
            if player.side == TeamSideV4::Away {
                player.pos.x = if player.is_goalkeeper { 100.0 } else { 75.0 };
                player.pos.y = 20.0;
            }
        }
        state.players[passer_idx].pos = Vec2::new(69.0, 34.0);
        state.players[receiver_idx].pos = Vec2::new(86.0, 34.0);
        let passer_pos = state.players[passer_idx].pos;
        let target_pos = state.players[receiver_idx].pos;
        assert!(is_receiver_offside(&state, TeamSideV4::Home, passer_pos, target_pos));
        let context = evaluate_local_context(&state, &input, TeamSideV4::Home, passer_pos);
        let mut rng = DeterministicRng::new(seed);
        let outcome = execute_pass_and_first_touch(
            &mut state, &input, passer_idx, receiver_idx, PassKindV4::Short,
            17.0, 17.0, 0.0, None, context, &mut rng,
        );
        let flagged = outcome.events.iter().any(|e| e.kind == EventTypeV4::Offside);
        let misplaced = outcome.events.iter().any(|e| e.kind == EventTypeV4::PassFailed);
        assert!(!(flagged && misplaced), "a failed pass should not be whistled for receiving");
        if flagged {
            received_offside = true;
            assert_eq!(outcome.next_side, TeamSideV4::Away);
            assert_eq!(state.home.offsides, 1);
        }
        if misplaced {
            misplaced_without_offside = true;
            assert_eq!(state.home.offsides, 0);
        }
    }
    assert!(received_offside);
    assert!(misplaced_without_offside);
}

#[test]
fn p0_controlled_game_replays_identical_commands_and_fingerprint() {
    use crate::v4::session::{replay_v4_with_timeline, MatchCommandV4, MatchSessionV4};
    let mut input = make_v4_input(540001, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut bench = input.home_team.lineup[8].clone();
    bench.player_id = "H-BENCH-NEW".into();
    bench.display_name = "Home Substitute".into();
    input.home_team.bench.push(bench);

    let mut session = MatchSessionV4::new(input.clone()).unwrap();
    session.advance_steps(8).unwrap();
    let mut instructions = input.home_team.team_instructions.clone();
    instructions.pressing_intensity = 79;
    session.submit_command(MatchCommandV4::UpdateTactics {
        side: TeamSideV4::Home, instructions,
    }).unwrap();
    session.advance_steps(9).unwrap();
    session.submit_command(MatchCommandV4::Substitute {
        side: TeamSideV4::Home,
        player_out_id: "H08".into(),
        player_in_id: "H-BENCH-NEW".into(),
    }).unwrap();
    let actual = session.run_to_full_time().unwrap();
    assert_eq!(actual.command_timeline.len(), 2);
    assert!(actual.command_timeline[0].completed_beats < actual.command_timeline[1].completed_beats);

    let replay = replay_v4_with_timeline(input.clone(), &actual.command_timeline).unwrap();
    assert_eq!(actual.event_timeline_fingerprint, replay.event_timeline_fingerprint);
    assert_eq!(actual.summary, replay.summary);
    assert_eq!(actual.events, replay.events);

    let mut invalid = actual.command_timeline.clone();
    invalid[1].completed_beats = invalid[0].completed_beats.saturating_sub(1);
    assert!(replay_v4_with_timeline(input, &invalid).is_err());
}

#[test]
fn p0_shootout_scores_come_only_from_actual_kicks() {
    let mut verified = false;
    for seed in 5501..5571 {
        // Matched teams to make ties and penalty shootouts reasonably frequent.
        let inp = make_v4_input_teams(
            seed, SimulationMode::FullEvent, RulesModeV4::Knockout,
            make_v4_team("H", "HOME", 13), make_v4_team("A", "AWAY", 13),
        );
        let result = simulate_v4(inp).unwrap();
        if !result.summary.went_to_shootout { continue; }
        verified = true;
        let kicks: Vec<_> = result.events.iter()
            .filter(|ev| ev.kind == EventTypeV4::PenaltyShootoutKick).collect();
        assert!(!kicks.is_empty());
        let home_goals = kicks.iter().filter(|ev|
            ev.side == Some(TeamSideV4::Home) && ev.metadata.scored == Some(true)).count();
        let away_goals = kicks.iter().filter(|ev|
            ev.side == Some(TeamSideV4::Away) && ev.metadata.scored == Some(true)).count();
        assert_eq!(result.summary.shootout_home_goals, Some(home_goals as u8));
        assert_eq!(result.summary.shootout_away_goals, Some(away_goals as u8));
        assert_ne!(home_goals, away_goals, "sudden death must have a winner");
        for side in [TeamSideV4::Home, TeamSideV4::Away] {
            let kickers: Vec<_> = kicks.iter().filter(|ev| ev.side == Some(side))
                .map(|ev| ev.actor.clone().unwrap()).collect();
            let first_cycle = kickers.len().min(11);
            let unique: std::collections::HashSet<_> = kickers[..first_cycle].iter().collect();
            assert_eq!(unique.len(), first_cycle, "same kicker must not repeat before cycle");
        }
        break;
    }
    assert!(verified, "expected at least one knockout shootout in seeds");
}

#[test]
fn p1_regulation_clock_uses_independent_period_elapsed_seconds() {
    let result = simulate_v4(make_v4_input(
        23031, SimulationMode::FullEvent, RulesModeV4::Standard)).unwrap();
    assert!(result.summary.first_half_elapsed_seconds >= 2700);
    assert!(result.summary.second_half_elapsed_seconds >= 2700);
    assert_eq!(result.summary.final_game_second,
        result.summary.first_half_elapsed_seconds + result.summary.second_half_elapsed_seconds);
    let h2 = result.events.iter().find(|ev|
        ev.kind == EventTypeV4::Kickoff && ev.period == 2).unwrap();
    assert_eq!(h2.period_elapsed_second, 0,
        "second-half 45:00 must not inherit first-half added time");
    assert_eq!(h2.game_second, result.summary.first_half_elapsed_seconds);
    for ev in &result.events {
        assert!(ev.period_elapsed_second >= 0);
    }
}

#[test]
fn p0_every_goal_is_followed_by_opponent_kickoff_at_centre() {
    let mut n_goals = 0;
    for seed in 71001..71011 {
        let result = simulate_v4(make_v4_input(
            seed, SimulationMode::FullEvent, RulesModeV4::Standard)).unwrap();
        for (idx, ev) in result.events.iter().enumerate() {
            if ev.kind != EventTypeV4::Goal { continue; }
            n_goals += 1;
            let restart = result.events[idx+1..].iter().find(|e|
                e.kind == EventTypeV4::Kickoff || e.kind == EventTypeV4::Goal).unwrap();
            assert_eq!(restart.kind, EventTypeV4::Kickoff);
            assert_eq!(restart.side, Some(ev.side.unwrap().opposite()));
            assert!((restart.ball_pos.x - 52.5).abs() < 0.01);
            assert!((restart.ball_pos.y - 34.0).abs() < 0.01);
            assert!(restart.actor.is_some());
        }
    }
    assert!(n_goals > 0);
}

// P1-2: multi-player batches count as one in-play window; failures are atomic.
#[test]
fn p1_batch_substitution_counts_one_window_and_returns_authoritative_roster() {
    use crate::v4::session::{MatchCommandV4, MatchSessionV4, SubstitutionChangeV4};
    let mut input = make_v4_input(91021, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.ai_home_manager = false;
    for i in 0..5 {
        let mut bench = input.home_team.lineup[8].clone();
        bench.player_id = format!("SUB-{}", i);
        bench.display_name = bench.player_id.clone();
        input.home_team.bench.push(bench);
    }
    let mut session = MatchSessionV4::new(input).unwrap();
    session.advance_steps(1).unwrap();
    session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![
            SubstitutionChangeV4 { player_out_id: "H08".into(), player_in_id: "SUB-0".into() },
            SubstitutionChangeV4 { player_out_id: "H09".into(), player_in_id: "SUB-1".into() },
        ],
    }).unwrap();
    let step = session.advance_steps(1).unwrap();
    assert_eq!(step.home_roster.substitutions_used, 2);
    assert_eq!(step.home_roster.substitution_windows_used, 1);
    assert!(step.home_roster.on_pitch_player_ids.contains(&"SUB-0".to_string()));
    assert!(step.home_roster.on_pitch_player_ids.contains(&"SUB-1".to_string()));
    assert!(!step.home_roster.available_bench_player_ids.contains(&"SUB-0".to_string()));
    assert_eq!(step.new_events.iter().filter(|e| e.kind == EventTypeV4::Substitution).count(), 2);

    // Do not permit two simultaneous changes to chain: an incoming player
    // cannot itself be substituted off within the same group.
    assert!(session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![
            SubstitutionChangeV4 { player_out_id: "H07".into(), player_in_id: "SUB-2".into() },
            SubstitutionChangeV4 { player_out_id: "SUB-2".into(), player_in_id: "SUB-3".into() },
        ],
    }).is_err());
    let rejected = session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![
            SubstitutionChangeV4 { player_out_id: "H07".into(), player_in_id: "SUB-2".into() },
            SubstitutionChangeV4 { player_out_id: "H06".into(), player_in_id: "MISSING-BENCH".into() },
        ],
    });
    assert!(rejected.is_err());
    let snapshot = session.advance_steps(1).unwrap().home_roster;
    assert_eq!(snapshot.substitutions_used, 2, "an invalid group must not consume any player slot");
    assert_eq!(snapshot.substitution_windows_used, 1);
    assert!(snapshot.on_pitch_player_ids.contains(&"H07".to_string()));
    assert!(snapshot.available_bench_player_ids.contains(&"SUB-2".to_string()));
}

#[test]
fn p1_halftime_substitution_consumes_player_slot_not_inplay_window() {
    use crate::v4::session::{MatchCommandV4, MatchSessionV4, SubstitutionChangeV4};
    let mut input = make_v4_input(91022, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.ai_home_manager = false;
    let mut bench = input.home_team.lineup[8].clone();
    bench.player_id = "HALF-SUB".into();
    input.home_team.bench.push(bench);
    let mut session = MatchSessionV4::new(input).unwrap();
    let mut halftime = None;
    for _ in 0..1000 {
        let step = session.advance_steps(1).unwrap();
        if step.period == 2 && step.period_elapsed_second == 0 {
            halftime = Some(step);
            break;
        }
    }
    let step = halftime.expect("should reach second half start within 1000 action beats");
    assert!(step.home_roster.interval_substitution_allowed);
    session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![SubstitutionChangeV4 {
            player_out_id: "H08".into(), player_in_id: "HALF-SUB".into(),
        }],
    }).unwrap();
    let follow_up = session.advance_steps(1).unwrap();
    assert_eq!(follow_up.home_roster.substitutions_used, 1);
    assert_eq!(follow_up.home_roster.substitution_windows_used, 0,
        "halftime is not an in-play substitution opportunity");
}

#[test]
fn p1_grouped_substitutions_replay_deterministically() {
    use crate::v4::session::{replay_v4_with_timeline, MatchCommandV4, MatchSessionV4, SubstitutionChangeV4};
    let mut input = make_v4_input(91023, SimulationMode::FullEvent, RulesModeV4::Standard);
    for i in 0..2 {
        let mut bench = input.home_team.lineup[8].clone();
        bench.player_id = format!("REPLAY-BENCH-{}", i);
        input.home_team.bench.push(bench);
    }
    let mut session = MatchSessionV4::new(input.clone()).unwrap();
    session.advance_steps(5).unwrap();
    session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![
            SubstitutionChangeV4 { player_out_id: "H08".into(), player_in_id: "REPLAY-BENCH-0".into() },
            SubstitutionChangeV4 { player_out_id: "H09".into(), player_in_id: "REPLAY-BENCH-1".into() },
        ],
    }).unwrap();
    let actual = session.run_to_full_time().unwrap();
    assert_eq!(actual.command_timeline.len(), 1);
    let replay = replay_v4_with_timeline(input, &actual.command_timeline).unwrap();
    assert_eq!(actual.event_timeline_fingerprint, replay.event_timeline_fingerprint);
    assert_eq!(actual.summary, replay.summary);
}

#[test]
fn p1_fourth_inplay_window_is_rejected_even_with_player_slots_left() {
    use crate::v4::session::{MatchCommandV4, MatchSessionV4};
    let mut input = make_v4_input(91024, SimulationMode::FullEvent, RulesModeV4::Standard);
    for i in 0..4 {
        let mut bench = input.home_team.lineup[8].clone();
        bench.player_id = format!("WINDOW-SUB-{}", i);
        input.home_team.bench.push(bench);
    }
    let mut session = MatchSessionV4::new(input).unwrap();
    session.advance_steps(1).unwrap();
    for i in 0..3 {
        session.submit_command(MatchCommandV4::Substitute {
            side: TeamSideV4::Home,
            player_out_id: format!("H{:02}", i + 7),
            player_in_id: format!("WINDOW-SUB-{}", i),
        }).unwrap();
        session.advance_steps(1).unwrap();
    }
    let third = session.advance_steps(1).unwrap().home_roster;
    assert_eq!(third.substitutions_used, 3);
    assert_eq!(third.substitution_windows_used, 3);
    assert!(session.submit_command(MatchCommandV4::Substitute {
        side: TeamSideV4::Home,
        player_out_id: "H10".into(),
        player_in_id: "WINDOW-SUB-3".into(),
    }).is_err());
    let result = session.advance_steps(1).unwrap().home_roster;
    assert_eq!(result.substitutions_used, 3);
    assert_eq!(result.substitution_windows_used, 3);
}

// ============================================================================
// P2: own-goal causality, score attribution, event/summary reconciliation,
// 54-attribute transport integrity, and FastBatch/FullEvent parity.
// ============================================================================

#[test]
fn p2_own_goal_is_awarded_to_opponent_not_ordinary_scorer() {
    use crate::v4::own_goal::{award_own_goal, DeflectionCause};
    use crate::v4::state::MatchStateV4;
    let input = make_v4_input(82001, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut state = MatchStateV4::from_input(&input);
    let defender_idx = state.players.iter().position(|p| p.player_id == "A02").unwrap();
    let defender_id = state.players[defender_idx].player_id.clone();
    state.players[defender_idx].pos = Vec2::new(102.0, 34.0);
    let event = award_own_goal(&mut state, defender_idx, TeamSideV4::Home,
        DeflectionCause::BlockedShot, 42);
    assert_eq!(event.kind, EventTypeV4::OwnGoal);
    assert_eq!(event.side, Some(TeamSideV4::Home), "side is the beneficiary");
    assert_eq!(event.actor, Some(defender_id.clone()), "actor is the defending offender");
    assert_eq!(event.metadata.outcome.as_deref(), Some("OWN_GOAL"));
    assert_eq!(event.metadata.cause.as_deref(), Some("DEFLECTED_BLOCK"));
    assert_eq!(state.home.goals, 1);
    assert_eq!(state.home.own_goals_for, 1);
    assert_eq!(state.away.own_goals_conceded, 1);
    assert_eq!(state.players[defender_idx].own_goals, 1);
    assert_eq!(state.players[defender_idx].goals, 0);
    assert!(state.players.iter().all(|p| p.assists == 0));
    assert_eq!(state.ball.owner_side, None);
    assert_eq!(state.ball.last_touch_player_id.as_deref(), Some(defender_id.as_str()));
}

#[test]
fn p2_own_goals_only_possible_near_goal_and_skill_reduces_risk() {
    use crate::v4::own_goal::{own_goal_risk, DeflectionCause};
    let low = PlayerAttributes54::uniform(4).unwrap();
    let high = PlayerAttributes54::uniform(19).unwrap();
    for cause in [DeflectionCause::BlockedShot, DeflectionCause::MishitAerialClearance] {
        let near = Vec2::new(103.0, 34.0);
        let low_risk = own_goal_risk(near, TeamSideV4::Home, &low, cause);
        let high_risk = own_goal_risk(near, TeamSideV4::Home, &high, cause);
        assert!(low_risk > high_risk && high_risk > 0.0);
        assert_eq!(own_goal_risk(Vec2::new(60.0, 34.0), TeamSideV4::Home, &low, cause), 0.0);
        assert_eq!(own_goal_risk(Vec2::new(100.0, 34.0), TeamSideV4::Away, &low, cause), 0.0);
    }
}

#[test]
fn p2_actual_own_goal_outcome_depends_on_seed_and_correct_side() {
    use crate::v4::own_goal::{maybe_own_goal, DeflectionCause};
    use crate::v4::state::MatchStateV4;
    let input = make_v4_input(82002, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut found = false;
    for seed in 1..=2000i64 {
        let mut state = MatchStateV4::from_input(&input);
        let idx = state.players.iter().position(|p| p.player_id == "A02").unwrap();
        state.players[idx].pos = Vec2::new(103.0, 34.0);
        let attrs = PlayerAttributes54::uniform(4).unwrap();
        let mut rng = DeterministicRng::new(seed);
        let own = maybe_own_goal(&mut state, idx, TeamSideV4::Home,
            DeflectionCause::BlockedShot, &attrs, 1, &mut rng);
        if let Some(event) = own {
            assert_eq!(event.kind, EventTypeV4::OwnGoal);
            assert_eq!(state.home.goals, 1);
            assert_eq!(state.away.goals, 0);
            found = true;
            break;
        }
    }
    assert!(found, "an accidental near-goal deflection must be a possible outcome");
}

#[test]
fn p2_stats_breakdown_and_event_reconciliation_many_full_matches() {
    use crate::v4::stats::audit_result;
    for seed in 1..=30i64 {
        let input = make_v4_input(seed * 9721, SimulationMode::FullEvent, RulesModeV4::Standard);
        let result = simulate_v4(input).unwrap();
        audit_result(&result).unwrap_or_else(|err| panic!("seed {seed}: {err}"));
        for team in [&result.summary.home, &result.summary.away] {
            assert_eq!(team.shots, team.shots_on_target + team.shots_off_target + team.shots_blocked);
            assert!(team.saves <= result.summary.home.shots_on_target + result.summary.away.shots_on_target);
        }
    }
}

#[test]
fn p2_fastbatch_summary_and_timeline_match_fullevent() {
    use crate::v4::stats::{audit_result, validate_summary};
    for seed in 41..=56i64 {
        let full_input = make_v4_input(seed * 7717, SimulationMode::FullEvent, RulesModeV4::Standard);
        let mut fast_input = full_input.clone();
        fast_input.simulation_mode = SimulationMode::FastBatch;
        let full = simulate_v4(full_input).unwrap();
        let fast = simulate_v4(fast_input).unwrap();
        audit_result(&full).unwrap();
        validate_summary(&fast.summary).unwrap();
        assert!(fast.events.is_empty());
        assert_eq!(full.summary, fast.summary);
        assert_eq!(full.event_timeline_fingerprint, fast.event_timeline_fingerprint);
        assert_eq!(full.event_count, fast.event_count);
    }
}

#[test]
fn p2_all_54_named_attributes_serialise_validate_and_change_input_fingerprint() {
    let base = PlayerAttributes54::uniform(12).unwrap();
    let input = make_v4_input(82003, SimulationMode::FastBatch, RulesModeV4::Standard);
    let before = input.input_fingerprint();
    let baseline_value = serde_json::to_value(base).unwrap();
    let mut tested = 0;
    for (name, value) in base.all_named() {
        assert_eq!(value, 12);
        let mut object = baseline_value.clone();
        let mut found = 0;
        for group in ["technical", "mental", "physical", "goalkeeping", "hidden"] {
            if let Some(entry) = object.get_mut(group).and_then(|g| g.get_mut(name)) {
                *entry = serde_json::json!(19);
                found += 1;
            }
        }
        assert_eq!(found, 1, "every attribute has exactly one canonical JSON field: {name}");
        let variant: PlayerAttributes54 = serde_json::from_value(object).unwrap();
        variant.validate().unwrap();
        let mut updated = input.clone();
        updated.home_team.lineup[3].attributes = variant;
        assert_ne!(updated.input_fingerprint(), before, "attribute {name} must enter the immutable replay snapshot");
        tested += 1;
    }
    assert_eq!(tested, 54);
}

#[test]
fn p2_50_match_attributes_have_explicit_runtime_wiring_and_4_are_career_hooks() {
    // Structural wiring test only. This is deliberately NOT presented as
    // evidence of 50 independently measured behavioral effects.
    let sources = [
        include_str!("ball_play.rs"), include_str!("shooting.rs"),
        include_str!("goalkeeper.rs"), include_str!("set_pieces.rs"),
        include_str!("movement.rs"), include_str!("perception.rs"),
        include_str!("tactics.rs"), include_str!("psychology.rs"),
        include_str!("ai_manager.rs"), include_str!("session.rs"),
    ].join("
");
    let career = ["professionalism", "ambition", "adaptability", "loyalty"];
    let mut in_match = 0;
    for (name, _) in PlayerAttributes54::uniform(12).unwrap().all_named() {
        if career.contains(&name) { continue; }
        assert!(sources.contains(&format!(".{name}")),
            "in-match attribute {name} has no reading in runtime modules");
        in_match += 1;
    }
    assert_eq!(in_match, 50);
}
