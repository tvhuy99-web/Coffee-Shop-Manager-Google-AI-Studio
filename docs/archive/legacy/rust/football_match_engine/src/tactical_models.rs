use crate::{
    decision::Action,
    geometry::TeamSide,
    input::MatchInput,
    roster,
    state::MatchState,
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

pub fn home_perception_adjustment(input: &MatchInput, side: TeamSide) -> f64 {
    if input.neutral_ground { 0.0 } else if side == TeamSide::Home { 0.8 } else { -0.4 }
}

pub fn home_execution_adjustment(input: &MatchInput, side: TeamSide) -> f64 {
    if input.neutral_ground { 0.0 } else if side == TeamSide::Home { 0.9 } else { -0.45 }
}

pub fn home_pressure_adjustment(input: &MatchInput, defending_side: TeamSide) -> i32 {
    if input.neutral_ground { 0 } else if defending_side == TeamSide::Home { 2 } else { -1 }
}

pub fn home_shot_quality_adjustment(input: &MatchInput, side: TeamSide) -> f64 {
    if input.neutral_ground { 0.0 } else if side == TeamSide::Home { 1.6 } else { -0.8 }
}

pub fn outfield_deficit(state: &MatchState, side: TeamSide) -> i32 {
    let gk_id = state.team(side).designated_goalkeeper_id.as_deref();
    let mut active = 0;
    for (_, p) in &state.players {
        if p.on_pitch && p.side == side && Some(p.player_id.as_str()) != gk_id {
            active += 1;
        }
    }
    (10 - active).max(0)
}

pub fn numerical_pressure_penalty(state: &MatchState, side: TeamSide) -> i32 {
    outfield_deficit(state, side) * 3
}

pub fn defensive_disorganization(state: &MatchState, defending_side: TeamSide) -> i32 {
    if !state.possession.transition_active {
        return 0;
    }
    if state.possession.controlling_team != Some(defending_side.opposite()) {
        return 0;
    }
    let instructions = &state.team(defending_side).team_instructions;
    let gk_id = state.team(defending_side).designated_goalkeeper_id.as_deref();
    let mut passing_risk = 0;
    let mut outfield = 0;
    for (_, p) in &state.players {
        if !p.on_pitch || p.side != defending_side || Some(p.player_id.as_str()) == gk_id {
            continue;
        }
        if let Some(slot) = roster::slot_instructions(state, defending_side, &p.player_id) {
            passing_risk += slot.passing_risk;
            outfield += 1;
        }
    }
    let avg_passing_risk = if outfield == 0 { 50 } else { passing_risk / outfield };
    let value = (instructions.build_up_risk - 50).max(0) as f64 * 0.16
        + (instructions.transition_risk - 50).max(0) as f64 * 0.14
        + (instructions.pressing_intensity - 50).max(0) as f64 * 0.10
        + (avg_passing_risk - 50).max(0) as f64 * 0.28;
    java_round(value).clamp(0, 16)
}

pub fn effective_time_wasting(input: &MatchInput, state: &MatchState, side: TeamSide) -> i32 {
    let instruction = state.team(side).team_instructions.time_wasting;
    let (own, opp) = match side {
        TeamSide::Home => (state.score.home_goals, state.score.away_goals),
        TeamSide::Away => (state.score.away_goals, state.score.home_goals),
    };
    let regulation = input.match_rules.regulation_duration_seconds.max(1);
    let ratio = (state.clock.game_second as f64 / regulation as f64).clamp(0.0, 1.20);
    effective_time_wasting_raw(instruction, own - opp, ratio)
}

pub fn effective_time_wasting_raw(instruction: i32, score_difference: i32, match_progress: f64) -> i32 {
    let value = instruction.clamp(0, 100);
    if value == 0 {
        return 0;
    }
    if value >= 80 {
        return value;
    }
    if score_difference <= 0 {
        return 0;
    }
    let late = ((match_progress - 0.45) / 0.55).clamp(0.0, 1.0);
    java_round(value as f64 * (0.35 + late * 0.65))
}

#[derive(Clone, Copy, Debug)]
pub struct MatchDecisionContext {
    pub score_difference: i32,
    pub late_factor: f64,
    pub urgency: f64,
    pub control: f64,
}

impl MatchDecisionContext {
    pub fn from(input: &MatchInput, state: &MatchState, side: TeamSide) -> Self {
        let (own, opp) = match side {
            TeamSide::Home => (state.score.home_goals, state.score.away_goals),
            TeamSide::Away => (state.score.away_goals, state.score.home_goals),
        };
        let diff = own - opp;
        let regulation = input.match_rules.regulation_duration_seconds.max(1);
        let ratio = (state.clock.game_second as f64 / regulation as f64).clamp(0.0, 1.20);
        let late = ((ratio - 0.58) / 0.42).clamp(0.0, 1.0);
        let urgency = if diff < 0 { late * ((-diff) as f64 * 0.72).min(1.0) } else { 0.0 };
        let control = if diff > 0 { late * (diff as f64 * 0.72).min(1.0) } else { 0.0 };
        Self {
            score_difference: diff,
            late_factor: late,
            urgency,
            control,
        }
    }

    pub fn risk_adjustment(&self) -> i32 {
        java_round(self.urgency * 13.0 - self.control * 11.0)
    }

    pub fn action_adjustment(&self, kind: Action, progress_gain: i32) -> f64 {
        match kind {
            Action::Pass => {
                if progress_gain <= 2 {
                    self.control * 5.5 - self.urgency * 2.2
                } else {
                    self.urgency * 2.7 - self.control * 1.8
                }
            }
            Action::ThroughBall => self.urgency * 6.0 - self.control * 5.5,
            Action::Carry => self.urgency * 2.0 - self.control * 1.5,
            Action::Dribble => self.urgency * 3.4 - self.control * 4.0,
            Action::Cross => self.urgency * 5.0 - self.control * 5.0,
            Action::Shot => self.urgency * 4.8 - self.control * 3.8,
        }
    }

    pub fn perceived_match_state_adjustment(&self, kind: Action, progress_gain: i32, decisions: i32) -> f64 {
        let recognition = (0.38 + decisions.clamp(1, 99) as f64 * 0.0062).min(1.0);
        self.action_adjustment(kind, progress_gain) * recognition
    }
}
