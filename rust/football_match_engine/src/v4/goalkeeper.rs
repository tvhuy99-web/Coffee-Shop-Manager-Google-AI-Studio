use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, normalize_attr_elite, PlayerAttributes54};
use crate::v4::input::{MatchInputV4, WeatherCondition};
use crate::v4::spatial::{TeamSideV4, Vec3};
use crate::v4::state::MatchStateV4;

/// Outcome of a goalkeeper reacting to an on-target shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GkShotOutcome {
    GoalConceded,
    SavedHeld,
    SavedParriedRebound,
    SavedTippedCorner,
}

/// Outcome of a goalkeeper dealing with a high cross, corner, or lofted free kick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GkAerialOutcome {
    Claimed,
    PunchedClear,
    StayedOnLine,
}

/// Specialized 9-attribute Goalkeeper resolver:
/// - `reflexes`: close-range / high-velocity shot-stopping
/// - `handling`: clean catch vs parry/rebound (heavily impacted by wet weather)
/// - `one_on_ones`: composure & angle narrowing in 1v1 duels
/// - `rushing_out`: proactive sweep / angle closure
/// - `command_of_area`: authority on crosses & corners
/// - `aerial_reach`: high-ball interception (`z > 1.8m`)
/// - `punching`: clearing crowded box deliveries safely
/// - `kicking` & `throwing`: post-save distribution quality
pub fn resolve_gk_shot_reaction(
    gk_attrs: &PlayerAttributes54,
    gk_familiarity: f32,
    gk_aerobic: f32,
    shot_power_xg: f32,
    distance_m: f32,
    is_one_vs_one: bool,
    is_finesse: bool,
    is_header: bool,
    input: &MatchInputV4,
    rng: &mut DeterministicRng,
) -> GkShotOutcome {
    let g = &gk_attrs.goalkeeping;
    let reflexes = normalize_attr_elite(g.reflexes);
    let handling = normalize_attr(g.handling);
    let one_on_ones = normalize_attr_elite(g.one_on_ones);
    let rushing = normalize_attr(g.rushing_out);
    let aerial = normalize_attr(g.aerial_reach);
    let positioning = normalize_attr(gk_attrs.mental.positioning);
    let concentration = normalize_attr(gk_attrs.mental.concentration);

    let fatigue_mod = (0.78 + 0.22 * (gk_aerobic / 100.0)).clamp(0.72, 1.0);
    let wet_slip = match input.match_environment.weather {
        WeatherCondition::HeavyRain => 0.14,
        WeatherCondition::LightRain => 0.06,
        _ => 0.0,
    };

    // Shot-stopping rating tailored to situation
    let stop_skill = if is_one_vs_one {
        one_on_ones * 0.42 + reflexes * 0.28 + rushing * 0.18 + positioning * 0.12
    } else if distance_m <= 12.0 {
        reflexes * 0.52 + positioning * 0.20 + handling * 0.14 + concentration * 0.14
    } else if is_header {
        reflexes * 0.36 + aerial * 0.28 + positioning * 0.20 + handling * 0.16
    } else {
        // Long-range / curled shot
        let curve_factor = if is_finesse { 0.30 } else { 0.20 };
        positioning * 0.32 + aerial * curve_factor + reflexes * 0.28 + handling * (0.40 - curve_factor)
    };

    let effective_stop = (stop_skill * gk_familiarity * fatigue_mod).clamp(0.15, 1.0);
    // Goal probability conditioned on the shot being on target
    let concede_prob = (shot_power_xg * (1.38 - effective_stop * 0.72)).clamp(0.06, 0.82);

    if (rng.next_double() as f32) < concede_prob {
        return GkShotOutcome::GoalConceded;
    }

    // Shot was SAVED — now determine clean hold vs parry rebound vs corner tip
    // `handling` governs clean catch; wet weather & close hard shots increase rebound risk
    let close_difficulty = if distance_m <= 11.0 { 0.16 } else { 0.05 };
    let hold_prob = (0.28 + handling * 0.56 - wet_slip - close_difficulty - shot_power_xg * 0.22)
        .clamp(0.10, 0.88);
    let roll = rng.next_double() as f32;

    if roll < hold_prob {
        GkShotOutcome::SavedHeld
    } else if roll < hold_prob + (1.0 - hold_prob) * 0.58 {
        GkShotOutcome::SavedParriedRebound
    } else {
        GkShotOutcome::SavedTippedCorner
    }
}

/// Evaluates whether the goalkeeper claims, punches, or stays on the line against
/// an aerial delivery (cross, corner, lofted free kick) into the penalty area.
pub fn resolve_gk_aerial_claim(
    gk_attrs: &PlayerAttributes54,
    gk_height_cm: u16,
    crowded_box: bool,
    ball_z: f32,
    input: &MatchInputV4,
    rng: &mut DeterministicRng,
) -> GkAerialOutcome {
    let g = &gk_attrs.goalkeeping;
    let command = normalize_attr(g.command_of_area);
    let aerial = normalize_attr(g.aerial_reach);
    let handling = normalize_attr(g.handling);
    let punching = normalize_attr(g.punching);
    let bravery = normalize_attr(gk_attrs.mental.bravery);

    let height_bonus = ((gk_height_cm as f32 - 185.0) * 0.008).clamp(-0.08, 0.12);
    let wet_pen = match input.match_environment.weather {
        WeatherCondition::HeavyRain => 0.12,
        WeatherCondition::LightRain => 0.05,
        _ => 0.0,
    };
    let z_bonus = if ball_z >= 2.0 { 0.06 } else { 0.0 };

    let come_out_prob = (0.20 + command * 0.36 + aerial * 0.20 + bravery * 0.12 + height_bonus + z_bonus)
        .clamp(0.10, 0.82);
    if (rng.next_double() as f32) > come_out_prob {
        return GkAerialOutcome::StayedOnLine;
    }

    // GK comes off line: choose between clean claim (`handling`) and punch (`punching`)
    let crowd_pen = if crowded_box { 0.22 } else { 0.04 };
    let claim_prob = (0.58 + handling * 0.30 - punching * 0.12 - crowd_pen - wet_pen).clamp(0.15, 0.85);
    if (rng.next_double() as f32) < claim_prob {
        GkAerialOutcome::Claimed
    } else {
        GkAerialOutcome::PunchedClear
    }
}

/// Selects a teammate for GK distribution (`throwing` short/quick vs `kicking` long),
/// returning `(target_idx, is_long_kick, target_ball_pos)`.
pub fn choose_gk_distribution(
    state: &MatchStateV4,
    gk_attrs: &PlayerAttributes54,
    gk_side: TeamSideV4,
    rng: &mut DeterministicRng,
) -> (usize, bool, Vec3) {
    let kick = normalize_attr(gk_attrs.goalkeeping.kicking);
    let throw = normalize_attr(gk_attrs.goalkeeping.throwing);
    let prefer_long = (0.35 + (kick - throw) * 0.45).clamp(0.15, 0.80);
    let is_long = (rng.next_double() as f32) < prefer_long;

    let mut best_idx = None;
    let mut best_score = f32::NEG_INFINITY;
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != gk_side || p.is_goalkeeper {
            continue;
        }
        let dist = p.pos.distance(state.ball.pos.xy());
        let score = if is_long {
            dist
        } else {
            -dist
        };
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    let target_idx = best_idx.unwrap_or(0);
    let t_pos = state.players[target_idx].pos;
    let z = if is_long { 4.2 } else { 0.8 };
    (target_idx, is_long, Vec3::new(t_pos.x, t_pos.y, z).clamp_to_pitch())
}
