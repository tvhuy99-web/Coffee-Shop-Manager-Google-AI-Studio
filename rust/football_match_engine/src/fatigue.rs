#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task { Idle, Press, Recovery, SupportRun, RoamingRun, BallCarrier }
#[derive(Clone, Copy, Debug)]
pub struct FatigueInput {
    pub duration_seconds:i32, pub distance_tenths:i32, pub tempo:i32,
    pub pressing_intensity:i32, pub transition_risk:i32, pub active_outfield:i32,
    pub stamina:i32, pub task:Task, pub assigned_defensive_role:bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FatigueState {
    pub energy:i32, pub workload:i64, pub distance_tenths:i64,
    pub fatigue_accumulator:i64, pub acute_fatigue:i32,
}
impl FatigueState {pub fn fresh()->Self {Self {energy:100,workload:0,distance_tenths:0,fatigue_accumulator:0,acute_fatigue:0}}}
fn java_round_nonnegative(v:f64)->i32 { (v+0.5).floor() as i32 }
pub fn apply(state:&mut FatigueState, input: FatigueInput, on_pitch:bool) {
    if !on_pitch {return}
    let s=input.duration_seconds; let mut work=s.max(0)+input.distance_tenths.max(0)/5;
    match input.task {
        Task::Press=>work+=s*3, Task::Recovery=>work+=s*2,
        Task::SupportRun|Task::RoamingRun=>work+=s,
        Task::BallCarrier=>work+=1.max(s/2), Task::Idle=>(),
    }
    if input.task==Task::Press {
        let pressing_load=1.0+(input.pressing_intensity.clamp(0,100)-50) as f64*0.0060;
        work=java_round_nonnegative(work as f64*pressing_load).max(0);
    }
    if matches!(input.task,Task::SupportRun|Task::RoamingRun) {
        let overdrive=(input.transition_risk.clamp(0,100)-50).max(0);
        work+=s*overdrive/65;
    }
    let missing=(10-input.active_outfield.clamp(0,10)).max(0);
    if missing>0&&input.assigned_defensive_role {work+=s*missing/2;}
    let tempo_load=1.0+(input.tempo.clamp(0,100)-50) as f64*0.0020;
    work=java_round_nonnegative(work as f64*tempo_load).max(0);
    state.workload+=work as i64;
    state.distance_tenths+=input.distance_tenths.max(0) as i64;
    let burden=(work as i64)*(130-input.stamina) as i64;
    let accumulator=state.fatigue_accumulator+burden;
    let energy_loss=(accumulator/20000) as i32;
    state.fatigue_accumulator=accumulator%20000;
    if energy_loss>0 {state.energy=(state.energy-energy_loss).max(0);}
    state.acute_fatigue=(100-state.energy).max(0);
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn out_of_pitch_does_not_drain() {
        let mut s=FatigueState::fresh();let i=FatigueInput{duration_seconds:20,distance_tenths:45,tempo:68,pressing_intensity:77,transition_risk:55,active_outfield:10,stamina:70,task:Task::Press,assigned_defensive_role:false};
        apply(&mut s,i,false);assert_eq!(s.energy,100);assert_eq!(s.workload,0);
        apply(&mut s,i,true);assert!(s.workload>20);
    }
}
