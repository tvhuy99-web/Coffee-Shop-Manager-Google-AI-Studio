use std::collections::HashMap;
use crate::{
    events::{EventType, MatchEvent},
    geometry::TeamSide,
    input::MatchInput,
};

#[derive(Clone, Debug)]
pub struct PlayerMatchRating {
    pub player_id: String,
    pub display_name: String,
    pub side: TeamSide,
    pub primary_position: String,
    pub goals: i32,
    pub assists: i32,
    pub shots: i32,
    pub shots_on_target: i32,
    pub passes_completed: i32,
    pub passes_attempted: i32,
    pub tackles_won: i32,
    pub interceptions: i32,
    pub saves: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub own_goals: i32,

    pub rating_tenths: i32,
}

#[derive(Clone, Debug)]
pub struct MatchRatingsSummary {
    pub ratings: Vec<PlayerMatchRating>,
    pub man_of_the_match: Option<PlayerMatchRating>,
}

pub fn compute_player_ratings(
    input: &MatchInput,
    events: &[MatchEvent],
    home_goals: i32,
    away_goals: i32,
) -> MatchRatingsSummary {
    let mut map: HashMap<String, PlayerMatchRating> = HashMap::new();
    for (side, team) in [(TeamSide::Home, &input.home_team), (TeamSide::Away, &input.away_team)] {
        for p in team.lineup.iter().chain(team.bench.iter()) {
            map.insert(
                p.player_id.clone(),
                PlayerMatchRating {
                    player_id: p.player_id.clone(),
                    display_name: p.display_name.clone(),
                    side,
                    primary_position: p.primary_position.clone(),
                    goals: 0,
                    assists: 0,
                    shots: 0,
                    shots_on_target: 0,
                    passes_completed: 0,
                    passes_attempted: 0,
                    tackles_won: 0,
                    interceptions: 0,
                    saves: 0,
                    yellow_cards: 0,
                    red_cards: 0,
                    own_goals: 0,
                    rating_tenths: 62,
                },
            );
        }
    }

    let mut appeared: HashMap<String, bool> = HashMap::new();
    for p in input.home_team.lineup.iter().chain(input.away_team.lineup.iter()) {
        appeared.insert(p.player_id.clone(), true);
    }

    let mut last_passer_home: Option<(String, i32)> = None;
    let mut last_passer_away: Option<(String, i32)> = None;

    for ev in events {
        if let Some(actor) = ev.actor.as_deref() {
            appeared.insert(actor.to_string(), true);
        }
        if let Some(target) = ev.target.as_deref() {
            if ev.kind == EventType::Substitution {
                appeared.insert(target.to_string(), true);
            }
        }
        let actor = match ev.actor.as_deref() {
            Some(a) => a,
            None => continue,
        };
        match ev.kind {
            EventType::PassCompleted | EventType::ThroughBall | EventType::Cross => {
                if let Some(r) = map.get_mut(actor) {
                    r.passes_completed += 1;
                    r.passes_attempted += 1;
                }
                match ev.side {
                    Some(TeamSide::Home) => last_passer_home = Some((actor.to_string(), ev.game_second)),
                    Some(TeamSide::Away) => last_passer_away = Some((actor.to_string(), ev.game_second)),
                    None => {}
                }
            }
            EventType::PassFailed => {
                if let Some(r) = map.get_mut(actor) {
                    r.passes_attempted += 1;
                }
            }
            EventType::Shot | EventType::Header | EventType::PenaltyKick => {
                if let Some(r) = map.get_mut(actor) {
                    r.shots += 1;
                }
            }
            EventType::ShotOnTarget => {
                if let Some(r) = map.get_mut(actor) {
                    r.shots_on_target += 1;
                }
            }
            EventType::Goal => {
                let is_og = ev.metadata.iter().any(|(k, v)| k == "ownGoal" && v == "true");
                let is_pen = ev.metadata.iter().any(|(k, v)| k == "source" && v == "PENALTY");
                if !is_og {
                    if let Some(r) = map.get_mut(actor) {
                        r.goals += 1;
                    }
                    if !is_pen {
                        let candidate = match ev.side {
                            Some(TeamSide::Home) => last_passer_home.as_ref(),
                            Some(TeamSide::Away) => last_passer_away.as_ref(),
                            None => None,
                        };
                        if let Some((passer_id, sec)) = candidate {
                            if passer_id != actor && ev.game_second - *sec <= 18 {
                                if let Some(pr) = map.get_mut(passer_id) {
                                    pr.assists += 1;
                                }
                            }
                        }
                    }
                }
            }
            EventType::OwnGoal => {
                if let Some(r) = map.get_mut(actor) {
                    r.own_goals += 1;
                }
            }
            EventType::TackleWon => {
                if let Some(r) = map.get_mut(actor) {
                    r.tackles_won += 1;
                }
            }
            EventType::Interception | EventType::Block => {
                if let Some(r) = map.get_mut(actor) {
                    r.interceptions += 1;
                }
            }
            EventType::Save | EventType::GoalkeeperClaim | EventType::GoalkeeperPunch => {
                if let Some(r) = map.get_mut(actor) {
                    r.saves += 1;
                }
            }
            EventType::YellowCard => {
                if let Some(r) = map.get_mut(actor) {
                    r.yellow_cards += 1;
                }
            }
            EventType::RedCard => {
                if let Some(r) = map.get_mut(actor) {
                    r.red_cards += 1;
                }
            }
            _ => {}
        }
    }

    let mut ratings: Vec<PlayerMatchRating> = map
        .into_values()
        .filter(|r| appeared.get(&r.player_id).copied().unwrap_or(false))
        .map(|mut r| {
            let mut score = 6.2_f64;
            score += r.goals as f64 * 1.25;
            score += r.assists as f64 * 0.75;
            score += r.shots_on_target as f64 * 0.12;
            score += r.tackles_won as f64 * 0.18;
            score += r.interceptions as f64 * 0.14;
            score += r.saves as f64 * 0.28;
            if r.passes_attempted >= 5 {
                let acc = r.passes_completed as f64 / r.passes_attempted as f64;
                score += (acc - 0.72) * 1.1;
            }
            score -= r.yellow_cards as f64 * 0.35;
            score -= r.red_cards as f64 * 1.20;
            score -= r.own_goals as f64 * 0.90;

            let (own_g, opp_g) = match r.side {
                TeamSide::Home => (home_goals, away_goals),
                TeamSide::Away => (away_goals, home_goals),
            };
            if own_g > opp_g {
                score += 0.25;
            } else if own_g < opp_g {
                score -= 0.15;
            }
            if opp_g == 0 && (r.primary_position == "GK" || r.primary_position == "CB" || r.primary_position == "LB" || r.primary_position == "RB") {
                score += 0.35;
            }
            r.rating_tenths = (score * 10.0).round().clamp(45.0, 100.0) as i32;
            r
        })
        .collect();

    ratings.sort_by(|a, b| {
        b.rating_tenths
            .cmp(&a.rating_tenths)
            .then_with(|| b.goals.cmp(&a.goals))
            .then_with(|| b.assists.cmp(&a.assists))
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
    let man_of_the_match = ratings.first().cloned();
    MatchRatingsSummary {
        ratings,
        man_of_the_match,
    }
}
