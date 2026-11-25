use serde::{Deserialize, Serialize};
use match_contracts::spatial::Vec2;

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PlannedActionV4 {
    Pass {
        target_idx: usize,
        kind: PassKindV4,
        distance_m: f32,
        progress_gain_m: f32,
        lane_risk: f32,
        interceptor_idx: Option<usize>,
    },
    Dribble {
        defender_idx: Option<usize>,
    },
    HoldAndRecycle,
    Shoot {
        distance_m: f32,
        in_box: bool,
    },
    Clearance,
}

/// Read-only movement intention. Duration is a proposed action horizon;
/// a physical contest can interrupt it earlier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CarryIntentV4 {
    pub direction: Vec2,
    pub duration_ms: i64,
    pub shielding: bool,
}

/// DES decision bundle pairing the chosen football action with its kinematic reaction/decision
/// delay (`decision_delay_ms`) and one-touch vs multi-touch execution intent (`is_one_touch`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DecisionPlanV4 {
    pub action: PlannedActionV4,
    pub decision_delay_ms: i64,
    pub is_one_touch: bool,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub carry: Option<CarryIntentV4>,
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateOption {
    pub action: PlannedActionV4,
    pub utility: f32,
    pub risk: f32,
}

