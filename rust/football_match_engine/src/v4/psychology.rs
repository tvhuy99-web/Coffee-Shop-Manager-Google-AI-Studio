use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PitchQuality};
use crate::v4::movement::lookup_attrs;
use crate::v4::spatial::{TeamSideV4, Vec3};
use crate::v4::state::{InjuryKind, MatchStateV4};

/// Continuous-time exponential decay rate for team momentum (s^-1).
/// Calibrated so `(-MOMENTUM_DECAY_RATE_PER_SEC * 11.0).exp() ≈ 0.94`,
/// preserving V4.0 behavior while remaining invariant to DES step length.
pub const MOMENTUM_DECAY_RATE_PER_SEC: f32 = 0.005625;

/// Reference V4.0 beat duration (seconds) used to convert legacy per-beat
/// probabilities into continuous-time Poisson hazard rates.
pub const REFERENCE_BEAT_SEC: f32 = 11.0;

/// Computes deterministic pre-match form multiplier from `consistency`, `big_matches`,
/// `match_importance`, and the meta/career readiness hook (`professionalism` + `adaptability`).
pub fn compute_prematch_form_multiplier(
    attrs: &PlayerAttributes54,
    match_importance: u8,
    rng: &mut DeterministicRng,
) -> f32 {
    sample_prematch_form(attrs,match_importance,rng).1
}

/// One draw, two explicitly separated outputs:
/// (football match-day form, legacy career-influenced confidence factor).
/// Only the first is consumed by match decision uncertainty.
pub(crate) fn sample_prematch_form(
    attrs:&PlayerAttributes54,match_importance:u8,rng:&mut DeterministicRng
)->(f32,f32) {
    let cons = normalize_attr(attrs.hidden.consistency);
    let big = normalize_attr(attrs.hidden.big_matches);
    let readiness = attrs.career_readiness_hook();

    // Higher consistency shrinks random match-day variance
    let spread = (0.14 - cons * 0.11).clamp(0.025, 0.14);
    let variance = (rng.next_double() as f32 - 0.5) * 2.0 * spread;

    // Big-match temperament activates when match_importance >= 65
    let big_delta = if match_importance >= 65 {
        let weight = ((match_importance as f32 - 60.0) / 40.0).clamp(0.0, 1.0);
        (big - 0.5) * 0.16 * weight
    } else {
        0.0
    };

    let career_stability = (readiness - 0.5) * 0.04;
    let match_only=(1.0+variance+big_delta).clamp(0.84,1.16);
    let legacy=(1.0+variance+big_delta+career_stability).clamp(0.84,1.16);
    (match_only,legacy)
}

/// Updates dynamic in-match psychology (`concentration_level`, `pressure_load`, `composure_delta`)
/// and team `momentum` across all active players over an arbitrary DES step `dt_sec`.
pub fn update_in_match_psychology(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    game_second: i32,
    dt_sec: f32,
) {
    let importance = input.match_environment.match_importance as f32 / 100.0;
    let late_half = (2400..=2850).contains(&game_second) || game_second >= 4800;

    // Captain leadership aura per team
    let home_lead = team_leadership_aura(state, input, TeamSideV4::Home);
    let away_lead = team_leadership_aura(state, input, TeamSideV4::Away);

    // Continuous-time exponential decay of team momentum toward 0 (step-length invariant)
    let decay = (-MOMENTUM_DECAY_RATE_PER_SEC * dt_sec.max(0.0)).exp();
    state.home.momentum *= decay;
    state.away.momentum *= decay;

    for idx in 0..state.players.len() {
        if !state.players[idx].on_pitch || state.players[idx].sent_off {
            continue;
        }
        let side = state.players[idx].side;
        let opp = side.opposite();
        let goal_diff = state.team_state(side).goals as i32 - state.team_state(opp).goals as i32;
        let team_mom = state.team_state(side).momentum;
        let lead_aura = match side {
            TeamSideV4::Home => home_lead,
            TeamSideV4::Away => away_lead,
        };
        let attrs = *lookup_attrs(input, &state.players[idx]);

        let conc = normalize_attr(attrs.mental.concentration);
        let det = normalize_attr(attrs.mental.determination);
        let press_res = normalize_attr(attrs.hidden.pressure);

        // Concentration dips late in each half and under aerobic fatigue
        let fatigue_drop = ((100.0 - state.players[idx].fatigue.aerobic) / 100.0) * 0.18;
        let late_drop = if late_half { (1.0 - conc) * 0.14 } else { 0.0 };
        state.players[idx].psychology.concentration_level =
            (0.74 + conc * 0.26 - fatigue_drop - late_drop).clamp(0.35, 1.0);

        // Pressure load rises when trailing late or in high-importance fixtures
        let trailing_stress = if goal_diff < 0 {
            0.22 + if game_second >= 4200 { 0.24 } else { 0.0 }
        } else if goal_diff == 0 && game_second >= 4500 {
            0.14
        } else {
            0.0
        };
        let raw_load = (trailing_stress + importance * 0.20 - lead_aura * 0.12 - det * 0.10)
            * (1.15 - press_res * 0.55);
        state.players[idx].psychology.pressure_load = raw_load.clamp(0.0, 1.0);

        // Composure delta combines momentum + determination when trailing + pressure resistance
        let mom_effect = team_mom * 0.15;
        let det_rally = if goal_diff < 0 { (det - 0.5) * 0.10 } else { 0.0 };
        state.players[idx].psychology.composure_delta =
            (mom_effect + det_rally - state.players[idx].psychology.pressure_load * 0.18)
                .clamp(-0.28, 0.25);
    }
}

/// Applies a momentum impulse (e.g., +0.35 on Goal, -0.25 on Red Card).
pub fn shift_momentum(state: &mut MatchStateV4, side: TeamSideV4, delta: f32) {
    let t = state.team_state_mut(side);
    t.momentum = (t.momentum + delta).clamp(-1.0, 1.0);
    let opp = state.team_state_mut(side.opposite());
    opp.momentum = (opp.momentum - delta * 0.65).clamp(-1.0, 1.0);
}

/// Continuous-time pressing foul hazard probability for a DES step of length `dt_sec`.
/// Converts the legacy V4.0 per-beat probability `(0.046 + p_norm * 0.052 + ref_mod).clamp(0.018, 0.12)`
/// over `REFERENCE_BEAT_SEC` (11s) into an exact Poisson hazard rate `lambda` and evaluates
/// `1.0 - (-lambda * dt_sec).exp()`, ensuring invariance under arbitrary step subdivision.
pub fn pressing_foul_hazard_prob(p_norm: f32, ref_mod: f32, dt_sec: f32) -> f32 {
    let dt_pos = dt_sec.max(0.0);
    if dt_pos <= 0.0 {
        return 0.0;
    }
    let p_ref = (0.046 + p_norm.clamp(0.0, 1.0) * 0.052 + ref_mod).clamp(0.018, 0.12);
    let lambda_per_sec = -(1.0 - p_ref).ln() / REFERENCE_BEAT_SEC;
    (1.0 - (-lambda_per_sec * dt_pos).exp()).clamp(0.0, 0.25)
}

/// Checks for a non-contact muscle injury (`InjuryKind::MuscleNonContact`) during high-intensity
/// effort when aerobic/anaerobic reserves are depleted, governed by `injury_proneness`,
/// `natural_fitness`, and continuous-time hazard integration over `dt_sec`.
pub fn check_muscle_injury(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    dt_sec: f32,
    rng: &mut DeterministicRng,
) -> Option<EmittedPlayEvent> {
    let dt_pos = dt_sec.max(0.0);
    if dt_pos <= 0.0 {
        return None;
    }

    let pitch_pen = match input.match_environment.pitch_quality {
        PitchQuality::Poor => 1.35,
        PitchQuality::Heavy => 1.18,
        _ => 1.0,
    };

    for idx in 0..state.players.len() {
        if !state.players[idx].on_pitch
            || state.players[idx].sent_off
            || state.players[idx].injury.is_some()
            || state.players[idx].is_goalkeeper
        {
            continue;
        }
        let aero = state.players[idx].fatigue.aerobic;
        let ana = state.players[idx].fatigue.anaerobic;
        if aero > 46.0 && ana > 20.0 {
            continue;
        }
        let attrs = *lookup_attrs(input, &state.players[idx]);
        let prone = normalize_attr(attrs.hidden.injury_proneness);
        let nat = normalize_attr(attrs.physical.natural_fitness);
        let exhaustion = ((46.0 - aero).max(0.0) / 46.0) + ((20.0 - ana).max(0.0) / 20.0) * 0.6;
        let lambda_per_sec = (0.0018 / REFERENCE_BEAT_SEC)
            * (0.45 + prone * 1.65)
            * (1.25 - nat * 0.55)
            * (0.6 + exhaustion)
            * pitch_pen;
        let prob = (1.0 - (-lambda_per_sec * dt_pos).exp()).clamp(0.0, 0.045);

        if (rng.next_double() as f32) < prob {
            state.players[idx].injury = Some(InjuryKind::MuscleNonContact);
            state.players[idx].fatigue.aerobic = state.players[idx].fatigue.aerobic.min(28.0);
            let pid = state.players[idx].player_id.clone();
            let side = state.players[idx].side;
            let pos = Vec3::ground(state.players[idx].pos.x, state.players[idx].pos.y).clamp_to_pitch();
            let seq_id = state.next_highlight_sequence_id;
            state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);
            return Some(EmittedPlayEvent {
                kind: EventTypeV4::Injury,
                tier: HighlightTier::Extended,
                seq_id: Some(seq_id),
                side: Some(side),
                actor: Some(pid),
                target: None,
                ball_pos: pos,
                metadata: EventMetadataV4 {
                    injury_kind: Some(InjuryKind::MuscleNonContact),
                    cause: Some("MUSCLE_FATIGUE".into()),
                    ..EventMetadataV4::default()
                },
            });
        }
    }
    None
}

/// Checks for a contact/impact injury (`InjuryKind::Impact`) after a foul tackle,
/// governed by victim `injury_proneness`, tackler `aggression`, and `bravery`.
pub fn check_impact_injury(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    victim_idx: usize,
    tackler_idx: usize,
    rng: &mut DeterministicRng,
) -> Option<EmittedPlayEvent> {
    if state.players[victim_idx].injury.is_some() {
        return None;
    }
    let v_attrs = *lookup_attrs(input, &state.players[victim_idx]);
    let t_attrs = *lookup_attrs(input, &state.players[tackler_idx]);
    let prone = normalize_attr(v_attrs.hidden.injury_proneness);
    let agg = normalize_attr(t_attrs.mental.aggression);
    let brave = normalize_attr(v_attrs.mental.bravery);

    let prob = (0.012 + prone * 0.032 + agg * 0.018 + brave * 0.008).clamp(0.005, 0.08);
    if (rng.next_double() as f32) < prob {
        state.players[victim_idx].injury = Some(InjuryKind::Impact);
        state.players[victim_idx].fatigue.aerobic = state.players[victim_idx].fatigue.aerobic.min(32.0);
        let pid = state.players[victim_idx].player_id.clone();
        let side = state.players[victim_idx].side;
        let pos = Vec3::ground(state.players[victim_idx].pos.x, state.players[victim_idx].pos.y).clamp_to_pitch();
        let seq_id = state.next_highlight_sequence_id;
        state.next_highlight_sequence_id = state.next_highlight_sequence_id.saturating_add(1);
        return Some(EmittedPlayEvent {
            kind: EventTypeV4::Injury,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(side),
            actor: Some(pid),
            target: Some(state.players[tackler_idx].player_id.clone()),
            ball_pos: pos,
            metadata: EventMetadataV4 {
                injury_kind: Some(InjuryKind::Impact),
                cause: Some("TACKLE_IMPACT".into()),
                ..EventMetadataV4::default()
            },
        });
    }
    None
}

/// Evaluates card discipline for a foul (yellow card, second yellow -> red card, or direct red card)
/// based on `refereeProfile`, `temperament`, `sportsmanship`, and `aggression`.
pub fn resolve_foul_discipline(
    state: &mut MatchStateV4,
    input: &MatchInputV4,
    offender_idx: usize,
    seq_id: u32,
    pos: Vec3,
    rng: &mut DeterministicRng,
) -> Vec<EmittedPlayEvent> {
    let mut events = Vec::new();
    let side = state.players[offender_idx].side;
    let offender_id = state.players[offender_idx].player_id.clone();
    let attrs = *lookup_attrs(input, &state.players[offender_idx]);

    let ref_strict = input.referee_profile.strictness as f32 / 100.0;
    let card_thresh = input.referee_profile.card_threshold as f32 / 100.0;
    let low_temp = 1.0 - normalize_attr(attrs.hidden.temperament);
    let low_sport = 1.0 - normalize_attr(attrs.hidden.sportsmanship);
    let agg = normalize_attr(attrs.mental.aggression);

    // Direct red card chance on reckless fouls
    let direct_red_prob = (0.0008 + low_temp * 0.002 + agg * 0.0015 + (ref_strict - 0.5) * 0.001)
        .clamp(0.0005, 0.0045);
    if (rng.next_double() as f32) < direct_red_prob {
        state.players[offender_idx].sent_off = true;
        state.players[offender_idx].on_pitch = false;
        state.team_state_mut(side).red_cards += 1;
        shift_momentum(state, side, -0.28);
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::RedCard,
            tier: HighlightTier::GoalKey,
            seq_id: Some(seq_id),
            side: Some(side),
            actor: Some(offender_id),
            target: None,
            ball_pos: pos,
            metadata: EventMetadataV4 {
                ten_men: Some(true),
                cause: Some("STRAIGHT_RED".into()),
                ..EventMetadataV4::default()
            },
        });
        return events;
    }

    let booked_caution = if state.players[offender_idx].yellow_cards >= 1 { 0.14 } else { 1.0 };
    let yellow_prob = ((0.125
        + (ref_strict - card_thresh) * 0.10
        + low_temp * 0.06
        + low_sport * 0.04
        + agg * 0.03) * booked_caution)
        .clamp(0.012, 0.28);
    if (rng.next_double() as f32) < yellow_prob {
        state.players[offender_idx].yellow_cards += 1;
        state.team_state_mut(side).yellow_cards += 1;
        events.push(EmittedPlayEvent {
            kind: EventTypeV4::YellowCard,
            tier: HighlightTier::Extended,
            seq_id: Some(seq_id),
            side: Some(side),
            actor: Some(offender_id.clone()),
            target: None,
            ball_pos: pos,
            metadata: EventMetadataV4::default(),
        });

        if state.players[offender_idx].yellow_cards >= 2 {
            state.players[offender_idx].sent_off = true;
            state.players[offender_idx].on_pitch = false;
            state.team_state_mut(side).red_cards += 1;
            shift_momentum(state, side, -0.24);
            events.push(EmittedPlayEvent {
                kind: EventTypeV4::RedCard,
                tier: HighlightTier::GoalKey,
                seq_id: Some(seq_id),
                side: Some(side),
                actor: Some(offender_id),
                target: None,
                ball_pos: pos,
                metadata: EventMetadataV4 {
                    ten_men: Some(true),
                    cause: Some("SECOND_YELLOW".into()),
                    ..EventMetadataV4::default()
                },
            });
        }
    }

    events
}

/// Computes post-match career carryover delta (`morale_delta`) using `ambition` & `loyalty`
/// (completing the 4 meta/career attribute hooks alongside `professionalism` & `adaptability`).
pub fn compute_post_match_morale_delta(
    attrs: &PlayerAttributes54,
    won: bool,
    drew: bool,
    rating: f32,
) -> i8 {
    let amb = normalize_attr(attrs.hidden.ambition);
    let loy = normalize_attr(attrs.hidden.loyalty);
    let base: f32 = if won {
        4.0 + amb * 3.0
    } else if drew {
        (loy - amb) * 2.0
    } else {
        -4.0 - amb * 3.0 + loy * 1.5
    };
    let perf = (rating - 6.5) * 2.0;
    (base + perf).round().clamp(-12.0, 12.0) as i8
}

fn team_leadership_aura(state: &MatchStateV4, input: &MatchInputV4, side: TeamSideV4) -> f32 {
    let mut best = 0.0f32;
    for p in &state.players {
        if p.side == side && p.on_pitch && !p.sent_off {
            let lead = normalize_attr(lookup_attrs(input, p).mental.leadership);
            if lead > best {
                best = lead;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn momentum_decay_matches_legacy_beat_and_is_step_invariant() {
        let one_step = (-MOMENTUM_DECAY_RATE_PER_SEC * 11.0_f32).exp();
        assert!((one_step - 0.94).abs() < 1e-4);

        let split_two = (-MOMENTUM_DECAY_RATE_PER_SEC * 5.5_f32).exp().powi(2);
        let split_eleven = (-MOMENTUM_DECAY_RATE_PER_SEC * 1.0_f32).exp().powi(11);
        assert!((one_step - split_two).abs() < 1e-6);
        assert!((one_step - split_eleven).abs() < 1e-6);
    }

    #[test]
    fn pressing_foul_hazard_is_zero_at_zero_dt_and_step_invariant() {
        assert_eq!(pressing_foul_hazard_prob(0.5, 0.0, 0.0), 0.0);
        assert_eq!(pressing_foul_hazard_prob(0.5, 0.0, -2.0), 0.0);

        let p_full = pressing_foul_hazard_prob(0.6, 0.005, 11.0);
        let expected_ref = (0.046 + 0.6 * 0.052 + 0.005_f32).clamp(0.018, 0.12);
        assert!((p_full - expected_ref).abs() < 1e-5);

        let p1 = pressing_foul_hazard_prob(0.6, 0.005, 3.0);
        let p2 = pressing_foul_hazard_prob(0.6, 0.005, 8.0);
        let combined = 1.0 - (1.0 - p1) * (1.0 - p2);
        assert!((combined - p_full).abs() < 1e-5);
    }
}
