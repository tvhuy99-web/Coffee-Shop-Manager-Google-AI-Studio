#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum TeamSide {
    Home = 0,
    Away = 1,
}

impl TeamSide {
    pub fn opposite(self) -> Self {
        match self {
            Self::Home => Self::Away,
            Self::Away => Self::Home,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Home => "HOME",
            Self::Away => "AWAY",
        }
    }
}

pub fn clamp(value: i32) -> i32 { value.clamp(0, 100) }

pub fn distance(x1: i32, y1: i32, x2: i32, y2: i32) -> f64 {
    let dx = (x2 as i64) - (x1 as i64);
    let dy = (y2 as i64) - (y1 as i64);
    ((dx * dx + dy * dy) as f64).sqrt()
}

pub fn attacking_progress(side: TeamSide, y: i32) -> i32 {
    if side == TeamSide::Home { clamp(y) } else { 100 - clamp(y) }
}

pub fn progress_delta(side: TeamSide, from_y: i32, to_y: i32) -> i32 {
    attacking_progress(side, to_y) - attacking_progress(side, from_y)
}

pub fn oriented_x(side: TeamSide, x: i32) -> i32 {
    if side == TeamSide::Home { clamp(x) } else { 100 - clamp(x) }
}

pub fn oriented_y(side: TeamSide, y: i32) -> i32 {
    if side == TeamSide::Home { clamp(y) } else { 100 - clamp(y) }
}

pub fn own_goal_y_spatial(side: TeamSide) -> i32 {
    if side == TeamSide::Home { 4 } else { 96 }
}

pub fn attack_direction(side: TeamSide) -> i32 {
    if side == TeamSide::Home { 1 } else { -1 }
}

pub fn scale_width(x: i32, width: i32) -> i32 {
    let factor = 0.75_f64 + (width as f64 / 200.0_f64);
    clamp((50.0_f64 + ((x - 50) as f64) * factor + 0.5_f64).floor() as i32)
}

pub fn in_attacking_penalty_area(side: TeamSide, x: i32, y: i32) -> bool {
    attacking_progress(side, y) >= 88 && (26..=74).contains(&x)
}

pub fn in_defending_penalty_area(side: TeamSide, x: i32, y: i32) -> bool {
    in_attacking_penalty_area(side.opposite(), x, y)
}

pub fn wide(x: i32) -> bool { x <= 28 || x >= 72 }
pub fn central(x: i32) -> bool { (32..=68).contains(&x) }
pub fn goal_y(side: TeamSide) -> i32 { if side == TeamSide::Home { 100 } else { 0 } }
pub fn own_goal_y_geometry(side: TeamSide) -> i32 { if side == TeamSide::Home { 0 } else { 100 } }
pub fn penalty_spot_y(side: TeamSide) -> i32 { if side == TeamSide::Home { 88 } else { 12 } }
pub fn goal_kick_y(side: TeamSide) -> i32 { if side == TeamSide::Home { 7 } else { 93 } }
