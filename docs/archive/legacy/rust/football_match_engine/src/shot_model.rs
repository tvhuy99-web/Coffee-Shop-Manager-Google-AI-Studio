use crate::{
    contextual::{self, ShotContext},
    fingerprints as fp,
    geometry::{self, TeamSide},
    input::MatchInput,
    roster,
    state::MatchState,
    tactical_models,
};

const PITCH_LENGTH_M: f64 = 105.0;
const PITCH_WIDTH_M: f64 = 68.0;
const GOAL_WIDTH_M: f64 = 7.32;

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

#[derive(Clone, Debug)]
pub struct ShotContextV2 {
    pub side: TeamSide,
    pub shot_x: i32,
    pub shot_y: i32,
    pub progress: i32,
    pub centrality: i32,
    pub pressure: i32,
    pub nearby_defenders: i32,
    pub blockers: i32,
    pub best_blocker_id: Option<String>,
    pub best_blocker_perp_m: f64,
    pub best_blocker_distance_m: f64,
    pub distance_m: f64,
    pub goal_angle_deg: f64,
    pub shooting_lane_width_m: f64,
    pub header: bool,
    pub penalty: bool,
    pub direct_free_kick: bool,
    pub first_time: bool,
    pub one_vs_one: bool,
    pub goalkeeper_id: Option<String>,
    pub goalkeeper_distance_from_goal_m: f64,
    pub goalkeeper_lateral_error_m: f64,
    pub body_balance: i32,
    pub expected_goals: f64,
}

impl ShotContextV2 {
    pub fn build(
        _input: &MatchInput,
        state: &MatchState,
        side: TeamSide,
        shooter_id: &str,
        header: bool,
        pressure: i32,
        source: &str,
    ) -> Self {
        let x = state.ball.x;
        let y = state.ball.y;
        let progress = geometry::attacking_progress(side, y);
        let centrality = 100 - ((x - 50).abs() * 3).min(100);
        let goal_y = geometry::goal_y(side);
        let distance = physical_distance(x, y, 50, goal_y);
        let angle = goal_angle_degrees(side, x, y);
        let defending = side.opposite();
        let gk = state.team(defending).designated_goalkeeper_id.clone();
        let gr = gk.as_deref().and_then(|id| state.player(id));
        let (mut gk_advance, mut gk_lateral) = (0.0, 0.0);
        let gk_on_pitch = match gr {
            Some(r) if r.on_pitch => {
                gk_advance = (r.y - goal_y).abs() as f64 * PITCH_LENGTH_M / 100.0;
                gk_lateral = (r.x - 50).abs() as f64 * PITCH_WIDTH_M / 100.0;
                true
            }
            _ => false,
        };

        let mut nearby = 0;
        let mut blockers = 0;
        let mut active_outfield = 0;
        let mut best: Option<String> = None;
        let mut best_perp = 999.0_f64;
        let mut best_dist = 999.0_f64;
        let mut min_lane = 12.0_f64;
        let mut nearest_outfield = 999.0_f64;

        for (_, r) in &state.players {
            if !r.on_pitch || r.side != defending || gk.as_deref() == Some(r.player_id.as_str()) {
                continue;
            }
            active_outfield += 1;
            let ds = physical_distance(x, y, r.x, r.y);
            nearest_outfield = nearest_outfield.min(ds);
            if ds <= 7.5 {
                nearby += 1;
            }
            let (t, perp_m) = projection_to_goal(side, x, y, r.x, r.y);
            if (-0.04..0.96).contains(&t) {
                if t > 0.02 {
                    min_lane = min_lane.min(perp_m);
                }
                let allowed = if t > 0.02 { 6.80 + (1.0 - t) * 2.60 } else { 4.20 };
                let close_out = ds <= 9.5 && (-0.04..=0.72).contains(&t) && perp_m <= 9.0;
                if perp_m <= allowed || close_out {
                    blockers += 1;
                    if best.is_none()
                        || perp_m < best_perp
                        || ((perp_m - best_perp).abs() < 0.001 && ds < best_dist)
                    {
                        best = Some(r.player_id.clone());
                        best_perp = perp_m;
                        best_dist = ds;
                    }
                }
            }
        }
        if best.is_none() {
            best_perp = 999.0;
            best_dist = 999.0;
        }
        let penalty = source == "PENALTY";
        let direct_free_kick = source.contains("FREE_KICK") || source.contains("DIRECT_FK");
        let first_time = header
            || source.contains("FIRST_TIME")
            || source.contains("VOLLEY")
            || source.contains("CROSS_");
        let energy = state.player(shooter_id).map(|r| r.energy).unwrap_or(74);
        let body_balance = java_round(
            96.0 - pressure as f64 * 0.42
                - (nearby - 1).max(0) as f64 * 7.0
                - (55 - energy).max(0) as f64 * 0.22,
        )
        .clamp(25, 98);
        let one_vs_one = !penalty
            && !header
            && progress >= 88
            && distance <= 12.5
            && angle >= 30.0
            && blockers == 0
            && nearby == 0
            && nearest_outfield >= 18.0
            && gk_on_pitch;
        let missing_outfield = (10 - active_outfield).max(0);
        let xg = expected_goals(
            distance,
            angle,
            pressure,
            nearby,
            blockers,
            header,
            penalty,
            direct_free_kick,
            first_time,
            one_vs_one,
            body_balance,
            missing_outfield,
        );
        Self {
            side,
            shot_x: x,
            shot_y: y,
            progress,
            centrality,
            pressure,
            nearby_defenders: nearby,
            blockers,
            best_blocker_id: best,
            best_blocker_perp_m: best_perp,
            best_blocker_distance_m: best_dist,
            distance_m: distance,
            goal_angle_deg: angle,
            shooting_lane_width_m: min_lane,
            header,
            penalty,
            direct_free_kick,
            first_time,
            one_vs_one,
            goalkeeper_id: gk,
            goalkeeper_distance_from_goal_m: gk_advance,
            goalkeeper_lateral_error_m: gk_lateral,
            body_balance,
            expected_goals: xg,
        }
    }

    pub fn expected_goals_milli(&self) -> i32 {
        let scale = if self.penalty { 1000.0 } else { 890.0 };
        let lo = if self.penalty { 650 } else { 8 };
        let hi = if self.penalty { 900 } else { 650 };
        java_round(self.expected_goals * scale).clamp(lo, hi)
    }

    pub fn block_chance(&self, input: &MatchInput, state: &MatchState) -> i32 {
        if self.penalty || self.blockers <= 0 {
            return 0;
        }
        let bid = match self.best_blocker_id.as_deref() {
            Some(id) => id,
            None => return 0,
        };
        let bp = match input.team(self.side.opposite()).player(bid) {
            Some(p) => p,
            None => return 0,
        };
        let br = match state.player(bid) {
            Some(r) if r.on_pitch => r,
            _ => return 0,
        };
        let _ = br;
        let a = &bp.attributes;
        let geom = (4.5 - self.best_blocker_perp_m).max(0.0) * 7.0
            + (6.0 - self.best_blocker_distance_m).max(0.0) * 2.2;
        let skill = (a.get(16) as f64 - 50.0) * 0.10
            + (a.get(13) as f64 - 50.0) * 0.06
            + (a.get(9) as f64 - 50.0) * 0.035;
        let crowd = (self.blockers - 1).max(0) as f64 * 3.0;
        java_round(33.0 + geom * 0.82 + skill + crowd).clamp(20, 82)
    }

    pub fn goalkeeper_positioning_quality(&self, input: &MatchInput, state: &MatchState) -> i32 {
        let gid = match self.goalkeeper_id.as_deref() {
            Some(id) => id,
            None => return 20,
        };
        let gp = match input.team(self.side.opposite()).player(gid) {
            Some(p) => p,
            None => return 20,
        };
        let gr = match state.player(gid) {
            Some(r) if r.on_pitch => r,
            _ => return 20,
        };
        let _ = gr;
        let optimal_advance = if self.one_vs_one {
            3.8
        } else {
            (16.0 / self.distance_m.max(5.0)).clamp(0.7, 2.7)
        };
        let advance_error = (self.goalkeeper_distance_from_goal_m - optimal_advance).abs();
        let pos = 88.0
            - advance_error * 6.5
            - self.goalkeeper_lateral_error_m * 4.0
            + (gp.attributes.get(13) as f64 - 74.0) * 0.16;
        java_round(pos).clamp(30, 96)
    }
}

#[allow(clippy::too_many_arguments)]
fn expected_goals(
    distance: f64,
    angle: f64,
    pressure: i32,
    nearby: i32,
    blockers: i32,
    header: bool,
    penalty: bool,
    direct_free_kick: bool,
    first_time: bool,
    one_vs_one: bool,
    body_balance: i32,
    missing_outfield: i32,
) -> f64 {
    if penalty {
        return 0.760;
    }
    let mut z = -2.67;
    z += (22.0 - distance.min(32.0)) * 0.088;
    z += (angle - 30.0) * 0.017;
    z -= pressure as f64 * 0.0060;
    z -= nearby as f64 * 0.105;
    z -= blockers.min(2) as f64 * 0.05;
    z += (body_balance - 74) as f64 * 0.0030;
    if header { z -= 0.30; }
    if first_time && !header { z -= 0.06; }
    if direct_free_kick { z -= 0.28; }
    if one_vs_one { z += 0.47; }
    z += missing_outfield.max(0) as f64 * 0.38;
    let p = 1.0 / (1.0 + (-z).exp());
    p.clamp(0.008, 0.62)
}

fn projection_to_goal(side: TeamSide, sx: i32, sy: i32, px: i32, py: i32) -> (f64, f64) {
    let ax = sx as f64 * PITCH_WIDTH_M / 100.0;
    let ay = sy as f64 * PITCH_LENGTH_M / 100.0;
    let bx = 50.0 * PITCH_WIDTH_M / 100.0;
    let by = geometry::goal_y(side) as f64 * PITCH_LENGTH_M / 100.0;
    let qx = px as f64 * PITCH_WIDTH_M / 100.0;
    let qy = py as f64 * PITCH_LENGTH_M / 100.0;
    let vx = bx - ax;
    let vy = by - ay;
    let wx = qx - ax;
    let wy = qy - ay;
    let vv = vx * vx + vy * vy;
    let t = if vv <= 0.0 { 0.0 } else { (wx * vx + wy * vy) / vv };
    let cx = ax + t * vx;
    let cy = ay + t * vy;
    let dx = qx - cx;
    let dy = qy - cy;
    (t, (dx * dx + dy * dy).sqrt())
}

fn goal_angle_degrees(side: TeamSide, x: i32, y: i32) -> f64 {
    let sx = x as f64 * PITCH_WIDTH_M / 100.0;
    let sy = y as f64 * PITCH_LENGTH_M / 100.0;
    let gy = geometry::goal_y(side) as f64 * PITCH_LENGTH_M / 100.0;
    let cx = 50.0 * PITCH_WIDTH_M / 100.0;
    let half = GOAL_WIDTH_M / 2.0;
    let (ax, ay) = (cx - half - sx, gy - sy);
    let (bx, by) = (cx + half - sx, gy - sy);
    let na = (ax * ax + ay * ay).sqrt();
    let nb = (bx * bx + by * by).sqrt();
    if na < 0.001 || nb < 0.001 {
        return 90.0;
    }
    let cos = ((ax * bx + ay * by) / (na * nb)).clamp(-1.0, 1.0);
    cos.acos().to_degrees()
}

fn physical_distance(x1: i32, y1: i32, x2: i32, y2: i32) -> f64 {
    let dx = (x2 - x1) as f64 * PITCH_WIDTH_M / 100.0;
    let dy = (y2 - y1) as f64 * PITCH_LENGTH_M / 100.0;
    (dx * dx + dy * dy).sqrt()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShotQuality {
    pub quality: i32,
    pub expected_goals_milli: i32,
    pub progress: i32,
    pub centrality: i32,
    pub pressure: i32,
    pub nearby_defenders: i32,
    pub shot_type: String,
}

impl ShotQuality {
    pub fn calculate(
        input: &MatchInput,
        state: &MatchState,
        side: TeamSide,
        shooter_id: &str,
        context: &ShotContextV2,
    ) -> Result<Self, &'static str> {
        let player = input.team(side).player(shooter_id).ok_or("shooter snapshot required")?;
        let attr_ctx = ShotContext {
            pressure: context.pressure,
            progress: context.progress,
            centrality: context.centrality,
            nearby_defenders: context.nearby_defenders,
            header: context.header,
            penalty: context.penalty,
        };
        let contextual_adj = contextual::shot_execution_adjustment(&player.attributes, &attr_ctx);
        let geom = (context.goal_angle_deg - 30.0) * 0.12 + (18.0 - context.distance_m) * 0.30;
        let balance = (context.body_balance - 74) as f64 * 0.10;
        let one_vs_one = if context.one_vs_one { 4.0 } else { 0.0 };
        let mut q = 58.0
            + contextual_adj
            + geom
            + balance
            + one_vs_one
            - context.pressure as f64 * 0.08
            - context.blockers as f64 * 1.5
            + (player.form - 50) as f64 * 0.05
            + (roster::effective_morale(input, state, side, shooter_id) - 50) as f64 * 0.04
            + roster::team_mental_execution_adjustment(input, state, side)
            + tactical_models::home_shot_quality_adjustment(input, side);
        if context.header {
            q -= 3.0;
        }
        if context.penalty {
            q = 78.0 + contextual_adj * 0.35;
        }
        let quality = java_round(q).clamp(8, 95);
        let shot_type = if context.header {
            "HEADER"
        } else if context.penalty {
            "PENALTY"
        } else if context.direct_free_kick {
            "DIRECT_FREE_KICK"
        } else {
            "SHOT"
        };
        Ok(Self {
            quality,
            expected_goals_milli: context.expected_goals_milli(),
            progress: context.progress,
            centrality: context.centrality,
            pressure: context.pressure,
            nearby_defenders: context.nearby_defenders,
            shot_type: shot_type.into(),
        })
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.quality);
        h = fp::mix_i32(h, self.expected_goals_milli);
        h = fp::mix_i32(h, self.progress);
        h = fp::mix_i32(h, self.centrality);
        h = fp::mix_i32(h, self.pressure);
        h = fp::mix_i32(h, self.nearby_defenders);
        fp::mix_string(h, Some(&self.shot_type))
    }
}
