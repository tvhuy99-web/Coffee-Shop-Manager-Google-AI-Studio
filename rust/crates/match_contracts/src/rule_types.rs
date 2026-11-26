//! Shared rule, timing and action vocabulary; no adjudication.
use serde::{Deserialize,Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RulesModeV4 {
    #[default]
    Standard,
    Knockout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WeatherCondition {
    #[default]
    Clear,
    LightRain,
    HeavyRain,
    Windy,
    Hot,
}

impl WeatherCondition {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            WeatherCondition::Clear => 0,
            WeatherCondition::LightRain => 1,
            WeatherCondition::HeavyRain => 2,
            WeatherCondition::Windy => 3,
            WeatherCondition::Hot => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PitchQuality {
    Excellent,
    #[default]
    Good,
    Heavy,
    Poor,
}

impl PitchQuality {
    #[inline]
    pub fn code(self) -> i32 {
        match self {
            PitchQuality::Excellent => 0,
            PitchQuality::Good => 1,
            PitchQuality::Heavy => 2,
            PitchQuality::Poor => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Period {
    NotStarted = 0,
    FirstHalf = 1,
    SecondHalf = 2,
    ExtraTimeFirst = 3,
    ExtraTimeSecond = 4,
    Shootout = 5,
    FullTime = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShotKindV4 {
    Driven,
    Curled,
    Header,
    OneTouch,
    LongRange,
    Rebound,
}

impl ShotKindV4 {
    pub fn as_str(self) -> &'static str {
        match self {
            ShotKindV4::Driven => "DRIVEN",
            ShotKindV4::Curled => "CURLED",
            ShotKindV4::Header => "HEADER",
            ShotKindV4::OneTouch => "ONE_TOUCH",
            ShotKindV4::LongRange => "LONG_RANGE",
            ShotKindV4::Rebound => "REBOUND",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassKindV4 {
    Short,
    Long,
    Switch,
    Through,
    Cross,
}

impl PassKindV4 {
    pub fn as_str(self) -> &'static str {
        match self {
            PassKindV4::Short => "SHORT",
            PassKindV4::Long => "LONG",
            PassKindV4::Switch => "SWITCH",
            PassKindV4::Through => "THROUGH",
            PassKindV4::Cross => "CROSS",
        }
    }

    pub fn is_aerial(self) -> bool {
        matches!(
            self,
            PassKindV4::Long | PassKindV4::Switch | PassKindV4::Cross
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandLifecycleStatus {
    Accepted,
    QueuedForDeadBall,
    Scheduled,
    Applied,
    Rejected,
    Superseded,
    Expired,
}

