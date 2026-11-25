use crate::fingerprints;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerAttributesSnapshot {
    values: [u8; 20],
}

impl PlayerAttributesSnapshot {
    pub const ATTRIBUTE_NAMES: [&'static str; 20] = [
        "goalkeeping", "tackling", "passing", "crossing", "technique", "dribbling", "shooting",
        "heading", "pace", "agility", "strength", "stamina", "vision", "decisions", "composure",
        "offBallMovement", "defensivePositioning", "workRate", "charisma", "temperament",
    ];

    pub fn new(values: [u8; 20]) -> Result<Self, &'static str> {
        if values.iter().any(|v| *v < 1 || *v > 99) {
            Err("attribute out of range: expected 1..99")
        } else {
            Ok(Self { values })
        }
    }

    pub fn get(&self, index: usize) -> u8 {
        self.values[index]
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        for &v in &self.values {
            h = fingerprints::mix_i32(h, v as i32);
        }
        h
    }
}

#[deprecated(note = "Use crate::session::UnifiedMatchEngine::simulate instead")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPortStatus {
    NotPorted,
}

#[deprecated(note = "Use crate::session::UnifiedMatchEngine::simulate instead")]
#[allow(deprecated)]
pub fn simulate_match_not_yet_ported() -> Result<(), MatchPortStatus> {
    Err(MatchPortStatus::NotPorted)
}
