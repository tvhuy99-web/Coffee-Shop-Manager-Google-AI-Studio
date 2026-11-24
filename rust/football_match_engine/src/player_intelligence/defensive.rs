//! Coordinated defensive intentions from one read-only observation.
//! No outcome probabilities, tackles, physical displacement or score writes.
use crate::v4::spatial::{attacking_progress_m,TeamSideV4,Vec2};
use super::support::ObservedPlayer;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub(crate) enum DefensiveRole { Press, Screen, Cover, Mark }

#[derive(Debug,Clone,Copy)]
pub(crate) struct DefenderObservation<'a> {
    pub idx:usize,
    pub id:&'a str,
    pub pos:Vec2,
    pub shape:Vec2,
    pub speed:f32,
    pub work:f32,
    pub reading:f32,
    pub energy:f32,
}

#[derive(Debug,Clone,Copy)]
pub(crate) struct DefensiveIntent {
    pub idx:usize,
    pub target:Vec2,
    pub role:DefensiveRole,
    /// Observed attacking player whose receiving option is denied.
    pub marked_idx:Option<usize>,
}

fn toward(from:Vec2,to:Vec2,dist:f32)->Vec2 {
    let len=from.distance(to);
    if len<0.001 {return from;}
    let f=(dist/len).clamp(0.0,1.0);
    Vec2::new(from.x+(to.x-from.x)*f,from.y+(to.y-from.y)*f).clamp_to_pitch()
}

fn choose<'a>(defenders:&'a [DefenderObservation<'a>],target:Vec2,used:&[DefensiveIntent],
    radius:f32,shape_budget:f32)->Option<&'a DefenderObservation<'a>> {
    defenders.iter().filter(|p|!used.iter().any(|a|a.idx==p.idx)
        && p.pos.distance(target)<=radius && p.shape.distance(target)<=shape_budget)
        .min_by(|a,b|{
            let cost=|p:&DefenderObservation<'_>|p.pos.distance(target)/p.speed.max(1.0)
                +0.11*p.shape.distance(target)+0.55*(1.0-p.reading)+0.40*(1.0-p.work)
                +0.65*(1.0-p.energy);
            cost(a).total_cmp(&cost(b)).then_with(||a.id.cmp(b.id))
        })
}

/// Intensity governs willingness to leave shape, not magical extra reach.
/// Press the ball from the own-goal side, screen a threatening receiver,
/// and keep a deeper cover player. Unassigned players retain their shape.
pub(crate) fn assign_defense(def_side:TeamSideV4,ball:Vec2,carrier:usize,
    players:&[ObservedPlayer],defenders:&[DefenderObservation<'_>],
    intensity:f32,transition:bool)->Vec<DefensiveIntent> {
    let press=intensity.clamp(0.0,1.0);
    let dir=if def_side==TeamSideV4::Home{-1.0}else{1.0};
    let own_goal=Vec2::new(if def_side==TeamSideV4::Home{0.0}else{105.0},34.0);
    let mut result=Vec::with_capacity(3);
    // Jockeying is NOT a tackle. Keep a body/ball safety gap outside
    // ordinary foot reach. A separate, committed contact path is required
    // to enter tackling range. This is a geometric modeling assumption,
    // not an adjustment based on match goals or foul counts.
    let press_target=toward(ball,own_goal,2.0);
    let radius=5.0+17.0*press+if transition{3.0*press}else{0.0};
    let budget=6.0+18.0*press;
    if let Some(p)=choose(defenders,press_target,&result,radius,budget) {
        result.push(DefensiveIntent{idx:p.idx,target:press_target,role:DefensiveRole::Press,marked_idx:Some(carrier)});
    }else{return result;}
    // Choose the most threatening nearby outlet by progression towards
    // our goal, proximity, and openness. Do not use hidden AI intentions.
    let att=def_side.opposite();
    let receiver=players.iter().filter(|p|p.side==att && p.idx!=carrier && !p.is_gk
        && (5.0..=27.0).contains(&ball.distance(p.pos)))
        .max_by(|a,b|{
            let threat=|p:&ObservedPlayer| {
                let progress=attacking_progress_m(att,p.pos)/105.0;
                let central=1.0-(p.pos.y-34.0).abs()/34.0;
                let open=super::support::lane_openness(att,ball,p.pos,players);
                0.5*progress+0.20*central+0.20*open+0.10*(1.0-ball.distance(p.pos)/27.0)
            };
            threat(a).total_cmp(&threat(b)).then_with(||b.idx.cmp(&a.idx))
        });
    if let Some(r)=receiver {
        // Screen the passing channel, staying nearer the receiver than
        // the carrier to avoid piling two defenders onto the same ball.
        let target=toward(r.pos,ball,(ball.distance(r.pos)*0.28).clamp(1.5,5.0));
        if target.distance(press_target)>3.0 {
            if let Some(p)=choose(defenders,target,&result,7.0+12.0*press,5.0+13.0*press) {
                result.push(DefensiveIntent{idx:p.idx,target,role:DefensiveRole::Screen,marked_idx:Some(r.idx)});
            }
        }
    }
    // Cover goal-side of the press, shifted towards the dangerous inside
    // channel. Keep a real gap to press/screen and stay near tactical shape.
    let target=Vec2::new(ball.x+dir*(5.0+3.0*press),ball.y+(34.0-ball.y)*0.23).clamp_to_pitch();
    if result.iter().all(|a|a.target.distance(target)>3.0) {
        if let Some(p)=choose(defenders,target,&result,8.0+8.0*press,5.0+9.0*press) {
            result.push(DefensiveIntent{idx:p.idx,target,role:DefensiveRole::Cover,marked_idx:None});
        }
    }
    // Regional marking is an additional responsibility for players not
    // already pressing, screening or covering. No extra defender is
    // created; only an existing shape destination can be replaced.
    assign_regional_marks(def_side,ball,carrier,players,defenders,press,transition,&mut result);
    result
}

/// Mark the goal-side shoulder of an unassigned threat inside a defender's
/// own tactical neighborhood. Select pairs jointly, not independently.
/// Stay in shape when marginal denial does not justify leaving the zone.
/// No opponent future intent or hidden attribute is consulted.
fn assign_regional_marks(def_side:TeamSideV4,ball:Vec2,carrier:usize,
    players:&[ObservedPlayer],defenders:&[DefenderObservation<'_>],
    press:f32,transition:bool,result:&mut Vec<DefensiveIntent>) {
    let att=def_side.opposite();
    let own_goal=Vec2::new(if def_side==TeamSideV4::Home{0.0}else{105.0},34.0);
    // More aggressive instructions permit a larger local hand-off zone.
    // These are spatial priors, not target counts of marks or tackles.
    let zone=7.0+4.0*press;
    let reach=10.0+5.0*press;
    loop {
        let mut best:Option<(usize,usize,Vec2,f32,&str)>=None;
        for r in players.iter().filter(|p|p.side==att && p.idx!=carrier && !p.is_gk
            && ball.distance(p.pos)<=34.0 && attacking_progress_m(att,p.pos)>=attacking_progress_m(att,ball)-8.0) {
            if result.iter().any(|a|a.marked_idx==Some(r.idx)){continue;}
            let target=toward(r.pos,own_goal,1.8);
            if result.iter().any(|a|a.target.distance(target)<3.0){continue;}
            let lane=super::support::lane_openness(att,ball,r.pos,players);
            let progress=attacking_progress_m(att,r.pos)/105.0;
            let central=(1.0-(r.pos.y-34.0).abs()/34.0).clamp(0.0,1.0);
            let threat=(0.42*progress+0.28*lane+0.18*central+
                0.12*(1.0-ball.distance(r.pos)/34.0)).clamp(0.0,1.0);
            for p in defenders {
                if result.iter().any(|a|a.idx==p.idx){continue;}
                let deviation=p.shape.distance(target);
                let travel=p.pos.distance(target);
                if deviation>zone || travel>reach {continue;}
                // If the original shape already denies the same receiving
                // space, the added action has little marginal benefit.
                let existing=(1.0-p.shape.distance(r.pos)/5.0).clamp(0.0,1.0);
                let benefit=threat*(1.0-existing)*(0.55+0.45*p.reading);
                let cost=0.18+0.29*deviation/zone+0.14*travel/reach+
                    0.10*(1.0-p.energy)+if transition{0.08}else{0.0};
                let score=benefit-cost;
                if score<=0.0 {continue;}
                if best.as_ref().map_or(true,|(_,ri,_,old,id)|score>*old+1e-6 ||
                    ((score-*old).abs()<=1e-6 && (p.id,r.idx)<(*id,*ri))) {
                    best=Some((p.idx,r.idx,target,score,p.id));
                }
            }
        }
        match best {
            Some((idx,ri,target,_,_))=>result.push(DefensiveIntent{idx,target,role:DefensiveRole::Mark,marked_idx:Some(ri)}),
            None=>break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn defender(idx:usize,id:&str,x:f32,y:f32)->DefenderObservation<'_>{
        let pos=Vec2::new(x,y);
        DefenderObservation{idx,id,pos,shape:pos,speed:6.0,work:0.8,reading:0.8,energy:0.9}
    }
    #[test]
    fn pi_defense_assigns_distinct_press_screen_and_cover(){
        let ball=Vec2::new(52.0,34.0);
        let defenders=[defender(2,"A",55.0,34.0),defender(3,"B",59.0,44.0),
            defender(4,"C",64.0,34.0),defender(5,"D",95.0,8.0)];
        let players=[ObservedPlayer{idx:0,side:TeamSideV4::Home,pos:ball,is_gk:false},
            ObservedPlayer{idx:1,side:TeamSideV4::Home,pos:Vec2::new(60.0,46.0),is_gk:false}];
        let a=assign_defense(TeamSideV4::Away,ball,0,&players,&defenders,0.86,false);
        assert_eq!(a.len(),3);
        assert_eq!(a[0].role,DefensiveRole::Press);
        assert_eq!(a[1].role,DefensiveRole::Screen);
        assert_eq!(a[2].role,DefensiveRole::Cover);
        assert!((a[0].target.distance(ball)-2.0).abs()<0.001,
            "containment must stay outside ordinary tackle reach");
        assert!(a[2].target.x>ball.x);
        assert!(a.iter().all(|x|x.idx!=5));
        for i in 0..a.len(){for j in i+1..a.len(){assert_ne!(a[i].idx,a[j].idx);}}
    }
    #[test]
    fn pi_defense_regional_marks_are_unique_goal_side_and_local(){
        let ball=Vec2::new(64.0,34.0);
        let defenders=[defender(2,"A",75.0,23.0),defender(3,"B",75.0,45.0),
            defender(4,"C",100.0,3.0)];
        let players=[ObservedPlayer{idx:0,side:TeamSideV4::Home,pos:ball,is_gk:false},
            ObservedPlayer{idx:1,side:TeamSideV4::Home,pos:Vec2::new(80.0,23.0),is_gk:false},
            ObservedPlayer{idx:5,side:TeamSideV4::Home,pos:Vec2::new(80.0,45.0),is_gk:false}];
        let mut result=Vec::new();
        assign_regional_marks(TeamSideV4::Away,ball,0,&players,&defenders,0.86,false,&mut result);
        assert_eq!(result.len(),2);
        assert!(result.iter().all(|a|a.role==DefensiveRole::Mark && a.idx!=4));
        assert_ne!(result[0].idx,result[1].idx);
        assert_ne!(result[0].marked_idx,result[1].marked_idx);
        for a in &result {
            let r=players.iter().find(|p|Some(p.idx)==a.marked_idx).unwrap();
            assert!(a.target.x>r.pos.x && (a.target.distance(r.pos)-1.8).abs()<0.001);
            let p=defenders.iter().find(|p|p.idx==a.idx).unwrap();
            assert!(a.target.distance(p.shape)<=7.0+4.0*0.86);
        }
        let mut already=vec![DefensiveIntent{idx:2,target:Vec2::new(81.0,23.0),
            role:DefensiveRole::Screen,marked_idx:Some(1)}];
        assign_regional_marks(TeamSideV4::Away,ball,0,&players,&defenders,0.86,false,&mut already);
        assert_eq!(already.len(),2);
        assert_eq!(already[1].marked_idx,Some(5));
    }

    #[test]
    fn pi_defense_regional_marking_does_not_chase_outside_zone(){
        let ball=Vec2::new(50.0,34.0);
        let defenders=[defender(2,"A",80.0,10.0)];
        let players=[ObservedPlayer{idx:1,side:TeamSideV4::Home,pos:Vec2::new(62.0,45.0),is_gk:false}];
        let mut result=Vec::new();
        assign_regional_marks(TeamSideV4::Away,ball,0,&players,&defenders,0.86,false,&mut result);
        assert!(result.is_empty());
    }
    #[test]
    fn pi_defense_high_press_engages_from_farther_and_mirrors(){
        let ball=Vec2::new(42.0,34.0);
        let defs=[defender(1,"A",57.0,34.0)];
        assert!(assign_defense(TeamSideV4::Away,ball,0,&[],&defs,0.28,false).is_empty());
        let high=assign_defense(TeamSideV4::Away,ball,0,&[],&defs,0.86,false);
        assert_eq!(high.len(),1);
        let mirrored=[defender(1,"A",48.0,34.0)];
        let other=assign_defense(TeamSideV4::Home,Vec2::new(63.0,34.0),0,&[],&mirrored,0.86,false);
        assert_eq!(other.len(),1);
        assert!((high[0].target.x+other[0].target.x-105.0).abs()<0.001);
        assert_eq!(high[0].target.y,other[0].target.y);
    }
}
