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
use crate::v4::session::{simulate_batch_fast_v4, simulate_v4, MatchCommandV4, MatchSessionV4};
use crate::v4::spatial::{
    attacking_progress_m, distance_to_attacking_goal, is_in_attacking_penalty_box,
    is_in_attacking_six_yard_box, point_to_segment_distance_m, TeamSideV4, Vec2, Vec3,
    PITCH_LENGTH_M, PITCH_WIDTH_M,
};
use crate::v4::rules::{RulesetConfig, TimingModelConfig};
use crate::v4::{ENGINE_PROTOCOL_VERSION_V4, ENGINE_VERSION_V4, HALF_MS_V4, REGULATION_SECONDS_V4};

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
    low_nat.apply_interval(5.0, 25.0, true, false, 12, 4);
    high_nat.apply_interval(5.0, 25.0, true, false, 12, 19);
    low_nat.apply_interval(10.0, 12.0, false, false, 12, 4);
    high_nat.apply_interval(10.0, 12.0, false, false, 12, 19);
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
    assert!(
        actual.command_timeline[0].accepted_sim_time_ms
            <= actual.command_timeline[1].accepted_sim_time_ms
    );
    assert!(
        actual.command_timeline[0].scheduled_effective_sim_time_ms
            >= actual.command_timeline[0].accepted_sim_time_ms
    );
    assert!(
        actual.command_timeline[0].applied_step_index
            < actual.command_timeline[1].applied_step_index
    );

    let replay = replay_v4_with_timeline(input.clone(), &actual.command_timeline).unwrap();
    assert_eq!(actual.event_timeline_fingerprint, replay.event_timeline_fingerprint);
    assert_eq!(actual.summary, replay.summary);
    assert_eq!(actual.events, replay.events);

    let mut invalid = actual.command_timeline.clone();
    invalid[1].accepted_sim_time_ms = invalid[0].accepted_sim_time_ms.saturating_sub(1);
    invalid[1].applied_step_index = invalid[0].applied_step_index.saturating_sub(1);
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
    session.advance_steps(2).unwrap();
    session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![
            SubstitutionChangeV4 { player_out_id: "H08".into(), player_in_id: "SUB-0".into() },
            SubstitutionChangeV4 { player_out_id: "H09".into(), player_in_id: "SUB-1".into() },
        ],
    }).unwrap();
    let mut sub_events_seen = 0usize;
    let mut applied_step = None;
    for _ in 0..400 {
        let s = session.advance_steps(10).unwrap();
        sub_events_seen += s.new_events.iter().filter(|e| e.kind == EventTypeV4::Substitution).count();
        if s.home_roster.substitutions_used == 2 {
            applied_step = Some(s);
            break;
        }
        if s.stage == crate::v4::events::SessionStageV4::HalfTimeBreak {
            let next_p = session.start_next_period().unwrap();
            sub_events_seen += next_p.new_events.iter().filter(|e| e.kind == EventTypeV4::Substitution).count();
        }
        if s.is_full_time {
            break;
        }
    }
    let step = applied_step.expect("queued batch substitution must execute on the next dead ball");
    assert_eq!(step.home_roster.substitutions_used, 2);
    assert_eq!(step.home_roster.substitution_windows_used, 1);
    assert!(step.home_roster.on_pitch_player_ids.contains(&"SUB-0".to_string()));
    assert!(step.home_roster.on_pitch_player_ids.contains(&"SUB-1".to_string()));
    assert!(!step.home_roster.available_bench_player_ids.contains(&"SUB-0".to_string()));
    assert_eq!(sub_events_seen, 2);

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
    use crate::v4::events::SessionStageV4;
    use crate::v4::rules::HalfStageV4;
    let mut halftime = None;
    for _ in 0..2000 {
        let step = session.advance_steps(25).unwrap();
        if step.stage == SessionStageV4::HalfTimeBreak {
            halftime = Some(step);
            break;
        }
        assert!(!step.is_full_time, "must pause at HalfTimeBreak before FullTime");
    }
    let step = halftime.expect("should reach HalfTimeBreak in interactive DES session");
    assert_eq!(session.stage(), HalfStageV4::HalfTimeBreak);
    assert!(step.home_roster.interval_substitution_allowed);
    session.submit_command(MatchCommandV4::SubstituteBatch {
        side: TeamSideV4::Home,
        changes: vec![SubstitutionChangeV4 {
            player_out_id: "H08".into(), player_in_id: "HALF-SUB".into(),
        }],
    }).unwrap();
    let follow_up = session.start_next_period().unwrap();
    assert_eq!(follow_up.period, 2);
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
    use crate::v4::events::SessionStageV4;
    let mut session = MatchSessionV4::new(input).unwrap();
    session.advance_steps(2).unwrap();
    for i in 0..3 {
        // Ensure we are in active live play (not on a break) so each sub consumes an in-play window.
        if session.stage().is_break() {
            session.start_next_period().unwrap();
            session.advance_steps(2).unwrap();
        }
        session.submit_command(MatchCommandV4::Substitute {
            side: TeamSideV4::Home,
            player_out_id: format!("H{:02}", i + 7),
            player_in_id: format!("WINDOW-SUB-{}", i),
        }).unwrap();
        let mut applied = false;
        for _ in 0..400 {
            let s = session.advance_steps(10).unwrap();
            if s.home_roster.substitutions_used == (i + 1) as u8 {
                applied = true;
                break;
            }
            if s.stage == SessionStageV4::HalfTimeBreak {
                session.start_next_period().unwrap();
                session.advance_steps(2).unwrap();
            }
            if s.is_full_time {
                break;
            }
        }
        assert!(applied, "in-play substitution {i} must execute on next dead ball");
    }
    if session.stage().is_break() {
        session.start_next_period().unwrap();
        session.advance_steps(2).unwrap();
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
        // PI owns these policies since stage 5.1; compatibility reexports
        // are not the implementation. Keep the original 50-attribute gate.
        include_str!("../player_intelligence/decision.rs"),
        include_str!("../player_intelligence/timing.rs"),
        include_str!("../player_intelligence/off_ball.rs"),
        include_str!("../player_intelligence/goalkeeper.rs"),
        include_str!("../player_intelligence/utility.rs"),
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
// ============================================================================
// DES v1.4 Mandatory Invariant & Contract Tests (Section XII.A & Section 7)
// ============================================================================

#[test]
fn des_clock_dynamic_added_time_no_hard_cap() {
    use crate::clock::{DesClockState, Period};

    let mut clock = DesClockState::default();
    clock.begin_period(Period::SecondHalf, HALF_MS_V4);
    clock.advance_ms(HALF_MS_V4, true).unwrap();

    // 11 minutes of compensable stoppages (far above any legacy 4-6 min hard cap)
    let eleven_min_ms = 11 * 60 * 1_000;
    clock.record_compensable_lost_ms(eleven_min_ms);
    let ruleset = RulesetConfig::ifab_league_standard();
    let announced = ruleset.round_announced_added_time_ms(clock.compensable_lost_ms);
    assert_eq!(announced, eleven_min_ms);
    assert_eq!(clock.lock_announced_added_ms(announced), eleven_min_ms);
    assert_eq!(clock.effective_added_ms(), eleven_min_ms);
    assert_eq!(clock.period_target_ms(), HALF_MS_V4 + eleven_min_ms);

    // Additional stoppage during added time dynamically extends effective added time
    clock.record_compensable_lost_ms(135_000);
    assert_eq!(clock.additional_added_ms, 135_000);
    assert_eq!(clock.effective_added_ms(), eleven_min_ms + 135_000);
    assert_eq!(clock.period_target_ms(), HALF_MS_V4 + eleven_min_ms + 135_000);
}

#[test]
fn des_clock_no_double_counting_deadball_vs_compensable() {
    use crate::clock::{DesClockState, Period};
    use crate::v4::events::SessionStageV4;
    use crate::v4::session::MatchSessionV4;

    // 1. Direct DesClockState partition invariant
    let mut clock = DesClockState::default();
    clock.begin_period(Period::FirstHalf, HALF_MS_V4);
    clock.advance_ms(120_000, true).unwrap();
    clock.advance_ms(35_000, false).unwrap();
    clock.record_compensable_lost_ms(22_000);
    assert_eq!(clock.ball_in_play_ms + clock.ball_out_of_play_ms, clock.sim_time_ms);
    assert_eq!(clock.sim_time_ms, 155_000, "compensable lost time must never be double-added to sim_time_ms");
    assert!(clock.compensable_lost_ms <= clock.ball_out_of_play_ms);

    // 2. Live DES session partition invariant across multiple advances
    let input = make_v4_input(14002, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut session = MatchSessionV4::new(input).unwrap();
    for _ in 0..80 {
        let step = session.advance_steps(20).unwrap();
        assert_eq!(
            step.ball_in_play_ms + step.ball_out_of_play_ms,
            step.sim_time_ms,
            "ball_in_play_ms + ball_out_of_play_ms must equal sim_time_ms"
        );
        assert!(
            step.compensable_lost_ms <= step.ball_out_of_play_ms,
            "compensable_lost_ms ({}) must not exceed ball_out_of_play_ms ({})",
            step.compensable_lost_ms,
            step.ball_out_of_play_ms
        );
        if step.stage == SessionStageV4::HalfTimeBreak {
            session.start_next_period().unwrap();
        }
        if step.is_full_time {
            break;
        }
    }
}

#[test]
fn des_halftime_break_pauses_interactive_and_auto_advances_fastbatch() {
    use crate::v4::events::SessionStageV4;
    use crate::v4::rules::HalfStageV4;
    use crate::v4::session::MatchSessionV4;

    let input_interactive = make_v4_input(14003, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut session = MatchSessionV4::new(input_interactive.clone()).unwrap();

    let mut reached_break = false;
    for _ in 0..2000 {
        let step = session.advance_steps(30).unwrap();
        if step.stage == SessionStageV4::HalfTimeBreak {
            assert_eq!(session.stage(), HalfStageV4::HalfTimeBreak);
            assert_eq!(step.stop_reason, "HALF_TIME_BREAK");
            assert!(!step.is_full_time);
            reached_break = true;
            break;
        }
        assert!(!step.is_full_time, "interactive session must pause at HalfTimeBreak");
    }
    assert!(reached_break, "interactive session must hit HalfTimeBreak");

    // Calling advance_steps while paused at HalfTimeBreak must remain paused with 0 steps
    let still_paused = session.advance_steps(10).unwrap();
    assert_eq!(still_paused.stage, SessionStageV4::HalfTimeBreak);
    assert_eq!(still_paused.steps_executed, 0);

    // Explicit start_next_period resumes second half
    let h2 = session.start_next_period().unwrap();
    assert_eq!(h2.period, 2);
    assert_eq!(h2.stage, SessionStageV4::InHalf);

    // FastBatch auto-advances across HalfTimeBreak to Full Time
    let mut input_fast = input_interactive;
    input_fast.simulation_mode = SimulationMode::FastBatch;
    let fast_res = simulate_v4(input_fast).unwrap();
    assert!(fast_res.ok);
    assert!(fast_res.summary.first_half_elapsed_seconds >= 2700);
    assert!(fast_res.summary.second_half_elapsed_seconds >= 2700);
}

#[test]
fn des_goal_celebration_then_kickoff_nonzero_variable_duration() {
    let mut durations_ms = Vec::new();
    for seed in 14100..14125i64 {
        let res = simulate_v4(make_v4_input(
            seed,
            SimulationMode::FullEvent,
            RulesModeV4::Standard,
        ))
        .unwrap();
        for (idx, ev) in res.events.iter().enumerate() {
            if !matches!(ev.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal) {
                continue;
            }
            if let Some(kickoff) = res.events[idx + 1..]
                .iter()
                .take_while(|e| e.period == ev.period)
                .find(|e| e.kind == EventTypeV4::Kickoff)
            {
                let delta = kickoff.sim_time_ms - ev.sim_time_ms;
                assert!(
                    delta > 0,
                    "goal celebration + return + kickoff setup must take nonzero time, got {delta}ms"
                );
                durations_ms.push(delta);
            }
        }
        if durations_ms.len() >= 12 {
            break;
        }
    }
    assert!(
        durations_ms.len() >= 2,
        "expected at least 2 goal-to-kickoff pairs across seeds"
    );
    let unique: std::collections::HashSet<i64> = durations_ms.iter().copied().collect();
    assert!(
        unique.len() >= 2,
        "goal-to-kickoff durations must vary contextually, not collapse to one constant: {durations_ms:?}"
    );
}

#[test]
fn des_deadball_state_machine_blocks_open_play_events() {
    use crate::v4::rules::{
        initial_dead_ball_phase, next_dead_ball_phase, DeadBallKind, DeadBallPhase, DeadBallState,
    };

    // 1. State machine transition sequence for GoalCelebration (no illegal shortcuts)
    let mut db = DeadBallState::new(
        DeadBallKind::GoalCelebration,
        initial_dead_ball_phase(DeadBallKind::GoalCelebration, false, false, true),
        TeamSideV4::Away,
        Vec3::center_spot(),
        100_000,
        20_000,
        20_000,
    );
    assert_eq!(db.phase, DeadBallPhase::Celebrating);

    let p2 = next_dead_ball_phase(&db, true, false, false).unwrap();
    assert_eq!(p2, DeadBallPhase::ReturningToHalf);
    db.advance_to(p2, 120_000, 12_000, 12_000);

    let p3 = next_dead_ball_phase(&db, true, false, false).unwrap();
    assert_eq!(p3, DeadBallPhase::ProcessingSubstitutions);
    db.advance_to(p3, 132_000, 15_000, 15_000);

    let p4 = next_dead_ball_phase(&db, true, false, false).unwrap();
    assert_eq!(p4, DeadBallPhase::KickoffSetup);
    db.advance_to(p4, 147_000, 5_000, 5_000);

    let p5 = next_dead_ball_phase(&db, true, false, false).unwrap();
    assert_eq!(p5, DeadBallPhase::ExecutingRestart);
    db.advance_to(p5, 152_000, 1_000, 0);

    assert_eq!(next_dead_ball_phase(&db, true, false, false), None);

    // 2. Live match trace: no open-play ball events between Goal/OwnGoal and subsequent Kickoff
    let mut checked_goals = 0usize;
    for seed in 14200..14215i64 {
        let res = simulate_v4(make_v4_input(
            seed,
            SimulationMode::FullEvent,
            RulesModeV4::Standard,
        ))
        .unwrap();
        for (idx, ev) in res.events.iter().enumerate() {
            if !matches!(ev.kind, EventTypeV4::Goal | EventTypeV4::OwnGoal) {
                continue;
            }
            checked_goals += 1;
            for between in &res.events[idx + 1..] {
                if between.period != ev.period || between.kind == EventTypeV4::Kickoff {
                    break;
                }
                assert!(
                    !matches!(
                        between.kind,
                        EventTypeV4::PassCompleted
                            | EventTypeV4::PassFailed
                            | EventTypeV4::ThroughBall
                            | EventTypeV4::Cross
                            | EventTypeV4::Shot
                            | EventTypeV4::ShotOnTarget
                            | EventTypeV4::ShotOffTarget
                            | EventTypeV4::DribbleLost
                            | EventTypeV4::Carry
                            | EventTypeV4::DribbleWon
                            | EventTypeV4::TackleWon
                    ),
                    "open-play event {:?} emitted during dead-ball celebration window",
                    between.kind
                );
            }
        }
    }
    assert!(checked_goals > 0, "expected at least one goal across seeds");
}

#[test]
fn des_scheduler_tie_break_priority_sequence_deterministic() {
    use crate::clock::Period;
    use crate::v4::scheduler::{
        CancellationScope, DesEventPayload, EventPriority, EventScheduler,
    };

    let mut sched = EventScheduler::new();
    let seq_period = sched
        .schedule(
            5_000,
            EventPriority::PeriodBoundary,
            CancellationScope::PeriodScoped,
            DesEventPayload::CheckPeriodEnd {
                period: Period::FirstHalf,
            },
        )
        .unwrap();
    let seq_action = sched
        .schedule(
            5_000,
            EventPriority::ActionDecision,
            CancellationScope::LivePlayOnly,
            DesEventPayload::DecisionReady {
                player_id: "H09".into(),
                side: TeamSideV4::Home,
            },
        )
        .unwrap();
    let seq_tac = sched
        .schedule(
            5_000,
            EventPriority::TacticalAndCommand,
            CancellationScope::Persistent,
            DesEventPayload::TacticalCommandEffective {
                command_index: 0,
                side: TeamSideV4::Home,
            },
        )
        .unwrap();
    let seq_phys_1 = sched
        .schedule(
            5_000,
            EventPriority::PhysicsAndBoundary,
            CancellationScope::LivePlayOnly,
            DesEventPayload::EarlyInteractionResolved {
                interaction_tag: "HIT_A".into(),
                actor_id: Some("A04".into()),
            },
        )
        .unwrap();
    let seq_phys_2 = sched
        .schedule(
            5_000,
            EventPriority::PhysicsAndBoundary,
            CancellationScope::LivePlayOnly,
            DesEventPayload::EarlyInteractionResolved {
                interaction_tag: "HIT_B".into(),
                actor_id: Some("A05".into()),
            },
        )
        .unwrap();

    let order: Vec<u64> = (0..5)
        .map(|_| sched.pop_next_valid().unwrap().unwrap().insertion_sequence)
        .collect();
    assert_eq!(order, vec![seq_phys_1, seq_phys_2, seq_tac, seq_action, seq_period]);
    assert!(sched.pop_next_valid().unwrap().is_none());
}

#[test]
fn des_scheduler_cancels_stale_generation_events() {
    use crate::clock::Period;
    use crate::v4::scheduler::{
        CancellationScope, DesEventPayload, EventPriority, EventScheduler,
    };

    let mut sched = EventScheduler::new();
    sched
        .schedule(
            1_000,
            EventPriority::ActionDecision,
            CancellationScope::LivePlayOnly,
            DesEventPayload::DribbleStep {
                carrier_id: "H10".into(),
                side: TeamSideV4::Home,
            },
        )
        .unwrap();
    sched
        .schedule(
            2_000,
            EventPriority::PeriodBoundary,
            CancellationScope::PeriodScoped,
            DesEventPayload::AnnounceAddedTime {
                period: Period::FirstHalf,
            },
        )
        .unwrap();
    sched
        .schedule(
            3_000,
            EventPriority::TacticalAndCommand,
            CancellationScope::Persistent,
            DesEventPayload::TacticalCommandEffective {
                command_index: 7,
                side: TeamSideV4::Away,
            },
        )
        .unwrap();

    sched.invalidate_live_play();
    assert_eq!(sched.peek_next_valid_time_ms(), Some(2_000));

    sched.invalidate_period();
    assert_eq!(sched.peek_next_valid_time_ms(), Some(3_000));

    let surviving = sched.pop_next_valid().unwrap().unwrap();
    assert_eq!(surviving.event_time_ms, 3_000);
    assert_eq!(surviving.scope, CancellationScope::Persistent);
    assert!(sched.pop_next_valid().unwrap().is_none());
}

#[test]
fn des_scheduler_rejects_past_and_infinite_same_ms_loop() {
    use crate::v4::scheduler::{
        CancellationScope, DesEventPayload, EventPriority, EventScheduler, MAX_ZERO_DT_STREAK,
    };

    let mut sched = EventScheduler::new();
    sched.advance_cursor_to(10_000).unwrap();
    assert!(sched.advance_cursor_to(9_999).is_err());
    assert!(sched
        .schedule(
            9_999,
            EventPriority::ActionDecision,
            CancellationScope::LivePlayOnly,
            DesEventPayload::AiManagerCheck {
                side: TeamSideV4::Home,
            },
        )
        .is_err());

    for idx in 0..=MAX_ZERO_DT_STREAK {
        sched
            .schedule(
                10_000,
                EventPriority::ActionDecision,
                CancellationScope::Persistent,
                DesEventPayload::TacticalCommandEffective {
                    command_index: idx,
                    side: TeamSideV4::Home,
                },
            )
            .unwrap();
    }
    for _ in 0..MAX_ZERO_DT_STREAK {
        assert!(sched.pop_next_valid().unwrap().is_some());
    }
    assert!(
        sched.pop_next_valid().is_err(),
        "scheduler must trip zero_dt_streak guard after MAX_ZERO_DT_STREAK same-ms pops"
    );
}

#[test]
fn des_ci_defender_intercepts_before_receiver() {
    use crate::v4::interaction::{
        find_earliest_ball_interaction, BallTrajectory, DefenderCandidate, InteractionKind,
    };

    let traj = BallTrajectory {
        start_time_ms: 1_000,
        start_pos: Vec3::new(40.0, 34.0, 0.2),
        initial_vel: Vec3::new(18.0, 0.0, 0.0),
        gravity_z: 9.81,
        drag: 0.0,
        is_shot: false,
        is_carrier_dribble: false,
    };
    let defenders = vec![DefenderCandidate {
        player_id: "A04".into(),
        side: TeamSideV4::Away,
        pos: Vec2::new(58.0, 36.2),
        max_speed_mps: 8.2,
        reach_m: 1.15,
        reaction_delay_ms: 120,
        anticipation_norm: 0.85,
        fatigue_factor: 1.0,
    }];

    // Scheduled receiver arrival is at t=3000ms (x=76m); defender at x=58m must intercept earlier.
    let hit = find_earliest_ball_interaction(&traj, &defenders, 1_000, 3_000)
        .expect("defender in passing corridor must intercept before receiver arrival");
    assert_eq!(hit.kind, InteractionKind::PassInterception);
    assert!(hit.time_ms > 1_000 && hit.time_ms < 3_000);
    assert_eq!(hit.defender_id.as_deref(), Some("A04"));
    assert_eq!(hit.defender_side, Some(TeamSideV4::Away));
}

#[test]
fn des_tactical_command_delayed_effective_time_and_override_policy() {
    use crate::v4::rules::CommandLifecycleStatus;
    use crate::v4::session::{MatchCommandV4, MatchSessionV4};

    let mut input = make_v4_input(14010, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.ai_home_manager = false;
    input.ai_away_manager = false;

    let mut session = MatchSessionV4::new(input).unwrap();
    // Advance into active live play
    let mut in_live = false;
    for _ in 0..40 {
        let s = session.advance_steps(1).unwrap();
        if s.dead_ball_state == "LIVE_PLAY" {
            in_live = true;
            break;
        }
    }
    assert!(in_live, "session must enter LIVE_PLAY");

    // 1st tactical command during live play must have a positive propagation delay
    let mut ti1 = TeamInstructionsV4::default();
    ti1.pressing_intensity = 86;
    ti1.line_height = 78;
    session
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: ti1,
        })
        .unwrap();

    // 2nd tactical command immediately overrides 1st while still pending
    let mut ti2 = TeamInstructionsV4::default();
    ti2.pressing_intensity = 28;
    ti2.line_height = 32;
    session
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: ti2,
        })
        .unwrap();

    let result = session.run_to_full_time().unwrap();
    assert_eq!(result.command_timeline.len(), 2);
    let cmd1 = &result.command_timeline[0];
    let cmd2 = &result.command_timeline[1];

    assert!(
        cmd1.scheduled_effective_sim_time_ms > cmd1.accepted_sim_time_ms,
        "live tactical command must have nonzero propagation delay"
    );
    assert!(
        cmd2.scheduled_effective_sim_time_ms > cmd2.accepted_sim_time_ms,
        "override tactical command must also have nonzero propagation delay"
    );
    assert_eq!(
        cmd1.status,
        CommandLifecycleStatus::Superseded,
        "earlier pending tactical command for same side must be superseded"
    );
    assert_eq!(cmd2.status, CommandLifecycleStatus::Applied);
    assert!(
        cmd2.actual_effective_sim_time_ms
            .map_or(false, |t| t >= cmd2.scheduled_effective_sim_time_ms),
        "applied tactical command must take effect at or after scheduled_effective_sim_time_ms"
    );
}

#[test]
fn des_live_sub_waits_for_next_deadball() {
    use crate::v4::events::SessionStageV4;
    use crate::v4::rules::CommandLifecycleStatus;
    use crate::v4::session::{MatchCommandV4, MatchSessionV4};

    let mut input = make_v4_input(14011, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.ai_home_manager = false;
    let mut bench = input.home_team.lineup[8].clone();
    bench.player_id = "LIVE-SUB-0".into();
    bench.display_name = "Live Sub 0".into();
    input.home_team.bench.push(bench);

    let mut session = MatchSessionV4::new(input).unwrap();
    let mut reached_live = false;
    for _ in 0..40 {
        let s = session.advance_steps(1).unwrap();
        if s.dead_ball_state == "LIVE_PLAY" {
            reached_live = true;
            break;
        }
    }
    assert!(reached_live, "must reach LIVE_PLAY");

    session
        .submit_command(MatchCommandV4::Substitute {
            side: TeamSideV4::Home,
            player_out_id: "H08".into(),
            player_in_id: "LIVE-SUB-0".into(),
        })
        .unwrap();

    // Immediately after 1 live step (if still live play), sub must remain queued and not yet on pitch
    let immediate = session.advance_steps(1).unwrap();
    if immediate.dead_ball_state == "LIVE_PLAY" {
        assert_eq!(immediate.home_roster.substitutions_used, 0);
        assert!(immediate.home_roster.on_pitch_player_ids.contains(&"H08".to_string()));
        assert!(!immediate.home_roster.on_pitch_player_ids.contains(&"LIVE-SUB-0".to_string()));
        assert!(immediate.pending_commands.iter().any(|c| {
            c.command_type == "SUBSTITUTE" && c.status == CommandLifecycleStatus::QueuedForDeadBall
        }));
    }

    // Advance until next dead ball flushes the queued substitution
    let mut executed = false;
    for _ in 0..500 {
        let s = session.advance_steps(10).unwrap();
        if s.home_roster.substitutions_used == 1 {
            assert!(s.home_roster.on_pitch_player_ids.contains(&"LIVE-SUB-0".to_string()));
            assert!(!s.home_roster.on_pitch_player_ids.contains(&"H08".to_string()));
            assert_eq!(s.home_roster.substitution_windows_used, 1);
            executed = true;
            break;
        }
        if s.stage == SessionStageV4::HalfTimeBreak {
            session.start_next_period().unwrap();
        }
        if s.is_full_time {
            break;
        }
    }
    assert!(executed, "queued live substitution must execute on the next dead ball");
}

#[test]
fn des_checkpoint_roundtrip_exact_continuation_all_states() {
    use crate::v4::checkpoint::{verify_checkpoint, MatchCheckpointV4};
    use crate::v4::events::SessionStageV4;
    use crate::v4::session::{MatchCommandV4, MatchSessionV4};

    let mut input = make_v4_input(14012, SimulationMode::FullEvent, RulesModeV4::Standard);
    input.ai_home_manager = false;
    let mut bench = input.home_team.lineup[8].clone();
    bench.player_id = "CKPT-SUB-0".into();
    input.home_team.bench.push(bench);

    let mut baseline = MatchSessionV4::new(input.clone()).unwrap();
    let mut resumed = MatchSessionV4::new(input).unwrap();

    // 1. Mid-first-half InHalf checkpoint with queued commands
    baseline.advance_steps(35).unwrap();
    resumed.advance_steps(35).unwrap();

    let mut ti = TeamInstructionsV4::default();
    ti.pressing_intensity = 77;
    baseline
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: ti.clone(),
        })
        .unwrap();
    resumed
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: ti,
        })
        .unwrap();

    let sub_cmd = MatchCommandV4::Substitute {
        side: TeamSideV4::Home,
        player_out_id: "H08".into(),
        player_in_id: "CKPT-SUB-0".into(),
    };
    baseline.submit_command(sub_cmd.clone()).unwrap();
    resumed.submit_command(sub_cmd).unwrap();

    let ckpt_live = resumed.export_checkpoint().unwrap();
    let ckpt_live_json = serde_json::to_string(&ckpt_live).unwrap();
    let parsed_live: MatchCheckpointV4 = serde_json::from_str(&ckpt_live_json).unwrap();
    verify_checkpoint(&parsed_live).unwrap();

    // Tampered checksum must be rejected
    let mut tampered = parsed_live.clone();
    tampered.checksum_hex = "00000000000000000000000000000000".into();
    assert!(verify_checkpoint(&tampered).is_err());

    let mut resumed = MatchSessionV4::from_checkpoint(parsed_live).unwrap();

    // 2. Advance both to HalfTimeBreak and round-trip a second checkpoint at HalfTimeBreak
    for _ in 0..2000 {
        let s = baseline.advance_steps(30).unwrap();
        if s.stage == SessionStageV4::HalfTimeBreak {
            break;
        }
    }
    for _ in 0..2000 {
        let s = resumed.advance_steps(30).unwrap();
        if s.stage == SessionStageV4::HalfTimeBreak {
            break;
        }
    }
    assert_eq!(baseline.stage(), resumed.stage());

    let ckpt_ht = resumed.export_checkpoint().unwrap();
    let ckpt_ht_json = serde_json::to_string(&ckpt_ht).unwrap();
    let parsed_ht: MatchCheckpointV4 = serde_json::from_str(&ckpt_ht_json).unwrap();
    verify_checkpoint(&parsed_ht).unwrap();
    let mut resumed = MatchSessionV4::from_checkpoint(parsed_ht).unwrap();

    // 3. Finish both sessions and verify bit-exact parity
    let res_base = baseline.run_to_full_time().unwrap();
    let res_resumed = resumed.run_to_full_time().unwrap();
    assert_eq!(res_base.event_timeline_fingerprint, res_resumed.event_timeline_fingerprint);
    assert_eq!(res_base.event_count, res_resumed.event_count);
    assert_eq!(res_base.summary, res_resumed.summary);
    assert_eq!(res_base.events, res_resumed.events);
}

#[test]
fn des_fullevent_fastbatch_exact_parity_with_commands() {
    use crate::v4::session::{
        replay_v4_with_timeline, MatchCommandV4, MatchSessionV4, SubstitutionChangeV4,
    };

    let mut input_full = make_v4_input(14013, SimulationMode::FullEvent, RulesModeV4::Standard);
    input_full.ai_home_manager = false;
    for i in 0..2 {
        let mut bench = input_full.home_team.lineup[8].clone();
        bench.player_id = format!("PARITY-SUB-{i}");
        input_full.home_team.bench.push(bench);
    }

    let mut session = MatchSessionV4::new(input_full.clone()).unwrap();
    session.advance_steps(15).unwrap();
    let mut ti = TeamInstructionsV4::default();
    ti.tempo = 78;
    ti.pressing_intensity = 74;
    session
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: ti,
        })
        .unwrap();
    session.advance_steps(20).unwrap();
    session
        .submit_command(MatchCommandV4::SubstituteBatch {
            side: TeamSideV4::Home,
            changes: vec![
                SubstitutionChangeV4 {
                    player_out_id: "H08".into(),
                    player_in_id: "PARITY-SUB-0".into(),
                },
                SubstitutionChangeV4 {
                    player_out_id: "H09".into(),
                    player_in_id: "PARITY-SUB-1".into(),
                },
            ],
        })
        .unwrap();

    let live_res = session.run_to_full_time().unwrap();
    assert_eq!(live_res.command_timeline.len(), 2);

    let replay_full =
        replay_v4_with_timeline(input_full.clone(), &live_res.command_timeline).unwrap();
    let mut input_fast = input_full;
    input_fast.simulation_mode = SimulationMode::FastBatch;
    let replay_fast = replay_v4_with_timeline(input_fast, &live_res.command_timeline).unwrap();

    assert!(replay_fast.events.is_empty());
    assert_eq!(live_res.event_timeline_fingerprint, replay_full.event_timeline_fingerprint);
    assert_eq!(live_res.event_timeline_fingerprint, replay_fast.event_timeline_fingerprint);
    assert_eq!(live_res.event_count, replay_fast.event_count);
    assert_eq!(live_res.summary, replay_full.summary);
    assert_eq!(live_res.summary, replay_fast.summary);
}

#[test]
fn des_ruleset_version_immutable_per_match() {
    use crate::v4::rules::check_ruleset_compatibility;
    use crate::v4::session::MatchSessionV4;

    let input_v1 = make_v4_input(14014, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut input_v2 = input_v1.clone();
    input_v2.ruleset.identity.ruleset_version = 2;

    assert_ne!(
        input_v1.input_fingerprint(),
        input_v2.input_fingerprint(),
        "ruleset_version must enter the immutable match input fingerprint"
    );
    assert!(check_ruleset_compatibility(
        &input_v1.ruleset.identity,
        &input_v2.ruleset.identity
    )
    .is_err());

    let mut session = MatchSessionV4::new(input_v1).unwrap();
    session.advance_steps(10).unwrap();
    let mut ckpt = session.export_checkpoint().unwrap();
    ckpt.ruleset_identity.ruleset_version = 99;
    assert!(
        MatchSessionV4::from_checkpoint(ckpt).is_err(),
        "checkpoint with mutated ruleset_version must be rejected"
    );
}

#[test]
fn des_fatigue_step_splitting_invariance() {
    use crate::v4::state::TwoPoolFatigueState;

    // 1. Live high-intensity burst + dead-ball recovery: 1 coarse interval vs many fine sub-intervals
    let mut coarse = TwoPoolFatigueState::from_condition(95);
    coarse.apply_interval(10.0, 40.0, true, false, 14, 14);
    coarse.apply_interval(20.0, 10.0, false, true, 14, 14);

    let mut fine = TwoPoolFatigueState::from_condition(95);
    for _ in 0..20 {
        fine.apply_interval(0.5, 2.0, true, false, 14, 14);
    }
    for _ in 0..40 {
        fine.apply_interval(0.5, 0.25, false, true, 14, 14);
    }

    assert!(
        (coarse.aerobic - fine.aerobic).abs() < 1e-3,
        "aerobic fatigue must be step-splitting invariant: coarse={} fine={}",
        coarse.aerobic,
        fine.aerobic
    );
    assert!(
        (coarse.anaerobic - fine.anaerobic).abs() < 1e-3,
        "anaerobic fatigue must be step-splitting invariant: coarse={} fine={}",
        coarse.anaerobic,
        fine.anaerobic
    );
    assert!(
        (coarse.distance_m - fine.distance_m).abs() < 1e-3,
        "distance accumulation must be step-splitting invariant"
    );
}



#[test]
fn qa01_long_pass_interception_emitted_at_interaction_time() {
    let input = make_v4_input(202601, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut session = MatchSessionV4::new(input).expect("session");
    let result = session.run_to_full_time().expect("full match");
    let mid_flight_interceptions: Vec<_> = result
        .events
        .iter()
        .filter(|e| {
            e.kind == EventTypeV4::Interception
                && e.metadata.extra.contains_key("launchSimTimeMs")
                && e.metadata.extra.contains_key("plannedArrivalSimTimeMs")
        })
        .collect();
    assert!(
        !mid_flight_interceptions.is_empty(),
        "QA-01: expected mid-flight interceptions with launch/arrival metadata"
    );
    let mut strict_mid_flight = 0;
    for ev in &mid_flight_interceptions {
        let launch_ms: i64 = ev.metadata.extra["launchSimTimeMs"].parse().expect("launch ms");
        let arrival_ms: i64 = ev.metadata.extra["plannedArrivalSimTimeMs"].parse().expect("arrival ms");
        match ev.metadata.extra.get("passContactPhase").map(String::as_str) {
            Some("ARRIVAL") => assert_eq!(ev.sim_time_ms, arrival_ms, "arrival contest must occur at arrival"),
            Some("MID_FLIGHT") => {
                assert!(ev.sim_time_ms > launch_ms && ev.sim_time_ms < arrival_ms,
                    "mid-flight interception must be strictly between kick and arrival");
                strict_mid_flight += 1;
            }
            _ => panic!("interception lacks explicit physical contact phase"),
        }
    }
    assert!(strict_mid_flight > 0, "must exercise genuine mid-flight interceptions");
}

#[test]
fn qa02_shot_goal_emitted_at_arrival_not_kick_time() {
    let mut verified_goals = 0usize;
    for seed in 1001..1025 {
        let input = make_v4_input(seed, SimulationMode::FullEvent, RulesModeV4::Standard);
        let mut session = MatchSessionV4::new(input).expect("session");
        let result = session.run_to_full_time().expect("full match");
        for ev in &result.events {
            if ev.kind == EventTypeV4::Goal {
                if let Some(launch_str) = ev.metadata.extra.get("shotLaunchSimTimeMs") {
                    let launch_ms: i64 = launch_str.parse().expect("shot launch ms");
                    assert!(
                        ev.sim_time_ms > launch_ms,
                        "QA-02: goal ({}ms) must occur STRICTLY after shot kick ({}ms)",
                        ev.sim_time_ms,
                        launch_ms
                    );
                    verified_goals += 1;
                }
            }
        }
        if verified_goals >= 2 {
            break;
        }
    }
    assert!(verified_goals >= 1, "QA-02: expected at least one open-play goal with shotLaunchSimTimeMs");
}

#[test]
fn qa08_checkpoint_roundtrip_preserves_rng_root_seed_and_diagnostics() {
    let input = make_v4_input(202608, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut s1 = MatchSessionV4::new(input).expect("s1");
    s1.advance_until_ms(1_200_000, 1000).expect("advance s1");
    let ckpt = s1.export_checkpoint().expect("export");
    let orig_root = ckpt.payload.rng.root_seed;
    let orig_des_steps = ckpt.payload.diagnostics_summary.des.des_steps_total;

    let mut s2 = MatchSessionV4::from_checkpoint(ckpt).expect("restore s2");
    let ckpt2 = s2.export_checkpoint().expect("re-export");
    assert_eq!(
        ckpt2.payload.rng.root_seed, orig_root,
        "QA-08/CRITICAL-09: root_seed must survive checkpoint round-trip"
    );
    assert_eq!(
        ckpt2.payload.diagnostics_summary.des.des_steps_total, orig_des_steps,
        "QA-08/CRITICAL-09: DES diagnostics collector counters must survive checkpoint round-trip"
    );
    let r1 = s1.run_to_full_time().expect("finish s1");
    let r2 = s2.run_to_full_time().expect("finish s2");
    assert_eq!(r1.event_timeline_fingerprint, r2.event_timeline_fingerprint);
    assert_eq!(r1.summary.home.goals, r2.summary.home.goals);
    assert_eq!(r1.summary.away.goals, r2.summary.away.goals);
}

#[test]
fn qa10_mid_interval_tactical_event_preserves_total_movement_and_fatigue() {
    let input = make_v4_input(202610, SimulationMode::FullEvent, RulesModeV4::Standard);
    let mut s_base = MatchSessionV4::new(input.clone()).expect("base");
    let mut s_cmd = MatchSessionV4::new(input).expect("cmd");
    s_base.advance_until_ms(600_000, 1000).expect("advance base");
    s_cmd.advance_until_ms(600_000, 1000).expect("advance cmd");
    let mut instr = s_cmd.export_checkpoint().expect("c").payload.input.home_team.team_instructions;
    instr.tempo = (instr.tempo + 5).min(100);
    s_cmd
        .submit_command(MatchCommandV4::UpdateTactics {
            side: TeamSideV4::Home,
            instructions: instr,
        })
        .expect("submit tactics");
    let step_b = s_base.advance_until_ms(650_000, 500).expect("b2");
    let step_c = s_cmd.advance_until_ms(650_000, 500).expect("c2");
    assert_eq!(
        step_b.ball_in_play_ms + step_b.ball_out_of_play_ms,
        step_c.ball_in_play_ms + step_c.ball_out_of_play_ms,
        "QA-10: inserting a delayed tactical event must not add or lose match clock time"
    );
    let ck_b = s_base.export_checkpoint().expect("ck_b");
    let ck_c = s_cmd.export_checkpoint().expect("ck_c");
    let avg_aerobic_b: f32 = ck_b.payload.state.players.iter().map(|p| p.fatigue.aerobic).sum::<f32>()
        / ck_b.payload.state.players.len() as f32;
    let avg_aerobic_c: f32 = ck_c.payload.state.players.iter().map(|p| p.fatigue.aerobic).sum::<f32>()
        / ck_c.payload.state.players.len() as f32;
    assert!(
        (avg_aerobic_b - avg_aerobic_c).abs() < 1.5,
        "QA-10: splitting interval with tactical event must preserve aerobic fatigue ({} vs {})",
        avg_aerobic_b,
        avg_aerobic_c
    );
}

#[test]
fn qa11_injury_in_added_time_extends_period_dynamically() {
    let mut clock = crate::clock::DesClockState::default();
    clock.begin_period(crate::clock::Period::FirstHalf, 2_700_000);
    clock.advance_ms(2_600_000, true).expect("advance");
    clock.advance_ms(100_000, false).expect("stoppage");
    clock.record_compensable_lost_ms(60_000);
    clock.lock_announced_added_ms(60_000);
    // Injury occurs during added time
    clock.advance_ms(45_000, false).expect("injury stoppage");
    clock.record_compensable_lost_ms(45_000);
    assert_eq!(clock.announced_added_ms, 60_000);
    assert_eq!(clock.additional_added_ms, 45_000);
    assert_eq!(clock.effective_added_ms(), 105_000);
    assert!(clock.compensable_lost_ms <= clock.ball_out_of_play_ms);
}

#[test]
#[ignore]
fn qa12_monte_carlo_1000_matches_benchmark() {
    let inputs: Vec<MatchInputV4> = (0..1000)
        .map(|i| make_v4_input(202612 + i as i64, SimulationMode::FastBatch, RulesModeV4::Standard))
        .collect();
    let report = crate::v4::calibration::run_monte_carlo_v4(inputs).expect("monte carlo 1000");
    assert_eq!(report.match_count, 1000);
    assert!(report.matches_per_sec > 0.0);
    assert!(report.avg_ball_in_play_min > 0.0);
}

 
/// Diagnostic-only population export. Record failed seeds rather than discarding
/// the entire sample on the first failed match. Never relax the existing gates.
#[test]
#[ignore = "Run explicitly for DES calibration evidence"]
fn des_calibration_population_evidence() {
    use serde_json::json;
    use std::time::Instant;
    let scenario = std::env::var("DES_DIAG_SCENARIO").unwrap_or_else(|_| "balanced".into());
    let count = std::env::var("DES_DIAG_MATCHES").ok()
        .and_then(|x| x.parse::<usize>().ok()).unwrap_or(128).clamp(1, 3000);
    let mut seeds = Vec::with_capacity(count);
    let mut inputs = Vec::with_capacity(count);
    for i in 0..count {
        let seed = 104729 + i as i64 * 7907;
        seeds.push(seed);
        let (hl, al) = match scenario.as_str() {
            "baseline" => (14, 12),
            "balanced" | "high_press" | "low_block" => (13, 13),
            "strong_home" => (16, 9),
            "strong_away" => (9, 16),
            _ => panic!("Unknown diagnostic scenario {}", scenario),
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
        // Match the original baseline's manager settings, isolate tactics in all other cohorts.
        if scenario != "baseline" {
            input.ai_home_manager = false;
            input.ai_away_manager = false;
            input.neutral_ground = true;
        }
        inputs.push(input);
    }
    let started = Instant::now();
    let results = simulate_batch_fast_v4(inputs);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let mut rows = Vec::with_capacity(count);
    let mut succeeded = 0usize;
    let mut invalid = 0usize;
    let mut failed = 0usize;
    let mut goals = 0u64;
    let mut shots = 0u64;
    let mut attempts = 0u64;
    let mut completed = 0u64;
    let mut live_ms = 0i128;
    let mut dead_ms = 0i128;
    let mut diagnosed = 0usize;
    for (seed, outcome) in seeds.into_iter().zip(results) {
        match outcome {
            Ok(r) => {
                succeeded += 1;
                let h = &r.summary.home;
                let a = &r.summary.away;
                goals += h.goals as u64 + a.goals as u64;
                shots += h.shots as u64 + a.shots as u64;
                attempts += h.passes_attempted as u64 + a.passes_attempted as u64;
                completed += h.passes_completed as u64 + a.passes_completed as u64;
                let validation = match &r.diagnostics {
                    Some(d) => {
                        diagnosed += 1;
                        live_ms += d.des.ball_in_play_ms as i128;
                        dead_ms += d.des.ball_out_of_play_ms as i128;
                        d.validate_basic().err()
                    }
                    None => Some("Missing diagnostics".into()),
                };
                if validation.is_some() { invalid += 1; }
                rows.push(json!({
                    "seed": seed, "ok": true, "diagnosticError": validation,
                    "eventCount": r.event_count, "fingerprint": r.event_timeline_fingerprint,
                    "summary": r.summary, "diagnostics": r.diagnostics
                }));
            }
            Err(e) => {
                failed += 1;
                rows.push(json!({"seed": seed, "ok": false, "error": format!("{:?}", e)}));
            }
        }
    }
    let n = succeeded.max(1) as f64;
    let dn = diagnosed.max(1) as f64;
    let headline = json!({
        "attemptedMatches": count, "completedMatches": succeeded, "failedMatches": failed,
        "invalidDiagnostics": invalid, "matchesWithDiagnostics": diagnosed, "elapsedMs": elapsed_ms,
        "goalsPerCompletedMatch": goals as f64 / n,
        "shotsPerCompletedMatch": shots as f64 / n,
        "passesAttemptedPerCompletedMatch": attempts as f64 / n,
        "passAccuracyPct": 100.0 * completed as f64 / attempts.max(1) as f64,
        "ballInPlayMinPerDiagnosedMatch": live_ms as f64 / 60000.0 / dn,
        "ballOutOfPlayMinPerDiagnosedMatch": dead_ms as f64 / 60000.0 / dn
    });
    let report = json!({
        "schemaVersion": 1, "scenario": scenario,
        "note": "Synthetic fixtures. Averages describe completed matches only; failed seeds retained. No empirical realism pass/fail thresholds.",
        "headline": headline, "matches": rows
    });
    let output = std::env::var("DES_DIAG_REPORT_PATH").expect("DES_DIAG_REPORT_PATH");
    std::fs::write(&output, serde_json::to_string_pretty(&report).unwrap()).expect("write evidence");
    println!("DES_POPULATION_EVIDENCE {}", serde_json::to_string(&report["headline"]).unwrap());
    assert_eq!(succeeded + failed, count);
    assert_eq!(failed + invalid, 0, "See per-seed evidence in {}", output);
}


/// A previously failing seed from the 1000-match DES population run.
/// Do not raise/disable the clock watchdog to pass this regression.
#[test]
fn des_population_watchdog_seed_1970781_finishes_both_halves() {
    let result = simulate_v4(make_v4_input(
        1_970_781,
        SimulationMode::FullEvent,
        RulesModeV4::Standard,
    )).expect("previously failing population seed must finish");
    assert!(result.summary.first_half_elapsed_seconds >= 2700);
    assert!(result.summary.second_half_elapsed_seconds >= 2700);
    assert_eq!(
        result.events.iter().filter(|e| e.kind == EventTypeV4::HalfTime).count(),
        1
    );
    assert!(result.summary.final_game_second >= 5400);
    let d = result.diagnostics.expect("diagnostics");
    d.validate_basic().expect("valid diagnostics");
}


// Calibration stage 1: actual decision and movement functions, with a known
// opponent line. The referee's existing Law 11 semantics stay independent.
#[test]
fn des_offside_awareness_avoids_clearly_illegal_pass_target() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    use crate::v4::perception::{perceive_evaluate_and_decide_des, PlannedActionV4};
    use crate::v4::ball_play::is_receiver_offside;
    let input = make_v4_input_teams(12345, SimulationMode::FullEvent, RulesModeV4::Standard,
        make_v4_team("H", "H", 18), make_v4_team("A", "A", 13));
    let mut state = MatchStateV4::from_input(&input);
    let carrier = state.players.iter().position(|p| p.player_id == "H06").unwrap();
    let legal = state.players.iter().position(|p| p.player_id == "H07").unwrap();
    let illegal = state.players.iter().position(|p| p.player_id == "H10").unwrap();
    for (i, p) in state.players.iter_mut().enumerate() {
        if p.side == TeamSideV4::Home {
            p.on_pitch = i == carrier || i == legal || i == illegal;
            p.pos = if i == carrier { Vec2::new(60.0, 34.0) }
                else if i == legal { Vec2::new(72.0, 34.0) }
                else { Vec2::new(84.0, 34.0) };
        } else {
            p.pos = if p.is_goalkeeper { Vec2::new(100.0, 5.0) } else { Vec2::new(76.0, 5.0) };
        }
    }
    assert!(!is_receiver_offside(&state, TeamSideV4::Home, state.players[carrier].pos, state.players[legal].pos));
    assert!(is_receiver_offside(&state, TeamSideV4::Home, state.players[carrier].pos, state.players[illegal].pos));
    let ctx = evaluate_local_context(&state, &input, TeamSideV4::Home, state.players[carrier].pos);
    let mut legal_count = 0;
    let mut illegal_count = 0;
    for seed in 1..=128 {
        let mut rng = DeterministicRng::new(seed * 7919);
        let plan = perceive_evaluate_and_decide_des(&state, &input, &input.timing_model, carrier, ctx, &mut rng);
        if let PlannedActionV4::Pass { target_idx, .. } = plan.action {
            if target_idx == legal { legal_count += 1; }
            if target_idx == illegal { illegal_count += 1; }
        }
    }
    println!("OFFSIDE_DECISION legal={} illegal={}", legal_count, illegal_count);
    assert!(legal_count >= 64, "clear legal option should be preferred");
    assert!(illegal_count <= 8, "skilled player should rarely choose clearly offside runner");
}

#[test]
fn des_offside_runner_corrects_target_without_teleporting_or_stopping_in_flight() {
    use crate::v4::state::{MatchStateV4, BallControlState};
    use crate::v4::movement::{advance_off_ball_movement, player_attrs, player_max_speed_mps};
    use crate::v4::ball_play::offside_reference_progress_m;
    for side in [TeamSideV4::Home, TeamSideV4::Away] {
        let input = make_v4_input_teams(12346, SimulationMode::FullEvent, RulesModeV4::Standard,
            make_v4_team("H", "H", 18), make_v4_team("A", "A", 18));
        let mut state = MatchStateV4::from_input(&input);
        let id_prefix = if side == TeamSideV4::Home { "H" } else { "A" };
        let carrier = state.players.iter().position(|p| p.player_id == format!("{}06", id_prefix)).unwrap();
        let runner = state.players.iter().position(|p| p.player_id == format!("{}10", id_prefix)).unwrap();
        let x = |progress: f32| if side == TeamSideV4::Home { progress } else { PITCH_LENGTH_M - progress };
        for p in &mut state.players {
            if p.side != side {
                p.pos = Vec2::new(x(if p.is_goalkeeper { 100.0 } else { 75.0 }), 5.0);
            }
        }
        state.players[carrier].pos = Vec2::new(x(60.0), 34.0);
        state.players[runner].pos = Vec2::new(x(85.0), 34.0);
        state.ball.pos = Vec3::ground(x(60.0), 34.0);
        state.ball.control = BallControlState::Controlled;
        let line = offside_reference_progress_m(&state, side, state.players[carrier].pos).unwrap();
        assert!((line - 75.0).abs() < 0.01);
        let before = state.players[runner].pos;
        let speed = player_max_speed_mps(player_attrs(&state, &state.players[runner]), &state.players[runner]);
        let mut in_flight = state.clone();
        in_flight.ball.control = BallControlState::InFlight;
        let mut rng = DeterministicRng::new(12347);
        let mut rng_flight = DeterministicRng::new(12347);
        advance_off_ball_movement(&mut state, &input, side, carrier, 0.1, &mut rng);
        advance_off_ball_movement(&mut in_flight, &input, side, carrier, 0.1, &mut rng_flight);
        let p = &state.players[runner];
        assert!(attacking_progress_m(side, p.target_pos) < line);
        assert!(p.pos.distance(before) <= speed * 0.1 + 0.001, "run correction must respect movement budget");
        assert!(attacking_progress_m(side, p.pos) > line, "runner cannot teleport back onside");
        assert!(attacking_progress_m(side, in_flight.players[runner].target_pos) > line,
            "after release, runner must be allowed to run beyond the launch line");
    }
}

// Shared fixture permits independently compiled pre-PI and PI policies to be compared.
include!("pi_parity_fixture.rs");

#[test]
fn pi4_legacy_api_and_direct_policy_match() {
    use crate::player_intelligence as pi;
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    let input = make_v4_input(123456, SimulationMode::FullEvent, RulesModeV4::Standard);
    let state = MatchStateV4::from_input(&input);
    let carrier = state.players.iter().position(|p| p.on_pitch && !p.is_goalkeeper).unwrap();
    let p = &state.players[carrier];
    let ctx = evaluate_local_context(&state, &input, p.side, p.pos);
    for seed in 1..=64 {
        let mut a = DeterministicRng::new(seed);
        let mut b = a.clone();
        let old = crate::v4::perception::perceive_evaluate_and_decide_des(
            &state, &input, &input.timing_model, carrier, ctx, &mut a);
        let new = pi::perceive_evaluate_and_decide_des(
            &state, &input, &input.timing_model, carrier, ctx, &mut b);
        assert_eq!(old, new);
        assert_eq!(a.state(), b.state());
    }
    assert_eq!(pi::PI_MODULE_VERSION, "4.0.2");
}


#[test]
fn pi_on_ball_prefers_useful_progress_to_empty_switch() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    use crate::player_intelligence::{perceive_evaluate_and_decide_des,PlannedActionV4,PassKindV4};
    let input=make_v4_input_teams(19301,SimulationMode::FullEvent,RulesModeV4::Standard,
        make_v4_team("H","H",18),make_v4_team("A","A",13));
    let mut state=MatchStateV4::from_input(&input);
    let car=state.players.iter().position(|p|p.player_id=="H06").unwrap();
    let forward=state.players.iter().position(|p|p.player_id=="H07").unwrap();
    let switch=state.players.iter().position(|p|p.player_id=="H08").unwrap();
    for (i,p) in state.players.iter_mut().enumerate() {
        if p.side==TeamSideV4::Home {
            p.on_pitch=i==car || i==forward || i==switch;
            p.pos=if i==car {Vec2::new(47.0,10.0)} else if i==forward {
                Vec2::new(71.0,32.0)
            } else {Vec2::new(47.0,58.0)};
        } else {
            p.pos=Vec2::new(if p.is_goalkeeper {100.0} else {80.0},66.0);
        }
    }
    let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[car].pos);
    let mut useful=0;
    let mut empty=0;
    for seed in 1..=128 {
        let mut rng=DeterministicRng::new(seed*7919);
        let plan=perceive_evaluate_and_decide_des(&state,&input,&input.timing_model,car,ctx,&mut rng);
        if let PlannedActionV4::Pass{target_idx,kind,..}=plan.action {
            if target_idx==forward {useful+=1;}
            if target_idx==switch {assert_eq!(kind,PassKindV4::Switch);empty+=1;}
        }
    }
    println!("PI_ON_BALL_CHOICE useful={} empty_switch={}",useful,empty);
    assert!(useful>empty && useful>64,"clear progressive option should beat a pointless switch");
}
