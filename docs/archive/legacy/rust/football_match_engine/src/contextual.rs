use crate::snapshot::PlayerAttributesSnapshot as Attr;
#[inline] fn attr(a:&Attr,i:usize)->f64 {a.get(i) as f64}
#[inline] fn deviation(v:f64)->f64 {v-74.0}
#[inline] fn scaled_deviation(v:f64)->f64 {(v-74.0)*0.70}
#[inline] fn clamp01(v:f64)->f64 {v.max(0.0).min(1.0)}

pub struct PassContext {pub carrier_pressure:i32,pub target_pressure:i32,pub distance:f64,pub progress_gain:i32,pub tactical_risk:i32,pub lane_risk:i32,pub cover_shadow:i32}
pub struct CrossContext {pub pressure:i32,pub target_distance:f64,pub aerial_difficulty:i32}
pub struct ShotContext {pub pressure:i32,pub progress:i32,pub centrality:i32,pub nearby_defenders:i32,pub header:bool,pub penalty:bool}
pub struct GroundDuelContext {pub dribble:bool,pub pressure:i32,pub progress:i32,pub defensive_aggression:i32,pub energy:i32}
pub struct PressureContext {pub distance:f64,pub press_radius:f64,pub pressing_intensity:i32,pub line_height:i32,pub energy:i32}

pub fn pass_execution_adjustment(a:&Attr,c:&PassContext)->f64 {
    let pressure=clamp01((c.carrier_pressure as f64*0.78+c.target_pressure as f64*0.22)/100.0);
    let distance=clamp01((c.distance-8.0).max(0.0)/34.0);
    let progression=clamp01(c.progress_gain.max(0) as f64/28.0);
    let lane=clamp01((c.lane_risk as f64*0.75+c.cover_shadow.min(100) as f64*0.25)/100.0);
    let risk=clamp01((c.tactical_risk-35).max(0) as f64/65.0);
    let complexity=clamp01(distance*0.28+progression*0.28+lane*0.28+risk*0.16);
    let passing_w=0.172-pressure*0.018+distance*0.012;
    let technique_w=0.070+complexity*0.055;
    let composure_w=0.030+pressure*0.105;
    let decisions_w=0.036+pressure*0.045+complexity*0.050;
    let vision_w=0.014+progression*0.026+lane*0.020+distance*0.012;
    deviation(attr(a,2))*passing_w+deviation(attr(a,4))*technique_w+deviation(attr(a,14))*composure_w+
        deviation(attr(a,13))*decisions_w+deviation(attr(a,12))*vision_w
}

pub fn cross_execution_adjustment(a:&Attr,c:&CrossContext)->f64 {
    let pressure=clamp01(c.pressure as f64/100.0);
    let distance=clamp01(c.target_distance/42.0);
    let aerial=clamp01(c.aerial_difficulty as f64/100.0);
    let complexity=clamp01(distance*0.45+aerial*0.35+pressure*0.20);
    let crossing_w=0.215-pressure*0.020+distance*0.020;
    let technique_w=0.080+complexity*0.040;
    let vision_w=0.040+distance*0.035+aerial*0.020;
    let composure_w=0.020+pressure*0.075;
    let decisions_w=0.025+pressure*0.040+complexity*0.025;
    deviation(attr(a,3))*crossing_w+deviation(attr(a,4))*technique_w+deviation(attr(a,12))*vision_w+
        deviation(attr(a,14))*composure_w+deviation(attr(a,13))*decisions_w
}

pub fn shot_execution_adjustment(a:&Attr,c:&ShotContext)->f64 {
    let pressure=clamp01(c.pressure as f64/100.0);
    let angle_difficulty=clamp01((100-c.centrality) as f64/100.0);
    let range_difficulty=clamp01((88-c.progress).max(0) as f64/28.0);
    let congestion=clamp01(c.nearby_defenders as f64/4.0);
    let context_difficulty=clamp01(pressure*0.40+angle_difficulty*0.23+range_difficulty*0.22+congestion*0.15);
    if c.penalty {
        let shooting_w=0.115;
        let composure_w=0.225;
        let technique_w=0.045;
        let decisions_w=0.020;
        return deviation(attr(a,6))*shooting_w+deviation(attr(a,14))*composure_w+
            deviation(attr(a,4))*technique_w+deviation(attr(a,13))*decisions_w;
    }
    let mut primary_w=if c.header {0.220} else {0.230};
    primary_w-=pressure*0.030;
    let technique_w=0.048+context_difficulty*0.055;
    let composure_w=0.028+pressure*0.120+congestion*0.030;
    let decisions_w=0.010+angle_difficulty*0.030+pressure*0.025;
    let movement_w=if c.header {0.035+congestion*0.020} else {0.018+congestion*0.012};
    let primary=deviation(attr(a,if c.header {7}else{6}))*primary_w;
    primary+deviation(attr(a,4))*technique_w+deviation(attr(a,14))*composure_w+
        deviation(attr(a,13))*decisions_w+deviation(attr(a,15))*movement_w
}

fn normalize(w:&mut [f64],target:f64) {
    let mut total=0.0;
    for v in w.iter() {total+=*v;}
    if total<=0.0 {return}
    let factor=target/total;
    for v in w.iter_mut() {*v*=factor;}
}

pub fn ground_duel_attacker(a:&Attr,c:&GroundDuelContext)->f64 {
    let pressure=clamp01(c.pressure as f64/100.0);
    let open_field=clamp01((100-c.pressure) as f64/100.0)*clamp01((100-c.progress) as f64/55.0);
    let contact=clamp01(0.10+pressure*0.55+c.defensive_aggression as f64/400.0);
    let mut w=if c.dribble {[0.38,0.21,0.10,0.06,0.08,0.07]}else{[0.28,0.22,0.09,0.07,0.10,0.09]};
    w[0]+=pressure*0.025;w[1]+=pressure*0.020;w[2]+=pressure*0.040;
    w[3]+=open_field*0.035;w[4]+=pressure*0.090;w[5]+=contact*0.090;
    normalize(&mut w,0.85);
    let mut value=74.0*0.85;
    value+=scaled_deviation(attr(a,5))*w[0];
    value+=scaled_deviation(attr(a,4))*w[1];
    value+=scaled_deviation(attr(a,9))*w[2];
    value+=scaled_deviation(attr(a,8))*w[3];
    value+=scaled_deviation(attr(a,14))*w[4];
    value+=scaled_deviation(attr(a,10))*w[5];
    value+=c.energy as f64*0.07;
    value
}

pub fn ground_duel_defender(a:&Attr,c:&GroundDuelContext)->f64 {
    let pressure=clamp01(c.pressure as f64/100.0);
    let open_field=clamp01((100-c.pressure) as f64/100.0)*clamp01((100-c.progress) as f64/55.0);
    let contact=clamp01(0.30+pressure*0.45+c.defensive_aggression as f64/240.0);
    let mut w=[0.34,0.25,0.08,0.05,0.10,0.08];
    w[0]+=pressure*0.020;w[1]+=pressure*0.025;
    w[2]+=(if c.dribble {0.035}else{0.010})+pressure*0.020;
    w[3]+=open_field*0.035;w[4]+=contact*0.055;
    w[5]+=pressure*0.045+c.defensive_aggression as f64/1800.0;
    normalize(&mut w,0.90);
    let mut value=74.0*0.90;
    value+=scaled_deviation(attr(a,1))*w[0];
    value+=scaled_deviation(attr(a,16))*w[1];
    value+=scaled_deviation(attr(a,9))*w[2];
    value+=scaled_deviation(attr(a,8))*w[3];
    value+=scaled_deviation(attr(a,10))*w[4];
    value+=scaled_deviation(attr(a,13))*w[5];
    value+=c.energy as f64*0.05;
    value+=(c.defensive_aggression-50) as f64*0.055;
    value
}

pub fn pressure_attribute_adjustment(a:&Attr,c:&PressureContext)->f64 {
    let distance=clamp01(c.distance/c.press_radius.max(1.0));
    let pressing=clamp01(c.pressing_intensity as f64/100.0);
    let high_line=clamp01(c.line_height as f64/100.0);
    let fatigue=clamp01((55-c.energy) as f64/55.0);
    let work_rate_w=0.070+pressing*0.030+distance*0.050;
    let positioning_w=0.040+(1.0-distance)*0.035+high_line*0.012;
    let pace_w=0.012+distance*0.035+high_line*0.004;
    let decisions_w=0.026+pressing*0.020+(1.0-distance)*0.018;
    let stamina_w=fatigue*0.035;
    deviation(attr(a,17))*work_rate_w+deviation(attr(a,16))*positioning_w+
        deviation(attr(a,8))*pace_w+deviation(attr(a,13))*decisions_w+
        deviation(attr(a,11))*stamina_w
}
