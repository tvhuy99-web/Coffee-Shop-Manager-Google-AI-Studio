use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::ball_play::EmittedPlayEvent;
use crate::v4::events::{EventMetadataV4, EventTypeV4, HighlightTier};
use crate::v4::input::{MatchInputV4, PitchQuality, PlayerMatchSnapshotV4};
use crate::v4::movement::lookup_attrs;
use crate::v4::spatial::{TeamSideV4, Vec3};
use crate::v4::state::{InjuryKind, MatchStateV4, PlayerPsychologyState};

/// Continuous-time exponential decay rate for team momentum (s^-1).
/// Calibrated so `(-MOMENTUM_DECAY_RATE_PER_SEC * 11.0).exp() ≈ 0.94`,
/// preserving V4.0 behavior while remaining invariant to DES step length.
pub const MOMENTUM_DECAY_RATE_PER_SEC: f32 = 0.005625;

/// Initial modeling prior: personal feedback relaxes on the existing
/// momentum time scale, but is an independent actor-specific state.
pub const PERSONAL_CONFIDENCE_DECAY_RATE_PER_SEC:f32=MOMENTUM_DECAY_RATE_PER_SEC;
pub const PERSONAL_CONFIDENCE_BOUND:f32=0.12;

/// Reference V4.0 beat duration (seconds) used to convert legacy per-beat
/// probabilities into continuous-time Poisson hazard rates.
pub const REFERENCE_BEAT_SEC: f32 = 11.0;

pub use player_condition::{compute_prematch_form_multiplier, sample_prematch_form};

/// A reserve has a match-day form just like a starter. Sample it from
/// a named independent stream so the value is fixed for this match and
/// player, regardless of the entry minute, substitution order or match RNG
/// consumption. Keep the original starting-XI stream unchanged.
pub(crate) fn substitute_psychology(
    input:&MatchInputV4,side:TeamSideV4,player:&PlayerMatchSnapshotV4
)->PlayerPsychologyState {
    let mut psychology=PlayerPsychologyState::from_snapshot(player);
    let channel=format!("pi4-matchday-reserve:{side:?}:{}",player.player_id);
    let mut form_rng=DeterministicRng::new(input.seed).fork(&channel);
    let (form,legacy_confidence_factor)=sample_prematch_form(
        &player.attributes,input.match_environment.match_importance,&mut form_rng);
    psychology.match_form_multiplier=form;
    // Same initialization as for starters. Career-linked confidence is
    // still not consumed by the decision uncertainty policy.
    psychology.confidence=(psychology.confidence*legacy_confidence_factor).clamp(0.15,1.0);
    psychology
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
    let personal_decay=(-PERSONAL_CONFIDENCE_DECAY_RATE_PER_SEC*dt_sec.max(0.0)).exp();

    for idx in 0..state.players.len() {
        if !state.players[idx].on_pitch || state.players[idx].sent_off {
            continue;
        }
        state.players[idx].psychology.personal_confidence_delta*=personal_decay;
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

/// Apply feedback once at the resolved outcome emission boundary.
/// Only the event actor is affected; related events for a tackle name
/// different actors. No inference of blame from a failed pass, rebound,
/// foul, missed chance, or an unresolved intention. No RNG or score edits.
pub(crate) fn observe_personal_outcome(
    state:&mut MatchStateV4,kind:EventTypeV4,side:Option<TeamSideV4>,
    actor:Option<&str>,metadata:&EventMetadataV4,
) {
    let (direction,weight)=match kind {
        EventTypeV4::Goal|EventTypeV4::Save=>(1.0,0.50),
        EventTypeV4::TackleWon|EventTypeV4::DribbleWon=>(1.0,0.20),
        EventTypeV4::DribbleLost if metadata.outcome.as_deref()==Some("TACKLED")=>(-1.0,0.20),
        _=>return,
    };
    let (Some(side),Some(id))=(side,actor) else{return;};
    if let Some(p)=state.players.iter_mut().find(|p|p.side==side && p.player_id==id && p.on_pitch && !p.sent_off) {
        let prior=p.psychology.personal_confidence_delta;
        let target=direction*PERSONAL_CONFIDENCE_BOUND;
        // Convex update: diminishing response near the bounded extremes.
        p.psychology.personal_confidence_delta=(prior+weight*(target-prior))
            .clamp(-PERSONAL_CONFIDENCE_BOUND,PERSONAL_CONFIDENCE_BOUND);
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

pub use player_condition::compute_post_match_morale_delta;

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
