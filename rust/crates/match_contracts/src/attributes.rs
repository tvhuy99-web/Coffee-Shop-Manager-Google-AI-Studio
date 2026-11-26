use serde::{Deserialize, Serialize};

use crate::fingerprints;
use crate::error::V4Error;

pub const ATTR_MIN: u8 = 1;
pub const ATTR_MAX: u8 = 20;

/// Bit-exact non-linear S-curve lookup table for 1..=20 attributes.
/// Using a const LUT guarantees identical floating-point behavior across
/// x86_64 and aarch64-linux-android without libm `powf` drift.
const NORMALIZE_LUT: [f32; 21] = [
    0.000, // unused index 0
    0.040, // 1
    0.075, // 2
    0.110, // 3
    0.148, // 4  (lower tier)
    0.190, // 5
    0.236, // 6
    0.286, // 7
    0.340, // 8
    0.398, // 9
    0.460, // 10 (mid tier)
    0.524, // 11
    0.590, // 12
    0.656, // 13
    0.720, // 14 (strong tier)
    0.780, // 15
    0.835, // 16
    0.884, // 17 (elite tier)
    0.928, // 18
    0.966, // 19
    1.000, // 20
];

/// Convex elite-separation lookup table for high-leverage actions
/// (e.g., clinical finishing, world-class reflexes, vision breakthroughs),
/// ensuring clear separation between 16, 17, 18, 19, and 20.
const NORMALIZE_ELITE_LUT: [f32; 21] = [
    0.000, // unused index 0
    0.025, // 1
    0.050, // 2
    0.078, // 3
    0.110, // 4
    0.146, // 5
    0.186, // 6
    0.230, // 7
    0.278, // 8
    0.330, // 9
    0.386, // 10
    0.445, // 11
    0.506, // 12
    0.570, // 13
    0.636, // 14
    0.704, // 15
    0.772, // 16
    0.836, // 17
    0.896, // 18
    0.950, // 19
    1.000, // 20
];

#[inline]
pub fn normalize_attr(raw: u8) -> f32 {
    NORMALIZE_LUT[raw.clamp(ATTR_MIN, ATTR_MAX) as usize]
}

#[inline]
pub fn normalize_attr_elite(raw: u8) -> f32 {
    NORMALIZE_ELITE_LUT[raw.clamp(ATTR_MIN, ATTR_MAX) as usize]
}

/// 13 Technical attributes (1..=20), matching `england_v2` JSON keys verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TechnicalAttributes {
    pub finishing: u8,
    pub passing: u8,
    pub dribbling: u8,
    pub technique: u8,
    pub first_touch: u8,
    pub crossing: u8,
    pub heading: u8,
    pub long_shots: u8,
    pub tackling: u8,
    pub marking: u8,
    pub free_kicks: u8,
    pub penalties: u8,
    pub corners: u8,
}

/// 14 Mental attributes (1..=20), matching `england_v2` JSON keys verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MentalAttributes {
    pub vision: u8,
    pub decisions: u8,
    pub composure: u8,
    pub concentration: u8,
    pub off_the_ball: u8,
    pub positioning: u8,
    pub anticipation: u8,
    pub teamwork: u8,
    pub determination: u8,
    pub work_rate: u8,
    pub leadership: u8,
    pub flair: u8,
    pub aggression: u8,
    pub bravery: u8,
}

/// 8 Physical attributes (1..=20), matching `england_v2` JSON keys verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PhysicalAttributes {
    pub pace: u8,
    pub acceleration: u8,
    pub strength: u8,
    pub stamina: u8,
    pub balance: u8,
    pub agility: u8,
    pub jumping_reach: u8,
    pub natural_fitness: u8,
}

/// 9 Goalkeeping attributes (1..=20), matching `england_v2` JSON keys verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GoalkeepingAttributes {
    pub reflexes: u8,
    pub handling: u8,
    pub one_on_ones: u8,
    pub command_of_area: u8,
    pub aerial_reach: u8,
    pub kicking: u8,
    pub throwing: u8,
    pub rushing_out: u8,
    pub punching: u8,
}

/// 10 Hidden attributes (1..=20), matching `england_v2` JSON keys verbatim.
/// Split logically into:
/// - 6 In-Match hidden attributes: `consistency`, `big_matches`, `pressure`,
///   `injury_proneness`, `temperament`, `sportsmanship`
/// - 4 Meta/Career attributes: `professionalism`, `ambition`, `adaptability`, `loyalty`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HiddenAttributes {
    pub professionalism: u8,
    pub ambition: u8,
    pub consistency: u8,
    pub big_matches: u8,
    pub adaptability: u8,
    pub loyalty: u8,
    pub pressure: u8,
    pub injury_proneness: u8,
    pub temperament: u8,
    pub sportsmanship: u8,
}

/// Full 54-attribute player model for Match Engine V4.0.
/// 50 attributes actively shape in-match simulation; 4 hidden attributes
/// (`professionalism`, `ambition`, `adaptability`, `loyalty`) serve career &
/// inter-match hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayerAttributes54 {
    pub technical: TechnicalAttributes,
    pub mental: MentalAttributes,
    pub physical: PhysicalAttributes,
    pub goalkeeping: GoalkeepingAttributes,
    pub hidden: HiddenAttributes,
}

impl PlayerAttributes54 {
    /// Creates a uniform 54-attribute profile (useful for synthetic Attribute Impact Tests).
    pub fn uniform(value: u8) -> Result<Self, V4Error> {
        let profile = Self {
            technical: TechnicalAttributes {
                finishing: value,
                passing: value,
                dribbling: value,
                technique: value,
                first_touch: value,
                crossing: value,
                heading: value,
                long_shots: value,
                tackling: value,
                marking: value,
                free_kicks: value,
                penalties: value,
                corners: value,
            },
            mental: MentalAttributes {
                vision: value,
                decisions: value,
                composure: value,
                concentration: value,
                off_the_ball: value,
                positioning: value,
                anticipation: value,
                teamwork: value,
                determination: value,
                work_rate: value,
                leadership: value,
                flair: value,
                aggression: value,
                bravery: value,
            },
            physical: PhysicalAttributes {
                pace: value,
                acceleration: value,
                strength: value,
                stamina: value,
                balance: value,
                agility: value,
                jumping_reach: value,
                natural_fitness: value,
            },
            goalkeeping: GoalkeepingAttributes {
                reflexes: value,
                handling: value,
                one_on_ones: value,
                command_of_area: value,
                aerial_reach: value,
                kicking: value,
                throwing: value,
                rushing_out: value,
                punching: value,
            },
            hidden: HiddenAttributes {
                professionalism: value,
                ambition: value,
                consistency: value,
                big_matches: value,
                adaptability: value,
                loyalty: value,
                pressure: value,
                injury_proneness: value,
                temperament: value,
                sportsmanship: value,
            },
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Returns all 54 attributes in fixed canonical order `(field_name, value)`.
    pub fn all_named(&self) -> [(&'static str, u8); 54] {
        let t = &self.technical;
        let m = &self.mental;
        let p = &self.physical;
        let g = &self.goalkeeping;
        let h = &self.hidden;
        [
            // Technical (13)
            ("finishing", t.finishing),
            ("passing", t.passing),
            ("dribbling", t.dribbling),
            ("technique", t.technique),
            ("first_touch", t.first_touch),
            ("crossing", t.crossing),
            ("heading", t.heading),
            ("long_shots", t.long_shots),
            ("tackling", t.tackling),
            ("marking", t.marking),
            ("free_kicks", t.free_kicks),
            ("penalties", t.penalties),
            ("corners", t.corners),
            // Mental (14)
            ("vision", m.vision),
            ("decisions", m.decisions),
            ("composure", m.composure),
            ("concentration", m.concentration),
            ("off_the_ball", m.off_the_ball),
            ("positioning", m.positioning),
            ("anticipation", m.anticipation),
            ("teamwork", m.teamwork),
            ("determination", m.determination),
            ("work_rate", m.work_rate),
            ("leadership", m.leadership),
            ("flair", m.flair),
            ("aggression", m.aggression),
            ("bravery", m.bravery),
            // Physical (8)
            ("pace", p.pace),
            ("acceleration", p.acceleration),
            ("strength", p.strength),
            ("stamina", p.stamina),
            ("balance", p.balance),
            ("agility", p.agility),
            ("jumping_reach", p.jumping_reach),
            ("natural_fitness", p.natural_fitness),
            // Goalkeeping (9)
            ("reflexes", g.reflexes),
            ("handling", g.handling),
            ("one_on_ones", g.one_on_ones),
            ("command_of_area", g.command_of_area),
            ("aerial_reach", g.aerial_reach),
            ("kicking", g.kicking),
            ("throwing", g.throwing),
            ("rushing_out", g.rushing_out),
            ("punching", g.punching),
            // Hidden (10)
            ("professionalism", h.professionalism),
            ("ambition", h.ambition),
            ("consistency", h.consistency),
            ("big_matches", h.big_matches),
            ("adaptability", h.adaptability),
            ("loyalty", h.loyalty),
            ("pressure", h.pressure),
            ("injury_proneness", h.injury_proneness),
            ("temperament", h.temperament),
            ("sportsmanship", h.sportsmanship),
        ]
    }

    /// Validates that every single one of the 54 attributes lies in `1..=20`.
    pub fn validate(&self) -> Result<(), V4Error> {
        for (field, value) in self.all_named() {
            if !(ATTR_MIN..=ATTR_MAX).contains(&value) {
                return Err(V4Error::InvalidAttribute { field, value });
            }
        }
        Ok(())
    }

    /// Deterministic FNV-1a fingerprint over all 54 attributes.
    pub fn fingerprint(&self) -> u64 {
        let mut h = fingerprints::start();
        for (_, value) in self.all_named() {
            h = fingerprints::mix_i32(h, value as i32);
        }
        h
    }

    /// Composite attacking quality on 0.0..=1.0 using non-linear normalization.
    pub fn attacking_composite(&self) -> f32 {
        (normalize_attr_elite(self.technical.finishing) * 0.25
            + normalize_attr(self.technical.passing) * 0.15
            + normalize_attr(self.technical.dribbling) * 0.15
            + normalize_attr(self.technical.first_touch) * 0.15
            + normalize_attr(self.mental.off_the_ball) * 0.15
            + normalize_attr(self.mental.composure) * 0.15)
            .clamp(0.0, 1.0)
    }

    /// Composite defensive quality on 0.0..=1.0 using non-linear normalization.
    pub fn defensive_composite(&self) -> f32 {
        (normalize_attr(self.technical.tackling) * 0.22
            + normalize_attr(self.technical.marking) * 0.20
            + normalize_attr(self.mental.positioning) * 0.22
            + normalize_attr(self.mental.anticipation) * 0.18
            + normalize_attr(self.mental.concentration) * 0.10
            + normalize_attr(self.physical.strength) * 0.08)
            .clamp(0.0, 1.0)
    }

    /// Composite goalkeeping quality on 0.0..=1.0 using all 9 GK attributes.
    pub fn goalkeeping_composite(&self) -> f32 {
        let g = &self.goalkeeping;
        (normalize_attr_elite(g.reflexes) * 0.22
            + normalize_attr(g.handling) * 0.16
            + normalize_attr(g.one_on_ones) * 0.14
            + normalize_attr(g.command_of_area) * 0.12
            + normalize_attr(g.aerial_reach) * 0.10
            + normalize_attr(g.rushing_out) * 0.08
            + normalize_attr(g.punching) * 0.06
            + normalize_attr(g.kicking) * 0.06
            + normalize_attr(g.throwing) * 0.06)
            .clamp(0.0, 1.0)
    }

    /// Meta/career readiness hook (professionalism + adaptability) in 0.0..=1.0.
    pub fn career_readiness_hook(&self) -> f32 {
        (normalize_attr(self.hidden.professionalism) * 0.55
            + normalize_attr(self.hidden.adaptability) * 0.45)
            .clamp(0.0, 1.0)
    }
}
