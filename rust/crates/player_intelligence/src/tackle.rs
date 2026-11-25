//! Subjective tackle utility. Physical geometry factors are supplied by the engine forecast.
use super::utility::territory_value;

#[derive(Debug,Clone,Copy)]
pub struct TackleObservation {
    pub access:f32, // visible approach angle relative to the carrier's facing
    pub relative_speed:f32,
    pub body_overlap:f32,
    pub reach_time_s:f32,
    pub cover:f32,
    pub threat:f32,
    pub progress:f32,
    pub y:f32,
    pub own_box:bool,
    pub skill:f32,
    pub tackling:f32,
    pub decisions:f32,
    pub aggression:f32,
    pub composure:f32,
    pub energy:f32,
    pub intensity:f32,
    pub control_given_clean:f32,
    pub recovery_given_loose:f32,
}

#[derive(Debug,Clone,Copy)]
pub struct TackleEvaluation {
    pub tackle_utility:f32,
    pub contain_utility:f32,
    pub access:f32,
    /// Legal ball contact, not necessarily controlled possession.
    pub clean_win_probability:f32,
    pub foul_probability:f32,
    pub controlled_probability:f32,
    pub loose_probability:f32,
    pub recovery_given_loose:f32,
}
impl TackleEvaluation {
    pub fn commits(self)->bool {self.tackle_utility>self.contain_utility}
}

pub fn evaluate(o:TackleObservation, access_factor:f32, speed_factor:f32)->TackleEvaluation {
    let access=o.access.clamp(0.0,1.0);
    let skill=o.skill.clamp(0.0,1.0);
    let cover=o.cover.clamp(0.0,1.0);
    let energy=o.energy.clamp(0.0,1.0);
    let threat=o.threat.clamp(0.0,1.0);

    // A neutral opponent prior and observable ball access. Estimate the
    // chance of getting the better of the duel before judging legality.
    // This is a fallible PI forecast, not the engine's actual roll.
    let duel_win=((0.50+0.65*(skill-0.55))*access_factor
        *(0.75+0.25*energy)*speed_factor).clamp(0.02,0.82);
    // As in execution, a ball win can be unsafe, and a missed ball
    // can independently produce an unsafe body contact. Retain the
    // existing neutral discipline/referee prior: PI is not an oracle.
    let foul_propensity=(0.14+0.14*o.aggression+0.07-0.10*o.tackling).clamp(0.05,0.42);
    let missed_ball=1.0-duel_win;
    let missed_ball_foul=missed_ball*o.body_overlap.clamp(0.0,1.0)*foul_propensity;
    let foul=duel_win*foul_propensity+missed_ball_foul;
    let clean=duel_win*(1.0-foul_propensity);
    // Exclusive outcomes: a missed-ball foul must not also pay the
    // ordinary miss cost in the utility comparison.
    let miss=missed_ball-missed_ball_foul;

    // Both choices are measured relative to current opponent possession.
    // Winning changes the sign of possession value, including the value
    // of regaining the ball high up the pitch. Missing a tackle incurs
    // only the incremental exposure, not the full existing threat again.
    let before=territory_value(o.progress,o.y);
    let recovered=territory_value(105.0-o.progress,o.y);
    let gain=before+recovered;
    let missed_cost=before*(0.18+0.52*threat)*(1.0-0.65*cover)
        +0.012*(1.0-cover);
    // A penalty replaces (rather than adds to) the existing opportunity.
    // A non-penalty free kick is a modest incremental restart risk.
    let foul_cost=if o.own_box {(0.76-before).max(0.0)} else {0.012+0.25*before};
    let commitment_cost=0.006+0.008*o.reach_time_s.clamp(0.0,3.0)+0.010*(1.0-energy);
    let risk_weight=0.75+0.40*o.decisions+0.15*o.composure-0.15*o.aggression;
    // Split the same legal-contact probability. A loose ball that the
    // opponent recovers is not a won possession; value that branch as
    // continuation of the original possession, not as another foul/miss.
    let controlled=clean*o.control_given_clean.clamp(0.0,1.0);
    let loose=clean-controlled;
    let recovery=o.recovery_given_loose.clamp(0.0,1.0);
    let tackle_utility=(controlled+loose*recovery)*gain
        -risk_weight*(miss*missed_cost+foul*foul_cost)-commitment_cost;
    // Continued containment does not guarantee a turnover. Its marginal
    // benefit is denial of a portion of the current possession value.
    let contain_utility=before*(0.10+0.10*cover)*(1.15-0.40*o.intensity);
    TackleEvaluation{tackle_utility,contain_utility,access,
        clean_win_probability:clean,foul_probability:foul,
        controlled_probability:controlled,loose_probability:loose,recovery_given_loose:recovery}
}

