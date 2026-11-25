//! Read-only coach tactical proposal. Engine owns application and event publication.
use match_contracts::team_instructions::TeamInstructionsV4;

#[derive(Debug, Clone, Copy)]
pub struct CoachObservation {
    pub goal_diff: i32,
    pub own_xg: f32,
    pub opp_xg: f32,
    pub down_to_ten: bool,
    pub game_sec: i32,
}

pub fn propose_tactics(base_ti: TeamInstructionsV4, observation: CoachObservation) -> TeamInstructionsV4 {
    let CoachObservation { goal_diff, own_xg, opp_xg, down_to_ten, game_sec } = observation;
    let mut next_ti = base_ti;

    if down_to_ten {
        next_ti.line_height = next_ti.line_height.saturating_sub(18).max(20);
        next_ti.pressing_intensity = next_ti.pressing_intensity.saturating_sub(14).max(25);
        next_ti.build_up_risk = next_ti.build_up_risk.saturating_sub(15).max(20);
        next_ti.width = next_ti.width.saturating_sub(12).max(30);
    }

    if goal_diff < 0 && game_sec >= 3600 {
        let urgency: u8 = if game_sec >= 4680 { 20 } else { 12 };
        next_ti.line_height = (next_ti.line_height + urgency).min(88);
        next_ti.pressing_intensity = (next_ti.pressing_intensity + urgency).min(90);
        next_ti.tempo = (next_ti.tempo + urgency).min(90);
        next_ti.transition_risk = (next_ti.transition_risk + 14).min(90);
        next_ti.time_wasting = 5;
    } else if goal_diff > 0 && game_sec >= 4320 {
        let lock: u8 = if game_sec >= 4920 { 22 } else { 14 };
        next_ti.line_height = next_ti.line_height.saturating_sub(lock).max(22);
        next_ti.pressing_intensity = next_ti.pressing_intensity.saturating_sub(12).max(28);
        next_ti.tempo = next_ti.tempo.saturating_sub(14).max(25);
        next_ti.build_up_risk = next_ti.build_up_risk.saturating_sub(15).max(20);
        next_ti.time_wasting = if game_sec >= 4800 { 78 } else { 62 };
    } else if (opp_xg - own_xg) >= 0.85 && game_sec >= 2100 {
        next_ti.line_height = next_ti.line_height.saturating_sub(10).max(28);
        next_ti.concentration = (next_ti.concentration + 10).min(90);
    }

    next_ti
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_baseline_and_adapts_to_actual_match_context() {
        let base=TeamInstructionsV4::default();
        let o=CoachObservation{goal_diff:0,own_xg:0.0,opp_xg:0.0,down_to_ten:false,game_sec:0};
        assert_eq!(propose_tactics(base.clone(),o),base);
        let chase=propose_tactics(base.clone(),CoachObservation{goal_diff:-1,game_sec:4680,..o});
        assert_eq!((chase.tempo,chase.line_height,chase.time_wasting),(70,70,5));
        let protect=propose_tactics(base.clone(),CoachObservation{goal_diff:1,game_sec:4920,..o});
        assert_eq!((protect.line_height,protect.tempo,protect.time_wasting),(28,36,78));
        assert_eq!(propose_tactics(base.clone(),CoachObservation{down_to_ten:true,..o}).line_height,32);
    }
}
