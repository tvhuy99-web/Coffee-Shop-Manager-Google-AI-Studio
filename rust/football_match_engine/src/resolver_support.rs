use crate::{
    events::EventType,
    geometry::{self, TeamSide},
    input::MatchInput,
    rng::DeterministicRng,
    state::{MatchState, RestartType},
};

pub trait MatchRuntime {
    fn input(&self) -> &MatchInput;
    fn state(&self) -> &MatchState;
    fn state_mut(&mut self) -> &mut MatchState;
    fn rng_mut(&mut self) -> &mut DeterministicRng;
    fn emit(
        &mut self,
        kind: EventType,
        side: Option<TeamSide>,
        actor: Option<&str>,
        target: Option<&str>,
        metadata: Vec<(String, String)>,
    );
    fn advance_playing_time(&mut self, seconds: i32) -> i32;
    fn add_stoppage(&mut self, seconds: i32);
    fn schedule_restart(&mut self, kind: RestartType, side: TeamSide, reason: &str, x: i32, y: i32);
    fn change_possession(&mut self, side: TeamSide, player_id: &str, cause: &str);
    fn ensure_goalkeeper(&mut self, side: TeamSide, reason: &str);
}

#[inline]
pub fn kv(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

pub fn make_loose(
    c: &mut dyn MatchRuntime,
    last_side: Option<TeamSide>,
    last_player: Option<&str>,
    x: i32,
    y: i32,
    reason: &str,
) {
    let cx = x.clamp(1, 99);
    let cy = y.clamp(1, 99);
    c.state_mut()
        .ball
        .set_loose(cx, cy, last_side, last_player.map(|s| s.to_string()), Some(reason));
    c.emit(EventType::LooseBall, last_side, last_player, None, vec![kv("reason", reason)]);
}

pub fn best_defender(c: &dyn MatchRuntime, side: TeamSide, x: i32, y: i32) -> Option<String> {
    let state = c.state();
    let input = c.input();
    let team = input.team(side);
    let gk = state.team(side).designated_goalkeeper_id.as_deref();
    let def_prog = geometry::attacking_progress(side, y);
    let mut best: Option<String> = None;
    let mut best_score = f64::INFINITY;
    for (_, r) in &state.players {
        if r.side != side || !r.on_pitch || gk == Some(r.player_id.as_str()) {
            continue;
        }
        let p = match team.player(&r.player_id) {
            Some(p) => p,
            None => continue,
        };
        let slot = state.team(side).tactical_slot(&r.slot_id);
        let base_out_prog = match slot {
            None => 50,
            Some(s) => geometry::attacking_progress(side, geometry::oriented_y(side, s.out_y)),
        };
        let role_offset = if base_out_prog >= 70 && def_prog >= 56 { 5.5 } else { 0.0 };
        let score = geometry::distance(r.x, r.y, x, y)
            - p.attributes.get(16) as f64 * 0.028
            - p.attributes.get(8) as f64 * 0.007
            + role_offset;
        if best.is_none() || score < best_score || (score == best_score && r.player_id < *best.as_ref().unwrap()) {
            best = Some(r.player_id.clone());
            best_score = score;
        }
    }
    best.or_else(|| gk.map(|s| s.to_string()))
}

fn java_string_hash_code(s: &str) -> i32 {
    let mut h: i32 = 0;
    for unit in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as i32);
    }
    h
}

pub fn best_aerial(c: &dyn MatchRuntime, side: TeamSide, x: i32, y: i32, attacking: bool) -> Option<String> {
    let state = c.state();
    let team = c.input().team(side);
    let gk = state.team(side).designated_goalkeeper_id.as_deref();
    let sec = state.clock.game_second;
    let mut best: Option<String> = None;
    let mut best_v = f64::NEG_INFINITY;
    for (_, r) in &state.players {
        if r.side != side || !r.on_pitch || gk == Some(r.player_id.as_str()) {
            continue;
        }
        let p = match team.player(&r.player_id) {
            Some(p) => p,
            None => continue,
        };
        let d = geometry::distance(r.x, r.y, x, y);
        let prog = geometry::attacking_progress(side, r.y);
        let dist_weight = if attacking && prog >= 56 { 0.24 } else { 0.70 };

        let rot = if attacking && prog >= 56 {
            let h = java_string_hash_code(&r.slot_id).wrapping_mul(29).wrapping_add(sec.wrapping_mul(13));
            (h.unsigned_abs() % 13) as f64 * 1.15
        } else {
            0.0
        };
        let a = &p.attributes;
        let q = a.get(7) as f64 * 0.35
            + a.get(10) as f64 * 0.18
            + (if attacking { a.get(15) } else { a.get(16) }) as f64 * 0.22
            - d * dist_weight
            + rot;
        if best.is_none() || q > best_v {
            best = Some(r.player_id.clone());
            best_v = q;
        }
    }
    best
}

pub fn nearest_outfield(c: &dyn MatchRuntime, side: TeamSide, x: i32, y: i32) -> Option<String> {
    let state = c.state();
    let gk = state.team(side).designated_goalkeeper_id.as_deref();
    let mut best: Option<String> = None;
    let mut best_d = f64::INFINITY;
    for (_, r) in &state.players {
        if r.side == side && r.on_pitch && gk != Some(r.player_id.as_str()) {
            let q = geometry::distance(r.x, r.y, x, y);
            if best.is_none() || q < best_d {
                best = Some(r.player_id.clone());
                best_d = q;
            }
        }
    }
    best.or_else(|| gk.map(|s| s.to_string()))
}

pub fn choose_central(c: &dyn MatchRuntime, side: TeamSide) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_v = i32::MAX;
    for (_, r) in &c.state().players {
        if r.side == side && r.on_pitch {
            let q = (r.x - 50).abs() + (r.y - 50).abs();
            if best.is_none() || q < best_v {
                best = Some(r.player_id.clone());
                best_v = q;
            }
        }
    }
    best
}

#[inline]
pub fn duel_attr(value: i32) -> f64 {
    74.0 + (value - 74) as f64 * 0.70
}

#[inline]
pub fn calibrated_finishing_attr(value: i32) -> f64 {
    74.0 + (value - 74) as f64 * 0.60
}

#[inline]
pub fn java_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}
