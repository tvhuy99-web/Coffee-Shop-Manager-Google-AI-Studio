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

/// Observable own-team selection inputs. Indices are stable engine roster indices.
#[derive(Debug,Clone,Copy)]
pub struct SubstitutionCandidate {
    pub eligible:bool,
    pub injured:bool,
    pub goalkeeper:bool,
    pub yellow_cards:u8,
    pub aerobic:f32,
}
pub fn choose_outgoing(players:&[SubstitutionCandidate],game_sec:i32,injury_only:bool)->Option<usize>{
    if injury_only {return players.iter().position(|p|p.eligible&&p.injured);}
    let fatigue_threshold=if game_sec>=4500 {74.0}else{66.0};
    let mut worst_idx=None;
    let mut worst_score=f32::MAX;
    for (idx,p) in players.iter().enumerate(){
        if !p.eligible||p.goalkeeper {continue;}
        let card_pen=if p.yellow_cards>0 {10.0}else{0.0};
        let readiness=p.aerobic-card_pen;
        if readiness<fatigue_threshold&&readiness<worst_score {worst_score=readiness;worst_idx=Some(idx);}
    }
    worst_idx
}
/// Engine supplies original position familiarity; coach owns selection and tie order.
pub fn choose_incoming(candidates:&[Option<(f32,u8)>])->Option<usize>{
    let mut best_bench_idx=None;
    let mut best_fit=-1.0f32;
    for (idx,c) in candidates.iter().enumerate(){
        let Some((fam,condition))=*c else{continue;};
        let fit=fam*100.0+(condition as f32*0.25);
        if fit>best_fit{best_fit=fit;best_bench_idx=Some(idx);}
    }
    best_bench_idx
}
#[cfg(test)]
mod substitution_tests {
    use super::*;
    #[test]
    fn injury_priority_fatigue_cards_and_stable_ties(){
        let p=SubstitutionCandidate{eligible:true,injured:false,goalkeeper:false,yellow_cards:0,aerobic:70.0};
        let players=[p,SubstitutionCandidate{injured:true,goalkeeper:true,..p},SubstitutionCandidate{yellow_cards:1,..p},SubstitutionCandidate{eligible:false,aerobic:1.0,..p}];
        assert_eq!(choose_outgoing(&players,3600,true),Some(1));
        assert_eq!(choose_outgoing(&players,3600,false),Some(2));
        assert_eq!(choose_incoming(&[None,Some((0.8,80)),Some((0.8,80))]),Some(1));
        assert_eq!(choose_incoming(&[None]),None);
    }
}
