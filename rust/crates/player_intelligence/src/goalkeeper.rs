//! Read-only goalkeeper distribution policy. No authoritative world or engine access.
use match_contracts::attributes::{normalize_attr, PlayerAttributes54};
use match_contracts::ports::RngSampleSource;
use match_contracts::spatial::{TeamSideV4, Vec2, Vec3};

#[derive(Debug, Clone, Copy)]
pub struct DistributionPlayer {
    pub on_pitch: bool,
    pub sent_off: bool,
    pub side: TeamSideV4,
    pub is_goalkeeper: bool,
    pub pos: Vec2,
}

pub fn choose_gk_distribution(
    players: &[DistributionPlayer], ball_pos: Vec2, gk_attrs: &PlayerAttributes54,
    gk_side: TeamSideV4, rng: &mut impl RngSampleSource,
) -> (usize, bool, Vec3) {
    let kick = normalize_attr(gk_attrs.goalkeeping.kicking);
    let throw = normalize_attr(gk_attrs.goalkeeping.throwing);
    let prefer_long = (0.35 + (kick - throw) * 0.45).clamp(0.15, 0.80);
    let is_long = (rng.next_double() as f32) < prefer_long;

    let mut best_idx = None;
    let mut best_score = f32::NEG_INFINITY;
    for (idx, p) in players.iter().enumerate() {
        if !p.on_pitch || p.sent_off || p.side != gk_side || p.is_goalkeeper {
            continue;
        }
        let dist = p.pos.distance(ball_pos);
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
    let t_pos = players[target_idx].pos;
    let z = if is_long { 4.2 } else { 0.8 };
    (target_idx, is_long, Vec3::new(t_pos.x, t_pos.y, z).clamp_to_pitch())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Sample { value: f64, draws: usize }
    impl RngSampleSource for Sample {
        fn next_long(&mut self)->i64 { panic!("unexpected draw") }
        fn next_int(&mut self,_:i32)->i32 { panic!("unexpected draw") }
        fn next_boolean(&mut self)->bool { panic!("unexpected draw") }
        fn next_double(&mut self)->f64 { self.draws+=1;self.value }
    }
    #[test]
    fn distribution_uses_one_draw_and_only_eligible_teammates() {
        let p=DistributionPlayer{on_pitch:true,sent_off:false,side:TeamSideV4::Home,is_goalkeeper:false,pos:Vec2::new(20.0,34.0)};
        let players=[DistributionPlayer{is_goalkeeper:true,..p},p,DistributionPlayer{pos:Vec2::new(60.0,34.0),..p},DistributionPlayer{side:TeamSideV4::Away,pos:Vec2::new(100.0,34.0),..p},DistributionPlayer{sent_off:true,pos:Vec2::new(5.0,34.0),..p}];
        let attrs=PlayerAttributes54::uniform(12).unwrap();
        let mut r=Sample{value:0.0,draws:0};
        assert_eq!(choose_gk_distribution(&players,Vec2::new(5.0,34.0),&attrs,TeamSideV4::Home,&mut r).0,2);
        r.value=0.99;
        assert_eq!(choose_gk_distribution(&players,Vec2::new(5.0,34.0),&attrs,TeamSideV4::Home,&mut r).0,1);
        assert_eq!(r.draws,2);
    }
}
