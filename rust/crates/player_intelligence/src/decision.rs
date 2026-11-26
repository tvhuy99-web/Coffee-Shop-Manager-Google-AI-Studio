//! Individual policy: immutable observation, narrow forecast port and RNG samples only.
use match_contracts::{attributes::{normalize_attr,normalize_attr_elite},decision_observation::DecisionObservation,ports::{RngSampleSource,SpatialForecastPort,LocalSpatialContext},spatial::{attacking_progress_m,distance_to_attacking_goal,is_in_attacking_penalty_box,PITCH_WIDTH_M},tactical_phase::TacticalPhaseV4};
use super::intent::{CandidateOption,DecisionPlanV4,PassKindV4,PlannedActionV4};
use super::timing::evaluate_one_touch_intent;
use super::utility::{ActionValue,is_switch,possession_value,turnover_cost,shot_goal_probability,shot_value};
use super::on_ball::{choose_carry,choose_hold};

fn perception_quality(state:&DecisionObservation<'_>,idx:usize,pressure_norm:f32)->f32 {
    let carrier=&state.players[idx];
    let attrs=state.attrs;
    let vision=normalize_attr(attrs.mental.vision);
    let concentration=normalize_attr(attrs.mental.concentration);
    let composure=normalize_attr(attrs.mental.composure);
    (vision * 0.45
        + normalize_attr(attrs.mental.anticipation) * 0.25
        + concentration * 0.15
        + state.position_familiarity * 0.15
        - pressure_norm * (0.28 - composure * 0.14))
        .clamp(0.18, 1.0)
}

/// Diagnostic-only forecast, using the same perception model as selection.
/// No RNG, no outcome, no modification of the chosen intention.
pub fn observe_pass_risk(state:&DecisionObservation<'_>,forecast:&impl SpatialForecastPort,idx:usize,
    ctx:LocalSpatialContext,action:PlannedActionV4)->Option<f32> {
    let PlannedActionV4::Pass{target_idx,kind,..}=action else{return None;};
    let carrier=state.players.get(idx)?;
    let target=state.players.get(target_idx)?;
    if !target.on_pitch || target.sent_off || target.side!=carrier.side || target_idx==idx
        || carrier.pos.distance(target.pos)<0.5 {return None;}
    let awareness=perception_quality(state,idx,(ctx.pressure/100.0).clamp(0.0,1.0));
    Some(forecast.pass_risk(carrier.side,awareness,carrier.pos,target_idx,kind))
}

/// Shared candidate scoring. The observation policy controls which pass
/// kinds are noticed. No random draw occurs inside the scorer itself.
fn collect_pass_candidates(
    state:&DecisionObservation<'_>,
    carrier_idx:usize,carrier_ctx:LocalSpatialContext,forecast:&impl SpatialForecastPort,mut notice:impl FnMut(PassKindV4)->bool,
)->Vec<CandidateOption> {
    let carrier=&state.players[carrier_idx];
    let att_side=carrier.side;
    let attrs=state.attrs;
    let ti=state.own_instructions;
    let def_ti=state.opponent_line_height;
    let att_phase=state.phase;
    let vision=normalize_attr(attrs.mental.vision);
    let decisions=normalize_attr(attrs.mental.decisions);
    let concentration=normalize_attr(attrs.mental.concentration);
    let teamwork=normalize_attr(attrs.mental.teamwork);
    let pressure_norm=(carrier_ctx.pressure/100.0).clamp(0.0,1.0);
    let perception_quality=perception_quality(state,carrier_idx,pressure_norm);
    let max_vision_range_m=22.0+perception_quality*40.0;
    let carrier_progress=attacking_progress_m(att_side,carrier.pos);
    let risk_appetite=(ti.build_up_risk as f32/100.0).clamp(0.0,1.0);
    let tempo=ti.tempo as f32/100.0;
    let time_wasting=ti.time_wasting as f32/100.0;
    let loss=turnover_cost(carrier_progress)*(1.15-0.30*risk_appetite);
    let current_value=possession_value(carrier_progress,carrier.pos.y,pressure_norm);
    let urgency=0.75+0.5*tempo;
    let counter=if att_phase==TacticalPhaseV4::AttackingTransition{
        (ti.transition_risk as f32/100.0)*(0.008+0.008*def_ti as f32/100.0)
    }else{0.0};
    let readiness=(state.anaerobic/100.0).clamp(0.25,1.0);
    let mut candidates: Vec<CandidateOption> = Vec::with_capacity(14);
    let offside_line = forecast.offside_reference(att_side, carrier.pos);
    // Perception is fallible, especially under pressure; the Law 11 check remains exact.
    let timing_awareness = (vision * 0.40 + decisions * 0.40 + concentration * 0.20)
        * (1.0 - pressure_norm * 0.25);

    for (t_idx, mate) in state.players.iter().enumerate() {
        if t_idx == carrier_idx || !mate.on_pitch || mate.sent_off || mate.side != att_side {
            continue;
        }
        let dist_m = carrier.pos.distance(mate.pos);
        if dist_m < 3.5 || dist_m > max_vision_range_m {
            continue;
        }

        let mate_progress = attacking_progress_m(att_side, mate.pos);
        let progress_gain = mate_progress - carrier_progress;
        let mate_in_box = is_in_attacking_penalty_box(att_side, mate.pos);

        let carrier_wide = carrier.pos.y < 15.0 || carrier.pos.y > (PITCH_WIDTH_M - 15.0);
        let kind = if carrier_wide && carrier_progress > 65.0 && mate_in_box {
            PassKindV4::Cross
        } else if is_switch(carrier.pos.y, mate.pos.y, dist_m) {
            PassKindV4::Switch
        } else if progress_gain >= 16.0 && mate_progress >= 75.0 && (14.0..=34.0).contains(&dist_m)
        {
            PassKindV4::Through
        } else if dist_m >= 28.0 {
            PassKindV4::Long
        } else {
            PassKindV4::Short
        };

        if !notice(kind) {continue;}

        let (lane_risk, interceptor_idx) = forecast.passing_lane(
            att_side,
            carrier.pos,
            mate.pos,
            kind.is_aerial(),
        );
        let mate_ctx = forecast.local_context(att_side, mate.pos);
        let target_pressure = (mate_ctx.pressure / 100.0).clamp(0.0, 1.0);
        let skill = match kind {
            PassKindV4::Short => normalize_attr(attrs.technical.passing) * 0.7 + normalize_attr(attrs.technical.technique) * 0.3,
            PassKindV4::Cross => normalize_attr(attrs.technical.crossing) * 0.7 + normalize_attr(attrs.technical.technique) * 0.3,
            _ => normalize_attr(attrs.technical.passing) * 0.5 + vision * 0.25 + normalize_attr(attrs.technical.technique) * 0.25,
        };
        let touch = normalize_attr(mate.first_touch);
        // The estimates below describe what the player thinks is achievable,
        // not the engine's actual contact/accuracy calculation.
        // Retain the existing utility weight, but evaluate future
        // playable contacts rather than static distance to the lane.
        let predicted_risk=forecast.pass_risk(att_side,perception_quality,carrier.pos,t_idx,kind);
        let success = (0.76 + 0.18 * skill + 0.07 * touch - 0.46 * predicted_risk
            - 0.16 * target_pressure - 0.12 * pressure_norm
            - 0.12 * (dist_m / 65.0) - 0.07 * (1.0 - readiness)).clamp(0.06,0.98);
        let destination = possession_value(mate_progress,mate.pos.y,target_pressure);
        let support = if mate_ctx.overload && !carrier_ctx.overload { 0.010 * teamwork } else { 0.0 };
        let transition = counter * (progress_gain / 22.0).clamp(0.0,1.0);
        let cross_option = if kind == PassKindV4::Cross {
            0.012 * normalize_attr(mate.heading)
        } else { 0.0 };
        let safe_recycle = 0.008 * time_wasting * (1.0 - target_pressure);
        let gk_cost = if mate.is_goalkeeper && carrier_progress > 35.0 { 0.025 } else { 0.0 };
        let offside_penalty = offside_line.map_or(0.0, |line| {
            let beyond_m = (mate_progress - line - 0.001).max(0.0);
            if beyond_m <= 0.0 { 0.0 } else {
                (0.25 + 0.75 * (beyond_m / 3.0).min(1.0)) * (0.12 + 0.54 * timing_awareness)
            }
        });
        let utility = ActionValue {
            success,
            benefit: destination + support + transition + cross_option + safe_recycle,
            loss: (loss + turnover_cost(mate_progress)) * 0.5,
            cost: urgency * (0.003 + dist_m / 20.0 * 0.004) + gk_cost + offside_penalty,
        }.expected() - current_value;
        let risk = 1.0 - success;

        candidates.push(CandidateOption {
            action: PlannedActionV4::Pass {
                target_idx: t_idx,
                kind,
                distance_m: dist_m,
                progress_gain_m: progress_gain,
                lane_risk,
                interceptor_idx,
            },
            utility,
            risk,
        });
    }


    candidates
}

/// Final re-aim of the same prepared ground-kick family. No new shot,
/// carry or aerial kick is substituted into a committed short pass.
/// The extra preparation time is charged by DES, not silently waived.
#[derive(Debug,Clone,Copy)]
pub struct PassReaim {
    pub action:PlannedActionV4,
    pub delay_ms:i64,
    pub utility_gain:f32,
}

pub fn reconsider_short_pass(state:&DecisionObservation<'_>,forecast:&impl SpatialForecastPort,idx:usize,
    ctx:LocalSpatialContext,action:PlannedActionV4)->Option<PassReaim> {
    let PlannedActionV4::Pass{target_idx,kind:PassKindV4::Short,..}=action else{return None;};
    let carrier=state.players.get(idx)?;
    let original=state.players.get(target_idx)?;
    let options=collect_pass_candidates(state,idx,ctx,forecast,|kind|kind==PassKindV4::Short);
    let baseline=options.iter().find(|o|matches!(o.action,PlannedActionV4::Pass{target_idx:t,..} if t==target_idx))?;
    let attrs=state.attrs;
    let technique=normalize_attr(attrs.technical.technique);
    let agility=normalize_attr(attrs.physical.agility);
    let decisions=normalize_attr(attrs.mental.decisions);
    let pressure=(ctx.pressure/100.0).clamp(0.0,1.0);
    let awareness=perception_quality(state,idx,pressure);
    let progress=attacking_progress_m(carrier.side,carrier.pos);
    let loss=turnover_cost(progress);
    let urgency=0.75+0.5*state.own_instructions.tempo as f32/100.0;
    let old_dx=original.pos.x-carrier.pos.x;
    let old_dy=original.pos.y-carrier.pos.y;
    let old_len=carrier.pos.distance(original.pos).max(0.01);
    let mut best:Option<PassReaim>=None;
    for opt in options.iter() {
        let PlannedActionV4::Pass{target_idx:to,..}=opt.action else{continue;};
        if to==target_idx {continue;}
        let mate=&state.players[to];
        let len=carrier.pos.distance(mate.pos).max(0.01);
        let dot=((old_dx*(mate.pos.x-carrier.pos.x)+old_dy*(mate.pos.y-carrier.pos.y))/(old_len*len)).clamp(-1.0,1.0);
        let angle=dot.acos();
        // A change of aim consumes perceptual processing and foot/body
        // preparation time, increasing with the required rotation.
        let delay_ms=(100.0+150.0*(1.0-decisions)
            +angle*(110.0+130.0*(1.0-agility))+90.0*(1.0-technique)).round() as i64;
        let time=delay_ms as f32/1000.0;
        // Same goal-equivalent value units as the original decision. Extra
        // exposure under pressure and perception uncertainty are real costs.
        let cost=urgency*0.004*time+loss*pressure*0.18*time+0.008*(1.0-awareness);
        let gain=opt.utility-baseline.utility-cost;
        if gain>0.0 && best.map_or(true,|b|gain>b.utility_gain) {
            best=Some(PassReaim{action:opt.action,delay_ms,utility_gain:gain});
        }
    }
    best
}

/// Match-day form and concentration affect precision of judgment,
/// not kick quality, actual reach or the expected utility of a goal.
/// Career-influenced confidence is deliberately not used here.
pub fn decision_uncertainty(state:&DecisionObservation<'_>,idx:usize,pressure_norm:f32)->f32 {
    let p=&state.players[idx];
    let attrs=state.attrs;
    let decisions=normalize_attr(attrs.mental.decisions);
    let composure=normalize_attr(attrs.mental.composure);
    let mental_pressure=normalize_attr(attrs.hidden.pressure);
    let base=0.025*(1.0-decisions*0.78)
        +0.012*pressure_norm.clamp(0.0,1.0)*(1.0-(composure*0.5+mental_pressure*0.5)*0.65);
    // Relative to fresh concentration of this player, so this does not
    // add a second permanent concentration rating to the base model.
    let fresh=0.74+0.26*normalize_attr(attrs.mental.concentration);
    let focus=(state.concentration_level/fresh).clamp(0.35,1.0);
    let form=state.match_form_multiplier.clamp(0.84,1.16);
    // The existing psychology model already combines score pressure,
    // resistance, determination, leadership and decaying team momentum.
    // Consume that single contextual delta once, rather than charging
    // pressure_load again or treating it as extra physical pressing.
    // Neutral delta preserves 4.0.19. Keep permanent composure in the
    // base spatial-pressure term; no direct change to action success.
    let contextual=1.0-state.composure_delta.clamp(-0.28,0.25);
    // Personal confidence is residual to shared team context; only
    // observed actor-specific outcomes affect this additional term.
    let bound=state.personal_confidence_bound;
    let personal=1.0-state.personal_confidence_delta.clamp(-bound,bound);
    (base*contextual*personal/(form*focus)).clamp(0.002,0.04)
}

pub fn decide_with_forecast(
    state: &DecisionObservation<'_>,
    carrier_idx: usize,
    carrier_ctx: LocalSpatialContext,
    forecast: &impl SpatialForecastPort,
    rng: &mut impl RngSampleSource,
) -> DecisionPlanV4 {
    let carrier = &state.players[carrier_idx];
    let att_side = carrier.side;
    let def_side = att_side.opposite();
    let attrs = state.attrs;
    let ti = state.own_instructions;
    let def_ti = state.opponent_line_height;
    let att_phase = state.phase;

    let vision = normalize_attr(attrs.mental.vision);
    let decisions = normalize_attr(attrs.mental.decisions);
    let composure = normalize_attr(attrs.mental.composure);
    let flair = normalize_attr(attrs.mental.flair);
    let teamwork = normalize_attr(attrs.mental.teamwork);
    let pressure_norm = (carrier_ctx.pressure / 100.0).clamp(0.0, 1.0);

    let carrier_progress = attacking_progress_m(att_side, carrier.pos);
    let goal_dist = distance_to_attacking_goal(att_side, carrier.pos);
    let in_box = is_in_attacking_penalty_box(att_side, carrier.pos);

    // All choices share a scale of expected positional/opportunity value.
    // Tactical preferences affect risk and time, not hard action quotas.
    let risk_appetite = (ti.build_up_risk as f32 / 100.0).clamp(0.0, 1.0);
    let tempo = ti.tempo as f32 / 100.0;
    let time_wasting = ti.time_wasting as f32 / 100.0;
    let loss = turnover_cost(carrier_progress) * (1.15 - 0.30 * risk_appetite);
    let current_value = possession_value(carrier_progress,carrier.pos.y,pressure_norm);
    let urgency = 0.75 + 0.5 * tempo;
    let counter = if att_phase == TacticalPhaseV4::AttackingTransition {
        (ti.transition_risk as f32 / 100.0) * (0.008 + 0.008 * def_ti as f32 / 100.0)
    } else { 0.0 };
    let readiness = (state.anaerobic / 100.0).clamp(0.25,1.0);

    let mut candidates=collect_pass_candidates(state,carrier_idx,carrier_ctx,forecast,|kind|{
        if matches!(kind,PassKindV4::Through|PassKindV4::Switch) {
            let notice_prob=(0.30+vision*0.65-pressure_norm*0.20).clamp(0.15,0.96);
            (rng.next_double() as f32)<=notice_prob
        }else{true}
    });

    if !carrier.is_goalkeeper && goal_dist <= 28.5 {
        let fin = if in_box {
            normalize_attr_elite(attrs.technical.finishing)
        } else {
            normalize_attr_elite(attrs.technical.long_shots)
        };
        // Perceived shot quality: proximity, angle, technique and pressure.
        // This is a policy estimate, not the stochastic shot resolver.
        let central = (1.0 - (carrier.pos.y - 34.0).abs() / 28.0 * 0.55).clamp(0.38,1.0);
        let distance_quality = if in_box {
            (0.38 - goal_dist * 0.013).clamp(0.08,0.38)
        } else { (0.14 - (goal_dist - 16.5).max(0.0) * 0.0065).clamp(0.02,0.14) };
        // The policy estimates the same *sequence* of outcomes as execution,
        // without using RNG or the opponent's hidden attribute values.
        let one_v_one = in_box && goal_dist<=16.5 && carrier_ctx.nearest_defender_dist_m>=3.8;
        let shot_quality = (distance_quality * central * (0.72 + 0.56 * fin)
            * (1.0 - pressure_norm * (0.30 - composure * 0.14))
            * (0.8 + 0.2 * readiness) * if one_v_one {1.32} else {1.0}).clamp(0.015,0.72);
        let goal_center=match_contracts::spatial::attacking_goal_center(att_side);
        let (block_risk,_)=forecast.passing_lane(att_side,carrier.pos,goal_center,false);
        let unblocked=if one_v_one {1.0} else {1.0-0.60*block_risk};
        let on_target=(0.34+0.34*fin-pressure_norm*(0.30-composure*0.14)*0.55).clamp(0.18,0.78);
        // Neutral keeper prior, not omniscient knowledge of GK attributes.
        let conditional_goal=(shot_quality*(1.38-0.60*0.72)).clamp(0.06,0.82);
        let success=shot_goal_probability(unblocked,on_target,conditional_goal);
        // Only a non-scoring on-target shot can plausibly yield a rebound
        // or corner. A rough continuation prior, not guaranteed retention.
        let retain_if_miss=(unblocked*on_target*(1.0-conditional_goal)*0.25
            / (1.0-success).max(0.01)).clamp(0.0,1.0);
        let shot_utility=shot_value(success,retain_if_miss,current_value,
            turnover_cost(99.5),current_value,0.004*urgency+0.01*time_wasting);
        candidates.push(CandidateOption {
            action: PlannedActionV4::Shoot {
                distance_m: goal_dist,
                in_box,
            },
            utility: shot_utility,
            risk: 1.0 - success,
        });
    }

    let carry=if carrier.is_goalkeeper {None} else {
        choose_carry(state,forecast,carrier_idx,carrier_ctx,loss,urgency,counter,readiness)
    };
    if let Some((_,utility,risk))=carry {
        candidates.push(CandidateOption {
            action:PlannedActionV4::Dribble{defender_idx:carrier_ctx.nearest_defender_idx},utility,risk,
        });
    }

    if carrier_progress < 28.0 && pressure_norm > 0.58 {
        // Clearance sacrifices possession for territorial safety. Value it
        // only when the immediate danger is greater than the loss of control.
        let success = (0.70 + 0.20 * normalize_attr(attrs.technical.technique)
            - 0.15 * pressure_norm).clamp(0.05,0.98);
        candidates.push(CandidateOption {
            action: PlannedActionV4::Clearance,
            // A clearance generally hands possession to the opponent,
            // but farther from our goal if it succeeds.
            utility: -(success*turnover_cost((carrier_progress+32.0).min(100.0))
                +(1.0-success)*loss)-current_value-0.006,
            risk: 1.0 - success,
        });
    }
    // Hold is a real alternative, but standing still has no free forward gain.
    let hold_success = (0.95 - pressure_norm * (0.52 - 0.18 * composure)
        - 0.08 * (1.0-readiness)).clamp(0.05,0.98);
    candidates.push(CandidateOption {
        action: PlannedActionV4::HoldAndRecycle,
        utility: ActionValue {
            success: hold_success,
            benefit: current_value + 0.006 * (1.0-pressure_norm) * composure
                + 0.012 * time_wasting + if carrier_ctx.overload {0.004*teamwork} else {0.0},
            loss, cost: urgency * 0.012,
        }.expected()-current_value,
        risk: 1.0 - hold_success,
    });

    // Same action-value scale and correlated noise draws. Amplitude
    // reflects match-day form, mental focus and contextual composure.
    let noise_amplitude=decision_uncertainty(state,carrier_idx,pressure_norm);
    // Shared category uncertainty avoids an automatic lottery advantage
    // from having ten passing options versus one carry or one shot.
    let pass_noise = (rng.next_double() as f32 - 0.5) * 2.0 * noise_amplitude;
    let mut best_action = candidates[0].action;
    let mut best_score = f32::NEG_INFINITY;
    for cand in &candidates {
        let jitter = (rng.next_double() as f32 - 0.5) * 2.0 * noise_amplitude;
        let noise = if matches!(cand.action, PlannedActionV4::Pass { .. }) {
            pass_noise * 0.85 + jitter * 0.15
        } else { jitter };
        let creative_risk = flair * 0.004 * cand.risk;
        let score = cand.utility + creative_risk + noise;
        if score > best_score {
            best_score = score;
            best_action = cand.action;
        }
    }

    let is_one_touch = evaluate_one_touch_intent(
        &best_action,
        attrs,
        pressure_norm,
        ti.tempo,
        ti.time_wasting,
        rng,
    );

    let base_delay_ms = forecast.reaction_delay(
        attrs,
        carrier_ctx.pressure,
        ti.tempo,
        is_one_touch,
        rng,
    );

    // Deliberate "la pausa" timing hold: when not playing one-touch and under low pressure,
    // composed high-decision players may hold the ball an extra fraction of a second to pick
    // the optimal release window rather than always acting mechanically faster.
    let pausa_hold_ms = if !is_one_touch && pressure_norm < 0.40 {
        let tempo_norm = (ti.tempo as f32 / 100.0).clamp(0.0, 1.0);
        let action_hold_weight = match best_action {
            PlannedActionV4::HoldAndRecycle => 1.25,
            PlannedActionV4::Pass { .. } => 0.85,
            _ => 0.0,
        };
        let hold = (composure * 0.60 + decisions * 0.40)
            * (1.0 - pressure_norm / 0.40)
            * (1.0 - tempo_norm * 0.60)
            * 210.0
            * action_hold_weight;
        hold.round() as i64
    } else {
        0
    };

    let (min_delay,max_delay)=forecast.reaction_bounds();
    let decision_delay_ms = base_delay_ms
        .saturating_add(pausa_hold_ms)
        .clamp(min_delay,max_delay);

    DecisionPlanV4 {
        action: best_action,
        decision_delay_ms,
        is_one_touch,
        carry: match best_action {
            PlannedActionV4::Dribble{..}=>carry.map(|(intent,_,_)|intent),
            PlannedActionV4::HoldAndRecycle=>Some(choose_hold(state,carrier_idx,carrier_ctx)),
            _=>None,
        },
    }
}
