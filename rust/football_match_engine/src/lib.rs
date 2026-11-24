pub mod ai_manager;
pub mod clock;
pub mod commands;
pub mod contextual;
pub mod decision;
pub mod defensive;
pub mod discipline;
pub mod duel_resolver;
pub mod events;
pub mod fatigue;
pub mod fingerprints;
pub mod football_resolver;
pub mod geometry;
pub mod input;
pub mod jni_bridge;
pub mod match_day;
pub mod movement;
pub mod pass_resolver;
pub mod passing_lane;
pub mod player_ratings;
pub mod player_intelligence;
pub mod possession_decision;
pub mod pressure;
pub mod resolver_support;
pub mod rng;
pub mod roster;
pub mod rules;
pub mod session;
pub mod set_piece;
pub mod shot_model;
pub mod shot_resolver;
pub mod snapshot;
pub mod state;
pub mod stats;
pub mod tactical_models;
pub mod tactics;
pub mod v4;
pub mod wire_json;

#[cfg(test)]
mod parity_phase2;
#[cfg(test)]
mod parity_full_match;

use std::ffi::c_void;

/// Preserved V3 JNI port version for backward compatibility (`>= 4` check in Kotlin).
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativePortVersion(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    4
}

/// Separate JNI bridge version so V4 capabilities do not collide with V3 `nativePortVersion() == 4`.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeBridgeVersion(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    v4::JNI_BRIDGE_VERSION
}

/// Highest supported Match Engine wire protocol version (`5` = V4.1 DES 54-attribute 90-minute protocol).
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeEngineProtocolVersion(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    v4::ENGINE_PROTOCOL_VERSION_V4 as i32
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_rngSample(
    _env: *mut c_void,
    _class: *mut c_void,
    seed: i64,
    bound: i32,
) -> i32 {
    if bound <= 0 {
        return -1;
    }
    rng::DeterministicRng::new(seed).next_int(bound)
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeCoreSelfCheck(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    use clock::{ClockState, Period};
    use decision::{Action, Candidate};
    use events::{EventType, MatchEvent};
    use fatigue::{FatigueInput, FatigueState, Task};
    use geometry::TeamSide;

    let mut flags = 0;
    let mut c = ClockState::default();
    c.start_period(Period::FirstHalf);
    if c.set_game_second(2698).is_ok() {
        c.add_stoppage_seconds(19);
        c.add_stoppage_seconds(37);
        if c.lock_added_time(360) == 60 {
            flags |= 1;
        }
    }
    let mut s = FatigueState {
        energy: 96,
        workload: 0,
        distance_tenths: 0,
        fatigue_accumulator: 0,
        acute_fatigue: 0,
    };
    fatigue::apply(
        &mut s,
        FatigueInput {
            duration_seconds: 12,
            distance_tenths: 80,
            tempo: 73,
            pressing_intensity: 88,
            transition_risk: 66,
            active_outfield: 9,
            stamina: 69,
            task: Task::Press,
            assigned_defensive_role: false,
        },
        true,
    );
    if s.workload == 83 && s.fatigue_accumulator == 5063 && s.energy == 96 {
        flags |= 2;
    }
    let mut choices = vec![
        Candidate::new(Action::Pass, 15.0, 6.0, 0.0),
        Candidate::new(Action::Shot, 13.0, 1.0, 0.0),
    ];
    let mut rng = rng::DeterministicRng::new(101);
    if let Ok(choice) = decision::select(&mut choices, 80, 70, 35, 80, 70, 70, &mut rng) {
        if choice.action == Action::Pass {
            flags |= 4;
        }
    }
    let ev = MatchEvent {
        sequence: 81,
        game_second: 3762,
        kind: EventType::Goal,
        side: Some(TeamSide::Home),
        actor: Some("H09".into()),
        target: None,
        ball_x: 50,
        ball_y: 88,
        metadata: vec![
            ("cause".into(), "ONE_TOUCH".into()),
            ("goalkeeper".into(), "A00".into()),
        ],
        facts_before: None,
        facts_after: None,
    };
    if ev.fingerprint() as i64 == -4026996421991938600i64 {
        flags |= 8;
    }
    let a = passing_lane::DefenderAttributes {
        pace: 79,
        defensive_positioning: 84,
        decisions: 70,
        work_rate: 77,
        energy: 90,
        defensive_slot_out_progress: 31,
    };
    let d = passing_lane::prepare_defender("D1", 40, 41, a, true, false, Some("TARGET".into()), false);
    if passing_lane::evaluate((20, 20), (60, 60), "TARGET", &[d]).lane_risk > 0 {
        flags |= 16;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::*;
    use crate::session::{UnifiedMatchEngine, ENGINE_VERSION};

    #[test]
    fn spatial_java_equivalence() {
        use crate::geometry::{TeamSide, *};
        assert_eq!(attacking_progress(TeamSide::Home, 97), 97);
        assert_eq!(attacking_progress(TeamSide::Away, 97), 3);
        assert!(in_attacking_penalty_area(TeamSide::Home, 50, 88));
        assert!(in_attacking_penalty_area(TeamSide::Away, 50, 12));
        assert_eq!(scale_width(0, 50), 0);
        assert_eq!(goal_kick_y(TeamSide::Away), 93);
    }

    #[test]
    fn rules_follow_original() {
        let s = rules::MatchRules::standard();
        assert!(!s.requires_extra_time(1, 1));
        let k = rules::MatchRules::knockout();
        assert!(k.requires_extra_time(1, 1));
        assert!(!k.requires_extra_time(2, 1));
    }

    #[test]
    fn rng_matches_java_v317_exactly() {
        let cases: &[(i64, i64, i32, bool, u64, i64)] = &[
            (1, -7995527694508729151, 259, false, 0x3fdc7061a43b90b2, -7569645462458333503),
            (42, -4767286540954276203, 145, false, 0x3fd607387fc392b8, -7039776493598168826),
            (123456789, 2466975172287755897, 383, true, 0x3fe0a3c0c2c2d4b7, -8702667843733205468),
            (-27, -5313833362726698233, 412, true, 0x3fd96261e6c8b20c, -5552465284146965337),
        ];
        for &(seed, long, int, boolean, double_bits, fork) in cases {
            let mut r = rng::DeterministicRng::new(seed);
            assert_eq!(r.next_long(), long);
            assert_eq!(r.next_int(1000), int);
            assert_eq!(r.next_boolean(), boolean);
            assert_eq!(r.next_double().to_bits(), double_bits);
            assert_eq!(rng::derive_seed(seed, "BÌNH LUẬN 🎙️"), fork);
        }
    }

    #[test]
    fn fingerprints_and_rules_match_original_java() {
        let mut hash = fingerprints::start();
        hash = fingerprints::mix_string(hash, Some("Xin chào 🌍"));
        hash = fingerprints::mix_i32(hash, 123);
        hash = fingerprints::mix_i64(hash, -18);
        assert_eq!(hash as i64, 2449983695915557691);
        assert_eq!(rules::MatchRules::standard().fingerprint() as i64, 4539635157041289322);
        assert_eq!(rules::MatchRules::knockout().fingerprint() as i64, -4748859097362689852);
        assert_eq!(geometry::scale_width(31, 33), 33);
    }

    #[test]
    fn java_contextual_formulas_parity() {
        use crate::contextual::*;
        let p = snapshot::PlayerAttributesSnapshot::new([
            21, 77, 91, 78, 84, 86, 88, 76, 93, 90, 71, 87, 82, 89, 83, 85, 73, 91, 64, 55,
        ])
        .unwrap();
        assert_eq!(p.fingerprint() as i64, -9200139346538813823);
        fn check(value: f64, expected_bits: u64) {
            let expected = f64::from_bits(expected_bits);
            assert!(
                (value - expected).abs() <= 1e-11,
                "Java mismatch: Rust {value:?}; Java {expected:?}"
            );
        }
        check(
            pass_execution_adjustment(
                &p,
                &PassContext {
                    carrier_pressure: 44,
                    target_pressure: 53,
                    distance: 24.5,
                    progress_gain: 15,
                    tactical_risk: 76,
                    lane_risk: 37,
                    cover_shadow: 48,
                },
            ),
            0x4018792bc7627cc3,
        );
        check(
            cross_execution_adjustment(
                &p,
                &CrossContext {
                    pressure: 42,
                    target_distance: 33.5,
                    aerial_difficulty: 75,
                },
            ),
            0x400fed0e56041894,
        );
        check(
            shot_execution_adjustment(
                &p,
                &ShotContext {
                    pressure: 62,
                    progress: 84,
                    centrality: 65,
                    nearby_defenders: 3,
                    header: false,
                    penalty: false,
                },
            ),
            0x4016a463796ac9e0,
        );
        check(
            shot_execution_adjustment(
                &p,
                &ShotContext {
                    pressure: 52,
                    progress: 93,
                    centrality: 72,
                    nearby_defenders: 2,
                    header: true,
                    penalty: false,
                },
            ),
            0x4007f4acaff6d331,
        );
        check(
            shot_execution_adjustment(
                &p,
                &ShotContext {
                    pressure: 4,
                    progress: 88,
                    centrality: 100,
                    nearby_defenders: 0,
                    header: false,
                    penalty: true,
                },
            ),
            0x40118a3d70a3d70a,
        );
        check(
            ground_duel_attacker(
                &p,
                &GroundDuelContext {
                    dribble: true,
                    pressure: 54,
                    progress: 63,
                    defensive_aggression: 65,
                    energy: 79,
                },
            ),
            0x4052a8f1a6b8253a,
        );
        check(
            ground_duel_defender(
                &p,
                &GroundDuelContext {
                    dribble: true,
                    pressure: 54,
                    progress: 63,
                    defensive_aggression: 65,
                    energy: 79,
                },
            ),
            0x4052a8fb770d2c17,
        );
        check(
            pressure_attribute_adjustment(
                &p,
                &PressureContext {
                    distance: 8.4,
                    press_radius: 15.7,
                    pressing_intensity: 82,
                    line_height: 65,
                    energy: 55,
                },
            ),
            0x400b262d21096662,
        );
    }

    #[test]
    fn attributes_strict_bounds() {
        assert!(snapshot::PlayerAttributesSnapshot::new([74; 20]).is_ok());
        assert!(snapshot::PlayerAttributesSnapshot::new([0; 20]).is_err());
    }

    fn make_test_team(prefix: &str, team_id: &str) -> TeamMatchSnapshot {
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
        let mut lineup = Vec::new();
        let mut assignments = Vec::new();
        let mut tactical_slots = Vec::new();
        let mut slot_instructions = Vec::new();
        for (i, &(sid, ox, oy, ix, iy, pos)) in slots.iter().enumerate() {
            let pid = format!("{}{:02}", prefix, i);
            let attrs = snapshot::PlayerAttributesSnapshot::new([75; 20]).unwrap();
            lineup.push(PlayerMatchSnapshot::new(&pid, &pid, attrs, 100, 65, 55, 25, false, true, pos, "").unwrap());
            assignments.push(LineupAssignmentSnapshot::new(&pid, sid).unwrap());
            tactical_slots.push(TacticalSlotSnapshot::new(sid, ox, oy, ix, iy).unwrap());
            slot_instructions.push(SlotInstructionsSnapshot::new(sid, 55, 55, 55, 55, 55, 50, 35, 60).unwrap());
        }
        let ti = TeamInstructionsSnapshot::new(50, 50, 50, 50, 10, false, 50, 50, 50, 50).unwrap();
        TeamMatchSnapshot::new(
            team_id,
            team_id,
            lineup,
            Vec::new(),
            assignments,
            format!("{}00", prefix),
            "433-v3",
            tactical_slots,
            ti,
            slot_instructions,
            SetPiecePlanSnapshot::empty(),
            TeamCollectiveSnapshot::neutral(),
        )
        .unwrap()
    }

    #[test]
    fn full_match_simulation_completes_deterministically() {
        let home = make_test_team("H", "HOME_FC");
        let away = make_test_team("A", "AWAY_FC");
        let input = MatchInput::new(
            "RUST-SMOKE-1",
            123456789,
            ENGINE_VERSION,
            home,
            away,
            rules::MatchRules::standard(),
            false,
            "ENG_1",
            LeagueObjectiveSnapshot::unavailable(),
            LeagueObjectiveSnapshot::unavailable(),
            20,
        )
        .unwrap();
        let r1 = UnifiedMatchEngine::simulate(input.clone()).unwrap();
        let r2 = UnifiedMatchEngine::simulate(input).unwrap();
        assert_eq!(r1.fingerprint, r2.fingerprint);
        assert_eq!(r1.event_timeline_fingerprint, r2.event_timeline_fingerprint);
        assert!(r1.event_count > 100);
        assert!(r1.final_state.clock.is_full_time());
    }
}
