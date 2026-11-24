//! Attacking support selection from a shared read-only snapshot.
//! Shape anchors and Law 11 limits remain authoritative constraints;
//! the result is an intention, not movement or a promised reception.
use crate::v4::spatial::{attacking_progress_m,point_to_segment_distance_m,TeamSideV4,Vec2,PITCH_LENGTH_M};

#[derive(Clone,Copy,Debug)]
pub(crate) struct ObservedPlayer {
    pub idx:usize,
    pub side:TeamSideV4,
    pub pos:Vec2,
    pub is_gk:bool,
}

#[derive(Clone,Copy)]
pub(crate) struct SupportObservation<'a> {
    pub idx:usize,
    pub side:TeamSideV4,
    pub pos:Vec2,
    pub ball:Vec2,
    pub shape:Vec2,
    pub players:&'a [ObservedPlayer],
    pub awareness:f32,
    pub reach_m:f32,
    pub legal_limit:Option<f32>,
}

/// A cheap visible-lane proxy, deliberately not a guaranteed physical pass.
/// A defender at the source already creates pressure; an opponent between
/// source and receiver obscures the lane. Exclude teammates and keepers.
pub(crate) fn lane_openness(side:TeamSideV4,from:Vec2,to:Vec2,players:&[ObservedPlayer])->f32 {
    let len=from.distance(to);
    if len<1.0{return 0.0;}
    let mut risk=0.0f32;
    for p in players.iter().filter(|p|p.side!=side && !p.is_gk) {
        if p.pos.distance(from)<1.0 {continue;}
        let d=point_to_segment_distance_m(p.pos,from,to);
        risk=risk.max((1.0-d/3.0).clamp(0.0,1.0));
    }
    1.0-risk
}

fn legalize(mut p:Vec2,side:TeamSideV4,limit:Option<f32>)->Vec2 {
    p=p.clamp_to_pitch();
    if let Some(line)=limit {
        let l=line.clamp(0.0,PITCH_LENGTH_M);
        if attacking_progress_m(side,p)>l {p.x=if side==TeamSideV4::Home{l}else{PITCH_LENGTH_M-l};}
    }
    p
}

fn value(o:SupportObservation<'_>,target:Vec2)->f32 {
    let distance=o.ball.distance(target);
    let lane=lane_openness(o.side,o.ball,target,o.players);
    let space=o.players.iter().filter(|p|p.side!=o.side)
        .map(|p|p.pos.distance(target)).fold(12.0f32,f32::min);
    let separation=o.players.iter().filter(|p|p.side==o.side && p.idx!=o.idx)
        .map(|p|p.pos.distance(target)).fold(12.0f32,f32::min);
    // Useful nearby outlet, but no reward for crowding the carrier.
    let outlet=if distance<6.0 {distance/6.0-1.0}
        else {(1.0-(distance-13.0).abs()/26.0).clamp(0.0,1.0)};
    let advance=(attacking_progress_m(o.side,target)-attacking_progress_m(o.side,o.ball))/25.0;
    let shape_cost=o.shape.distance(target)/12.0;
    let move_cost=o.pos.distance(target)/o.reach_m.max(1.0);
    (0.40*lane+0.23*(space/9.0).clamp(0.0,1.0)+0.18*outlet
        +0.14*(separation/7.0).clamp(0.0,1.0)+0.05*advance.clamp(-1.0,1.0))
        *(0.55+0.45*o.awareness)-0.18*shape_cost-0.055*move_cost.min(4.0)
}

/// Find a nearby opening around the tactical destination, without every
/// teammate chasing the same global optimum. Remote players retain shape.
pub(crate) fn select_support_target(o:SupportObservation<'_>)->Vec2 {
    let shape=legalize(o.shape,o.side,o.legal_limit);
    if o.ball.distance(shape)>36.0 || o.awareness<0.10 {return shape;}
    let radius=3.0+5.0*o.awareness.clamp(0.0,1.0);
    let dir=if o.side==TeamSideV4::Home{1.0}else{-1.0};
    let mut best=shape;
    let mut score=value(o,shape);
    for (dx,dy) in [(1.0,0.0),(-1.0,0.0),(0.0,-1.0),(0.0,1.0),(-0.707,-0.707),(-0.707,0.707),(0.707,-0.707),(0.707,0.707)] {
        let p=legalize(Vec2::new(shape.x+dir*dx*radius,shape.y+dy*radius),o.side,o.legal_limit);
        let s=value(o,p);
        if s>score+1e-5 {best=p;score=s;}
    }
    best
}

/// Number of visible, currently available short/medium outlets. Used only
/// for evidence, never for quota feedback. Legal receiving line supplied
/// by the engine. A nearby opponent makes a receiver contested.
pub(crate) fn count_open_outlets(side:TeamSideV4,carrier:usize,ball:Vec2,players:&[ObservedPlayer],line:Option<f32>)->u32 {
    players.iter().filter(|p|p.idx!=carrier && p.side==side && !p.is_gk
        && (4.0..=28.0).contains(&ball.distance(p.pos))
        && line.map_or(true,|l|attacking_progress_m(side,p.pos)<=l+0.001)
        && lane_openness(side,ball,p.pos,players)>=0.65
        && players.iter().filter(|q|q.side!=side).all(|q|q.pos.distance(p.pos)>=3.0)).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pi_support_opens_blocked_lane_and_respects_shape() {
        let players=[ObservedPlayer{idx:0,side:TeamSideV4::Home,pos:Vec2::new(45.0,34.0),is_gk:false},
            ObservedPlayer{idx:1,side:TeamSideV4::Home,pos:Vec2::new(58.0,34.0),is_gk:false},
            ObservedPlayer{idx:2,side:TeamSideV4::Away,pos:Vec2::new(52.0,34.0),is_gk:false}];
        let o=SupportObservation{idx:1,side:TeamSideV4::Home,pos:players[1].pos,ball:players[0].pos,
            shape:players[1].pos,players:&players,awareness:0.95,reach_m:6.0,legal_limit:Some(60.0)};
        let p=select_support_target(o);
        assert!(lane_openness(o.side,o.ball,p,&players)>lane_openness(o.side,o.ball,o.shape,&players));
        assert!(p.distance(o.shape)<=8.001 && p.x<=60.001);
        assert_eq!(count_open_outlets(o.side,0,o.ball,&players,Some(60.0)),0);
        let mut moved=players; moved[1].pos=p;
        assert_eq!(count_open_outlets(o.side,0,o.ball,&moved,Some(60.0)),1);
    }
    #[test]
    fn pi_support_mirrors_and_preserves_distant_shape() {
        let players=[ObservedPlayer{idx:2,side:TeamSideV4::Away,pos:Vec2::new(52.0,34.0),is_gk:false}];
        let o=SupportObservation{idx:1,side:TeamSideV4::Home,pos:Vec2::new(58.0,34.0),ball:Vec2::new(45.0,34.0),
            shape:Vec2::new(58.0,34.0),players:&players,awareness:0.95,reach_m:6.0,legal_limit:Some(60.0)};
        let p=select_support_target(o);
        let mirror=[ObservedPlayer{side:TeamSideV4::Home,pos:Vec2::new(53.0,34.0),..players[0]}];
        let q=select_support_target(SupportObservation{side:TeamSideV4::Away,pos:Vec2::new(47.0,34.0),
            ball:Vec2::new(60.0,34.0),shape:Vec2::new(47.0,34.0),players:&mirror,..o});
        assert!((p.x+q.x-105.0).abs()<0.001 && (p.y-q.y).abs()<0.001);
        let far=SupportObservation{shape:Vec2::new(12.0,5.0),..o};
        assert_eq!(select_support_target(far),far.shape);
    }
}
