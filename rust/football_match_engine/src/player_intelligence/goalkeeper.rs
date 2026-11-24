//! Goalkeeper distribution intention only; saving and claiming remain engine outcomes.
use crate::rng::DeterministicRng;
use crate::v4::attributes::{normalize_attr, PlayerAttributes54};
use crate::v4::spatial::{TeamSideV4, Vec3};
use crate::v4::state::MatchStateV4;

/// Selects a teammate for GK distribution (`throwing` short/quick vs `kicking` long),
/// returning `(target_idx, is_long_kick, target_ball_pos)`.
pub fn choose_gk_distribution(
    state: &MatchStateV4,
    gk_attrs: &PlayerAttributes54,
    gk_side: TeamSideV4,
    rng: &mut DeterministicRng,
) -> (usize, bool, Vec3) {
    let kick = normalize_attr(gk_attrs.goalkeeping.kicking);
    let throw = normalize_attr(gk_attrs.goalkeeping.throwing);
    let prefer_long = (0.35 + (kick - throw) * 0.45).clamp(0.15, 0.80);
    let is_long = (rng.next_double() as f32) < prefer_long;

    let mut best_idx = None;
    let mut best_score = f32::NEG_INFINITY;
    for (idx, p) in state.players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != gk_side || p.is_goalkeeper {
            continue;
        }
        let dist = p.pos.distance(state.ball.pos.xy());
        let score = if is_long {
            dist
        } else {
            -dist
        };
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    let target_idx = best_idx.unwrap_or(0);
    let t_pos = state.players[target_idx].pos;
    let z = if is_long { 4.2 } else { 0.8 };
    (target_idx, is_long, Vec3::new(t_pos.x, t_pos.y, z).clamp_to_pitch())
}
