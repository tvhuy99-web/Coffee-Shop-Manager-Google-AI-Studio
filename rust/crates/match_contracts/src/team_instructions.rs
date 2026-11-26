use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInstructionsV4 {
    pub pressing_intensity: u8,
    pub tempo: u8,
    pub line_height: u8,
    pub width: u8,
    pub time_wasting: u8,
    pub offside_trap: bool,
    pub aggression: u8,
    pub transition_risk: u8,
    pub build_up_risk: u8,
    pub concentration: u8,
}

impl Default for TeamInstructionsV4 {
    fn default() -> Self {
        Self {
            pressing_intensity: 50,
            tempo: 50,
            line_height: 50,
            width: 50,
            time_wasting: 10,
            offside_trap: false,
            aggression: 50,
            transition_risk: 50,
            build_up_risk: 50,
            concentration: 50,
        }
    }
}

