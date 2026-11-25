use crate::rng::{DeterministicRng,derive_seed};
use crate::geometry::TeamSide;
pub fn adjustment(match_seed:i64,side:TeamSide,team_id:&str)->f64 {
    let name=if side==TeamSide::Home {"HOME"}else{"AWAY"};
    let seed=derive_seed(match_seed,&format!("MATCH_DAY:{}:{}",name,team_id));
    let mut rng=DeterministicRng::new(seed);
    let mut centered=-3.0;
    for _ in 0..6 {centered+=rng.next_double();}
    centered*3.2
}
