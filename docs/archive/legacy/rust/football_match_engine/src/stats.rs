use crate::{
    events::{EventType, MatchEvent},
    fingerprints as fp,
    geometry::TeamSide,
    input::{MatchInput, TeamMatchSnapshot},
    state::MatchState,
};

#[inline]
fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerMatchStatistics {
    pub player_id: String,
    pub side: TeamSide,
    pub minutes_played: i32,
    pub goals: i32,
    pub assists: i32,
    pub shots: i32,
    pub shots_on_target: i32,
    pub expected_goals_milli: i32,
    pub passes_attempted: i32,
    pub passes_completed: i32,
    pub progressive_passes: i32,
    pub crosses: i32,
    pub dribbles_attempted: i32,
    pub dribbles_won: i32,
    pub tackles_attempted: i32,
    pub tackles_won: i32,
    pub interceptions: i32,
    pub blocks: i32,
    pub clearances: i32,
    pub saves: i32,
    pub fouls: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub offsides: i32,
    pub turnovers: i32,
    pub rating_hundredths: i32,
    pub goalkeeper_duty: bool,
}

impl PlayerMatchStatistics {
    pub fn rating(&self) -> f64 { self.rating_hundredths as f64 / 100.0 }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_string(h, Some(&self.player_id));
        let vals = [
            self.side as i32, self.minutes_played, self.goals, self.assists, self.shots,
            self.shots_on_target, self.expected_goals_milli, self.passes_attempted,
            self.passes_completed, self.progressive_passes, self.crosses, self.dribbles_attempted,
            self.dribbles_won, self.tackles_attempted, self.tackles_won, self.interceptions,
            self.blocks, self.clearances, self.saves, self.fouls, self.yellow_cards,
            self.red_cards, self.offsides, self.turnovers, self.rating_hundredths,
        ];
        for v in vals {
            h = fp::mix_i32(h, v);
        }
        fp::mix_bool(h, self.goalkeeper_duty)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamMatchStatistics {
    pub side: TeamSide,
    pub possession_seconds: i32,
    pub shots: i32,
    pub shots_on_target: i32,
    pub goals: i32,
    pub expected_goals_milli: i32,
    pub passes_attempted: i32,
    pub passes_completed: i32,
    pub progressive_passes: i32,
    pub crosses: i32,
    pub dribbles_attempted: i32,
    pub dribbles_won: i32,
    pub tackles_attempted: i32,
    pub tackles_won: i32,
    pub interceptions: i32,
    pub blocks: i32,
    pub clearances: i32,
    pub saves: i32,
    pub fouls: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub offsides: i32,
    pub corners: i32,
    pub penalties: i32,
}

impl TeamMatchStatistics {
    pub fn possession_percent(&self, total: i32) -> f64 {
        if total <= 0 { 50.0 } else { 100.0 * self.possession_seconds as f64 / total as f64 }
    }

    pub fn expected_goals(&self) -> f64 { self.expected_goals_milli as f64 / 1000.0 }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        let vals = [
            self.side as i32, self.possession_seconds, self.shots, self.shots_on_target,
            self.goals, self.expected_goals_milli, self.passes_attempted, self.passes_completed,
            self.progressive_passes, self.crosses, self.dribbles_attempted, self.dribbles_won,
            self.tackles_attempted, self.tackles_won, self.interceptions, self.blocks,
            self.clearances, self.saves, self.fouls, self.yellow_cards, self.red_cards,
            self.offsides, self.corners, self.penalties,
        ];
        for v in vals {
            h = fp::mix_i32(h, v);
        }
        h
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchStatistics {
    pub home: TeamMatchStatistics,
    pub away: TeamMatchStatistics,
    pub players: Vec<(String, PlayerMatchStatistics)>,
    pub possession_tracked_seconds: i32,
}

impl MatchStatistics {
    pub fn player(&self, id: &str) -> Option<&PlayerMatchStatistics> {
        self.players.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i64(h, self.home.fingerprint() as i64);
        h = fp::mix_i64(h, self.away.fingerprint() as i64);
        h = fp::mix_i32(h, self.possession_tracked_seconds);
        for (k, v) in &self.players {
            h = fp::mix_string(h, Some(k));
            h = fp::mix_i64(h, v.fingerprint() as i64);
        }
        h
    }
}

#[derive(Clone, Debug)]
struct AssistCandidate {
    passer_id: String,
    target_id: String,
    second: i32,
}

#[derive(Clone, Debug)]
struct TeamBuf {
    side: TeamSide,
    possession_seconds: i32,
    shots: i32,
    shots_on_target: i32,
    goals: i32,
    xg_milli: i32,
    passes_attempted: i32,
    passes_completed: i32,
    progressive_passes: i32,
    crosses: i32,
    dribbles_attempted: i32,
    dribbles_won: i32,
    tackles_attempted: i32,
    tackles_won: i32,
    interceptions: i32,
    blocks: i32,
    clearances: i32,
    saves: i32,
    fouls: i32,
    yellow: i32,
    red: i32,
    offsides: i32,
    corners: i32,
    penalties: i32,
}

impl TeamBuf {
    fn new(side: TeamSide) -> Self {
        Self {
            side, possession_seconds: 0, shots: 0, shots_on_target: 0, goals: 0, xg_milli: 0,
            passes_attempted: 0, passes_completed: 0, progressive_passes: 0, crosses: 0,
            dribbles_attempted: 0, dribbles_won: 0, tackles_attempted: 0, tackles_won: 0,
            interceptions: 0, blocks: 0, clearances: 0, saves: 0, fouls: 0, yellow: 0, red: 0,
            offsides: 0, corners: 0, penalties: 0,
        }
    }

    fn freeze(&self) -> TeamMatchStatistics {
        TeamMatchStatistics {
            side: self.side,
            possession_seconds: self.possession_seconds,
            shots: self.shots,
            shots_on_target: self.shots_on_target,
            goals: self.goals,
            expected_goals_milli: self.xg_milli,
            passes_attempted: self.passes_attempted,
            passes_completed: self.passes_completed,
            progressive_passes: self.progressive_passes,
            crosses: self.crosses,
            dribbles_attempted: self.dribbles_attempted,
            dribbles_won: self.dribbles_won,
            tackles_attempted: self.tackles_attempted,
            tackles_won: self.tackles_won,
            interceptions: self.interceptions,
            blocks: self.blocks,
            clearances: self.clearances,
            saves: self.saves,
            fouls: self.fouls,
            yellow_cards: self.yellow,
            red_cards: self.red,
            offsides: self.offsides,
            corners: self.corners,
            penalties: self.penalties,
        }
    }
}

#[derive(Clone, Debug)]
struct PlayerBuf {
    id: String,
    side: TeamSide,
    slot_id: Option<String>,
    goalkeeper_duty: bool,
    seconds: i32,
    goals: i32,
    assists: i32,
    shots: i32,
    shots_on_target: i32,
    xg_milli: i32,
    passes_attempted: i32,
    passes_completed: i32,
    progressive_passes: i32,
    crosses: i32,
    dribbles_attempted: i32,
    dribbles_won: i32,
    tackles_attempted: i32,
    tackles_won: i32,
    interceptions: i32,
    blocks: i32,
    clearances: i32,
    saves: i32,
    fouls: i32,
    yellow: i32,
    red: i32,
    offsides: i32,
    turnovers: i32,
}

impl PlayerBuf {
    fn new(id: String, side: TeamSide) -> Self {
        Self {
            id, side, slot_id: None, goalkeeper_duty: false, seconds: 0, goals: 0, assists: 0,
            shots: 0, shots_on_target: 0, xg_milli: 0, passes_attempted: 0, passes_completed: 0,
            progressive_passes: 0, crosses: 0, dribbles_attempted: 0, dribbles_won: 0,
            tackles_attempted: 0, tackles_won: 0, interceptions: 0, blocks: 0, clearances: 0,
            saves: 0, fouls: 0, yellow: 0, red: 0, offsides: 0, turnovers: 0,
        }
    }

    fn freeze(&self, mins: i32, rating: i32) -> PlayerMatchStatistics {
        PlayerMatchStatistics {
            player_id: self.id.clone(),
            side: self.side,
            minutes_played: mins,
            goals: self.goals,
            assists: self.assists,
            shots: self.shots,
            shots_on_target: self.shots_on_target,
            expected_goals_milli: self.xg_milli,
            passes_attempted: self.passes_attempted,
            passes_completed: self.passes_completed,
            progressive_passes: self.progressive_passes,
            crosses: self.crosses,
            dribbles_attempted: self.dribbles_attempted,
            dribbles_won: self.dribbles_won,
            tackles_attempted: self.tackles_attempted,
            tackles_won: self.tackles_won,
            interceptions: self.interceptions,
            blocks: self.blocks,
            clearances: self.clearances,
            saves: self.saves,
            fouls: self.fouls,
            yellow_cards: self.yellow,
            red_cards: self.red,
            offsides: self.offsides,
            turnovers: self.turnovers,
            rating_hundredths: rating,
            goalkeeper_duty: self.goalkeeper_duty,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatsCollector {
    home: TeamBuf,
    away: TeamBuf,
    players: Vec<(String, PlayerBuf)>,
    active_since: Vec<(String, i32)>,
    control: Option<TeamSide>,
    control_started: i32,
    possession_tracked: i32,
    home_assist: Option<AssistCandidate>,
    away_assist: Option<AssistCandidate>,
    finished: bool,
}

impl StatsCollector {
    pub fn new(input: &MatchInput) -> Self {
        let mut players = Vec::with_capacity(40);
        Self::add_players(&mut players, &input.home_team, TeamSide::Home);
        Self::add_players(&mut players, &input.away_team, TeamSide::Away);
        let mut active_since = Vec::with_capacity(22);
        for id in input.home_team.lineup_player_ids_in_slot_order() {
            active_since.push((id, 0));
        }
        for id in input.away_team.lineup_player_ids_in_slot_order() {
            active_since.push((id, 0));
        }
        Self {
            home: TeamBuf::new(TeamSide::Home),
            away: TeamBuf::new(TeamSide::Away),
            players,
            active_since,
            control: None,
            control_started: 0,
            possession_tracked: 0,
            home_assist: None,
            away_assist: None,
            finished: false,
        }
    }

    fn add_players(out: &mut Vec<(String, PlayerBuf)>, t: &TeamMatchSnapshot, side: TeamSide) {
        for x in &t.lineup {
            let mut b = PlayerBuf::new(x.player_id.clone(), side);
            b.slot_id = t.slot_for_player(&x.player_id).map(|s| s.slot_id.clone());
            b.goalkeeper_duty = x.player_id == t.designated_goalkeeper_id;
            out.push((x.player_id.clone(), b));
        }
        for x in &t.bench {
            out.push((x.player_id.clone(), PlayerBuf::new(x.player_id.clone(), side)));
        }
    }

    fn pb_mut(&mut self, id: Option<&str>) -> Option<&mut PlayerBuf> {
        let id = id?;
        self.players.iter_mut().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    fn close_possession(&mut self, sec: i32) {
        if let Some(c) = self.control.take() {
            let d = (sec - self.control_started).max(0);
            match c {
                TeamSide::Home => self.home.possession_seconds += d,
                TeamSide::Away => self.away.possession_seconds += d,
            }
            self.possession_tracked += d;
        }
    }

    fn finish_active(&mut self, id: Option<&str>, sec: i32) {
        let id = match id {
            Some(i) => i,
            None => return,
        };
        if let Some(pos) = self.active_since.iter().position(|(k, _)| k == id) {
            let (_, start) = self.active_since.remove(pos);
            if let Some(b) = self.pb_mut(Some(id)) {
                b.seconds += (sec - start).max(0);
            }
        }
    }

    fn iv(e: &MatchEvent, key: &str, default: i32) -> i32 {
        e.meta(key).and_then(|v| v.parse::<i32>().ok()).unwrap_or(default)
    }

    pub fn accept(&mut self, e: &MatchEvent) {
        if self.finished {
            return;
        }
        let sec = e.game_second;
        if e.kind == EventType::PossessionStarted {
            if self.control.is_some() && self.control != e.side {
                self.close_possession(sec);
            }
            if self.control != e.side {
                self.control = e.side;
                self.control_started = sec;
            }
        } else if matches!(
            e.kind,
            EventType::RestartScheduled | EventType::HalfTime | EventType::ExtraTimeHalfTime | EventType::FullTime
        ) {
            self.close_possession(sec);
        }

        let side = e.side;
        let actor = e.actor.as_deref();
        let target = e.target.as_deref();

        match e.kind {
            EventType::PassStarted | EventType::ThroughBall => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.passes_attempted += 1, TeamSide::Away => self.away.passes_attempted += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.passes_attempted += 1; }
            }
            EventType::PassCompleted => {
                let pg = Self::iv(e, "progressGain", 0);
                if let Some(s) = side {
                    let tb = match s { TeamSide::Home => &mut self.home, TeamSide::Away => &mut self.away };
                    tb.passes_completed += 1;
                    if pg >= 10 { tb.progressive_passes += 1; }
                }
                if let Some(a) = self.pb_mut(actor) {
                    a.passes_completed += 1;
                    if pg >= 10 { a.progressive_passes += 1; }
                }
                if let (Some(p), Some(t)) = (actor, target) {
                    let cand = AssistCandidate { passer_id: p.into(), target_id: t.into(), second: sec };
                    match side {
                        Some(TeamSide::Home) => self.home_assist = Some(cand),
                        Some(TeamSide::Away) => self.away_assist = Some(cand),
                        None => {}
                    }
                }
            }
            EventType::PassFailed => {
                if let Some(a) = self.pb_mut(actor) { a.turnovers += 1; }
            }
            EventType::Cross => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.crosses += 1, TeamSide::Away => self.away.crosses += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.crosses += 1; }
                if let (Some(p), Some(t)) = (actor, target) {
                    let cand = AssistCandidate { passer_id: p.into(), target_id: t.into(), second: sec };
                    match side {
                        Some(TeamSide::Home) => self.home_assist = Some(cand),
                        Some(TeamSide::Away) => self.away_assist = Some(cand),
                        None => {}
                    }
                }
            }
            EventType::DribbleStarted => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.dribbles_attempted += 1, TeamSide::Away => self.away.dribbles_attempted += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.dribbles_attempted += 1; }
            }
            EventType::DribbleWon => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.dribbles_won += 1, TeamSide::Away => self.away.dribbles_won += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.dribbles_won += 1; }
            }
            EventType::DribbleLost => {
                if let Some(a) = self.pb_mut(actor) { a.turnovers += 1; }
            }
            EventType::TackleAttempted => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.tackles_attempted += 1, TeamSide::Away => self.away.tackles_attempted += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.tackles_attempted += 1; }
            }
            EventType::TackleWon => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.tackles_won += 1, TeamSide::Away => self.away.tackles_won += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.tackles_won += 1; }
            }
            EventType::Interception => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.interceptions += 1, TeamSide::Away => self.away.interceptions += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.interceptions += 1; }
            }
            EventType::Block => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.blocks += 1, TeamSide::Away => self.away.blocks += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.blocks += 1; }
            }
            EventType::Clearance => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.clearances += 1, TeamSide::Away => self.away.clearances += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.clearances += 1; }
            }
            EventType::Shot | EventType::Header => {
                let xg = Self::iv(e, "xgMilli", 0);
                if let Some(s) = side {
                    let tb = match s { TeamSide::Home => &mut self.home, TeamSide::Away => &mut self.away };
                    tb.shots += 1;
                    tb.xg_milli += xg;
                }
                if let Some(a) = self.pb_mut(actor) {
                    a.shots += 1;
                    a.xg_milli += xg;
                }
            }
            EventType::ShotOnTarget => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.shots_on_target += 1, TeamSide::Away => self.away.shots_on_target += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.shots_on_target += 1; }
            }
            EventType::Goal => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.goals += 1, TeamSide::Away => self.away.goals += 1 } }
                let is_own_goal = e.meta("ownGoal") == Some("true");
                let is_penalty = e.meta("source") == Some("PENALTY");

                if !is_own_goal {
                    if let Some(a) = self.pb_mut(actor) {
                        a.goals += 1;
                    }
                    if !is_penalty {
                        let assist_passer = match side {
                            Some(TeamSide::Home) => self.home_assist.as_ref(),
                            Some(TeamSide::Away) => self.away_assist.as_ref(),
                            None => None,
                        }
                        .and_then(|ac| {
                            if Some(ac.target_id.as_str()) == actor
                                && sec - ac.second <= 42
                                && Some(ac.passer_id.as_str()) != actor
                            {
                                Some(ac.passer_id.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(pid) = assist_passer {
                            if let Some(ap) = self.pb_mut(Some(&pid)) {
                                ap.assists += 1;
                            }
                        }
                    }
                }
                match side {
                    Some(TeamSide::Home) => self.home_assist = None,
                    Some(TeamSide::Away) => self.away_assist = None,
                    None => {}
                }
            }
            EventType::Save => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.saves += 1, TeamSide::Away => self.away.saves += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.saves += 1; }
            }
            EventType::Foul => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.fouls += 1, TeamSide::Away => self.away.fouls += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.fouls += 1; }
            }
            EventType::YellowCard => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.yellow += 1, TeamSide::Away => self.away.yellow += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.yellow += 1; }
            }
            EventType::RedCard => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.red += 1, TeamSide::Away => self.away.red += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.red += 1; }
                self.finish_active(actor, sec);
            }
            EventType::Offside => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.offsides += 1, TeamSide::Away => self.away.offsides += 1 } }
                if let Some(a) = self.pb_mut(actor) { a.offsides += 1; }
            }
            EventType::Corner => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.corners += 1, TeamSide::Away => self.away.corners += 1 } }
                if let (Some(p), Some(t)) = (actor, target) {
                    let cand = AssistCandidate { passer_id: p.into(), target_id: t.into(), second: sec };
                    match side {
                        Some(TeamSide::Home) => self.home_assist = Some(cand),
                        Some(TeamSide::Away) => self.away_assist = Some(cand),
                        None => {}
                    }
                }
            }
            EventType::FreeKick => {
                if e.meta("delivery") == Some("true") {
                    if let (Some(p), Some(t)) = (actor, target) {
                        let cand = AssistCandidate { passer_id: p.into(), target_id: t.into(), second: sec };
                        match side {
                            Some(TeamSide::Home) => self.home_assist = Some(cand),
                            Some(TeamSide::Away) => self.away_assist = Some(cand),
                            None => {}
                        }
                    }
                }
            }
            EventType::PenaltyAwarded => {
                if let Some(s) = side { match s { TeamSide::Home => self.home.penalties += 1, TeamSide::Away => self.away.penalties += 1 } }
            }
            EventType::Injury => {
                if e.meta("unableToContinue") == Some("true") {
                    self.finish_active(actor, sec);
                }
            }
            EventType::Substitution => {
                self.finish_active(actor, sec);
                if let Some(t) = target {
                    if let Some(entry) = self.active_since.iter_mut().find(|(k, _)| k == t) {
                        entry.1 = sec;
                    } else {
                        self.active_since.push((t.into(), sec));
                    }
                    let slot_id = e.meta("slotId").map(|s| s.to_string());
                    let gk_duty = e.meta("goalkeeperDuty") == Some("true");
                    if let Some(incoming) = self.pb_mut(Some(t)) {
                        incoming.slot_id = slot_id;
                        if gk_duty { incoming.goalkeeper_duty = true; }
                    }
                }
            }
            EventType::GoalkeeperReassigned => {
                if let Some(a) = self.pb_mut(actor) { a.goalkeeper_duty = true; }
            }
            EventType::CommandAccepted => {
                if e.meta("commandType") == Some("REASSIGN_SLOT") {
                    if let Some(sid) = e.meta("slotId").map(|s| s.to_string()) {
                        if let Some(a) = self.pb_mut(actor) { a.slot_id = Some(sid); }
                    }
                }
            }
            EventType::Turnover => {
                match side {
                    Some(TeamSide::Home) => self.away_assist = None,
                    Some(TeamSide::Away) => self.home_assist = None,
                    None => {}
                }
            }
            _ => {}
        }

        if e.kind == EventType::RestartScheduled {
            self.home_assist = None;
            self.away_assist = None;
        }
    }

    pub fn finish(&mut self, input: &MatchInput, final_state: &MatchState) -> MatchStatistics {
        let end = final_state.clock.game_second;
        self.close_possession(end);
        let remaining: Vec<(String, i32)> = self.active_since.drain(..).collect();
        for (id, start) in remaining {
            if let Some(b) = self.pb_mut(Some(&id)) {
                b.seconds += (end - start).max(0);
            }
        }
        let mut out = Vec::with_capacity(self.players.len());
        for (_, b) in &self.players {
            let mins = if b.seconds <= 0 {
                0
            } else {
                java_round(b.seconds as f64 / 60.0).max(1)
            };
            let rating = compute_rating(b, mins, input);
            out.push((b.id.clone(), b.freeze(mins, rating)));
        }
        self.finished = true;
        MatchStatistics {
            home: self.home.freeze(),
            away: self.away.freeze(),
            players: out,
            possession_tracked_seconds: self.possession_tracked,
        }
    }
}

fn compute_rating(b: &PlayerBuf, mins: i32, input: &MatchInput) -> i32 {
    if mins <= 0 {
        return 600;
    }
    let team = input.team(b.side);
    let slot = b.slot_id.as_deref().and_then(|s| team.slot_instructions(s));
    let goalkeeper = b.goalkeeper_duty || b.id == team.designated_goalkeeper_id;
    let attack_expectation = slot.map(|s| (s.forward_movement + s.shooting_tendency) as f64 / 200.0).unwrap_or(0.5);
    let create_expectation = slot.map(|s| (s.passing_risk + s.support_distance + s.roaming) as f64 / 300.0).unwrap_or(0.5);
    let defend_expectation = slot.map(|s| s.defensive_aggression as f64 / 100.0).unwrap_or(0.5);
    let risk_allowance = slot.map(|s| s.passing_risk as f64 / 100.0).unwrap_or(0.0);
    let mut r = 6.0;
    r += b.goals as f64 * (0.58 + 0.18 * attack_expectation)
        + b.assists as f64 * (0.38 + 0.16 * create_expectation);
    r += b.shots_on_target as f64 * (0.020 + 0.018 * attack_expectation)
        + b.progressive_passes as f64 * (0.004 + 0.006 * create_expectation)
        + b.passes_completed as f64 * 0.0012;
    r += b.dribbles_won as f64 * 0.020
        + b.tackles_won as f64 * (0.018 + 0.018 * defend_expectation)
        + b.interceptions as f64 * (0.020 + 0.020 * defend_expectation)
        + b.blocks as f64 * (0.020 + 0.016 * defend_expectation)
        + b.clearances as f64 * (0.008 + 0.010 * defend_expectation);
    if goalkeeper {
        r += b.saves as f64 * 0.065;
    } else {
        r += b.saves as f64 * 0.025;
    }
    r -= b.turnovers as f64 * (0.012 - 0.004 * risk_allowance)
        + b.fouls as f64 * 0.022
        + b.yellow as f64 * 0.15
        + b.red as f64 * 0.70
        + b.offsides as f64 * 0.012;
    let xg = b.xg_milli as f64 / 1000.0;
    if (b.goals as f64) > xg {
        r += ((b.goals as f64 - xg) * 0.11).min(0.32);
    } else if xg - b.goals as f64 > 0.75 {
        r -= ((xg - b.goals as f64) * 0.06).min(0.22);
    }
    if mins < 25 {
        r = 6.0 + (r - 6.0) * (mins as f64 / 25.0);
    }
    java_round((r * 100.0).clamp(400.0, 1000.0))
}

#[derive(Clone, Debug)]
pub struct MatchResult {
    pub match_id: String,
    pub engine_version: String,
    pub seed: i64,
    pub input_fingerprint: u64,
    pub competition_context: String,
    pub home_goals: i32,
    pub away_goals: i32,
    pub home_shootout_goals: i32,
    pub away_shootout_goals: i32,
    pub final_game_second: i32,
    pub final_state: MatchState,
    pub events: Vec<MatchEvent>,
    pub statistics: MatchStatistics,
    pub event_count: i32,
    pub event_timeline_fingerprint: u64,
    pub full_event_timeline_retained: bool,
    pub fingerprint: u64,
}

impl MatchResult {
    pub fn new(
        input: &MatchInput,
        state: &MatchState,
        events: Vec<MatchEvent>,
        statistics: MatchStatistics,
        event_count: i32,
        event_timeline_fingerprint: u64,
        full_event_timeline_retained: bool,
    ) -> Self {
        let input_fingerprint = input.fingerprint();
        let home_goals = state.score.home_goals;
        let away_goals = state.score.away_goals;
        let home_shootout_goals = state.score.home_shootout_goals;
        let away_shootout_goals = state.score.away_shootout_goals;
        let final_game_second = state.clock.game_second;
        let final_state = state.clone();
        let event_count = event_count.max(0);

        let mut h = fp::start();
        h = fp::mix_string(h, Some(&input.match_id));
        h = fp::mix_string(h, Some(&input.engine_version));
        h = fp::mix_i64(h, input.seed);
        h = fp::mix_i64(h, input_fingerprint as i64);
        h = fp::mix_string(h, Some(&input.competition_context));
        h = fp::mix_i32(h, home_goals);
        h = fp::mix_i32(h, away_goals);
        h = fp::mix_i32(h, home_shootout_goals);
        h = fp::mix_i32(h, away_shootout_goals);
        h = fp::mix_i32(h, final_game_second);
        h = fp::mix_i64(h, final_state.fingerprint() as i64);
        h = fp::mix_i64(h, statistics.fingerprint() as i64);
        h = fp::mix_i32(h, event_count);
        let fingerprint = fp::mix_i64(h, event_timeline_fingerprint as i64);

        Self {
            match_id: input.match_id.clone(),
            engine_version: input.engine_version.clone(),
            seed: input.seed,
            input_fingerprint,
            competition_context: input.competition_context.clone(),
            home_goals,
            away_goals,
            home_shootout_goals,
            away_shootout_goals,
            final_game_second,
            final_state,
            events,
            statistics,
            event_count,
            event_timeline_fingerprint,
            full_event_timeline_retained,
            fingerprint,
        }
    }
}
