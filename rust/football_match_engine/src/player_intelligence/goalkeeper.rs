//! Engine adapter projects only visible fields, preserving roster indices and RNG order.
use crate::contracts::{RngSampleSource,PlayerAttributes54,TeamSideV4,Vec3};
use crate::v4::state::MatchStateV4;
use ::player_intelligence::goalkeeper::{DistributionPlayer,choose_gk_distribution as choose};

pub fn choose_gk_distribution(state:&MatchStateV4,gk_attrs:&PlayerAttributes54,gk_side:TeamSideV4,rng:&mut impl RngSampleSource)->(usize,bool,Vec3) {
    let players:Vec<_>=state.players.iter().map(|p|DistributionPlayer{on_pitch:p.on_pitch,sent_off:p.sent_off,side:p.side,is_goalkeeper:p.is_goalkeeper,pos:p.pos}).collect();
    choose(&players,state.ball.pos.xy(),gk_attrs,gk_side,rng)
}
