use std::collections::{HashMap, HashSet};
use crate::snapshot::PlayerAttributesSnapshot;
use crate::input::{
    LineupAssignmentSnapshot, PlayerMatchSnapshot,
    SetPiecePlanSnapshot, SlotInstructionsSnapshot, TacticalSlotSnapshot,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Position {
    Gk,
    Sw,
    Lb,
    Rb,
    Lwb,
    Rwb,
    Cb,
    B,
    Dm,
    Lm,
    Rm,
    Cm,
    M,
    Lw,
    Rw,
    Am,
    Cf,
    F,
}

impl Position {
    pub fn from_code(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "GK" => Self::Gk,
            "SW" => Self::Sw,
            "LB" => Self::Lb,
            "RB" => Self::Rb,
            "LWB" => Self::Lwb,
            "RWB" => Self::Rwb,
            "CB" => Self::Cb,
            "B" | "DF" | "D" => Self::B,
            "DM" | "CDM" => Self::Dm,
            "LM" => Self::Lm,
            "RM" => Self::Rm,
            "CM" => Self::Cm,
            "M" | "MF" => Self::M,
            "LW" => Self::Lw,
            "RW" => Self::Rw,
            "AM" | "CAM" => Self::Am,
            "CF" | "SS" => Self::Cf,
            "F" | "ST" | "FW" => Self::F,
            _ => Self::Cm,
        }
    }

    pub fn ordinal(self) -> u8 {
        match self {
            Self::Gk => 0,
            Self::Sw => 1,
            Self::Lb => 2,
            Self::Rb => 3,
            Self::Lwb => 4,
            Self::Rwb => 5,
            Self::Cb => 6,
            Self::B => 7,
            Self::Dm => 8,
            Self::Lm => 9,
            Self::Rm => 10,
            Self::Cm => 11,
            Self::M => 12,
            Self::Lw => 13,
            Self::Rw => 14,
            Self::Am => 15,
            Self::Cf => 16,
            Self::F => 17,
        }
    }

    pub fn line_group(self) -> usize {
        match self {
            Self::Gk => 0,
            Self::Sw | Self::Lb | Self::Rb | Self::Lwb | Self::Rwb | Self::Cb | Self::B => 1,
            Self::Dm | Self::Lm | Self::Rm | Self::Cm | Self::M | Self::Am => 2,
            Self::Lw | Self::Rw | Self::Cf | Self::F => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TacticalShape {
    pub shape_id: String,
    pub display_name: String,
    pub slots: Vec<TacticalSlotSnapshot>,
}

pub fn shape_from_positions(display_name: &str, positions: &[Position]) -> TacticalShape {
    let mut totals: HashMap<Position, usize> = HashMap::new();
    for &p in positions {
        *totals.entry(p).or_insert(0) += 1;
    }
    let mut seen: HashMap<Position, usize> = HashMap::new();
    let mut slots = Vec::with_capacity(positions.len());
    for (i, &p) in positions.iter().enumerate() {
        let occ = *seen.get(&p).unwrap_or(&0);
        let total = *totals.get(&p).unwrap_or(&1);
        seen.insert(p, occ + 1);
        let (out_x, out_y) = anchor_for_position(p, occ, total, false);
        let (in_x, in_y) = anchor_for_position(p, occ, total, true);
        let sid = format!("S{:02}", i);
        slots.push(TacticalSlotSnapshot::new(sid, out_x, out_y, in_x, in_y).expect("valid coords"));
    }
    let fp = positions_fingerprint(positions);
    TacticalShape {
        shape_id: format!("shape-v3-{:x}", fp),
        display_name: display_name.to_string(),
        slots,
    }
}

fn anchor_for_position(p: Position, occ: usize, total: usize, in_poss: bool) -> (i32, i32) {
    let (x, y) = match p {
        Position::Gk => (50, if in_poss { 8 } else { 5 }),
        Position::Sw => (spread(occ, total, 28), if in_poss { 27 } else { 15 }),
        Position::Lb => (16, if in_poss { 42 } else { 23 }),
        Position::Rb => (84, if in_poss { 42 } else { 23 }),
        Position::Lwb => (12, if in_poss { 54 } else { 34 }),
        Position::Rwb => (88, if in_poss { 54 } else { 34 }),
        Position::Cb => (spread(occ, total, 30), if in_poss { 31 } else { 22 }),
        Position::B => (spread(occ, total, 34), if in_poss { 35 } else { 25 }),
        Position::Dm => (spread(occ, total, 28), if in_poss { 49 } else { 39 }),
        Position::Lm => (17, if in_poss { 62 } else { 51 }),
        Position::Rm => (83, if in_poss { 62 } else { 51 }),
        Position::Cm => (spread(occ, total, 34), if in_poss { 61 } else { 51 }),
        Position::M => (spread(occ, total, 38), if in_poss { 63 } else { 52 }),
        Position::Lw => (if in_poss { 18 } else { 12 }, if in_poss { 79 } else { 66 }),
        Position::Rw => (if in_poss { 82 } else { 88 }, if in_poss { 79 } else { 66 }),
        Position::Am => (spread(occ, total, 28), if in_poss { 76 } else { 65 }),
        Position::Cf => (spread(occ, total, 32), if in_poss { 86 } else { 80 }),
        Position::F => (spread(occ, total, 36), if in_poss { 88 } else { 82 }),
    };
    (x.clamp(0, 100), y.clamp(0, 100))
}

fn spread(occ: usize, total: usize, width: i32) -> i32 {
    if total <= 1 {
        return 50;
    }
    let eff = if total == 2 {
        width as f64 * 0.80
    } else {
        width as f64 * 1.25
    };
    let start = 50.0 - eff / 2.0;
    let step = eff / (total - 1) as f64;
    (start + occ as f64 * step).round() as i32
}

fn positions_fingerprint(positions: &[Position]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &p in positions {
        hash ^= p.ordinal() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Clone, Debug)]
pub struct ShapeValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ShapeValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

struct PhaseGeometry {
    count: usize,
    width: i32,
    depth: i32,
    mean_y: f64,
}

pub fn validate_shape(shape: &TacticalShape) -> ShapeValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if shape.shape_id.trim().is_empty() {
        errors.push("shapeId is required".into());
    }
    if shape.slots.len() != 11 {
        errors.push("shape must contain exactly 11 slots".into());
        return ShapeValidationResult { errors, warnings };
    }

    let mut ids = HashSet::new();
    for s in &shape.slots {
        if s.slot_id.trim().is_empty()
            || !(0..=100).contains(&s.out_x)
            || !(0..=100).contains(&s.out_y)
            || !(0..=100).contains(&s.in_x)
            || !(0..=100).contains(&s.in_y)
        {
            errors.push("every slot must have a valid id and coordinates".into());
            continue;
        }
        if !ids.insert(s.slot_id.as_str()) {
            errors.push(format!("duplicate slotId: {}", s.slot_id));
        }
    }
    if !errors.is_empty() {
        return ShapeValidationResult { errors, warnings };
    }

    let keeper_idx = goalkeeper_anchor(&shape.slots);
    match keeper_idx {
        -2 => errors.push("shape has multiple goalkeeper anchors in the defensive phase".into()),
        x if x < 0 => errors.push("shape needs one clear goalkeeper anchor in the defensive phase".into()),
        idx => {
            let keeper = &shape.slots[idx as usize];
            if keeper.in_y > 45 {
                errors.push("goalkeeper anchor advances too far in possession".into());
            } else if keeper.in_y > 30 {
                warnings.push("goalkeeper anchor is unusually high in possession".into());
            }
        }
    }

    let out_g = phase_geometry(&shape.slots, keeper_idx, false);
    let in_g = phase_geometry(&shape.slots, keeper_idx, true);
    validate_geometry("out of possession", &out_g, &mut errors, &mut warnings);
    validate_geometry("in possession", &in_g, &mut errors, &mut warnings);

    let mut severe_collisions = 0;
    let mut dense_clusters = 0;
    for i in 0..shape.slots.len() {
        if i as i32 == keeper_idx {
            continue;
        }
        let a = &shape.slots[i];
        let mut neighbors = 0;
        for j in 0..shape.slots.len() {
            if j == i || j as i32 == keeper_idx {
                continue;
            }
            let b = &shape.slots[j];
            let od = dist(a.out_x, a.out_y, b.out_x, b.out_y);
            let id = dist(a.in_x, a.in_y, b.in_x, b.in_y);
            if j > i && od < 3.0 && id < 3.0 {
                severe_collisions += 1;
            }
            if od <= 8.0 && id <= 8.0 {
                neighbors += 1;
            }
        }
        if neighbors >= 3 {
            dense_clusters += 1;
        }
        let mv = dist(a.out_x, a.out_y, a.in_x, a.in_y);
        if mv > 70.0 {
            errors.push(format!("slot {} moves an implausible distance between phases", a.slot_id));
        } else if mv > 50.0 {
            warnings.push(format!("slot {} has a very large phase transition", a.slot_id));
        }
    }
    if severe_collisions > 0 {
        errors.push("multiple outfield slots overlap in both tactical phases".into());
    }
    if dense_clusters > 0 {
        errors.push("too many outfield slots are concentrated in the same small area".into());
    }
    if out_g.mean_y - in_g.mean_y > 25.0 {
        errors.push("the whole team retreats implausibly when gaining possession".into());
    } else if out_g.mean_y - in_g.mean_y > 10.0 {
        warnings.push("the team retreats substantially when gaining possession".into());
    }

    ShapeValidationResult { errors, warnings }
}

fn validate_geometry(phase: &str, g: &PhaseGeometry, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    if g.count != 10 {
        return;
    }
    if g.width < 18 {
        errors.push(format!("{} shape is too horizontally compressed", phase));
    } else if g.width < 32 {
        warnings.push(format!("{} shape is unusually narrow", phase));
    }
    if g.depth < 18 {
        errors.push(format!("{} shape has insufficient vertical depth", phase));
    } else if g.depth < 32 {
        warnings.push(format!("{} shape has unusually little vertical depth", phase));
    }
    if g.mean_y < 18.0 || g.mean_y > 86.0 {
        errors.push(format!("{} outfield centroid is outside a plausible team area", phase));
    } else if g.mean_y < 27.0 || g.mean_y > 76.0 {
        warnings.push(format!("{} outfield centroid is unusually extreme", phase));
    }
}

fn goalkeeper_anchor(slots: &[TacticalSlotSnapshot]) -> i32 {
    let mut idx = -1;
    for (i, s) in slots.iter().enumerate() {
        if s.out_y <= 12 && (25..=75).contains(&s.out_x) {
            if idx >= 0 {
                return -2;
            }
            idx = i as i32;
        }
    }
    idx
}

fn phase_geometry(slots: &[TacticalSlotSnapshot], keeper_idx: i32, in_poss: bool) -> PhaseGeometry {
    let (mut min_x, mut max_x, mut min_y, mut max_y, mut sum_y, mut count) = (101, -1, 101, -1, 0, 0);
    for (i, s) in slots.iter().enumerate() {
        if i as i32 == keeper_idx {
            continue;
        }
        let x = if in_poss { s.in_x } else { s.out_x };
        let y = if in_poss { s.in_y } else { s.out_y };
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        sum_y += y;
        count += 1;
    }
    PhaseGeometry {
        count,
        width: max_x - min_x,
        depth: max_y - min_y,
        mean_y: if count == 0 { 0.0 } else { sum_y as f64 / count as f64 },
    }
}

fn dist(ax: i32, ay: i32, bx: i32, by: i32) -> f64 {
    let dx = (ax - bx) as f64;
    let dy = (ay - by) as f64;
    (dx * dx + dy * dy).sqrt()
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PlayerProfile {
    Goalkeeper,
    DefensiveAnchor,
    WideDefender,
    CentralController,
    CentralCreator,
    WideAttacker,
    MobileAttacker,
    PhysicalTarget,
    Balanced,
}

impl PlayerProfile {
    pub const ALL: [PlayerProfile; 9] = [
        Self::Goalkeeper,
        Self::DefensiveAnchor,
        Self::WideDefender,
        Self::CentralController,
        Self::CentralCreator,
        Self::WideAttacker,
        Self::MobileAttacker,
        Self::PhysicalTarget,
        Self::Balanced,
    ];
}

pub fn profile_score(a: &PlayerAttributesSnapshot, profile: PlayerProfile) -> i32 {
    match profile {
        PlayerProfile::Goalkeeper => weighted(&[a.get(0), a.get(9), a.get(13), a.get(14), a.get(2)], &[55, 14, 12, 10, 9]),
        PlayerProfile::DefensiveAnchor => weighted(&[a.get(16), a.get(1), a.get(13), a.get(10), a.get(7), a.get(17)], &[24, 22, 17, 13, 12, 12]),
        PlayerProfile::WideDefender => weighted(&[a.get(16), a.get(1), a.get(8), a.get(11), a.get(17), a.get(3)], &[19, 17, 18, 17, 15, 14]),
        PlayerProfile::CentralController => weighted(&[a.get(2), a.get(13), a.get(4), a.get(14), a.get(11), a.get(16)], &[22, 19, 17, 16, 14, 12]),
        PlayerProfile::CentralCreator => weighted(&[a.get(12), a.get(2), a.get(4), a.get(13), a.get(14), a.get(5)], &[24, 20, 18, 16, 12, 10]),
        PlayerProfile::WideAttacker => weighted(&[a.get(8), a.get(5), a.get(3), a.get(15), a.get(4), a.get(11)], &[22, 21, 17, 16, 13, 11]),
        PlayerProfile::MobileAttacker => weighted(&[a.get(6), a.get(15), a.get(14), a.get(8), a.get(4), a.get(5)], &[24, 22, 18, 14, 12, 10]),
        PlayerProfile::PhysicalTarget => weighted(&[a.get(6), a.get(7), a.get(10), a.get(15), a.get(14), a.get(4)], &[22, 21, 19, 15, 13, 10]),
        PlayerProfile::Balanced => weighted(&[a.get(2), a.get(4), a.get(13), a.get(8), a.get(11), a.get(17)], &[18, 18, 18, 15, 15, 16]),
    }
}

pub fn best_profile(a: &PlayerAttributesSnapshot) -> PlayerProfile {
    let mut best = PlayerProfile::Balanced;
    let mut best_val = -1;
    for p in PlayerProfile::ALL {
        let s = profile_score(a, p);
        if s > best_val {
            best_val = s;
            best = p;
        }
    }
    best
}

pub fn profile_similarity(a: &PlayerAttributesSnapshot, b: &PlayerAttributesSnapshot) -> i32 {
    let mut dist_sum = 0;
    for p in PlayerProfile::ALL {
        dist_sum += (profile_score(a, p) - profile_score(b, p)).abs();
    }
    (100 - dist_sum / PlayerProfile::ALL.len() as i32).max(0)
}

fn weighted(values: &[u8], weights: &[i32]) -> i32 {
    let (mut sum, mut total) = (0i32, 0i32);
    for (&v, &w) in values.iter().zip(weights.iter()) {
        sum += (v as i32) * w;
        total += w;
    }
    if total == 0 {
        return 0;
    }
    ((sum as f32 / total as f32).round() as i32).clamp(1, 99)
}

pub fn slot_profile(slot: &TacticalSlotSnapshot) -> PlayerProfile {
    if slot.out_y <= 12 && (25..=75).contains(&slot.out_x) {
        return PlayerProfile::Goalkeeper;
    }
    let wide = slot.out_x <= 24 || slot.out_x >= 76;
    let mean_y = (slot.out_y + slot.in_y) / 2;
    match (mean_y, wide) {
        (0..=36, true) => PlayerProfile::WideDefender,
        (0..=36, false) => PlayerProfile::DefensiveAnchor,
        (37..=66, true) => PlayerProfile::WideAttacker,
        (37..=56, false) => PlayerProfile::CentralController,
        (57..=72, false) => PlayerProfile::CentralCreator,
        (_, true) => PlayerProfile::WideAttacker,
        _ => PlayerProfile::MobileAttacker,
    }
}

pub fn position_compatibility_bonus(pos: Position, slot: &TacticalSlotSnapshot) -> f64 {
    let wide_left = slot.out_x <= 30;
    let wide_right = slot.out_x >= 70;
    let central = !wide_left && !wide_right;
    let y = (slot.out_y + slot.in_y) / 2;
    match pos {
        Position::Gk => if slot.out_y <= 12 { 14.0 } else { -45.0 },
        Position::Sw | Position::Cb | Position::B => if y <= 38 && central { 10.0 } else if y <= 42 { 3.0 } else { -12.0 },
        Position::Lb | Position::Lwb => if wide_left && y <= 52 { 10.0 } else if wide_right && y <= 46 { 3.0 } else { -8.0 },
        Position::Rb | Position::Rwb => if wide_right && y <= 52 { 10.0 } else if wide_left && y <= 46 { 3.0 } else { -8.0 },
        Position::Dm => if central && (32..=58).contains(&y) { 10.0 } else if central && y <= 65 { 4.0 } else { -7.0 },
        Position::Cm | Position::M => if central && (42..=68).contains(&y) { 10.0 } else if (35..=74).contains(&y) { 4.0 } else { -6.0 },
        Position::Lm => if wide_left && (42..=75).contains(&y) { 10.0 } else if wide_right && (45..=75).contains(&y) { 3.0 } else { -7.0 },
        Position::Rm => if wide_right && (42..=75).contains(&y) { 10.0 } else if wide_left && (45..=75).contains(&y) { 3.0 } else { -7.0 },
        Position::Am => if central && (55..=80).contains(&y) { 10.0 } else if (48..=85).contains(&y) { 4.0 } else { -7.0 },
        Position::Lw => if wide_left && y >= 58 { 10.0 } else if wide_right && y >= 62 { 5.0 } else { -6.0 },
        Position::Rw => if wide_right && y >= 58 { 10.0 } else if wide_left && y >= 62 { 5.0 } else { -6.0 },
        Position::Cf | Position::F => if y >= 70 && central { 10.0 } else if y >= 62 { 5.0 } else { -10.0 },
    }
}

pub fn player_slot_suitability(player: &PlayerMatchSnapshot, slot: &TacticalSlotSnapshot) -> i32 {
    let target_profile = slot_profile(slot);
    let base = profile_score(&player.attributes, target_profile) as f64;
    let primary = Position::from_code(&player.primary_position);
    let mut pos_bonus = position_compatibility_bonus(primary, slot);
    if !player.secondary_position.is_empty() {
        let sec = Position::from_code(&player.secondary_position);
        let sec_bonus = position_compatibility_bonus(sec, slot) * 0.72;
        if sec_bonus > pos_bonus {
            pos_bonus = sec_bonus;
        }
    }

    let readiness = (player.condition as f64 - 75.0) * 0.14
        + (player.morale as f64 - 50.0) * 0.06
        + (player.form as f64 - 50.0) * 0.08;
    let penalty = if player.injured || !player.eligible { -40.0 } else { 0.0 };
    (base + pos_bonus + readiness + penalty).round().clamp(1.0, 100.0) as i32
}

#[derive(Clone, Debug)]
pub struct PlannedLineup {
    pub starters: Vec<PlayerMatchSnapshot>,
    pub bench: Vec<PlayerMatchSnapshot>,
    pub assignments: Vec<LineupAssignmentSnapshot>,
    pub designated_goalkeeper_id: String,
    pub total_suitability: i32,
}

pub fn plan_optimal_lineup(
    squad: &[PlayerMatchSnapshot],
    slots: &[TacticalSlotSnapshot],
    bench_size: usize,
) -> Option<PlannedLineup> {
    if squad.len() < 11 || slots.len() != 11 {
        return None;
    }
    let gk_slot_idx = slots
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.out_y)
        .map(|(i, _)| i)?;

    let n = squad.len();
    let mut scores = vec![vec![0i32; 11]; n];
    for i in 0..n {
        for s in 0..11 {
            scores[i][s] = player_slot_suitability(&squad[i], &slots[s]);
        }
    }

    let mut used_players = HashSet::new();
    let mut slot_to_player = vec![usize::MAX; 11];
    let best_gk = (0..n)
        .max_by_key(|&i| (scores[i][gk_slot_idx], squad[i].attributes.get(0)))
        .unwrap();
    slot_to_player[gk_slot_idx] = best_gk;
    used_players.insert(best_gk);

    let mut unassigned_slots: Vec<usize> = (0..11).filter(|&s| s != gk_slot_idx).collect();
    while !unassigned_slots.is_empty() {
        let mut chosen_slot_pos = 0;
        let mut best_gap = i32::MIN;
        let mut chosen_player = 0;
        for (pos, &s) in unassigned_slots.iter().enumerate() {
            let mut top1 = (i32::MIN, 0usize);
            let mut top2 = i32::MIN;
            for i in 0..n {
                if used_players.contains(&i) {
                    continue;
                }
                let sc = scores[i][s];
                if sc > top1.0 {
                    top2 = top1.0;
                    top1 = (sc, i);
                } else if sc > top2 {
                    top2 = sc;
                }
            }
            let gap = if top2 == i32::MIN { top1.0 } else { top1.0 - top2 };
            if gap > best_gap {
                best_gap = gap;
                chosen_slot_pos = pos;
                chosen_player = top1.1;
            }
        }
        let s = unassigned_slots.remove(chosen_slot_pos);
        slot_to_player[s] = chosen_player;
        used_players.insert(chosen_player);
    }

    let mut improved = true;
    let mut passes = 0;
    while improved && passes < 12 {
        improved = false;
        passes += 1;
        for a in 0..11 {
            if a == gk_slot_idx {
                continue;
            }
            for b in (a + 1)..11 {
                if b == gk_slot_idx {
                    continue;
                }
                let pa = slot_to_player[a];
                let pb = slot_to_player[b];
                if scores[pa][b] + scores[pb][a] > scores[pa][a] + scores[pb][b] {
                    slot_to_player[a] = pb;
                    slot_to_player[b] = pa;
                    improved = true;
                }
            }

            let pa = slot_to_player[a];
            for cand in 0..n {
                if used_players.contains(&cand) {
                    continue;
                }
                if scores[cand][a] > scores[pa][a] {
                    used_players.remove(&pa);
                    used_players.insert(cand);
                    slot_to_player[a] = cand;
                    improved = true;
                    break;
                }
            }
        }
    }

    let mut starters = Vec::with_capacity(11);
    let mut assignments = Vec::with_capacity(11);
    let mut total_suitability = 0;
    for s in 0..11 {
        let p_idx = slot_to_player[s];
        total_suitability += scores[p_idx][s];
        let p = squad[p_idx].clone();
        assignments.push(LineupAssignmentSnapshot::new(p.player_id.clone(), slots[s].slot_id.clone()).unwrap());
        starters.push(p);
    }
    let designated_goalkeeper_id = squad[slot_to_player[gk_slot_idx]].player_id.clone();

    let mut remaining: Vec<usize> = (0..n).filter(|i| !used_players.contains(i)).collect();
    let mut bench_indices = Vec::new();
    let target_lines = [0usize, 1, 2, 3];
    for line in target_lines {
        if bench_indices.len() >= bench_size {
            break;
        }
        if let Some(pos) = remaining
            .iter()
            .enumerate()
            .filter(|(_, &i)| Position::from_code(&squad[i].primary_position).line_group() == line)
            .max_by_key(|(_, &i)| profile_score(&squad[i].attributes, best_profile(&squad[i].attributes)))
            .map(|(p, _)| p)
        {
            bench_indices.push(remaining.remove(pos));
        }
    }
    remaining.sort_by_key(|&i| std::cmp::Reverse(profile_score(&squad[i].attributes, best_profile(&squad[i].attributes))));
    for i in remaining {
        if bench_indices.len() >= bench_size {
            break;
        }
        bench_indices.push(i);
    }
    let bench = bench_indices.into_iter().map(|i| squad[i].clone()).collect();

    Some(PlannedLineup {
        starters,
        bench,
        assignments,
        designated_goalkeeper_id,
        total_suitability,
    })
}

pub fn default_slot_instructions(slots: &[TacticalSlotSnapshot]) -> Vec<SlotInstructionsSnapshot> {
    slots
        .iter()
        .map(|s| {
            let is_gk = s.out_y <= 12 && (25..=75).contains(&s.out_x);
            let wide = s.out_x <= 24 || s.out_x >= 76;
            let fwd_move = if is_gk {
                12
            } else {
                (35 + (s.in_y - s.out_y) * 2).clamp(20, 88)
            };
            let pass_risk = if is_gk {
                25
            } else if s.in_y >= 65 {
                64
            } else if s.in_y >= 45 {
                56
            } else {
                44
            };
            let crossing = if is_gk {
                10
            } else if wide {
                72
            } else {
                34
            };
            let dribbling = if is_gk {
                10
            } else if wide && s.in_y >= 60 {
                72
            } else if s.in_y >= 70 {
                66
            } else {
                48
            };
            let def_agg = if is_gk {
                20
            } else if s.out_y <= 42 {
                64
            } else {
                50
            };
            let support = if is_gk { 30 } else { (55 - (s.in_y - 50).abs() / 3).clamp(35, 65) };
            let roaming = if is_gk { 5 } else if s.in_y >= 65 && !wide { 55 } else { 35 };
            let shooting = if is_gk {
                5
            } else if s.in_y >= 78 {
                78
            } else if s.in_y >= 60 {
                60
            } else {
                32
            };
            SlotInstructionsSnapshot::new(
                s.slot_id.clone(),
                fwd_move,
                pass_risk,
                crossing,
                dribbling,
                def_agg,
                support,
                roaming,
                shooting,
            )
            .expect("clamped slot instructions")
        })
        .collect()
}

pub fn build_auto_set_piece_plan(
    starters: &[PlayerMatchSnapshot],
    assignments: &[LineupAssignmentSnapshot],
    gk_id: &str,
) -> SetPiecePlanSnapshot {
    let slot_by_player: HashMap<&str, &str> = assignments
        .iter()
        .map(|a| (a.player_id.as_str(), a.slot_id.as_str()))
        .collect();

    let outfield: Vec<(&PlayerMatchSnapshot, &str)> = starters
        .iter()
        .filter(|p| p.player_id != gk_id)
        .filter_map(|p| slot_by_player.get(p.player_id.as_str()).map(|&sid| (p, sid)))
        .collect();

    let top_slots = |count: usize, score_fn: &dyn Fn(&PlayerMatchSnapshot) -> i32| -> Vec<String> {
        let mut ranked: Vec<(i32, &str)> = outfield.iter().map(|(p, sid)| (score_fn(p), *sid)).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        ranked.into_iter().take(count).map(|(_, sid)| sid.to_string()).collect()
    };

    let penalty_takers = top_slots(3, &|p| (p.attributes.get(6) as i32) * 4 + (p.attributes.get(14) as i32) * 4 + (p.attributes.get(13) as i32) * 2);
    let corner_takers = top_slots(3, &|p| (p.attributes.get(3) as i32) * 4 + (p.attributes.get(2) as i32) * 3 + (p.attributes.get(4) as i32) * 2 + (p.attributes.get(12) as i32));
    let close_fk_takers = top_slots(3, &|p| (p.attributes.get(6) as i32) * 4 + (p.attributes.get(4) as i32) * 3 + (p.attributes.get(14) as i32) * 2 + (p.attributes.get(2) as i32));
    let far_fk_takers = top_slots(3, &|p| (p.attributes.get(3) as i32) * 4 + (p.attributes.get(2) as i32) * 3 + (p.attributes.get(12) as i32) * 3);
    let captains = top_slots(3, &|p| (p.attributes.get(18) as i32) * 5 + (p.attributes.get(14) as i32) * 2 + (p.attributes.get(13) as i32) * 2 + p.age);

    let primary_corner = corner_takers.first().cloned().unwrap_or_default();
    let primary_fk = far_fk_takers.first().cloned().unwrap_or_default();
    let box_targets = |exclude: &str| -> Vec<String> {
        let mut ranked: Vec<(i32, &str)> = outfield
            .iter()
            .filter(|(_, sid)| *sid != exclude)
            .map(|(p, sid)| {
                let aerial = (p.attributes.get(7) as i32) * 4 + (p.attributes.get(10) as i32) * 3 + (p.attributes.get(15) as i32) * 2 + (p.attributes.get(6) as i32);
                (aerial, *sid)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        ranked.into_iter().take(5).map(|(_, sid)| sid.to_string()).collect()
    };

    SetPiecePlanSnapshot {
        penalty_taker_slot_ids: penalty_takers,
        corner_taker_slot_ids: corner_takers,
        close_free_kick_taker_slot_ids: close_fk_takers,
        far_free_kick_taker_slot_ids: far_fk_takers,
        captain_slot_ids: captains,
        corner_box_slot_ids: box_targets(&primary_corner),
        close_free_kick_box_slot_ids: box_targets(&primary_fk),
        far_free_kick_box_slot_ids: box_targets(&primary_fk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_player(id: &str, pos: &str, base: i32, gk: i32) -> PlayerMatchSnapshot {
        let mut vals = [base as u8; 20];
        vals[0] = gk as u8;
        PlayerMatchSnapshot::new(
            id,
            id,
            PlayerAttributesSnapshot::new(vals).unwrap(),
            92,
            70,
            65,
            25,
            false,
            true,
            pos,
            "",
        )
        .unwrap()
    }

    #[test]
    fn shape_factory_and_validator_accept_standard_433() {
        let shape = shape_from_positions(
            "4-3-3",
            &[
                Position::Gk, Position::Lb, Position::Cb, Position::Cb, Position::Rb,
                Position::Dm, Position::Cm, Position::Cm, Position::Lw, Position::Cf, Position::Rw,
            ],
        );
        let res = validate_shape(&shape);
        assert!(res.is_valid(), "errors={:?}", res.errors);
    }

    #[test]
    fn planner_assigns_gk_and_builds_complete_set_piece_plan() {
        let shape = shape_from_positions(
            "4-3-3",
            &[
                Position::Gk, Position::Lb, Position::Cb, Position::Cb, Position::Rb,
                Position::Dm, Position::Cm, Position::Cm, Position::Lw, Position::Cf, Position::Rw,
            ],
        );
        let squad = vec![
            mk_player("P_GK", "GK", 50, 86),
            mk_player("P_GK2", "GK", 45, 72),
            mk_player("P_LB", "LB", 76, 20),
            mk_player("P_CB1", "CB", 80, 20),
            mk_player("P_CB2", "CB", 78, 20),
            mk_player("P_RB", "RB", 75, 20),
            mk_player("P_DM", "DM", 79, 20),
            mk_player("P_CM1", "CM", 81, 20),
            mk_player("P_CM2", "CM", 77, 20),
            mk_player("P_LW", "LW", 82, 20),
            mk_player("P_CF", "CF", 85, 20),
            mk_player("P_RW", "RW", 80, 20),
            mk_player("P_SUB1", "CB", 70, 20),
            mk_player("P_SUB2", "CM", 71, 20),
            mk_player("P_SUB3", "CF", 72, 20),
        ];
        let plan = plan_optimal_lineup(&squad, &shape.slots, 7).expect("planned");
        assert_eq!(plan.designated_goalkeeper_id, "P_GK");
        assert_eq!(plan.starters.len(), 11);
        assert_eq!(plan.bench.len(), 4);
        let sp = build_auto_set_piece_plan(&plan.starters, &plan.assignments, &plan.designated_goalkeeper_id);
        assert_eq!(sp.penalty_taker_slot_ids.len(), 3);
        assert_eq!(sp.corner_box_slot_ids.len(), 5);
    }
}
