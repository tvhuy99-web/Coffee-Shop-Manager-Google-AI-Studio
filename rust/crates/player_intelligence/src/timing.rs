use match_contracts::ports::RngSampleSource;
use match_contracts::attributes::{normalize_attr, PlayerAttributes54};
use super::intent::{PassKindV4, PlannedActionV4};

/// Evaluates whether the carrier intends a one-touch release vs taking a controlling touch.
///
/// Deliberately avoids the naive shortcut "better player = always faster":
/// - High `first_touch`, `vision`, `decisions`, high team `tempo`, and tight opponent pressure
///   increase one-touch intent to beat the press.
/// - Conversely, high `composure` + `decisions` under low-to-moderate pressure introduces
///   "la pausa" restraint — taking a beat to draw defenders and pick the optimal release window.
pub fn evaluate_one_touch_intent(
    action: &PlannedActionV4,
    attrs: &PlayerAttributes54,
    pressure_norm: f32,
    tempo: u8,
    time_wasting: u8,
    rng: &mut impl RngSampleSource,
) -> bool {
    let p_norm = pressure_norm.clamp(0.0, 1.0);
    let ft = normalize_attr(attrs.technical.first_touch);
    let vis = normalize_attr(attrs.mental.vision);
    let dec = normalize_attr(attrs.mental.decisions);
    let comp = normalize_attr(attrs.mental.composure);
    let tempo_norm = (tempo as f32 / 100.0).clamp(0.0, 1.0);
    let tw_norm = (time_wasting as f32 / 100.0).clamp(0.0, 1.0);

    match *action {
        PlannedActionV4::Dribble { .. } | PlannedActionV4::HoldAndRecycle => false,
        PlannedActionV4::Clearance => p_norm >= 0.42 || (rng.next_double() as f32) < 0.65,
        PlannedActionV4::Shoot { in_box, .. } => {
            if !in_box {
                return false;
            }
            // Composed finishers in space take a touch to set body shape; under pressure they strike first-time.
            let calm_set_restraint = (comp * 0.65 + dec * 0.35) * (1.0 - p_norm) * 0.30;
            let prob = (0.18 + ft * 0.26 + dec * 0.12 + p_norm * 0.30 + tempo_norm * 0.10
                - calm_set_restraint)
                .clamp(0.05, 0.78);
            (rng.next_double() as f32) < prob
        }
        PlannedActionV4::Pass {
            kind, distance_m, ..
        } => {
            if matches!(kind, PassKindV4::Switch) || distance_m > 34.0 {
                return false;
            }
            let kind_bias = match kind {
                PassKindV4::Short => 0.14,
                PassKindV4::Through => 0.08,
                PassKindV4::Cross => 0.05,
                PassKindV4::Long => -0.14,
                PassKindV4::Switch => -0.25,
            };
            // "La pausa": composed, high-decision players in space deliberately take a touch
            // to bait the press and wait for teammates' runs instead of rushing one-touch.
            let pausa_restraint = (comp * 0.60 + dec * 0.40)
                * (1.0 - p_norm)
                * (1.0 - tempo_norm * 0.65)
                * 0.38;
            let skill_readiness = ft * 0.34 + vis * 0.24 + dec * 0.16;
            let press_drive = p_norm * (0.24 + ft * 0.14);
            let prob = (0.08
                + kind_bias
                + skill_readiness * 0.36
                + tempo_norm * 0.22
                + press_drive
                - pausa_restraint
                - tw_norm * 0.18)
                .clamp(0.03, 0.72);
            (rng.next_double() as f32) < prob
        }
    }
}

