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
    let summary=&result.summary;
    let ht=result.events.iter().find(|ev|ev.kind==EventTypeV4::HalfTime).unwrap();
    let h2=result.events.iter().find(|ev|
        ev.kind==EventTypeV4::Kickoff && ev.period==2).unwrap();
    let ft=result.events.iter().find(|ev|ev.kind==EventTypeV4::FullTime).unwrap();

    // The DES clock is authoritative in milliseconds. The public
    // second fields independently truncate the respective exact values.
    // floor(a+b) need not equal floor(a)+floor(b).
    let first_ms=ht.period_elapsed_ms;
    let second_ms=ft.period_elapsed_ms;
    assert!(first_ms>=HALF_MS_V4 && second_ms>=HALF_MS_V4);
    assert_eq!(ht.sim_time_ms,first_ms);
    assert_eq!(h2.sim_time_ms,ht.sim_time_ms,"halftime must not consume simulation time");
    assert_eq!(h2.period_elapsed_ms,0);
    assert_eq!(ft.sim_time_ms,first_ms+second_ms);
    let diag=&result.diagnostics.as_ref().unwrap().des;
    assert_eq!(diag.ball_in_play_ms+diag.ball_out_of_play_ms,ft.sim_time_ms);
    assert_eq!(summary.first_half_elapsed_seconds,(first_ms/1000) as i32);
    assert_eq!(summary.second_half_elapsed_seconds,(second_ms/1000) as i32);
    assert_eq!(summary.final_game_second,((first_ms+second_ms)/1000) as i32);
    assert_eq!(h2.period_elapsed_second,0,
        "second-half 45:00 must not inherit first-half added time");
    assert_eq!(h2.game_second,summary.first_half_elapsed_seconds);
    for ev in &result.events {
        assert!(ev.period_elapsed_ms>=0);
        assert_eq!(ev.game_second,(ev.sim_time_ms/1000) as i32);
        assert_eq!(ev.period_elapsed_second,(ev.period_elapsed_ms/1000) as i32);
        match ev.period {
            1=>assert_eq!(ev.sim_time_ms,ev.period_elapsed_ms),
            2=>assert_eq!(ev.sim_time_ms,first_ms+ev.period_elapsed_ms),
            _=>{}
        }
    }
    println!("PI_CLOCK_EXACT first_ms={first_ms} second_ms={second_ms} total_ms={} display_seconds={}/{}/{}",
        ft.sim_time_ms,summary.first_half_elapsed_seconds,summary.second_half_elapsed_seconds,summary.final_game_second);
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
        include_str!("../player_intelligence/on_ball.rs"),
        include_str!("../player_intelligence/support.rs"),
        include_str!("../player_intelligence/defensive.rs"),
        include_str!("../player_intelligence/tackle.rs"),
        include_str!("../player_intelligence/pass_forecast.rs"),
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
    // Compare actual identical time horizons, not two different final
    // event timestamps that happen to precede the same requested time.
    // These are explicit test-only scheduler boundaries in BOTH sessions.
    for s in [&mut s_base,&mut s_cmd] {
        // Starting the first period invalidates previous period-scoped
        // events, so install probes only after kickoff initialization.
        s.start_next_period().expect("start first period");
        s.schedule_test_clock_probe(600_000).expect("initial probe");
        s.schedule_test_clock_probe(650_000).expect("final probe");
        let step=s.advance_until_ms(600_000,1000).expect("advance to command horizon");
        assert_eq!(step.sim_time_ms,600_000);
        assert_eq!(step.ball_in_play_ms+step.ball_out_of_play_ms,600_000);
    }
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
    for step in [&step_b,&step_c] {
        assert_eq!(step.sim_time_ms,650_000,"QA-10: common actual horizon");
        assert_eq!(step.ball_in_play_ms+step.ball_out_of_play_ms,step.sim_time_ms,
            "QA-10: no unaccounted or double-counted time");
    }
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
    // The manifest is frozen independently from runtime constants, not generated from them.
    let m: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/pi4_baseline_identity.json")).unwrap();
    assert_eq!(m["schema"].as_u64(), Some(1));
    assert_eq!(m["pi_module_version"].as_str(), Some(pi::PI_MODULE_VERSION));
    assert_eq!(m["pi_policy_id"].as_str(), Some(pi::PI_POLICY_ID));
    assert_eq!(m["pi_extraction_baseline"].as_str(), Some(pi::PI_BASELINE_COMMIT));
    assert_eq!(m["engine_version"].as_str(), Some(ENGINE_VERSION_V4));
    assert_eq!(m["jni_bridge_version"].as_i64(), Some(crate::v4::JNI_BRIDGE_VERSION as i64));
    assert_eq!(m["protocol_version"].as_u64(), Some(ENGINE_PROTOCOL_VERSION_V4 as u64));
    assert_eq!(m["event_schema_version"].as_u64(), Some(crate::v4::EVENT_SCHEMA_VERSION_V4 as u64));
    assert_eq!(m["checkpoint_schema_version"].as_u64(), Some(crate::v4::CHECKPOINT_SCHEMA_VERSION_V4 as u64));
    let checkpoint = MatchSessionV4::new(input).unwrap().export_checkpoint().unwrap();
    crate::v4::verify_checkpoint(&checkpoint).unwrap();
    assert_eq!(m["pi_policy_id"].as_str(), Some(checkpoint.payload.pi_policy_id.as_str()));
    assert_eq!(m["checkpoint_schema_version"].as_u64(), Some(checkpoint.checkpoint_schema_version as u64));
    assert_eq!(m["event_schema_version"].as_u64(), Some(checkpoint.event_schema_version as u64));
    assert_eq!(m["protocol_version"].as_u64(), Some(checkpoint.protocol_version as u64));
    assert_eq!(m["engine_version"].as_str(), Some(checkpoint.engine_version.as_str()));

    // Mutate policy then recompute checksum: rejection must be compatibility,
    // not just the generic checksum guard. The valid checkpoint still restores.
    let mut wrong_policy = checkpoint.clone();
    wrong_policy.payload.pi_policy_id = "incompatible-a0-negative-control".into();
    wrong_policy.checksum_hex = crate::v4::compute_checkpoint_checksum(
        &serde_json::to_string(&wrong_policy.payload).unwrap());
    assert!(crate::v4::verify_checkpoint(&wrong_policy).unwrap_err().to_string().contains("PI policy mismatch"));
    assert!(MatchSessionV4::from_checkpoint(wrong_policy).is_err());
    let mut wrong_schema = checkpoint.clone();
    wrong_schema.checkpoint_schema_version += 1;
    assert!(crate::v4::verify_checkpoint(&wrong_schema).unwrap_err().to_string().contains("unsupported checkpoint schema"));
    let mut wrong_protocol = checkpoint.clone();
    wrong_protocol.protocol_version += 1;
    assert!(crate::v4::verify_checkpoint(&wrong_protocol).is_err());
    let mut wrong_event = checkpoint.clone();
    wrong_event.event_schema_version += 1;
    assert!(crate::v4::verify_checkpoint(&wrong_event).is_err());
    let mut wrong_checksum = checkpoint.clone();
    wrong_checksum.checksum_hex = "0".repeat(32);
    assert!(crate::v4::verify_checkpoint(&wrong_checksum).unwrap_err().to_string().contains("checkpoint checksum mismatch"));
    MatchSessionV4::from_checkpoint(checkpoint).unwrap();
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


#[test]
fn pi_on_ball_shot_vs_continuation_scenario() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    use crate::player_intelligence::{perceive_evaluate_and_decide_des,PlannedActionV4};
    let input=make_v4_input_teams(19302,SimulationMode::FullEvent,RulesModeV4::Standard,
        make_v4_team("H","H",18),make_v4_team("A","A",13));
    let mut state=MatchStateV4::from_input(&input);
    let car=state.players.iter().position(|p|p.player_id=="H06").unwrap();
    let mate=state.players.iter().position(|p|p.player_id=="H07").unwrap();
    for (i,p) in state.players.iter_mut().enumerate() {
        if p.side==TeamSideV4::Home {
            p.on_pitch=i==car || i==mate;
            p.pos=if i==car {Vec2::new(86.0,50.0)} else {Vec2::new(96.0,34.0)};
        } else {
            p.pos=Vec2::new(if p.is_goalkeeper {104.0} else {100.0},if p.is_goalkeeper {34.0} else {66.0});
        }
    }
    let mut bad_shots=0;
    let mut close_shots=0;
    for clear in [false,true] {
        if clear {
            state.players[car].pos=Vec2::new(101.0,34.0);
            state.players[mate].on_pitch=false;
        }
        let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[car].pos);
        for seed in 1..=128 {
            let mut rng=DeterministicRng::new(seed*7919);
            let plan=perceive_evaluate_and_decide_des(&state,&input,&input.timing_model,car,ctx,&mut rng);
            if matches!(plan.action,PlannedActionV4::Shoot{..}) {
                if clear {close_shots+=1;} else {bad_shots+=1;}
            }
        }
    }
    println!("PI_SHOT_CHOICE speculative={} clear={}",bad_shots,close_shots);
    assert!(close_shots>64 && close_shots>bad_shots,"clear chance must be preferred over speculative shooting");
}


#[test]
fn pi_support_final_target_respects_speed_and_direction() {
    use crate::v4::state::{MatchStateV4,BallControlState};
    use crate::v4::movement::{advance_off_ball_movement,player_attrs,player_max_speed_mps};
    for side in [TeamSideV4::Home,TeamSideV4::Away] {
        let input=make_v4_input_teams(19501,SimulationMode::FullEvent,RulesModeV4::Standard,
            make_v4_team("H","H",18),make_v4_team("A","A",18));
        let mut state=MatchStateV4::from_input(&input);
        let idx=state.players.iter().position(|p|p.on_pitch && !p.is_goalkeeper && p.side==side).unwrap();
        state.players[idx].pos=Vec2::new(52.5,34.0);
        state.ball.pos=Vec3::ground(52.5,34.0);
        state.ball.control=BallControlState::Controlled;
        let mut rng=DeterministicRng::new(19502);
        for dt in [0.01f32,0.05,0.2,0.7,1.6] {
            let before:Vec<_>=state.players.iter().map(|p|(p.pos,player_max_speed_mps(player_attrs(&state,p),p))).collect();
            advance_off_ball_movement(&mut state,&input,side,idx,dt,&mut rng);
            for (i,p) in state.players.iter().enumerate().filter(|(_,p)|p.on_pitch && !p.sent_off) {
                let (old,speed)=before[i];
                let moved=old.distance(p.pos);
                assert!(moved<=speed*dt+0.002,"{} exceeded final target speed budget",p.player_id);
                assert!(p.pos.distance(p.target_pos)<=old.distance(p.target_pos)+0.002);
                assert!((moved+p.pos.distance(p.target_pos)-old.distance(p.target_pos)).abs()<0.004,
                    "{} stepped off final intended segment",p.player_id);
            }
        }
    }
}


#[test]
fn pi_defense_actual_movement_respects_budget_and_approaches_ball() {
    use crate::v4::state::{MatchStateV4,BallControlState};
    use crate::v4::movement::{advance_off_ball_movement_with_report,player_attrs,player_max_speed_mps};
    use crate::player_intelligence::DefensiveRole;
    for side in [TeamSideV4::Home,TeamSideV4::Away] {
        let mut home=make_v4_team("H","H",18);
        let mut away=make_v4_team("A","A",18);
        home.team_instructions.pressing_intensity=86;
        away.team_instructions.pressing_intensity=86;
        let input=make_v4_input_teams(19601,SimulationMode::FullEvent,RulesModeV4::Standard,home,away);
        let mut state=MatchStateV4::from_input(&input);
        let carrier=state.players.iter().position(|p|p.on_pitch && !p.is_goalkeeper && p.side==side).unwrap();
        let ball=Vec2::new(52.5,34.0);
        state.players[carrier].pos=ball;
        state.ball.pos=Vec3::ground(ball.x,ball.y);
        state.ball.control=BallControlState::Controlled;
        let before:Vec<_>=state.players.iter().map(|p|(p.pos,player_max_speed_mps(player_attrs(&state,p),p))).collect();
        let mut rng=DeterministicRng::new(19602);
        let assignments=advance_off_ball_movement_with_report(&mut state,&input,side,carrier,0.2,&mut rng);
        let press=assignments.iter().find(|a|a.role==DefensiveRole::Press).expect("nearby defender engages at high intensity");
        assert!(state.players[press.idx].pos.distance(ball)<=before[press.idx].0.distance(ball)+0.002);
        for a in &assignments {
            let p=&state.players[a.idx];
            assert_eq!(p.side,side.opposite());
            assert!(!p.is_goalkeeper);
            assert!(before[a.idx].0.distance(p.pos)<=before[a.idx].1*0.2+0.002);
            assert_eq!(p.target_pos,a.target);
        }
    }
}


#[test]
fn pi_checkpoint_float_diagnostics_preserve_exact_bits() {
    // Accumulators sum f32 forecasts in f64. Verify full f64 JSON
    // roundtrip rather than masking a checksum mismatch by rounding.
    let mut total=0.0f64;
    for i in 1..=4096 {
        total+=f64::from((i as f32*0.0137).sin().abs());
        let encoded=serde_json::to_string(&total).unwrap();
        let decoded:f64=serde_json::from_str(&encoded).unwrap();
        assert_eq!(total.to_bits(),decoded.to_bits());
    }
}

#[test]
fn pi_checkpoint_observed_pass_risk_accounting_roundtrip() {
    let input=make_v4_input(14012,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut s=MatchSessionV4::new(input).unwrap();
    s.advance_until_ms(300_000,1500).unwrap();
    let ck=s.export_checkpoint().unwrap();
    let before=&ck.payload.diagnostics_summary.des;
    assert!(before.pass_risk_pairs>0);
    assert_eq!(before.pass_risk_pairs,before.pass_risk_increased+before.pass_risk_decreased+before.pass_risk_stable);
    assert!(before.pass_risk_initial_sum<=f64::from(before.pass_risk_pairs));
    assert!(before.pass_risk_release_sum<=f64::from(before.pass_risk_pairs));
    let json=serde_json::to_string(&ck).unwrap();
    let parsed=serde_json::from_str(&json).unwrap();
    crate::v4::checkpoint::verify_checkpoint(&parsed).unwrap();
    let restored=MatchSessionV4::from_checkpoint(parsed).unwrap().export_checkpoint().unwrap();
    assert_eq!(ck.checksum_hex,restored.checksum_hex);
}


#[test]
fn pi_pass_reaim_prefers_open_compatible_lane_and_charges_time() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    use crate::player_intelligence::{reconsider_short_pass,PlannedActionV4,PassKindV4};
    let input=make_v4_input_teams(19801,SimulationMode::FullEvent,RulesModeV4::Standard,
        make_v4_team("H","H",18),make_v4_team("A","A",13));
    let mut state=MatchStateV4::from_input(&input);
    let car=state.players.iter().position(|p|p.player_id=="H06").unwrap();
    let original=state.players.iter().position(|p|p.player_id=="H07").unwrap();
    let alternative=state.players.iter().position(|p|p.player_id=="H08").unwrap();
    let defender=state.players.iter().position(|p|p.side==TeamSideV4::Away&&!p.is_goalkeeper).unwrap();
    for (i,p) in state.players.iter_mut().enumerate() {
        p.vel=Vec2::default();
        if p.side==TeamSideV4::Home {
            p.on_pitch=i==car||i==original||i==alternative;
            p.pos=if i==car{Vec2::new(42.0,34.0)}else if i==original{Vec2::new(57.0,34.0)}else{Vec2::new(58.0,44.0)};
        }else{p.pos=Vec2::new(92.0,66.0);}
    }
    state.players[defender].pos=Vec2::new(49.0,34.0);
    let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[car].pos);
    let action=PlannedActionV4::Pass{target_idx:original,kind:PassKindV4::Short,
        distance_m:15.0,progress_gain_m:15.0,lane_risk:0.0,interceptor_idx:None};
    let change=reconsider_short_pass(&state,&input,car,ctx,action).expect("blocked old lane and open alternate");
    assert!(matches!(change.action,PlannedActionV4::Pass{target_idx,kind:PassKindV4::Short,..} if target_idx==alternative));
    assert!(change.delay_ms>0 && change.utility_gain>0.0);
    state.players[alternative].on_pitch=false;
    assert!(reconsider_short_pass(&state,&input,car,ctx,action).is_none());
    assert!(reconsider_short_pass(&state,&input,car,ctx,PlannedActionV4::HoldAndRecycle).is_none());
}


// Stage 5.7A: controlled sensitivity screening, not empirical calibration.
// One named attribute changes in a fixed role cohort. The 12-level
// baseline is reused for all attributes within each group and seed.
const PI_AUDIT_GROUPS:[&str;5]=["technical","mental","physical","goalkeeping","hidden"];
const PI_CAREER_HOOKS:[&str;4]=["professionalism","ambition","adaptability","loyalty"];

fn pi_audit_profile(group:&str,name:&str,level:u8)->PlayerAttributes54 {
    let baseline=PlayerAttributes54::uniform(12).unwrap();
    let mut value=serde_json::to_value(baseline).unwrap();
    *value.get_mut(group).and_then(|g|g.get_mut(name)).expect("canonical attribute")=serde_json::json!(level);
    let profile:PlayerAttributes54=serde_json::from_value(value).unwrap();
    profile.validate().unwrap();
    for (key,actual) in profile.all_named() {
        assert_eq!(actual,if key==name{level}else{12});
    }
    profile
}

fn pi_audit_input(seed:i64,group:&str,name:&str,level:u8)->MatchInputV4 {
    let profile=pi_audit_profile(group,name,level);
    let neutral=PlayerAttributes54::uniform(12).unwrap();
    let home=make_v4_team_with_attrs("H","HOME_AUDIT",|i|{
        let eligible=match group {
            "goalkeeping"=>i==0,
            "technical"=>i!=0,
            _=>true,
        };
        if eligible{profile}else{neutral}
    });
    let mut input=make_v4_input_teams(seed,SimulationMode::FastBatch,RulesModeV4::Standard,
        home,make_v4_team("A","AWAY_AUDIT",12));
    // Fixed tactics, not manager adaptation. Standard match context;
    // rare set pieces and big matches may require separate fixtures.
    input.ai_home_manager=false;
    input.ai_away_manager=false;
    input
}

#[test]
fn pi_attribute_audit_changes_only_the_named_field_and_role() {
    let baseline=serde_json::to_value(PlayerAttributes54::uniform(12).unwrap()).unwrap();
    let mut count=0;
    for group in PI_AUDIT_GROUPS {
        for name in baseline[group].as_object().unwrap().keys() {
            let low=pi_audit_input(22331,group,name,5);
            let mid=pi_audit_input(22331,group,name,12);
            let high=pi_audit_input(22331,group,name,18);
            assert_ne!(low.input_fingerprint(),mid.input_fingerprint());
            assert_ne!(high.input_fingerprint(),mid.input_fingerprint());
            for (i,(a,b)) in low.home_team.lineup.iter().zip(&high.home_team.lineup).enumerate() {
                let eligible=match group {"goalkeeping"=>i==0,"technical"=>i!=0,_=>true};
                for ((ka,va),(kb,vb)) in a.attributes.all_named().into_iter().zip(b.attributes.all_named()) {
                    assert_eq!(ka,kb);
                    if eligible && ka==name {
                        assert_eq!((va,vb),(5,18));
                    }else{assert_eq!((va,vb),(12,12));}
                }
            }
            assert_eq!(serde_json::to_value(&low.away_team).unwrap(),serde_json::to_value(&high.away_team).unwrap());
            count+=1;
        }
    }
    assert_eq!(count,54);
}

fn pi_audit_match(input:MatchInputV4)->serde_json::Value {
    let result=simulate_v4(input).expect("attribute audit match must simulate");
    let diag=result.diagnostics.as_ref().expect("FastBatch diagnostics");
    diag.validate_basic().expect("valid diagnostics");
    assert!(result.ok);
    serde_json::json!({
        "seed":result.seed,"inputFingerprint":result.input_fingerprint,
        "timelineFingerprint":result.event_timeline_fingerprint,
        "eventCount":result.event_count,"summary":result.summary,"diagnostics":diag
    })
}

#[test]
#[ignore = "population evidence: run in the dedicated five-group workflow"]
fn pi_attribute_audit_population() {
    let group=std::env::var("PI_AUDIT_GROUP").expect("set PI_AUDIT_GROUP");
    assert!(PI_AUDIT_GROUPS.contains(&group.as_str()));
    let path=std::env::var("PI_AUDIT_REPORT_PATH").expect("set PI_AUDIT_REPORT_PATH");
    let profile=serde_json::to_value(PlayerAttributes54::uniform(12).unwrap()).unwrap();
    let names:Vec<_>=profile[&group].as_object().unwrap().keys().cloned().collect();
    assert!(!names.is_empty());
    let seeds:Vec<i64>=(0..8).map(|i|104729+i*7907).collect();
    let mut baselines=Vec::new();
    for seed in &seeds {
        baselines.push(pi_audit_match(pi_audit_input(*seed,&group,&names[0],12)));
    }
    let mut rows=Vec::new();
    let mut career_differences=0usize;
    let mut career_carryover_differences=0usize;
    for name in &names {
        let career=PI_CAREER_HOOKS.contains(&name.as_str());
        for (i,seed) in seeds.iter().enumerate() {
            let low=pi_audit_match(pi_audit_input(*seed,&group,name,5));
            let high=pi_audit_match(pi_audit_input(*seed,&group,name,18));
            let mid=&baselines[i];
            let same=|a:&serde_json::Value,b:&serde_json::Value|{
                ["timelineFingerprint","eventCount","summary","diagnostics"].iter().all(|key|a[*key]==b[*key])
            };
            let low_same=same(&low,mid);
            let high_same=same(&high,mid);
            // Only the explicitly documented home-player morale carryover
            // is permitted to differ for ambition/loyalty. Every other
            // field, including opponent morale and timeline, remains exact.
            let carryover_allowed=matches!(name.as_str(),"ambition"|"loyalty");
            let low_play_same=pi_audit_same_play(&low,mid,carryover_allowed);
            let high_play_same=pi_audit_same_play(&high,mid,carryover_allowed);
            if career && (!low_play_same||!high_play_same){career_differences+=1;}
            if career && low_play_same && high_play_same && (!low_same||!high_same) {
                career_carryover_differences+=1;
            }
            if carryover_allowed {
                pi_audit_check_morale(&low,name,5);
                pi_audit_check_morale(mid,name,12);
                pi_audit_check_morale(&high,name,18);
            }
            rows.push(serde_json::json!({
                "attribute":name,"seed":seed,"careerHook":career,
                "lowEqualsBaseline":low_same,"highEqualsBaseline":high_same,
                "lowPlayEqualsBaseline":low_play_same,"highPlayEqualsBaseline":high_play_same,
                "low":low,"high":high
            }));
        }
    }
    let simulations=baselines.len()+2*rows.len();
    let report=serde_json::json!({
        "schemaVersion":1,"piVersion":crate::player_intelligence::PI_MODULE_VERSION,
        "policyId":crate::player_intelligence::PI_POLICY_ID,
        "group":group,"levels":[5,12,18],"seeds":seeds,
        "roleCohort":match group.as_str(){"goalkeeping"=>"home goalkeeper only","technical"=>"home outfield only",_=>"all home starters"},
        "simulationMode":"FAST_BATCH","fixedTactics":true,
        "baseline":baselines,"pairs":rows,"simulations":simulations,
        "careerDifferences":career_differences,
        "careerCarryoverDifferences":career_carryover_differences,
        "limitations":["Only eight matched seeds per attribute; descriptive screening, not validation of effect size.",
            "Same seed does not imply the same later random draws after decisions diverge.",
            "Whole-match outcomes can reflect indirect changes in opportunities, not direct skill execution.",
            "Rare events, big matches and action-level probabilities require isolated fixtures."]
    });
    std::fs::write(&path,serde_json::to_string(&report).unwrap()).expect("write full evidence");
    println!("PI_ATTRIBUTE_AUDIT group={} attributes={} simulations={} careerDifferences={}",group,names.len(),simulations,career_differences);
    println!("PI_CAREER_CARRYOVER group={} pairsWithAllowedMoraleChanges={}",group,career_carryover_differences);
    assert_eq!(career_differences,0,"career-only hooks changed football output beyond allowed morale carryover");
}


fn pi_audit_same_play(a:&serde_json::Value,b:&serde_json::Value,allow_home_morale:bool)->bool {
    if ["timelineFingerprint","eventCount","diagnostics"].iter().any(|k|a[*k]!=b[*k]) {return false;}
    let project=|r:&serde_json::Value|{
        let mut s=r["summary"].clone();
        if allow_home_morale {
            for p in s["players"].as_array_mut().unwrap() {
                if p["playerId"].as_str().unwrap().starts_with("H") {
                    p.as_object_mut().unwrap().remove("moraleDelta");
                }
            }
        }
        s
    };
    project(a)==project(b)
}

fn pi_audit_check_morale(row:&serde_json::Value,name:&str,level:u8) {
    use crate::v4::psychology::compute_post_match_morale_delta;
    let profile=pi_audit_profile("hidden",name,level);
    let neutral=PlayerAttributes54::uniform(12).unwrap();
    let s=&row["summary"];
    let h=s["home"]["goals"].as_u64().unwrap();
    let a=s["away"]["goals"].as_u64().unwrap();
    for p in s["players"].as_array().unwrap() {
        let home=p["playerId"].as_str().unwrap().starts_with("H");
        let expected=compute_post_match_morale_delta(if home{&profile}else{&neutral},
            if home{h>a}else{a>h},h==a,p["rating"].as_f64().unwrap() as f32);
        assert_eq!(p["moraleDelta"].as_i64().unwrap(),i64::from(expected));
    }
}

#[test]
fn pi_attribute_isolated_career_projection_preserves_all_other_fields() {
    let a=serde_json::json!({"timelineFingerprint":"a","eventCount":1,"diagnostics":{},
        "summary":{"home":{"goals":1},"players":[{"playerId":"H00","moraleDelta":2,"distanceM":12},
        {"playerId":"A00","moraleDelta":-2}]}});
    let mut b=a.clone();
    b["summary"]["players"][0]["moraleDelta"]=serde_json::json!(4);
    assert!(!pi_audit_same_play(&a,&b,false));
    assert!(pi_audit_same_play(&a,&b,true));
    b["summary"]["players"][1]["moraleDelta"]=serde_json::json!(0);
    assert!(!pi_audit_same_play(&a,&b,true));
    b=a.clone();
    b["summary"]["players"][0]["distanceM"]=serde_json::json!(13);
    assert!(!pi_audit_same_play(&a,&b,true));
    b=a.clone();
    b["timelineFingerprint"]=serde_json::json!("b");
    assert!(!pi_audit_same_play(&a,&b,true));
}

#[test]
fn pi_attribute_isolated_goalkeeper_contexts() {
    use crate::v4::goalkeeper::{resolve_gk_aerial_claim,GkAerialOutcome};
    let input=make_v4_input(22401,SimulationMode::FastBatch,RulesModeV4::Standard);
    // Fresh paired RNG per opportunity: branch-dependent draws cannot
    // desynchronize subsequent attempts.
    for (name,one_vs_one,distance) in [("reflexes",false,9.0),("one_on_ones",true,11.0)] {
        let mut conceded=[0u32;3];
        for (i,level) in [5,12,18].into_iter().enumerate() {
            let attrs=pi_audit_profile("goalkeeping",name,level);
            for k in 0..4096i64 {
                let mut rng=DeterministicRng::new(104729+k*7907);
                if resolve_gk_shot_reaction(&attrs,1.0,90.0,0.32,distance,one_vs_one,false,false,&input,&mut rng)==GkShotOutcome::GoalConceded {
                    conceded[i]+=1;
                }
            }
        }
        println!("PI_ISOLATED attribute={name} opportunities=4096 conceded_5_12_18={conceded:?}");
        assert!(conceded[0]>conceded[1] && conceded[1]>conceded[2]);
    }
    let mut punches=[0u32;3];
    let mut come_out=[0u32;3];
    for (i,level) in [5,12,18].into_iter().enumerate() {
        let attrs=pi_audit_profile("goalkeeping","punching",level);
        for k in 0..4096i64 {
            let mut rng=DeterministicRng::new(104729+k*7907);
            match resolve_gk_aerial_claim(&attrs,185,true,2.35,&input,&mut rng) {
                GkAerialOutcome::PunchedClear=>{punches[i]+=1;come_out[i]+=1;},
                GkAerialOutcome::Claimed=>come_out[i]+=1,
                GkAerialOutcome::StayedOnLine=>{},
            }
        }
    }
    println!("PI_ISOLATED attribute=punching opportunities=4096 punches_5_12_18={punches:?} engaged={come_out:?}");
    assert_eq!(come_out[0],come_out[1]);
    assert_eq!(come_out[1],come_out[2]);
    assert!(punches[0]<punches[1] && punches[1]<punches[2]);
}

#[test]
fn pi_attribute_isolated_set_piece_contexts() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::set_pieces::{execute_corner_v4,execute_free_kick_v4,execute_penalty_kick_v4_with_rules};
    for name in ["corners","free_kicks","penalties"] {
        let mut successes=[0u32;3];
        let mut opportunities=[0u32;3];
        for (i,level) in [5,12,18].into_iter().enumerate() {
            let mut input=pi_audit_input(22402,"technical",name,level);
            // Fix the taker, not merely the team distribution.
            input.home_team.set_piece_plan.corner_taker_slot_ids=vec!["CF".into()];
            input.home_team.set_piece_plan.close_free_kick_taker_slot_ids=vec!["CF".into()];
            input.home_team.set_piece_plan.penalty_taker_slot_ids=vec!["CF".into()];
            let template=MatchStateV4::from_input(&input);
            for k in 0..4096i64 {
                let mut state=template.clone();
                let mut rng=DeterministicRng::new(104729+k*7907);
                let out=match name {
                    "corners"=>execute_corner_v4(&mut state,&input,TeamSideV4::Home,&mut rng),
                    "free_kicks"=>execute_free_kick_v4(&mut state,&input,TeamSideV4::Home,Vec2::new(82.0,34.0),&mut rng),
                    _=>execute_penalty_kick_v4_with_rules(&mut state,&input,TeamSideV4::Home,false,&mut rng),
                };
                if name=="corners" {
                    if let Some(duel)=out.events.iter().find(|e|e.kind==EventTypeV4::AerialDuel) {
                        opportunities[i]+=1;
                        successes[i]+=u32::from(duel.side==Some(TeamSideV4::Home));
                    }
                } else {
                    opportunities[i]+=1;
                    successes[i]+=u32::from(if name=="free_kicks"{
                        out.events.iter().any(|e|e.kind==EventTypeV4::ShotOnTarget && e.side==Some(TeamSideV4::Home))
                    }else{
                        out.events.iter().any(|e|e.kind==EventTypeV4::Goal && e.side==Some(TeamSideV4::Home))
                    });
                }
            }
        }
        println!("PI_ISOLATED attribute={name} successes_5_12_18={successes:?} opportunities={opportunities:?}");
        assert!(opportunities[0]>0);
        assert_eq!(opportunities[0],opportunities[1]);
        assert_eq!(opportunities[1],opportunities[2]);
        assert!(successes[0]<successes[1] && successes[1]<successes[2]);
    }
}

#[test]
fn pi_attribute_isolated_long_shot_context() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::shooting::{resolve_shot_v4_des,ShotKindV4};
    let mut xg=[0u64;3];
    let mut on_target=[0u32;3];
    for (i,level) in [5,12,18].into_iter().enumerate() {
        let input=pi_audit_input(22403,"technical","long_shots",level);
        let mut template=MatchStateV4::from_input(&input);
        let idx=template.players.iter().position(|p|p.player_id=="H10").unwrap();
        for p in &mut template.players {
            if p.side==TeamSideV4::Away && !p.is_goalkeeper {p.on_pitch=false;}
        }
        template.players[idx].pos=Vec2::new(80.0,34.0);
        template.ball.pos=Vec3::ground(80.0,34.0);
        for k in 0..2048i64 {
            let mut state=template.clone();
            let mut rng=DeterministicRng::new(104729+k*7907);
            resolve_shot_v4_des(&mut state,&input,&input.timing_model,idx,None,Some(ShotKindV4::LongRange),false,&mut rng);
            xg[i]+=u64::from(state.home.xg_millis);
            on_target[i]+=u32::from(state.home.shots_on_target>0);
        }
    }
    println!("PI_ISOLATED attribute=long_shots opportunities=2048 xg_millis_5_12_18={xg:?} on_target={on_target:?}");
    assert!(xg[0]<xg[1] && xg[1]<xg[2]);
    assert!(on_target[0]<on_target[1] && on_target[1]<on_target[2]);
}

#[test]
fn pi_attribute_isolated_prematch_contexts() {
    use crate::v4::psychology::compute_prematch_form_multiplier;
    let mut spread=[0.0f64;3];
    let mut standard=[0.0f64;3];
    let mut final_match=[0.0f64;3];
    for (i,level) in [5,12,18].into_iter().enumerate() {
        let cons=pi_audit_profile("hidden","consistency",level);
        let big=pi_audit_profile("hidden","big_matches",level);
        for k in 0..4096i64 {
            let seed=104729+k*7907;
            let mut rng=DeterministicRng::new(seed);
            spread[i]+=f64::from((compute_prematch_form_multiplier(&cons,50,&mut rng)-1.0).abs());
            let mut rng=DeterministicRng::new(seed);
            standard[i]+=f64::from(compute_prematch_form_multiplier(&big,50,&mut rng));
            let mut rng=DeterministicRng::new(seed);
            final_match[i]+=f64::from(compute_prematch_form_multiplier(&big,95,&mut rng));
        }
    }
    println!("PI_ISOLATED attribute=consistency mean_abs_deviation_5_12_18={:?}",spread.map(|x|x/4096.0));
    println!("PI_ISOLATED attribute=big_matches standard_5_12_18={:?} final_5_12_18={:?}",standard.map(|x|x/4096.0),final_match.map(|x|x/4096.0));
    assert!(spread[0]>spread[1] && spread[1]>spread[2]);
    assert_eq!(standard[0],standard[1]);
    assert_eq!(standard[1],standard[2]);
    assert!(final_match[0]<final_match[1] && final_match[1]<final_match[2]);
}


#[test]
fn pi_psychology_match_form_uses_one_draw_and_excludes_career_readiness() {
    use crate::v4::psychology::{sample_prematch_form,compute_prematch_form_multiplier};
    for seed in 1..=128i64 {
        let attrs=PlayerAttributes54::uniform(12).unwrap();
        for name in ["professionalism","adaptability"] {
            let mut low=DeterministicRng::new(seed);
            let mut high=DeterministicRng::new(seed);
            let l=pi_audit_profile("hidden",name,5);
            let h=pi_audit_profile("hidden",name,18);
            let (form_l,_)=sample_prematch_form(&l,95,&mut low);
            let (form_h,_)=sample_prematch_form(&h,95,&mut high);
            assert_eq!(form_l.to_bits(),form_h.to_bits());
            assert_eq!(low.next_double(),high.next_double());
        }
        let mut paired=DeterministicRng::new(seed);
        let mut legacy=DeterministicRng::new(seed);
        let (_,legacy_value)=sample_prematch_form(&attrs,95,&mut paired);
        assert_eq!(legacy_value,compute_prematch_form_multiplier(&attrs,95,&mut legacy));
        assert_eq!(paired.next_double(),legacy.next_double());
    }
}

#[test]
fn pi_psychology_matchday_form_and_focus_reach_policy() {
    use crate::v4::psychology::update_in_match_psychology;
    use crate::player_intelligence::decision_uncertainty;
    let input=make_v4_input(22501,SimulationMode::FullEvent,RulesModeV4::Standard);
    let session=MatchSessionV4::new(input.clone()).unwrap();
    let checkpoint=session.export_checkpoint().unwrap();
    let mut state=checkpoint.payload.state.clone();
    let idx=state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let form=state.players[idx].psychology.match_form_multiplier;
    assert!((0.84..=1.16).contains(&form));
    // Verify initialization uses the exact per-player seeded sample,
    // independent of any later tactical decisions.
    let mut rng=DeterministicRng::new(input.seed);
    for p in &state.players {
        let attrs=crate::v4::movement::lookup_attrs(&input,p);
        let (expected,_)=crate::v4::psychology::sample_prematch_form(attrs,input.match_environment.match_importance,&mut rng);
        assert_eq!(p.psychology.match_form_multiplier.to_bits(),expected.to_bits());
    }
    let json=serde_json::to_string(&checkpoint).unwrap();
    let restored=MatchSessionV4::from_checkpoint(serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(checkpoint.checksum_hex,restored.export_checkpoint().unwrap().checksum_hex);
    state.players[idx].psychology.match_form_multiplier=1.0;
    state.players[idx].fatigue.aerobic=98.0;
    update_in_match_psychology(&mut state,&input,600,0.0);
    let fresh=decision_uncertainty(&state,idx,0.4);
    state.players[idx].psychology.match_form_multiplier=1.16;
    let good=decision_uncertainty(&state,idx,0.4);
    state.players[idx].psychology.match_form_multiplier=0.84;
    let bad=decision_uncertainty(&state,idx,0.4);
    assert!(good<fresh && fresh<bad);
    state.players[idx].psychology.match_form_multiplier=1.0;
    state.players[idx].fatigue.aerobic=35.0;
    update_in_match_psychology(&mut state,&input,4800,0.0);
    let tired=decision_uncertainty(&state,idx,0.4);
    assert!(tired>fresh);
    state.players[idx].psychology.confidence=0.15;
    assert_eq!(tired,decision_uncertainty(&state,idx,0.4));
    println!("PI_PSYCHOLOGY uncertainty good={good} fresh={fresh} bad={bad} fatigued={tired}");
}

#[test]
fn pi_psychology_contextual_form_changes_actual_policy_choices() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    use crate::player_intelligence::perceive_evaluate_and_decide_des;
    let input=make_v4_input(22502,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut state=MatchStateV4::from_input(&input);
    let idx=state.players.iter().position(|p|p.player_id=="H06").unwrap();
    state.players[idx].pos=Vec2::new(52.5,34.0);
    state.ball.pos=Vec3::ground(52.5,34.0);
    let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[idx].pos);
    let mut low=state.clone();
    let mut high=state.clone();
    low.players[idx].psychology.match_form_multiplier=0.84;
    high.players[idx].psychology.match_form_multiplier=1.16;
    let mut different=0;
    for seed in 1..=512i64 {
        let mut l=DeterministicRng::new(seed*7907);
        let mut h=DeterministicRng::new(seed*7907);
        let a=perceive_evaluate_and_decide_des(&low,&input,&input.timing_model,idx,ctx,&mut l);
        let b=perceive_evaluate_and_decide_des(&high,&input,&input.timing_model,idx,ctx,&mut h);
        if a.action!=b.action {different+=1;}
    }
    println!("PI_PSYCHOLOGY paired_action_changes={different}/512");
    assert!(different>0,"match-day form must reach actual policy selection in a fixed observation");
}


#[test]
fn pi_psychology_reserve_form_is_independent_of_entry_route_and_minute() {
    use crate::v4::ai_manager::{apply_manual_substitution_group,evaluate_ai_managers};
    use crate::v4::psychology::{sample_prematch_form,substitute_psychology};
    use crate::v4::state::MatchStateV4;
    let mut input=make_v4_input(22601,SimulationMode::FullEvent,RulesModeV4::Standard);
    input.match_environment.match_importance=95;
    input.ai_home_manager=true;
    input.ai_away_manager=false;
    let mut reserve=input.home_team.lineup[8].clone();
    reserve.player_id="H_RES_FORM".into();
    reserve.display_name="Reserve form fixture".into();
    reserve.eligible=true;
    reserve.injured=false;
    reserve.attributes.hidden.consistency=5;
    reserve.attributes.hidden.big_matches=18;
    input.home_team.bench=vec![reserve.clone()];
    let channel=format!("pi4-matchday-reserve:{:?}:{}",TeamSideV4::Home,reserve.player_id);
    let mut independent=DeterministicRng::new(input.seed).fork(&channel);
    let (expected,_)=sample_prematch_form(&reserve.attributes,95,&mut independent);
    let mut manual=MatchStateV4::from_input(&input);
    let mut automatic=manual.clone();
    manual.clock.game_second=900;
    let out=input.home_team.lineup[8].player_id.clone();
    apply_manual_substitution_group(&mut manual,&input,TeamSideV4::Home,
        &[(out.clone(),reserve.player_id.clone())],false).unwrap();
    automatic.clock.game_second=4000;
    for player in &mut automatic.players {
        if player.side==TeamSideV4::Home {
            player.fatigue.aerobic=if player.player_id==out{40.0}else{98.0};
        }
    }
    let events=evaluate_ai_managers(&mut automatic,&input);
    assert!(events.iter().any(|e|e.kind==EventTypeV4::Substitution));
    let entered=|state:&MatchStateV4|state.players.iter()
        .find(|p|p.player_id==reserve.player_id && p.on_pitch).unwrap().psychology;
    let m=entered(&manual);
    let a=entered(&automatic);
    assert_eq!(m.match_form_multiplier.to_bits(),expected.to_bits());
    assert_eq!(a.match_form_multiplier.to_bits(),expected.to_bits());
    assert_eq!(m,a,"entry mechanism and minute must not change initial psychology");
    assert_eq!(manual.home.substitutions_used,1);
    assert_eq!(automatic.home.substitutions_used,1);
    let restored:MatchStateV4=serde_json::from_str(&serde_json::to_string(&automatic).unwrap()).unwrap();
    assert_eq!(entered(&restored),a);
    // Seed and identity are relevant, career readiness is not a direct
    // input into the football form component.
    let mut distinct=std::collections::BTreeSet::new();
    for seed in 22601..22633i64 {
        input.seed=seed;
        let f=substitute_psychology(&input,TeamSideV4::Home,&reserve);
        let mut changed=reserve.clone();
        changed.attributes.hidden.professionalism=5;
        changed.attributes.hidden.adaptability=18;
        let g=substitute_psychology(&input,TeamSideV4::Home,&changed);
        assert_eq!(f.match_form_multiplier.to_bits(),g.match_form_multiplier.to_bits());
        distinct.insert(f.match_form_multiplier.to_bits());
    }
    assert!(distinct.len()>1,"reserve form cannot be a constant");
    println!("PI_PSYCHOLOGY reserve_form={expected} distinct_seed_forms={}",distinct.len());
}


#[test]
fn pi_psychology_contextual_pressure_is_not_physical_pressing() {
    use crate::v4::psychology::update_in_match_psychology;
    use crate::player_intelligence::decision_uncertainty;
    use crate::v4::state::MatchStateV4;
    let mut input=make_v4_input(22701,SimulationMode::FullEvent,RulesModeV4::Standard);
    input.match_environment.match_importance=95;
    let template=MatchStateV4::from_input(&input);
    let idx=template.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let mut ahead=template.clone();
    let mut behind=template.clone();
    ahead.home.goals=1;
    behind.away.goals=1;
    for state in [&mut ahead,&mut behind] {
        // Hold physical condition, match form, opponent proximity and
        // clock fixed. Only the psychological context differs.
        state.players[idx].fatigue.aerobic=90.0;
        state.players[idx].psychology.match_form_multiplier=1.0;
        update_in_match_psychology(state,&input,4900,0.0);
    }
    let a=ahead.players[idx].psychology;
    let b=behind.players[idx].psychology;
    assert_eq!(a.concentration_level,b.concentration_level);
    assert!(b.pressure_load>a.pressure_load);
    assert!(b.composure_delta<a.composure_delta);
    for physical in [0.0,0.4,0.9] {
        let calm=decision_uncertainty(&ahead,idx,physical);
        let stressed=decision_uncertainty(&behind,idx,physical);
        assert!(stressed>calm);
        println!("PI_PSYCHOLOGY contextual physical_pressure={physical} ahead={calm} behind={stressed}");
    }
    let baseline=decision_uncertainty(&behind,idx,0.4);
    // The load is already reflected in composure_delta. It must not
    // be counted a second time by the policy.
    behind.players[idx].psychology.pressure_load=1.0;
    assert_eq!(baseline,decision_uncertainty(&behind,idx,0.4));
    behind.players[idx].psychology.confidence=0.15;
    assert_eq!(baseline,decision_uncertainty(&behind,idx,0.4));
}

#[test]
fn pi_psychology_leadership_and_pressure_resistance_buffer_late_stress() {
    use crate::v4::psychology::update_in_match_psychology;
    use crate::player_intelligence::decision_uncertainty;
    use crate::v4::state::MatchStateV4;
    // Change leadership of the team, not the decision maker's other
    // attributes. The existing aura uses the strongest active leader.
    let mut low=pi_audit_input(22702,"mental","leadership",5);
    let mut high=pi_audit_input(22702,"mental","leadership",18);
    low.match_environment.match_importance=95;
    high.match_environment.match_importance=95;
    let mut a=MatchStateV4::from_input(&low);
    let mut b=MatchStateV4::from_input(&high);
    let idx=a.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    a.away.goals=1;
    b.away.goals=1;
    update_in_match_psychology(&mut a,&low,4900,0.0);
    update_in_match_psychology(&mut b,&high,4900,0.0);
    let weak=a.players[idx].psychology;
    let strong=b.players[idx].psychology;
    assert!(strong.pressure_load<weak.pressure_load);
    assert!(strong.composure_delta>weak.composure_delta);
    assert!(decision_uncertainty(&b,idx,0.4)<decision_uncertainty(&a,idx,0.4));
    // Remove high leadership from the pitch: the aura must be derived
    // only from active players, not the static bench or sent-off list.
    for p in &mut b.players {
        if p.side==TeamSideV4::Home && p.player_id!=a.players[idx].player_id {
            p.sent_off=true;
        }
    }
    // A single remaining high-leadership player still supplies the
    // max aura. Verify that semantics explicitly.
    update_in_match_psychology(&mut b,&high,4900,0.0);
    assert_eq!(strong.pressure_load,b.players[idx].psychology.pressure_load);

    let mut low_p=pi_audit_input(22703,"hidden","pressure",5);
    let mut high_p=pi_audit_input(22703,"hidden","pressure",18);
    low_p.match_environment.match_importance=95;
    high_p.match_environment.match_importance=95;
    let mut c=MatchStateV4::from_input(&low_p);
    let mut d=MatchStateV4::from_input(&high_p);
    c.away.goals=1;
    d.away.goals=1;
    update_in_match_psychology(&mut c,&low_p,4900,0.0);
    update_in_match_psychology(&mut d,&high_p,4900,0.0);
    // At zero local physical pressure the old static uncertainty term
    // is identical. Difference comes from contextual pressure buffering.
    assert!(d.players[idx].psychology.pressure_load<c.players[idx].psychology.pressure_load);
    assert!(decision_uncertainty(&d,idx,0.0)<decision_uncertainty(&c,idx,0.0));
}

#[test]
fn pi_psychology_momentum_recovery_and_checkpoint_context() {
    use crate::v4::psychology::{shift_momentum,update_in_match_psychology};
    use crate::player_intelligence::decision_uncertainty;
    let input=make_v4_input(22704,SimulationMode::FullEvent,RulesModeV4::Standard);
    let session=MatchSessionV4::new(input.clone()).unwrap();
    let mut checkpoint=session.export_checkpoint().unwrap();
    let state=&mut checkpoint.payload.state;
    let idx=state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    update_in_match_psychology(state,&input,1200,0.0);
    let neutral=decision_uncertainty(state,idx,0.4);
    shift_momentum(state,TeamSideV4::Home,0.35);
    update_in_match_psychology(state,&input,1200,0.0);
    let after=decision_uncertainty(state,idx,0.4);
    assert!(after<neutral);
    let mut split=state.clone();
    update_in_match_psychology(state,&input,1320,120.0);
    for second in 1201..=1320 {
        update_in_match_psychology(&mut split,&input,second,1.0);
    }
    let recovered=decision_uncertainty(state,idx,0.4);
    assert!(after<recovered && recovered<neutral);
    assert!((state.home.momentum-split.home.momentum).abs()<1e-5);
    assert!((recovered-decision_uncertainty(&split,idx,0.4)).abs()<1e-6);
    // Roundtrip real psychology state separately from the existing
    // full-session checksum/continuation tests (which remain enabled).
    let restored:crate::v4::state::MatchStateV4=serde_json::from_str(&serde_json::to_string(state).unwrap()).unwrap();
    assert_eq!(state.players[idx].psychology,restored.players[idx].psychology);
    assert_eq!(recovered,decision_uncertainty(&restored,idx,0.4));
    println!("PI_PSYCHOLOGY momentum neutral={neutral} after={after} recovered={recovered}");
}


#[test]
fn pi_defense_contact_audit_shared_angle_and_speed() {
    use crate::v4::state::MatchStateV4;
    use crate::v4::ball_play::{duel_contact_probabilities,resolve_carry_contact};
    use crate::v4::movement::evaluate_local_context;
    let input=make_v4_input(22901,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut state=MatchStateV4::from_input(&input);
    let a=state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let b=state.players.iter().position(|p|p.side==TeamSideV4::Away&&!p.is_goalkeeper).unwrap();
    state.players[a].pos=Vec2::new(50.0,34.0);
    state.players[b].pos=Vec2::new(51.0,34.0);
    state.players[a].vel=Vec2::default();
    state.players[b].vel=Vec2::default();
    state.players[a].facing_rad=0.0;
    let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[a].pos);
    let mut rear=state.clone();
    rear.players[a].facing_rad=std::f32::consts::PI;
    let front_pi=crate::player_intelligence::evaluate_tackle(&state,a,b,0.0);
    let rear_pi=crate::player_intelligence::evaluate_tackle(&rear,a,b,0.0);
    let front=duel_contact_probabilities(&state,&input,a,b,ctx,false);
    let back=duel_contact_probabilities(&rear,&input,a,b,ctx,false);
    assert!(front_pi.access>rear_pi.access);
    assert!(front_pi.clean_win_probability>rear_pi.clean_win_probability);
    // The same attribute and fatigue profile must now distinguish
    // front and rear access in the actual resolver, as PI already did.
    assert!(front.0>back.0);
    let mut fast=state.clone();
    fast.players[b].vel=Vec2::new(-6.0,0.0);
    let fast_probs=duel_contact_probabilities(&fast,&input,a,b,ctx,false);
    assert!(fast_probs.0<front.0);
    let mut mirror=state.clone();
    mirror.players[a].pos=Vec2::new(55.0,34.0);
    mirror.players[b].pos=Vec2::new(54.0,34.0);
    mirror.players[a].facing_rad=std::f32::consts::PI;
    assert!((duel_contact_probabilities(&mirror,&input,a,b,ctx,false).0-front.0).abs()<1e-6);
    let mut counts=[[0u32;3];3];
    for k in 0..4096i64 {
        for (j,template) in [&state,&rear,&fast].into_iter().enumerate() {
            let mut x=template.clone();
            let mut rng=DeterministicRng::new(104729+k*7907);
            let out=resolve_carry_contact(&mut x,&input,a,Some(b),ctx,false,&mut rng);
            for (m,kind) in [EventTypeV4::TackleWon,EventTypeV4::Foul,EventTypeV4::AdvantagePlayed].into_iter().enumerate() {
                counts[j][m]+=u32::from(out.events.iter().any(|e|e.kind==kind ||
                    (kind==EventTypeV4::TackleWon && e.kind==EventTypeV4::LooseBall && e.metadata.cause.as_deref()==Some("TACKLE_CONTACT_LOOSE"))));
            }
        }
    }
    for (j,expected) in [front,back,fast_probs].into_iter().enumerate() {
        for (observed,prob) in counts[j].into_iter().zip([expected.0,expected.1,expected.2]) {
            assert!((observed as f32/4096.0-prob).abs()<0.04,"branch frequency vs exact resolver");
        }
    }
    assert!(counts[0][0]>counts[1][0]&&counts[0][0]>counts[2][0]);
    println!("PI_TACKLE_SHARED_GEOMETRY front_pi={} rear_pi={} resolver_front={front:?} resolver_rear={back:?} resolver_fast={fast_probs:?} observed={counts:?} n=4096_per_context",
        front_pi.clean_win_probability,rear_pi.clean_win_probability);
}


#[test]
fn pi_defense_body_contact_is_separate_from_ball_success() {
    use crate::v4::interaction::missed_ball_body_overlap;
    use crate::v4::ball_play::{duel_contact_probabilities,duel_missed_ball_foul_probability,resolve_carry_contact};
    use crate::v4::state::MatchStateV4;
    use crate::v4::movement::evaluate_local_context;
    let c=Vec2::new(50.0,34.0);
    let d=Vec2::new(51.0,34.0);
    assert_eq!(missed_ball_body_overlap(c,0.0,d,1.0),0.0);
    assert!(missed_ball_body_overlap(c,std::f32::consts::PI,d,1.0)>0.95);
    assert_eq!(missed_ball_body_overlap(c,0.0,Vec2::new(47.0,34.0),1.0),0.0);
    let mirrored=missed_ball_body_overlap(Vec2::new(55.0,34.0),0.0,Vec2::new(54.0,34.0),1.0);
    assert!((mirrored-missed_ball_body_overlap(c,std::f32::consts::PI,d,1.0)).abs()<1e-5);
    let input=make_v4_input(23001,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut front=MatchStateV4::from_input(&input);
    let a=front.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let b=front.players.iter().position(|p|p.side==TeamSideV4::Away&&!p.is_goalkeeper).unwrap();
    front.players[a].pos=c;
    front.players[b].pos=d;
    front.players[a].vel=Vec2::default();
    front.players[b].vel=Vec2::default();
    front.players[a].facing_rad=0.0;
    let mut rear=front.clone();
    rear.players[a].facing_rad=std::f32::consts::PI;
    let mut ctx=evaluate_local_context(&front,&input,TeamSideV4::Home,c);
    ctx.overload=false;
    let f=duel_contact_probabilities(&front,&input,a,b,ctx,false);
    let r=duel_contact_probabilities(&rear,&input,a,b,ctx,false);
    assert_eq!(duel_missed_ball_foul_probability(&front,&input,a,b),0.0);
    assert!(duel_missed_ball_foul_probability(&rear,&input,a,b)>0.0);
    assert!(f.0>r.0&&r.1>f.1);
    assert!(r.0+r.1+r.2<=1.0);
    // Forecast now recognizes the same unsafe-miss branch without
    // reading the opponent's real technical attributes or the RNG.
    let pi_front=crate::player_intelligence::evaluate_tackle(&front,a,b,0.0);
    let pi_rear=crate::player_intelligence::evaluate_tackle(&rear,a,b,0.0);
    assert!(pi_front.clean_win_probability>pi_rear.clean_win_probability);
    assert!(pi_rear.foul_probability>pi_front.foul_probability);
    assert!(pi_rear.clean_win_probability+pi_rear.foul_probability<=1.0);
    println!("PI_TACKLE_BODY_FORECAST_MATCHED front_clean={} front_foul={} rear_clean={} rear_foul={}",
        pi_front.clean_win_probability,pi_front.foul_probability,pi_rear.clean_win_probability,pi_rear.foul_probability);
    let mut counts=[[0u32;4];2];
    for seed in 0..4096i64 {
        for (j,template) in [&front,&rear].into_iter().enumerate() {
            let mut state=template.clone();
            let mut rng=DeterministicRng::new(104729+seed*7907);
            let out=resolve_carry_contact(&mut state,&input,a,Some(b),ctx,false,&mut rng);
            let clean=out.events.iter().any(|e|e.kind==EventTypeV4::TackleWon ||
                (e.kind==EventTypeV4::LooseBall && e.metadata.cause.as_deref()==Some("TACKLE_CONTACT_LOOSE")));
            let foul=out.events.iter().any(|e|e.kind==EventTypeV4::Foul);
            let body=out.events.iter().any(|e|e.kind==EventTypeV4::Foul&&e.metadata.extra.get("contactBranch").map(String::as_str)==Some("MISSED_BALL_BODY"));
            assert!(!(clean&&foul));
            if body {
                assert!(foul);
                assert_eq!(out.next_side,TeamSideV4::Home);
                assert_eq!(state.ball.control,crate::v4::state::BallControlState::DeadBall);
            }
            counts[j][0]+=u32::from(clean);
            counts[j][1]+=u32::from(foul);
            counts[j][2]+=u32::from(body);
            counts[j][3]+=u32::from(!clean&&!foul);
        }
    }
    assert_eq!(counts[0][2],0);
    assert!(counts[1][2]>0&&counts[1][1]>counts[0][1]);
    assert_eq!(counts[1][0]+counts[1][1]+counts[1][3],4096);
    for (j,p) in [f,r].into_iter().enumerate(){
        assert!((counts[j][0] as f32/4096.0-p.0).abs()<0.04);
        assert!((counts[j][1] as f32/4096.0-p.1).abs()<0.04);
    }
    println!("PI_TACKLE_BODY_CONTACT front={f:?} rear={r:?} counts_clean_foul_body_miss={counts:?}");
}


#[test]
fn pi_defense_legal_tackle_contact_does_not_guarantee_control() {
    use crate::v4::ball_play::{duel_contact_probabilities,tackle_control_probability,resolve_carry_contact};
    use crate::v4::state::{MatchStateV4,BallControlState};
    use crate::v4::movement::evaluate_local_context;
    let input=make_v4_input(23101,SimulationMode::FullEvent,RulesModeV4::Standard);
    let mut state=MatchStateV4::from_input(&input);
    let a=state.players.iter().position(|p|p.side==TeamSideV4::Home&&!p.is_goalkeeper).unwrap();
    let b=state.players.iter().position(|p|p.side==TeamSideV4::Away&&!p.is_goalkeeper).unwrap();
    state.players[a].pos=Vec2::new(50.0,34.0);
    state.players[b].pos=Vec2::new(51.0,34.0);
    state.players[a].facing_rad=0.0;
    state.players[a].vel=Vec2::new(4.0,0.0);
    state.players[b].vel=Vec2::default();
    let ctx=evaluate_local_context(&state,&input,TeamSideV4::Home,state.players[a].pos);
    let legal=duel_contact_probabilities(&state,&input,a,b,ctx,false).0;
    let control=tackle_control_probability(&state,&input,a,b);
    let mut wins=0u32;let mut loose=0u32;
    for k in 0..4096i64 {
        let mut x=state.clone();
        let mut rng=DeterministicRng::new(104729+k*7907);
        let out=resolve_carry_contact(&mut x,&input,a,Some(b),ctx,false,&mut rng);
        if out.events.iter().any(|e|e.kind==EventTypeV4::TackleWon) {
            wins+=1;
            assert_eq!(x.ball.control,BallControlState::Controlled);
            assert_eq!(out.next_side,TeamSideV4::Away);
        }
        if out.events.iter().any(|e|e.kind==EventTypeV4::LooseBall&&e.metadata.cause.as_deref()==Some("TACKLE_CONTACT_LOOSE")) {
            loose+=1;
            assert_eq!(x.ball.control,BallControlState::Loose);
            assert!(x.ball.owner_side.is_none()&&x.ball.owner_player_id.is_none());
            assert_eq!(x.ball.last_touch_side,Some(TeamSideV4::Away));
            assert_eq!(x.ball.last_touch_player_id.as_deref(),Some(state.players[b].player_id.as_str()));
            assert!(x.ball.vel.x.abs()+x.ball.vel.y.abs()>0.0);
            assert!((x.ball.pos.x-50.45).abs()<1e-4);
            assert!((x.ball.pos.y-34.0).abs()<1e-4);
            assert_eq!(out.next_side,TeamSideV4::Home);
            assert!(!out.events.iter().any(|e|matches!(e.kind,EventTypeV4::TackleWon|EventTypeV4::DribbleLost|EventTypeV4::Foul)));
        }
    }
    assert!(wins>0&&loose>0);
    assert!((wins as f32/4096.0-legal*control).abs()<0.04);
    assert!((loose as f32/4096.0-legal*(1.0-control)).abs()<0.04);
    println!("PI_TACKLE_CONTROL legal={legal} conditional_control={control} controlled={wins} loose={loose} n=4096");
}


#[test]
fn pi_defense_tackle_loose_kinematics_uses_contact_point_and_moving_frame() {
    use crate::v4::interaction::{tackle_loose_contact_point,tackle_loose_velocity,blocked_ball_velocity};
    let carrier=Vec2::new(50.0,34.0);
    let defender=Vec2::new(51.0,34.0);
    let zero=Vec2::default();
    let front=tackle_loose_contact_point(carrier,0.0,defender,1.0);
    assert!((front.x-50.45).abs()<1e-4);
    let rear=tackle_loose_contact_point(carrier,std::f32::consts::PI,defender,1.0);
    assert!(rear.xy().distance(defender)<=1.0001);
    assert!((rear.x-50.0).abs()<1e-4);
    let incoming=Vec2::new(4.0,1.0);
    let stationary=tackle_loose_velocity(incoming,zero,defender,front);
    assert_eq!(stationary,blocked_ball_velocity(Vec3::new(4.0,1.0,0.0),defender,front));
    let moving=Vec2::new(-3.0,0.0);
    let displaced=tackle_loose_velocity(zero,moving,defender,front);
    assert!(displaced.x<0.0);
    assert_eq!(displaced.y,0.0);
    let relative_before=(incoming.x-moving.x).powi(2)+(incoming.y-moving.y).powi(2);
    let rebound=tackle_loose_velocity(incoming,moving,defender,front);
    let relative_after=(rebound.x-moving.x).powi(2)+(rebound.y-moving.y).powi(2);
    assert!(relative_after<=relative_before+1e-5);
    let mirror_carrier=Vec2::new(55.0,34.0);
    let mirror_defender=Vec2::new(54.0,34.0);
    let mirror=tackle_loose_contact_point(mirror_carrier,std::f32::consts::PI,mirror_defender,1.0);
    let mirror_v=tackle_loose_velocity(Vec2::new(-incoming.x,incoming.y),
        Vec2::new(-moving.x,moving.y),mirror_defender,mirror);
    assert!((mirror.x+front.x-105.0).abs()<1e-4);
    assert!((mirror_v.x+rebound.x).abs()<1e-4&&(mirror_v.y-rebound.y).abs()<1e-4);
    let comoving=tackle_loose_velocity(incoming,incoming,defender,front);
    assert_eq!(comoving,Vec3::new(incoming.x,incoming.y,0.0));
    println!("PI_TACKLE_KINEMATICS_FIXTURE front={front:?} rear={rear:?} stationary={stationary:?} moving={displaced:?}");
}
