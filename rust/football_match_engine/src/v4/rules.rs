//! Engine compatibility paths; actual rules and timing live in match_rules.
pub use match_rules::*;
#[cfg(test)]
use crate::{clock::Period,rng::DeterministicRng};
#[cfg(test)]
use crate::v4::{input::{PitchQuality,WeatherCondition},perception::PassKindV4,spatial::{TeamSideV4,Vec3}};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_presets_and_compatibility_check() {
        let league = RulesetConfig::ifab_league_standard();
        assert_eq!(league.identity.ruleset_id, "ifab_league_standard");
        assert_eq!(league.identity.ruleset_version, 1);
        assert_eq!(league.half_duration_ms, 2_700_000);
        assert_eq!(league.max_substitutions, 5);
        assert_eq!(league.max_in_play_sub_windows, 3);
        assert!(league.interval_subs_exempt_from_windows);
        assert!(league.allow_quick_free_kick);
        assert!(league.allow_penalty_rebound);
        assert_eq!(league.added_time_rounding_ms, 60_000);
        assert_eq!(league.technical_watchdog_max_added_ms, 1_800_000);
        assert_eq!(league.max_events_per_same_ms, 64);
        assert!(!league.extra_time_enabled);
        assert!(!league.penalty_shootout_enabled);

        let cup = RulesetConfig::ifab_knockout_cup();
        assert_eq!(cup.identity.ruleset_id, "ifab_knockout_cup");
        assert!(cup.extra_time_enabled);
        assert!(cup.penalty_shootout_enabled);
        assert_eq!(cup.max_substitutions_for_period(Period::ExtraTimeFirst), 6);
        assert_eq!(cup.max_in_play_sub_windows_for_period(Period::ExtraTimeFirst), 4);

        assert!(check_ruleset_compatibility(&league.identity, &league.identity).is_ok());
        assert!(check_ruleset_compatibility(&league.identity, &cup.identity).is_err());
    }

    #[test]
    fn triangular_sampling_stays_bounded_and_non_constant() {
        let range = BoundedDurationRangeMs::new(4_000, 8_500, 18_000);
        let mut rng = DeterministicRng::new(42);
        let mut min_seen = i64::MAX;
        let mut max_seen = i64::MIN;
        for _ in 0..200 {
            let v = range.sample_triangular(&mut rng, 1.0);
            assert!((4_000..=18_000).contains(&v));
            min_seen = min_seen.min(v);
            max_seen = max_seen.max(v);
        }
        assert!(max_seen > min_seen + 2_000);
    }

    #[test]
    fn goal_celebration_follows_full_lifecycle_without_shortcut() {
        let mut state = DeadBallState::new(
            DeadBallKind::GoalCelebration,
            DeadBallPhase::Celebrating,
            TeamSideV4::Away,
            Vec3::center_spot(),
            1_000_000,
            24_000,
            24_000,
        );

        let p1 = next_dead_ball_phase(&state, true, false, false);
        assert_eq!(p1, Some(DeadBallPhase::ReturningToHalf));
        state.advance_to(DeadBallPhase::ReturningToHalf, 1_024_000, 12_000, 12_000);

        let p2 = next_dead_ball_phase(&state, true, false, false);
        assert_eq!(p2, Some(DeadBallPhase::ProcessingSubstitutions));
        state.advance_to(DeadBallPhase::ProcessingSubstitutions, 1_036_000, 20_000, 20_000);

        let p3 = next_dead_ball_phase(&state, true, false, false);
        assert_eq!(p3, Some(DeadBallPhase::KickoffSetup));
        state.advance_to(DeadBallPhase::KickoffSetup, 1_056_000, 5_000, 0);

        let p4 = next_dead_ball_phase(&state, false, false, false);
        assert_eq!(p4, Some(DeadBallPhase::ExecutingRestart));
        state.advance_to(DeadBallPhase::ExecutingRestart, 1_061_000, 1_000, 0);

        assert_eq!(next_dead_ball_phase(&state, false, false, false), None);
        assert_eq!(state.accumulated_compensable_ms, 56_000);
    }

    #[test]
    fn dead_ball_phase_estimator_separates_routine_and_compensable_time() {
        let timing = TimingModelConfig::default();
        let mut rng = DeterministicRng::new(99);

        let (cel_dur, cel_comp) = estimate_dead_ball_phase_ms(
            &timing,
            DeadBallKind::GoalCelebration,
            DeadBallPhase::Celebrating,
            0,
            2_000_000,
            20,
            50,
            0,
            &mut rng,
        );
        assert_eq!(cel_dur, cel_comp);
        assert!(cel_dur >= timing.goal_celebration_ms.min_ms);

        let (throw_dur, throw_comp) = estimate_dead_ball_phase_ms(
            &timing,
            DeadBallKind::ThrowIn,
            DeadBallPhase::SettingBall,
            0,
            1_200_000,
            20,
            50,
            0,
            &mut rng,
        );
        assert!(throw_dur >= timing.throw_in_prep_ms.min_ms);
        assert_eq!(throw_comp, 0);
    }

    #[test]
    fn ball_flight_and_reaction_estimators_respond_to_context() {
        let timing = TimingModelConfig::default();
        let mut rng_a = DeterministicRng::new(777);
        let mut rng_b = DeterministicRng::new(777);

        let short_ms = estimate_ball_flight_ms(
            &timing,
            10.0,
            BallFlightKind::from(PassKindV4::Short),
            WeatherCondition::Clear,
            PitchQuality::Good,
            &mut rng_a,
        );
        let long_ms = estimate_ball_flight_ms(
            &timing,
            45.0,
            BallFlightKind::from(PassKindV4::Switch),
            WeatherCondition::Windy,
            PitchQuality::Heavy,
            &mut rng_b,
        );
        assert!(long_ms > short_ms);

        let mut rng_c = DeterministicRng::new(888);
        let mut rng_d = DeterministicRng::new(888);
        let one_touch_ms = estimate_reaction_decision_ms(
            &timing, 18, 18, 18, 18, 20.0, 75, true, &mut rng_c,
        );
        let slow_ms = estimate_reaction_decision_ms(
            &timing, 7, 7, 7, 7, 85.0, 25, false, &mut rng_d,
        );
        assert!(slow_ms > one_touch_ms);
    }
}
