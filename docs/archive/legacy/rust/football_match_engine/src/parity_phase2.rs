use crate::{
    clock::{ClockState, Period},
    decision::{self, Action, Candidate},
    events::{EventType, MatchEvent},
    fatigue::{self, FatigueInput, FatigueState, Task},
    geometry::TeamSide,
    passing_lane::{self, DefenderAttributes},
    rng::DeterministicRng,
};

const JAVA: &str = include_str!("../../../tests/parity/java_phase2_expected.csv");

fn value(key: &str) -> &'static str {
    JAVA.lines()
        .find_map(|line| line.strip_prefix(&format!("{},", key)))
        .unwrap_or_else(|| panic!("missing Java fixture {}", key))
}

#[test]
fn clock_matches_java() {
    let mut c = ClockState::default();
    c.start_period(Period::FirstHalf);
    c.set_game_second(2698).unwrap();
    c.add_stoppage_seconds(19);
    c.add_stoppage_seconds(37);
    assert_eq!(c.lock_added_time(360), value("clockLocked").parse::<i32>().unwrap());
    c.add_stoppage_seconds(12);
    assert_eq!(c.effective_added_time(), value("clockEffective").parse::<i32>().unwrap());
}

#[test]
fn decision_matches_java() {
    let mut options = vec![
        Candidate::new(Action::Pass, 15.0, 6.0, 0.0),
        Candidate::new(Action::Shot, 13.0, 1.0, 0.0),
    ];
    let mut rng = DeterministicRng::new(101);
    let d = decision::select(&mut options, 80, 70, 35, 80, 70, 70, &mut rng).unwrap();
    assert_eq!(d.action.name(), value("decisionType"));
    assert_eq!(d.utility_summary, value("decisionSummary"));
    assert!((d.perceived_utility - 23.328106085026985).abs() < 1e-8);
    assert!((d.evaluation_error - 3.219349166610134).abs() < 1e-8);
}

#[test]
fn event_fingerprint_matches_java() {
    let m = MatchEvent {
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
    let wanted = value("eventGoal").parse::<i64>().unwrap();
    assert_eq!(m.fingerprint() as i64, wanted);
}

#[test]
fn fatigue_matches_java() {
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
    assert_eq!(
        format!(
            "{},{},{},{},{}",
            s.energy, s.fatigue_accumulator, s.acute_fatigue, s.workload, s.distance_tenths
        ),
        value("fatigue")
    );
}

#[test]
fn passing_lane_matches_java() {
    let mut defenders = Vec::new();
    let mut passer = (0, 0);
    let mut target = (0, 0);
    for line in JAVA.lines() {
        if let Some(csv) = line.strip_prefix("laneDef,") {
            let fields: Vec<&str> = csv.split(',').collect();
            let int = |i: usize| fields[i].parse::<i32>().unwrap();
            let attrs = DefenderAttributes {
                pace: int(3),
                defensive_positioning: int(4),
                decisions: int(5),
                work_rate: int(6),
                energy: int(7),
                defensive_slot_out_progress: int(8),
            };
            defenders.push(passing_lane::prepare_defender(
                fields[0],
                int(1),
                int(2),
                attrs,
                fields[9] == "true",
                fields[10] == "true",
                if fields[11] == "null" {
                    None
                } else {
                    Some(fields[11].into())
                },
                fields[12] == "true",
            ));
        }
        if let Some(csv) = line.strip_prefix("lanePoints,") {
            let vals: Vec<i32> = csv.split(',').map(|x| x.parse().unwrap()).collect();
            passer = (vals[0], vals[1]);
            target = (vals[2], vals[3]);
        }
    }
    assert_eq!(defenders.len(), 11);
    let e = passing_lane::evaluate(passer, target, "H09", &defenders);
    let actual = format!(
        "{},{},{},{},{},{},{}",
        e.lane_risk,
        e.cover_shadow,
        e.interceptor_id.unwrap_or_default(),
        e.interception_chance,
        e.intercept_x,
        e.intercept_y,
        e.blocker_count
    );
    assert_eq!(actual, value("lane"));
}
