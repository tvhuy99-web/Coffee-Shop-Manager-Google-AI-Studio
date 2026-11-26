use crate::{
    geometry::{self, TeamSide},
    input::MatchInput,
    state::MatchState,
};

#[derive(Clone, Debug)]
pub struct Defender {
    pub player_id: String,
    pub x: i32,
    pub y: i32,
    pub reach: f64,
    pub quality: f64,
    pub cover: bool,
    pub presses_passer: bool,
    pub marking_player_id: Option<String>,
    pub goalkeeper: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DefenderAttributes {
    pub pace: i32,
    pub defensive_positioning: i32,
    pub decisions: i32,
    pub work_rate: i32,
    pub energy: i32,
    pub defensive_slot_out_progress: i32,
}

pub fn prepare_defender(
    player_id: &str,
    x: i32,
    y: i32,
    a: DefenderAttributes,
    cover: bool,
    presses_passer: bool,
    marking_player_id: Option<String>,
    goalkeeper: bool,
) -> Defender {
    let energy_factor = 0.82 + a.energy as f64 * 0.0018;
    let striker_reach = if a.defensive_slot_out_progress >= 72 { 0.62 } else { 1.0 };
    let striker_quality = if a.defensive_slot_out_progress >= 72 { 0.58 } else { 1.0 };
    let reach = (2.66
        + a.pace as f64 * 0.012
        + a.defensive_positioning as f64 * 0.022
        + a.decisions as f64 * 0.012)
        * energy_factor
        * striker_reach;
    let quality = ((a.defensive_positioning as f64 * 0.42
        + a.decisions as f64 * 0.31
        + a.pace as f64 * 0.06
        + a.work_rate as f64 * 0.11
        + a.energy as f64 * 0.06
        + 3.0)
        / 100.0)
        * striker_quality;
    Defender {
        player_id: player_id.into(),
        x,
        y,
        reach,
        quality,
        cover,
        presses_passer,
        marking_player_id,
        goalkeeper,
    }
}

#[derive(Clone, Debug)]
pub struct Evaluation {
    pub lane_risk: i32,
    pub cover_shadow: i32,
    pub interceptor_id: Option<String>,
    pub interception_chance: i32,
    pub interceptor_distance_to_lane: f64,
    pub interceptor_projection: f64,
    pub intercept_x: i32,
    pub intercept_y: i32,
    pub blocker_count: i32,
}

impl Evaluation {
    pub fn clear() -> Self {
        Self {
            lane_risk: 0,
            cover_shadow: 0,
            interceptor_id: None,
            interception_chance: 0,
            interceptor_distance_to_lane: 99.0,
            interceptor_projection: 0.0,
            intercept_x: 0,
            intercept_y: 0,
            blocker_count: 0,
        }
    }

    pub fn has_geometric_interceptor(&self) -> bool {
        self.interceptor_id.is_some() && self.lane_risk > 0
    }
}

#[derive(Clone, Debug)]
struct Blocker {
    id: String,
    score: f64,
    shadow: f64,
    perpendicular: f64,
    projection: f64,
    intercept_x: i32,
    intercept_y: i32,
}

fn better(a: &Blocker, b: &Blocker) -> bool {
    if a.score != b.score {
        return a.score > b.score;
    }
    if a.perpendicular != b.perpendicular {
        return a.perpendicular < b.perpendicular;
    }
    a.id < b.id
}

#[inline]
fn jround(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

pub fn evaluate(passer: (i32, i32), target: (i32, i32), target_id: &str, defenders: &[Defender]) -> Evaluation {
    let (ax, ay) = (passer.0 as f64, passer.1 as f64);
    let (bx, by) = (target.0 as f64, target.1 as f64);
    let vx = bx - ax;
    let vy = by - ay;
    let length_sq = vx * vx + vy * vy;
    if length_sq < 1.0 {
        return Evaluation::clear();
    }
    let pass_length = length_sq.sqrt();
    let mut leaders: Vec<Blocker> = Vec::with_capacity(3);
    let mut blocker_count = 0;
    for d in defenders {
        let wx = d.x as f64 - ax;
        let wy = d.y as f64 - ay;
        let projection = (wx * vx + wy * vy) / length_sq;
        if projection <= 0.035 || projection >= 0.985 {
            continue;
        }
        let closest_x = ax + projection * vx;
        let closest_y = ay + projection * vy;
        let dx = d.x as f64 - closest_x;
        let dy = d.y as f64 - closest_y;
        let perpendicular_squared = dx * dx + dy * dy;
        let mut reach = d.reach;
        if d.goalkeeper && projection < 0.72 {
            reach *= 0.42;
        }
        let outer = reach * 1.45;
        if perpendicular_squared > outer * outer {
            continue;
        }
        let perp = perpendicular_squared.sqrt();
        let geometry = (1.0 - perp / reach.max(0.1)).max(0.0);
        let centrality = (1.0 - (projection - 0.50).abs() * 1.05).clamp(0.35, 1.0);
        let marks_target = d.marking_player_id.as_deref() == Some(target_id);
        let role_bonus = (if marks_target { 10.0 } else { 0.0 })
            + (if d.cover { 9.0 } else { 0.0 })
            + (if d.presses_passer && projection < 0.38 { 6.0 } else { 0.0 });
        let mut score = geometry * centrality * (48.0 + d.quality * 30.0) + role_bonus;
        if perp > reach {
            score *= 0.48;
        }
        score = score.clamp(0.0, 96.0);
        let mut shadow = 0.0;
        if (0.10..=0.90).contains(&projection) {
            shadow = geometry * 42.0
                + (if d.cover { 22.0 } else { 0.0 })
                + (if marks_target { 14.0 } else { 0.0 })
                + (if d.presses_passer && projection <= 0.40 { 18.0 } else { 0.0 });
            shadow = shadow.clamp(0.0, 100.0);
        }
        if score < 2.0 && shadow < 2.0 {
            continue;
        }
        let b = Blocker {
            id: d.player_id.clone(),
            score,
            shadow,
            perpendicular: perp,
            projection,
            intercept_x: jround(closest_x),
            intercept_y: jround(closest_y),
        };
        blocker_count += 1;
        let index = leaders.iter().position(|other| better(&b, other)).unwrap_or(leaders.len());
        if index < 3 {
            leaders.insert(index, b);
            if leaders.len() > 3 {
                leaders.pop();
            }
        }
    }
    if leaders.is_empty() {
        return Evaluation::clear();
    }
    let first = &leaders[0];
    let combined = first.score
        + leaders.get(1).map(|x| x.score * 0.34).unwrap_or(0.0)
        + leaders.get(2).map(|x| x.score * 0.16).unwrap_or(0.0)
        + ((pass_length - 24.0) * 0.16).clamp(0.0, 8.0);
    let risk = jround(combined.clamp(0.0, 100.0));
    let shadow = leaders.iter().map(|b| b.shadow).fold(0.0, f64::max);
    let interception = jround((7.0 + first.score * 0.64 + risk as f64 * 0.16).clamp(7.0, 88.0));
    Evaluation {
        lane_risk: risk,
        cover_shadow: jround(shadow),
        interceptor_id: Some(first.id.clone()),
        interception_chance: interception,
        interceptor_distance_to_lane: first.perpendicular,
        interceptor_projection: first.projection,
        intercept_x: first.intercept_x.clamp(0, 100),
        intercept_y: first.intercept_y.clamp(0, 100),
        blocker_count,
    }
}

pub struct Analyzer {
    passer_pos: Option<(i32, i32)>,
    defenders: Vec<Defender>,
}

impl Analyzer {
    pub fn new(input: &MatchInput, state: &MatchState, attacking_side: TeamSide, passer_id: &str) -> Self {
        let passer_pos = state
            .player(passer_id)
            .filter(|r| r.on_pitch)
            .map(|r| (r.x, r.y));
        let defending_side = attacking_side.opposite();
        let defending = input.team(defending_side);
        let gk_id = state.team(defending_side).designated_goalkeeper_id.as_deref();
        let mut defenders = Vec::with_capacity(11);
        for (_, r) in &state.players {
            if !r.on_pitch || r.side != defending_side {
                continue;
            }
            let p = match defending.player(&r.player_id) {
                Some(p) => p,
                None => continue,
            };
            let a = &p.attributes;
            let d_slot = state.team(defending_side).tactical_slot(&r.slot_id);
            let d_out_prog = match d_slot {
                None => 50,
                Some(s) => geometry::attacking_progress(defending_side, geometry::oriented_y(defending_side, s.out_y)),
            };
            let attrs = DefenderAttributes {
                pace: a.get(8) as i32,
                defensive_positioning: a.get(16) as i32,
                decisions: a.get(13) as i32,
                work_rate: a.get(17) as i32,
                energy: r.energy,
                defensive_slot_out_progress: d_out_prog,
            };
            defenders.push(prepare_defender(
                &r.player_id,
                r.x,
                r.y,
                attrs,
                r.current_task.starts_with("COVER"),
                r.pressing_player_id.as_deref() == Some(passer_id),
                r.marking_player_id.clone(),
                Some(r.player_id.as_str()) == gk_id,
            ));
        }
        Self { passer_pos, defenders }
    }

    pub fn evaluate_target(&self, state: &MatchState, target_id: &str) -> Evaluation {
        let passer = match self.passer_pos {
            Some(p) => p,
            None => return Evaluation::clear(),
        };
        let target = match state.player(target_id).filter(|r| r.on_pitch) {
            Some(t) => (t.x, t.y),
            None => return Evaluation::clear(),
        };
        evaluate(passer, target, target_id, &self.defenders)
    }
}

pub fn evaluate_in_state(
    input: &MatchInput,
    state: &MatchState,
    attacking_side: TeamSide,
    passer_id: &str,
    target_id: &str,
) -> Evaluation {
    Analyzer::new(input, state, attacking_side, passer_id).evaluate_target(state, target_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_empty_and_shadow() {
        assert_eq!(evaluate((20, 20), (60, 60), "TARGET", &[]).lane_risk, 0);
        let d = prepare_defender(
            "D1",
            40,
            41,
            DefenderAttributes {
                pace: 79,
                defensive_positioning: 84,
                decisions: 70,
                work_rate: 77,
                energy: 90,
                defensive_slot_out_progress: 31,
            },
            true,
            false,
            Some("TARGET".into()),
            false,
        );
        let e = evaluate((20, 20), (60, 60), "TARGET", &[d]);
        assert!(e.has_geometric_interceptor());
        assert!(e.lane_risk > 0);
    }
}
