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
