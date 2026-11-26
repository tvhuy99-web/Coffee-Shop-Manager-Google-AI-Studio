use std::fmt::Write;
use crate::{
    ai_manager::AiManagerConfig,
    player_ratings::{self, MatchRatingsSummary, PlayerMatchRating},
    commands::{CommandType, MatchCommand},
    events::MatchEvent,
    geometry::TeamSide,
    input::*,
    rules::MatchRules,
    session::{MatchSession, ENGINE_VERSION},
    snapshot::PlayerAttributesSnapshot,
    stats::{MatchResult, TeamMatchStatistics},
};

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JsonValue>),
    Obj(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn as_obj(&self) -> Option<&[(String, JsonValue)]> {
        match self { Self::Obj(v) => Some(v), _ => None }
    }
    pub fn as_arr(&self) -> Option<&[JsonValue]> {
        match self { Self::Arr(v) => Some(v), _ => None }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self { Self::Str(s) => Some(s), _ => None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self { Self::Bool(b) => Some(*b), _ => None }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self { Self::Num(n) => Some(*n as i64), _ => None }
    }
    pub fn as_i32(&self) -> Option<i32> {
        self.as_i64().map(|v| v as i32)
    }
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_obj()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }
    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|v| v.as_i32())
    }
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_i64())
    }
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self { Self { bytes: s.as_bytes(), pos: 0 } }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b' ' | b'\n' | b'\r' | b'\t') {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        match self.peek().ok_or_else(|| "unexpected EOF".to_string())? {
            b'{' => self.parse_obj(),
            b'[' => self.parse_arr(),
            b'"' => self.parse_string().map(JsonValue::Str),
            b't' => self.expect_lit(b"true").map(|_| JsonValue::Bool(true)),
            b'f' => self.expect_lit(b"false").map(|_| JsonValue::Bool(false)),
            b'n' => self.expect_lit(b"null").map(|_| JsonValue::Null),
            b'-' | b'0'..=b'9' => self.parse_number(),
            c => Err(format!("unexpected char '{}' at {}", c as char, self.pos)),
        }
    }

    fn expect_lit(&mut self, lit: &[u8]) -> Result<(), String> {
        self.skip_ws();
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(())
        } else {
            Err(format!("expected literal at {}", self.pos))
        }
    }

    fn parse_obj(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut entries = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Obj(entries));
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(format!("expected string key at {}", self.pos));
            }
            let key = self.parse_string()?;
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at {}", self.pos));
            }
            self.pos += 1;
            let val = self.parse_value()?;
            entries.push((key, val));
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => { self.pos += 1; break; }
                _ => return Err(format!("expected ',' or '}}' at {}", self.pos)),
            }
        }
        Ok(JsonValue::Obj(entries))
    }

    fn parse_arr(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Arr(items));
        }
        loop {
            items.push(self.parse_value()?);
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => { self.pos += 1; break; }
                _ => return Err(format!("expected ',' or ']' at {}", self.pos)),
            }
        }
        Ok(JsonValue::Arr(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.skip_ws();
        if self.bytes.get(self.pos) != Some(&b'"') {
            return Err(format!("expected '\"' at {}", self.pos));
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let esc = *self.bytes.get(self.pos).ok_or_else(|| "unterminated escape".to_string())?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if self.pos + 4 > self.bytes.len() {
                            return Err("truncated unicode escape".into());
                        }
                        let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
                            .map_err(|_| "invalid utf8 in unicode escape".to_string())?;
                        let code = u32::from_str_radix(hex, 16)
                            .map_err(|_| "invalid hex in unicode escape".to_string())?;
                        self.pos += 4;
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                    }
                    _ => return Err(format!("unknown escape '\\{}'", esc as char)),
                }
            } else {
                let rest = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "invalid utf8 in json string".to_string())?;
                let ch = rest.chars().next().ok_or_else(|| "eof in string".to_string())?;
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("unterminated string".into())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "invalid number utf8".to_string())?;
        let n = s.parse::<f64>().map_err(|_| format!("invalid number '{}'", s))?;
        Ok(JsonValue::Num(n))
    }
}

pub fn parse_json(s: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(s);
    p.parse_value()
}

pub fn escape_json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn parse_side(s: &str) -> Result<TeamSide, String> {
    match s {
        "HOME" => Ok(TeamSide::Home),
        "AWAY" => Ok(TeamSide::Away),
        _ => Err(format!("invalid team side: {}", s)),
    }
}

fn parse_string_list(val: Option<&JsonValue>) -> Vec<String> {
    val.and_then(|v| v.as_arr())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

fn parse_player(v: &JsonValue) -> Result<PlayerMatchSnapshot, String> {
    let id = v.get_str("playerId").ok_or("playerId missing")?;
    let name = v.get_str("displayName").unwrap_or(id);
    let attr_arr = v.get("attributes").and_then(|a| a.as_arr()).ok_or("attributes array missing")?;
    if attr_arr.len() != 20 {
        return Err("attributes must have 20 values".into());
    }
    let mut vals = [74u8; 20];
    for (i, item) in attr_arr.iter().enumerate() {
        vals[i] = item.as_i32().ok_or("attribute must be int")?.clamp(1, 99) as u8;
    }
    let attrs = PlayerAttributesSnapshot::new(vals).map_err(|e| e.to_string())?;
    PlayerMatchSnapshot::new(
        id,
        name,
        attrs,
        v.get_i32("condition").unwrap_or(100),
        v.get_i32("morale").unwrap_or(65),
        v.get_i32("form").unwrap_or(55),
        v.get_i32("age").unwrap_or(24),
        v.get_bool("injured").unwrap_or(false),
        v.get_bool("eligible").unwrap_or(true),
        v.get_str("primaryPosition").unwrap_or(""),
        v.get_str("secondaryPosition").unwrap_or(""),
    )
    .map_err(|e| e.to_string())
}

fn parse_team(v: &JsonValue) -> Result<TeamMatchSnapshot, String> {
    let team_id = v.get_str("teamId").ok_or("teamId missing")?;
    let display_name = v.get_str("displayName").unwrap_or(team_id);
    let shape_id = v.get_str("shapeId").unwrap_or("433-v3");
    let gk_id = v.get_str("designatedGoalkeeperId").ok_or("designatedGoalkeeperId missing")?;

    let lineup_arr = v.get("lineup").and_then(|a| a.as_arr()).ok_or("lineup missing")?;
    let mut lineup = Vec::with_capacity(lineup_arr.len());
    for item in lineup_arr {
        lineup.push(parse_player(item)?);
    }
    let mut bench = Vec::new();
    if let Some(bench_arr) = v.get("bench").and_then(|a| a.as_arr()) {
        for item in bench_arr {
            bench.push(parse_player(item)?);
        }
    }
    let assign_arr = v.get("assignments").and_then(|a| a.as_arr()).ok_or("assignments missing")?;
    let mut assignments = Vec::with_capacity(assign_arr.len());
    for item in assign_arr {
        assignments.push(
            LineupAssignmentSnapshot::new(
                item.get_str("playerId").ok_or("assignment playerId missing")?,
                item.get_str("slotId").ok_or("assignment slotId missing")?,
            )
            .map_err(|e| e.to_string())?,
        );
    }
    let slots_arr = v.get("tacticalSlots").and_then(|a| a.as_arr()).ok_or("tacticalSlots missing")?;
    let mut tactical_slots = Vec::with_capacity(slots_arr.len());
    for item in slots_arr {
        tactical_slots.push(
            TacticalSlotSnapshot::new(
                item.get_str("slotId").ok_or("slotId missing")?,
                item.get_i32("outX").unwrap_or(50),
                item.get_i32("outY").unwrap_or(50),
                item.get_i32("inX").unwrap_or(50),
                item.get_i32("inY").unwrap_or(50),
            )
            .map_err(|e| e.to_string())?,
        );
    }
    let ti_v = v.get("teamInstructions");
    let team_instructions = TeamInstructionsSnapshot::new(
        ti_v.and_then(|x| x.get_i32("pressingIntensity")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("tempo")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("lineHeight")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("width")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("timeWasting")).unwrap_or(10),
        ti_v.and_then(|x| x.get_bool("offsideTrap")).unwrap_or(false),
        ti_v.and_then(|x| x.get_i32("aggression")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("transitionRisk")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("buildUpRisk")).unwrap_or(50),
        ti_v.and_then(|x| x.get_i32("concentration")).unwrap_or(50),
    )
    .map_err(|e| e.to_string())?;

    let si_arr = v.get("slotInstructions").and_then(|a| a.as_arr()).ok_or("slotInstructions missing")?;
    let mut slot_instructions = Vec::with_capacity(si_arr.len());
    for item in si_arr {
        slot_instructions.push(
            SlotInstructionsSnapshot::new(
                item.get_str("slotId").ok_or("slotInstruction slotId missing")?,
                item.get_i32("forwardMovement").unwrap_or(50),
                item.get_i32("passingRisk").unwrap_or(50),
                item.get_i32("crossingTendency").unwrap_or(55),
                item.get_i32("dribblingTendency").unwrap_or(55),
                item.get_i32("defensiveAggression").unwrap_or(55),
                item.get_i32("supportDistance").unwrap_or(50),
                item.get_i32("roaming").unwrap_or(35),
                item.get_i32("shootingTendency").unwrap_or(60),
            )
            .map_err(|e| e.to_string())?,
        );
    }

    let sp = match v.get("setPiecePlan") {
        Some(p) => SetPiecePlanSnapshot {
            penalty_taker_slot_ids: parse_string_list(p.get("penaltyTakerSlotIds")),
            corner_taker_slot_ids: parse_string_list(p.get("cornerTakerSlotIds")),
            close_free_kick_taker_slot_ids: parse_string_list(p.get("closeFreeKickTakerSlotIds")),
            far_free_kick_taker_slot_ids: parse_string_list(p.get("farFreeKickTakerSlotIds")),
            captain_slot_ids: parse_string_list(p.get("captainSlotIds")),
            corner_box_slot_ids: parse_string_list(p.get("cornerBoxSlotIds")),
            close_free_kick_box_slot_ids: parse_string_list(p.get("closeFreeKickBoxSlotIds")),
            far_free_kick_box_slot_ids: parse_string_list(p.get("farFreeKickBoxSlotIds")),
        },
        None => SetPiecePlanSnapshot::empty(),
    };

    let collective = match v.get("collectiveState") {
        Some(c) => TeamCollectiveSnapshot::new(
            c.get_i32("cohesion").unwrap_or(50),
            c.get_i32("systemFamiliarity").unwrap_or(50),
            c.get_i32("confidence").unwrap_or(50),
        )
        .map_err(|e| e.to_string())?,
        None => TeamCollectiveSnapshot::neutral(),
    };

    TeamMatchSnapshot::new(
        team_id,
        display_name,
        lineup,
        bench,
        assignments,
        gk_id,
        shape_id,
        tactical_slots,
        team_instructions,
        slot_instructions,
        sp,
        collective,
    )
    .map_err(|e| e.to_string())
}

pub fn parse_match_input(json: &str) -> Result<MatchInput, String> {
    let root = parse_json(json)?;
    let match_id = root.get_str("matchId").unwrap_or("LAB-RUST");
    let seed = root.get_i64("seed").unwrap_or(123456789);
    let engine_version = root.get_str("engineVersion").unwrap_or(ENGINE_VERSION);
    let neutral_ground = root.get_bool("neutralGround").unwrap_or(false);
    let competition = root.get_str("competitionContext").unwrap_or("ENG_1");
    let sim_mins = root.get_i32("simulatedActionMinutes").unwrap_or(20);
    let rules = match root.get_str("rulesMode").unwrap_or("STANDARD") {
        "KNOCKOUT" => MatchRules::knockout(),
        _ => MatchRules::standard(),
    };
    let home = parse_team(root.get("homeTeam").ok_or("homeTeam missing")?)?;
    let away = parse_team(root.get("awayTeam").ok_or("awayTeam missing")?)?;
    MatchInput::new(
        match_id,
        seed,
        engine_version,
        home,
        away,
        rules,
        neutral_ground,
        competition,
        LeagueObjectiveSnapshot::unavailable(),
        LeagueObjectiveSnapshot::unavailable(),
        sim_mins,
    )
    .map_err(|e| e.to_string())
}

pub fn parse_ai_config(json: &str) -> AiManagerConfig {
    let Ok(root) = parse_json(json) else {
        return AiManagerConfig::none();
    };
    AiManagerConfig {
        control_home: root.get_bool("aiHomeManager").unwrap_or(false),
        control_away: root.get_bool("aiAwayManager").unwrap_or(false),
    }
}

pub fn parse_match_command(json: &str) -> Result<MatchCommand, String> {
    let root = parse_json(json)?;
    let sec = root.get_i32("gameSecond").unwrap_or(0);
    let side = parse_side(root.get_str("side").ok_or("side missing")?)?;
    let kind = match root.get_str("type").ok_or("type missing")? {
        "SUBSTITUTE" => CommandType::Substitute,
        "REASSIGN_SLOT" => CommandType::ReassignSlot,
        "ASSIGN_GOALKEEPER" => CommandType::AssignGoalkeeper,
        "CHANGE_SHAPE" => CommandType::ChangeShape,
        "CHANGE_PRESSING" => CommandType::ChangePressing,
        "CHANGE_TEMPO" => CommandType::ChangeTempo,
        "CHANGE_WIDTH" => CommandType::ChangeWidth,
        "CHANGE_LINE_HEIGHT" => CommandType::ChangeLineHeight,
        "CHANGE_TEAM_INSTRUCTION" => CommandType::ChangeTeamInstruction,
        "CHANGE_SLOT_INSTRUCTION" => CommandType::ChangeSlotInstruction,
        "CHANGE_SET_PIECE_PLAN" => CommandType::ChangeSetPiecePlan,
        "CHANGE_PLAYER_MORALE" => CommandType::ChangePlayerMorale,
        other => return Err(format!("unknown command type: {}", other)),
    };
    MatchCommand::new(
        sec,
        side,
        kind,
        root.get_str("playerId").map(|s| s.to_string()),
        root.get_str("secondaryPlayerId").map(|s| s.to_string()),
        root.get_str("slotId").map(|s| s.to_string()),
        root.get_i32("intValue").unwrap_or(0),
        root.get_str("stringValue").map(|s| s.to_string()),
        Vec::new(),
        None,
    )
    .map_err(|e| e.to_string())
}

fn write_opt_str(out: &mut String, s: Option<&str>) {
    match s {
        Some(v) => escape_json_str(out, v),
        None => out.push_str("null"),
    }
}

fn write_event(out: &mut String, e: &MatchEvent) {
    out.push('{');
    let _ = write!(out, "\"sequence\":{},\"gameSecond\":{},\"type\":", e.sequence, e.game_second);
    escape_json_str(out, e.kind.as_str());
    out.push_str(",\"side\":");
    write_opt_str(out, e.side.map(|s| s.as_str()));
    out.push_str(",\"actor\":");
    write_opt_str(out, e.actor.as_deref());
    out.push_str(",\"target\":");
    write_opt_str(out, e.target.as_deref());
    let _ = write!(out, ",\"ballX\":{},\"ballY\":{},\"metadata\":{{", e.ball_x, e.ball_y);
    for (i, (k, v)) in e.metadata.iter().enumerate() {
        if i > 0 { out.push(','); }
        escape_json_str(out, k);
        out.push(':');
        escape_json_str(out, v);
    }
    out.push('}');
    if let Some(ref f) = e.facts_after {
        let _ = write!(
            out,
            ",\"minute\":{},\"homeGoals\":{},\"awayGoals\":{},\"transitionActive\":{},\"displayMinute\":",
            f.clock.minute, f.home_goals, f.away_goals, f.transition_active
        );
        escape_json_str(out, &f.clock.display_minute);
        out.push_str(",\"tacticalPhase\":");
        escape_json_str(out, f.tactical_phase.as_str());
        out.push_str(",\"restartType\":");
        escape_json_str(out, f.restart_type.as_str());
    }
    out.push('}');
}

fn write_team_stats(out: &mut String, s: &TeamMatchStatistics, total_poss: i32) {
    let _ = write!(
        out,
        "{{\"possessionSeconds\":{},\"possessionPercent\":{:.1},\"shots\":{},\"shotsOnTarget\":{},\"goals\":{},\"expectedGoalsMilli\":{},\"passesAttempted\":{},\"passesCompleted\":{},\"progressivePasses\":{},\"crosses\":{},\"tacklesWon\":{},\"interceptions\":{},\"saves\":{},\"fouls\":{},\"yellowCards\":{},\"redCards\":{},\"offsides\":{},\"corners\":{},\"penalties\":{}}}",
        s.possession_seconds,
        s.possession_percent(total_poss),
        s.shots,
        s.shots_on_target,
        s.goals,
        s.expected_goals_milli,
        s.passes_attempted,
        s.passes_completed,
        s.progressive_passes,
        s.crosses,
        s.tackles_won,
        s.interceptions,
        s.saves,
        s.fouls,
        s.yellow_cards,
        s.red_cards,
        s.offsides,
        s.corners,
        s.penalties
    );
}

pub fn serialize_match_result(result: &MatchResult) -> String {
    let mut out = String::with_capacity(result.events.len() * 180 + 1024);
    out.push_str("{\"ok\":true,\"matchId\":");
    escape_json_str(&mut out, &result.match_id);
    let _ = write!(
        out,
        ",\"seed\":{},\"homeGoals\":{},\"awayGoals\":{},\"homeShootoutGoals\":{},\"awayShootoutGoals\":{},\"finalGameSecond\":{},\"eventCount\":{},\"eventTimelineFingerprint\":{},\"fingerprint\":{},\"statistics\":{{\"possessionTrackedSeconds\":{},\"home\":",
        result.seed,
        result.home_goals,
        result.away_goals,
        result.home_shootout_goals,
        result.away_shootout_goals,
        result.final_game_second,
        result.event_count,
        result.event_timeline_fingerprint as i64,
        result.fingerprint as i64,
        result.statistics.possession_tracked_seconds
    );
    write_team_stats(&mut out, &result.statistics.home, result.statistics.possession_tracked_seconds);
    out.push_str(",\"away\":");
    write_team_stats(&mut out, &result.statistics.away, result.statistics.possession_tracked_seconds);
    out.push_str("},\"events\":[");
    for (i, ev) in result.events.iter().enumerate() {
        if i > 0 { out.push(','); }
        write_event(&mut out, ev);
    }
    out.push_str("]}");
    out
}

fn write_player_rating(out: &mut String, r: &PlayerMatchRating) {
    out.push_str("{\"playerId\":");
    escape_json_str(out, &r.player_id);
    out.push_str(",\"displayName\":");
    escape_json_str(out, &r.display_name);
    out.push_str(",\"side\":\"");
    out.push_str(match r.side {
        TeamSide::Home => "HOME",
        TeamSide::Away => "AWAY",
    });
    out.push_str("\",\"primaryPosition\":");
    escape_json_str(out, &r.primary_position);
    let _ = write!(
        out,
        ",\"ratingTenths\":{},\"goals\":{},\"assists\":{},\"shots\":{},\"shotsOnTarget\":{},\"passesCompleted\":{},\"passesAttempted\":{},\"tacklesWon\":{},\"interceptions\":{},\"saves\":{},\"yellowCards\":{},\"redCards\":{}}}",
        r.rating_tenths,
        r.goals,
        r.assists,
        r.shots,
        r.shots_on_target,
        r.passes_completed,
        r.passes_attempted,
        r.tackles_won,
        r.interceptions,
        r.saves,
        r.yellow_cards,
        r.red_cards
    );
}

pub fn serialize_match_result_with_ratings(input: &MatchInput, result: &MatchResult) -> String {
    let summary: MatchRatingsSummary =
        player_ratings::compute_player_ratings(input, &result.events, result.home_goals, result.away_goals);
    let mut base = serialize_match_result(result);
    if base.ends_with('}') {
        base.pop();
    }
    base.push_str(",\"manOfTheMatch\":");
    match &summary.man_of_the_match {
        Some(m) => write_player_rating(&mut base, m),
        None => base.push_str("null"),
    }
    base.push_str(",\"playerRatings\":[");
    for (i, r) in summary.ratings.iter().take(12).enumerate() {
        if i > 0 {
            base.push(',');
        }
        write_player_rating(&mut base, r);
    }
    base.push_str("]}");
    base
}

pub fn serialize_session_step(session: &MatchSession, new_events: &[MatchEvent]) -> String {
    let snap = session.snapshot();
    let mut out = String::with_capacity(new_events.len() * 180 + 256);
    let _ = write!(
        out,
        "{{\"ok\":true,\"fullTime\":{},\"gameSecond\":{},\"homeGoals\":{},\"awayGoals\":{},\"eventCount\":{},\"events\":[",
        session.is_full_time(),
        session.game_second(),
        snap.score.home_goals,
        snap.score.away_goals,
        session.event_count()
    );
    for (i, ev) in new_events.iter().enumerate() {
        if i > 0 { out.push(','); }
        write_event(&mut out, ev);
    }
    out.push_str("]}");
    out
}

pub fn serialize_error(msg: &str) -> String {
    let mut out = String::from("{\"ok\":false,\"error\":");
    escape_json_str(&mut out, msg);
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::UnifiedMatchEngine;

    #[test]
    fn wire_json_roundtrip_simulates_full_match() {
        let make_team = |id: &str, pref: &str| {
            let slots = [
                ("GK", 50, 5, 50, 8, "GK"), ("LB", 15, 20, 15, 38, "LB"),
                ("LCB", 38, 18, 38, 28, "CB"), ("RCB", 62, 18, 62, 28, "CB"),
                ("RB", 85, 20, 85, 38, "RB"), ("DM", 50, 38, 50, 48, "DM"),
                ("LCM", 35, 50, 35, 60, "CM"), ("RCM", 65, 50, 65, 60, "CM"),
                ("LW", 15, 70, 15, 84, "LW"), ("RW", 85, 70, 85, 84, "RW"),
                ("CF", 50, 78, 50, 88, "CF"),
            ];
            let mut lineup = Vec::new();
            let mut assigns = Vec::new();
            let mut tslots = Vec::new();
            let mut sinstr = Vec::new();
            for (i, (sid, ox, oy, ix, iy, pos)) in slots.iter().enumerate() {
                let pid = format!("{}{:02}", pref, i);
                lineup.push(format!(
                    "{{\"playerId\":\"{}\",\"displayName\":\"{}\",\"attributes\":[75,75,75,75,75,75,75,75,75,75,75,75,75,75,75,75,75,75,75,75],\"primaryPosition\":\"{}\"}}",
                    pid, pid, pos
                ));
                assigns.push(format!("{{\"playerId\":\"{}\",\"slotId\":\"{}\"}}", pid, sid));
                tslots.push(format!("{{\"slotId\":\"{}\",\"outX\":{},\"outY\":{},\"inX\":{},\"inY\":{}}}", sid, ox, oy, ix, iy));
                sinstr.push(format!("{{\"slotId\":\"{}\"}}", sid));
            }
            format!(
                "{{\"teamId\":\"{}\",\"displayName\":\"{}\",\"designatedGoalkeeperId\":\"{}00\",\"lineup\":[{}],\"assignments\":[{}],\"tacticalSlots\":[{}],\"slotInstructions\":[{}]}}",
                id, id, pref, lineup.join(","), assigns.join(","), tslots.join(","), sinstr.join(",")
            )
        };
        let payload = format!(
            "{{\"matchId\":\"WIRE-1\",\"seed\":123456789,\"simulatedActionMinutes\":20,\"homeTeam\":{},\"awayTeam\":{}}}",
            make_team("HOME", "H"),
            make_team("AWAY", "A")
        );
        let input = parse_match_input(&payload).unwrap();
        let res = UnifiedMatchEngine::simulate(input).unwrap();
        let out_json = serialize_match_result(&res);
        let parsed = parse_json(&out_json).unwrap();
        assert_eq!(parsed.get_bool("ok"), Some(true));
        assert!(parsed.get_i32("eventCount").unwrap() > 100);
        assert!(parsed.get("events").and_then(|e| e.as_arr()).unwrap().len() > 100);
    }
}
