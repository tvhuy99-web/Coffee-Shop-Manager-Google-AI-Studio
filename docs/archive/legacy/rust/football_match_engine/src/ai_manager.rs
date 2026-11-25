use std::collections::HashSet;
use crate::{
    clock::Period,
    commands::{CommandType, MatchCommand},
    geometry::TeamSide,
    input::{MatchInput, PlayerMatchSnapshot},
    state::MatchState,
    tactics,
};

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct AiManagerConfig {
    pub control_home: bool,
    pub control_away: bool,
}

impl AiManagerConfig {
    pub fn none() -> Self {
        Self {
            control_home: false,
            control_away: false,
        }
    }

    pub fn away_only() -> Self {
        Self {
            control_home: false,
            control_away: true,
        }
    }

    pub fn is_controlled(&self, side: TeamSide) -> bool {
        match side {
            TeamSide::Home => self.control_home,
            TeamSide::Away => self.control_away,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SideTracker {
    last_scan_second: i32,
    last_tactical_second: i32,
    last_sub_second: i32,
    tactical_mode: i8,
}

#[derive(Clone, Debug)]
pub struct AiManagerDirector {
    pub config: AiManagerConfig,
    home: SideTracker,
    away: SideTracker,
}

impl AiManagerDirector {
    pub fn new(config: AiManagerConfig) -> Self {
        Self {
            config,
            home: SideTracker {
                last_scan_second: -999,
                last_tactical_second: -999,
                last_sub_second: -999,
                tactical_mode: 0,
            },
            away: SideTracker {
                last_scan_second: -999,
                last_tactical_second: -999,
                last_sub_second: -999,
                tactical_mode: 0,
            },
        }
    }

    pub fn evaluate(&mut self, input: &MatchInput, state: &MatchState) -> Vec<MatchCommand> {
        let mut out = Vec::new();
        if state.clock.is_full_time() || state.clock.period == Period::Shootout {
            return out;
        }
        for side in [TeamSide::Home, TeamSide::Away] {
            if !self.config.is_controlled(side) {
                continue;
            }
            self.evaluate_side(side, input, state, &mut out);
        }
        out
    }

    fn tracker_mut(&mut self, side: TeamSide) -> &mut SideTracker {
        match side {
            TeamSide::Home => &mut self.home,
            TeamSide::Away => &mut self.away,
        }
    }

    fn evaluate_side(
        &mut self,
        side: TeamSide,
        input: &MatchInput,
        state: &MatchState,
        out: &mut Vec<MatchCommand>,
    ) {
        let sec = state.clock.game_second;
        let team = state.team(side);

        if team.requires_goalkeeper_assignment {
            if let Some(gk_id) = pick_emergency_gk(side, input, state) {
                if let Ok(cmd) = MatchCommand::assign_goalkeeper(sec, side, gk_id) {
                    out.push(cmd);
                    return;
                }
            }
        }

        let tr = self.tracker_mut(side);
        if sec - tr.last_scan_second < 150 {
            return;
        }
        tr.last_scan_second = sec;

        let minute = (sec / 60).max(1);
        let (own_goals, opp_goals) = match side {
            TeamSide::Home => (state.score.home_goals, state.score.away_goals),
            TeamSide::Away => (state.score.away_goals, state.score.home_goals),
        };
        let goal_diff = own_goals - opp_goals;
        let opp_team = state.team(side.opposite());

        let max_subs = input.match_rules.substitution_limit;
        let max_windows = input.match_rules.substitution_windows;
        let can_sub = team.substitutions_used < max_subs
            && (team.substitution_windows_used < max_windows
                || !team.substitution_consumes_window(
                    sec,
                    state.clock.period,
                    state.clock.period_started_at_second,
                ));
        if can_sub && sec - self.tracker_mut(side).last_sub_second >= 360 {
            if let Some((out_id, in_id)) = choose_substitution(side, input, state, minute) {
                if let Ok(cmd) = MatchCommand::substitute(sec, side, out_id, in_id) {
                    self.tracker_mut(side).last_sub_second = sec;
                    out.push(cmd);
                }
            }
        }

        if sec - self.tracker_mut(side).last_tactical_second < 360 {
            return;
        }
        let ti = &team.team_instructions;
        let desired_mode: i8 = if team.red_cards > 0 {
            -1
        } else if goal_diff < 0 && minute >= 58 {
            1
        } else if goal_diff > 0 && minute >= 74 {
            -1
        } else if goal_diff == 0 && opp_team.shots - team.shots >= 5 && minute >= 35 {
            -1
        } else {
            0
        };

        if desired_mode != self.tracker_mut(side).tactical_mode {
            self.tracker_mut(side).tactical_mode = desired_mode;
            self.tracker_mut(side).last_tactical_second = sec;
            match desired_mode {
                1 => {

                    push_int_cmd(out, sec, side, CommandType::ChangeTempo, (ti.tempo + 14).clamp(55, 88));
                    push_int_cmd(out, sec, side, CommandType::ChangePressing, (ti.pressing_intensity + 12).clamp(60, 88));
                    push_int_cmd(out, sec, side, CommandType::ChangeLineHeight, (ti.line_height + 10).clamp(58, 82));
                    push_team_instr_cmd(out, sec, side, "TIME_WASTING", 0);
                }
                -1 => {

                    push_int_cmd(out, sec, side, CommandType::ChangeLineHeight, (ti.line_height - 14).clamp(26, 46));
                    push_int_cmd(out, sec, side, CommandType::ChangeTempo, (ti.tempo - 10).clamp(32, 52));
                    if goal_diff > 0 && minute >= 74 {
                        push_team_instr_cmd(out, sec, side, "TIME_WASTING", 68);
                    }
                    if team.red_cards > 0 {
                        push_int_cmd(out, sec, side, CommandType::ChangeWidth, 40);
                    }
                }
                _ => {
                    push_int_cmd(out, sec, side, CommandType::ChangeTempo, 52);
                    push_team_instr_cmd(out, sec, side, "TIME_WASTING", 10);
                }
            }
        }
    }
}

fn push_team_instr_cmd(out: &mut Vec<MatchCommand>, sec: i32, side: TeamSide, key: &str, val: i32) {
    if let Ok(cmd) = MatchCommand::new(
        sec,
        side,
        CommandType::ChangeTeamInstruction,
        None,
        None,
        None,
        val,
        Some(key.to_string()),
        Vec::new(),
        None,
    ) {
        out.push(cmd);
    }
}

fn push_int_cmd(out: &mut Vec<MatchCommand>, sec: i32, side: TeamSide, kind: CommandType, val: i32) {
    if let Ok(cmd) = MatchCommand::new(sec, side, kind, None, None, None, val, None, Vec::new(), None) {
        out.push(cmd);
    }
}

fn pick_emergency_gk(side: TeamSide, input: &MatchInput, state: &MatchState) -> Option<String> {
    let team_snap = input.team(side);
    state
        .players
        .iter()
        .filter(|(_, p)| p.side == side && p.on_pitch && !p.red_card && !p.injured)
        .max_by_key(|(id, _)| {
            team_snap
                .player(id)
                .map(|s| s.attributes.get(0))
                .unwrap_or(10)
        })
        .map(|(id, _)| id.clone())
}

fn choose_substitution(
    side: TeamSide,
    input: &MatchInput,
    state: &MatchState,
    minute: i32,
) -> Option<(String, String)> {
    let team_snap = input.team(side);
    let team_rt = state.team(side);
    let gk_id = team_rt.designated_goalkeeper_id.as_deref().unwrap_or("");

    let mut ever_on_pitch: HashSet<&str> = HashSet::new();
    for (id, p) in &state.players {
        if p.side == side {
            ever_on_pitch.insert(id.as_str());
        }
    }
    let available_bench: Vec<&PlayerMatchSnapshot> = team_snap
        .bench
        .iter()
        .filter(|b| b.eligible && !b.injured && !ever_on_pitch.contains(b.player_id.as_str()))
        .collect();
    if available_bench.is_empty() {
        return None;
    }

    let mut worst: Option<(&str, &str, i32)> = None;
    for (id, p) in &state.players {
        if p.side != side || !p.on_pitch || p.red_card || id == gk_id {
            continue;
        }
        let urgency = (100 - p.energy) + p.yellow_cards * 16 + if p.injured { 100 } else { 0 };
        let threshold = if minute >= 72 {
            28
        } else if minute >= 58 {
            38
        } else {
            62
        };
        if urgency >= threshold {
            if worst.map(|(_, _, u)| urgency > u).unwrap_or(true) {
                worst = Some((id.as_str(), p.slot_id.as_str(), urgency));
            }
        }
    }
    let (out_id, slot_id, _) = worst?;
    let slot = team_rt.tactical_slot(slot_id)?;
    let best_in = available_bench
        .iter()
        .max_by_key(|cand| tactics::player_slot_suitability(cand, slot))?;
    Some((out_id.to_string(), best_in.player_id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_disables_both_sides_for_golden_parity() {
        let cfg = AiManagerConfig::none();
        assert!(!cfg.is_controlled(TeamSide::Home));
        assert!(!cfg.is_controlled(TeamSide::Away));
        let away = AiManagerConfig::away_only();
        assert!(!away.is_controlled(TeamSide::Home));
        assert!(away.is_controlled(TeamSide::Away));
    }
}
