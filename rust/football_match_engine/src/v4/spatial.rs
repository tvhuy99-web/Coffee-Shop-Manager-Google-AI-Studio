use serde::{Deserialize, Serialize};

/// Standard FIFA pitch dimensions in continuous metric `f32` coordinates.
/// Coordinate convention:
/// - `x`: pitch length (`0.0` = Home goal line, `105.0` = Away goal line)
/// - `y`: pitch width (`0.0` = left touchline, `68.0` = right touchline, `34.0` = center)
/// - `z`: ball elevation above turf in meters (`>= 0.0`)
pub const PITCH_LENGTH_M: f32 = 105.0;
pub const PITCH_WIDTH_M: f32 = 68.0;
pub const PITCH_HALF_LENGTH_M: f32 = 52.5;
pub const PITCH_HALF_WIDTH_M: f32 = 34.0;

pub const GOAL_WIDTH_M: f32 = 7.32;
pub const GOAL_HEIGHT_M: f32 = 2.44;
pub const GOAL_LEFT_POST_Y: f32 = PITCH_HALF_WIDTH_M - (GOAL_WIDTH_M * 0.5);
pub const GOAL_RIGHT_POST_Y: f32 = PITCH_HALF_WIDTH_M + (GOAL_WIDTH_M * 0.5);

pub const PENALTY_AREA_LENGTH_M: f32 = 16.5;
pub const PENALTY_AREA_WIDTH_M: f32 = 40.32;
pub const PENALTY_AREA_MIN_Y: f32 = PITCH_HALF_WIDTH_M - (PENALTY_AREA_WIDTH_M * 0.5);
pub const PENALTY_AREA_MAX_Y: f32 = PITCH_HALF_WIDTH_M + (PENALTY_AREA_WIDTH_M * 0.5);

pub const SIX_YARD_LENGTH_M: f32 = 5.5;
pub const SIX_YARD_WIDTH_M: f32 = 18.32;
pub const SIX_YARD_MIN_Y: f32 = PITCH_HALF_WIDTH_M - (SIX_YARD_WIDTH_M * 0.5);
pub const SIX_YARD_MAX_Y: f32 = PITCH_HALF_WIDTH_M + (SIX_YARD_WIDTH_M * 0.5);

pub const PENALTY_SPOT_DISTANCE_M: f32 = 11.0;
pub const CENTER_CIRCLE_RADIUS_M: f32 = 9.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TeamSideV4 {
    Home,
    Away,
}

impl TeamSideV4 {
    #[inline]
    pub fn opposite(self) -> Self {
        match self {
            TeamSideV4::Home => TeamSideV4::Away,
            TeamSideV4::Away => TeamSideV4::Home,
        }
    }

    #[inline]
    pub fn code(self) -> i32 {
        match self {
            TeamSideV4::Home => 0,
            TeamSideV4::Away => 1,
        }
    }
}

/// Identifies which of the four pitch perimeter lines a ball ray intersects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PitchBoundarySide {
    /// `x = 0.0` (Home defends this goal line).
    HomeGoalLine,
    /// `x = PITCH_LENGTH_M` (`105.0`, Away defends this goal line).
    AwayGoalLine,
    /// `y = 0.0`.
    LeftTouchline,
    /// `y = PITCH_WIDTH_M` (`68.0`).
    RightTouchline,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn center_spot() -> Self {
        Self {
            x: PITCH_HALF_LENGTH_M,
            y: PITCH_HALF_WIDTH_M,
        }
    }

    #[inline]
    pub fn distance_sq(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    #[inline]
    pub fn distance(self, other: Self) -> f32 {
        self.distance_sq(other).sqrt()
    }

    #[inline]
    pub fn clamp_to_pitch(self) -> Self {
        Self {
            x: self.x.clamp(0.0, PITCH_LENGTH_M),
            y: self.y.clamp(0.0, PITCH_WIDTH_M),
        }
    }

    #[inline]
    pub fn is_out_of_bounds(self) -> bool {
        self.x < 0.0 || self.x > PITCH_LENGTH_M || self.y < 0.0 || self.y > PITCH_WIDTH_M
    }

    /// Converts normalized tactical slot coords (`norm_x` = width 0..=100, `norm_y` = depth 0..=100)
    /// into continuous metric pitch coordinates (`105m x 68m`).
    pub fn from_normalized_slot(side: TeamSideV4, norm_width: f32, norm_depth: f32) -> Self {
        let w = (norm_width.clamp(0.0, 100.0) / 100.0) * PITCH_WIDTH_M;
        let d = (norm_depth.clamp(0.0, 100.0) / 100.0) * PITCH_LENGTH_M;
        match side {
            TeamSideV4::Home => Self { x: d, y: w },
            TeamSideV4::Away => Self {
                x: PITCH_LENGTH_M - d,
                y: PITCH_WIDTH_M - w,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub const fn ground(x: f32, y: f32) -> Self {
        Self { x, y, z: 0.0 }
    }

    #[inline]
    pub const fn center_spot() -> Self {
        Self {
            x: PITCH_HALF_LENGTH_M,
            y: PITCH_HALF_WIDTH_M,
            z: 0.0,
        }
    }

    #[inline]
    pub fn xy(self) -> Vec2 {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }

    #[inline]
    pub fn clamp_to_pitch(self) -> Self {
        Self {
            x: self.x.clamp(0.0, PITCH_LENGTH_M),
            y: self.y.clamp(0.0, PITCH_WIDTH_M),
            z: self.z.max(0.0),
        }
    }

    #[inline]
    pub fn distance_3d(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// Quantizes meters to integer millimeters for cross-architecture deterministic hashing.
#[inline]
pub fn quantize_mm(meters: f32) -> i32 {
    (meters * 1000.0).round() as i32
}

/// Attacking progress along pitch length in meters (`0.0` own goal line -> `105.0` opponent goal line).
#[inline]
pub fn attacking_progress_m(side: TeamSideV4, pos: Vec2) -> f32 {
    match side {
        TeamSideV4::Home => pos.x.clamp(0.0, PITCH_LENGTH_M),
        TeamSideV4::Away => (PITCH_LENGTH_M - pos.x).clamp(0.0, PITCH_LENGTH_M),
    }
}

/// Center of the goal that `side` is attacking.
#[inline]
pub fn attacking_goal_center(side: TeamSideV4) -> Vec2 {
    match side {
        TeamSideV4::Home => Vec2::new(PITCH_LENGTH_M, PITCH_HALF_WIDTH_M),
        TeamSideV4::Away => Vec2::new(0.0, PITCH_HALF_WIDTH_M),
    }
}

/// Distance in meters from `pos` to the center of the attacking goal.
#[inline]
pub fn distance_to_attacking_goal(side: TeamSideV4, pos: Vec2) -> f32 {
    pos.distance(attacking_goal_center(side))
}

/// Penalty spot coordinates for a penalty taken by `attacking_side`.
#[inline]
pub fn penalty_spot(attacking_side: TeamSideV4) -> Vec2 {
    match attacking_side {
        TeamSideV4::Home => Vec2::new(PITCH_LENGTH_M - PENALTY_SPOT_DISTANCE_M, PITCH_HALF_WIDTH_M),
        TeamSideV4::Away => Vec2::new(PENALTY_SPOT_DISTANCE_M, PITCH_HALF_WIDTH_M),
    }
}

#[inline]
pub fn is_in_attacking_penalty_box(side: TeamSideV4, pos: Vec2) -> bool {
    let in_y = pos.y >= PENALTY_AREA_MIN_Y && pos.y <= PENALTY_AREA_MAX_Y;
    if !in_y {
        return false;
    }
    match side {
        TeamSideV4::Home => pos.x >= (PITCH_LENGTH_M - PENALTY_AREA_LENGTH_M) && pos.x <= PITCH_LENGTH_M,
        TeamSideV4::Away => pos.x >= 0.0 && pos.x <= PENALTY_AREA_LENGTH_M,
    }
}

#[inline]
pub fn is_in_attacking_six_yard_box(side: TeamSideV4, pos: Vec2) -> bool {
    let in_y = pos.y >= SIX_YARD_MIN_Y && pos.y <= SIX_YARD_MAX_Y;
    if !in_y {
        return false;
    }
    match side {
        TeamSideV4::Home => pos.x >= (PITCH_LENGTH_M - SIX_YARD_LENGTH_M) && pos.x <= PITCH_LENGTH_M,
        TeamSideV4::Away => pos.x >= 0.0 && pos.x <= SIX_YARD_LENGTH_M,
    }
}

#[inline]
pub fn is_in_own_penalty_box(side: TeamSideV4, pos: Vec2) -> bool {
    is_in_attacking_penalty_box(side.opposite(), pos)
}

/// Shortest Euclidean distance in meters from `point` to line segment `a -> b`.
pub fn point_to_segment_distance_m(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let ab_len_sq = ab_x * ab_x + ab_y * ab_y;
    if ab_len_sq <= 1e-6 {
        return point.distance(a);
    }
    let ap_x = point.x - a.x;
    let ap_y = point.y - a.y;
    let t = ((ap_x * ab_x + ap_y * ab_y) / ab_len_sq).clamp(0.0, 1.0);
    let proj = Vec2::new(a.x + ab_x * t, a.y + ab_y * t);
    point.distance(proj)
}

/// Pure analytic ray-vs-pitch-boundary intersection for DES continuous interaction.
///
/// Computes the earliest time `t` in `[0.0, max_t]` when `p(t) = p0 + v * t` hits or
/// exits across `x = 0.0` (`HomeGoalLine`), `x = 105.0` (`AwayGoalLine`),
/// `y = 0.0` (`LeftTouchline`), or `y = 68.0` (`RightTouchline`).
/// Goal lines take deterministic precedence over touchlines on exact corner ties.
pub fn ray_pitch_boundary_hit_time(
    p0: Vec2,
    v: Vec2,
    max_t: f32,
) -> Option<(f32, PitchBoundarySide)> {
    if !max_t.is_finite() || max_t < 0.0 {
        return None;
    }

    // Immediate out-of-bounds check (goal lines prioritized over touchlines).
    if p0.x < 0.0 {
        return Some((0.0, PitchBoundarySide::HomeGoalLine));
    }
    if p0.x > PITCH_LENGTH_M {
        return Some((0.0, PitchBoundarySide::AwayGoalLine));
    }
    if p0.y < 0.0 {
        return Some((0.0, PitchBoundarySide::LeftTouchline));
    }
    if p0.y > PITCH_WIDTH_M {
        return Some((0.0, PitchBoundarySide::RightTouchline));
    }

    const EPS: f32 = 1e-6;
    let mut best: Option<(f32, PitchBoundarySide)> = None;

    let mut consider = |t: f32, side: PitchBoundarySide| {
        if t.is_finite() && t >= 0.0 && t <= max_t {
            match best {
                None => best = Some((t, side)),
                Some((best_t, _)) if t < best_t => best = Some((t, side)),
                _ => {}
            }
        }
    };

    if v.x < -EPS {
        consider((0.0 - p0.x) / v.x, PitchBoundarySide::HomeGoalLine);
    } else if v.x > EPS {
        consider((PITCH_LENGTH_M - p0.x) / v.x, PitchBoundarySide::AwayGoalLine);
    }

    if v.y < -EPS {
        consider((0.0 - p0.y) / v.y, PitchBoundarySide::LeftTouchline);
    } else if v.y > EPS {
        consider((PITCH_WIDTH_M - p0.y) / v.y, PitchBoundarySide::RightTouchline);
    }

    best
}

/// Returns `true` if `(y, z)` lies within the FIFA goal mouth opening:
/// `y` in `[GOAL_LEFT_POST_Y, GOAL_RIGHT_POST_Y]` and `0.0 <= z <= GOAL_HEIGHT_M`.
#[inline]
pub fn is_in_goal_mouth(y: f32, z: f32) -> bool {
    y >= GOAL_LEFT_POST_Y && y <= GOAL_RIGHT_POST_Y && z >= 0.0 && z <= GOAL_HEIGHT_M
}

/// Computes the closest approach time `t_closest` in `[0.0, max_t.max(0.0)]` and minimum
/// horizontal distance `dist_m` between a linearly moving ball `p_ball(t) = p_ball0 + v_ball * t`
/// and a player located at `p_player`.
pub fn closest_approach_time(
    p_ball0: Vec2,
    v_ball: Vec2,
    p_player: Vec2,
    max_t: f32,
) -> (f32, f32) {
    let clamped_max_t = if max_t.is_finite() {
        max_t.max(0.0)
    } else {
        0.0
    };
    let dx = p_ball0.x - p_player.x;
    let dy = p_ball0.y - p_player.y;
    let v_sq = v_ball.x * v_ball.x + v_ball.y * v_ball.y;

    let t_closest = if v_sq <= 1e-8 || clamped_max_t <= 0.0 {
        0.0
    } else {
        let t_raw = -(dx * v_ball.x + dy * v_ball.y) / v_sq;
        t_raw.clamp(0.0, clamped_max_t)
    };

    let ball_at_t = Vec2::new(
        p_ball0.x + v_ball.x * t_closest,
        p_ball0.y + v_ball.y * t_closest,
    );
    (t_closest, ball_at_t.distance(p_player))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_pitch_boundary_hits_earliest_side() {
        let hit = ray_pitch_boundary_hit_time(Vec2::new(95.0, 34.0), Vec2::new(20.0, 0.0), 2.0);
        let (t, side) = hit.expect("expected AwayGoalLine hit");
        assert_eq!(side, PitchBoundarySide::AwayGoalLine);
        assert!((t - 0.5).abs() < 1e-4);

        let touch = ray_pitch_boundary_hit_time(Vec2::new(50.0, 64.0), Vec2::new(5.0, 8.0), 2.0);
        let (t2, side2) = touch.expect("expected RightTouchline hit");
        assert_eq!(side2, PitchBoundarySide::RightTouchline);
        assert!((t2 - 0.5).abs() < 1e-4);

        // Beyond max_t returns None
        assert!(ray_pitch_boundary_hit_time(Vec2::new(50.0, 34.0), Vec2::new(5.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn goal_mouth_bounds_check() {
        assert!(is_in_goal_mouth(PITCH_HALF_WIDTH_M, 1.2));
        assert!(is_in_goal_mouth(GOAL_LEFT_POST_Y, GOAL_HEIGHT_M));
        assert!(!is_in_goal_mouth(GOAL_LEFT_POST_Y - 0.05, 1.0));
        assert!(!is_in_goal_mouth(PITCH_HALF_WIDTH_M, GOAL_HEIGHT_M + 0.05));
    }

    #[test]
    fn closest_approach_time_analytic() {
        let (t, dist) = closest_approach_time(
            Vec2::new(10.0, 20.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(25.0, 23.0),
            3.0,
        );
        assert!((t - 1.5).abs() < 1e-4);
        assert!((dist - 3.0).abs() < 1e-4);
    }
}
