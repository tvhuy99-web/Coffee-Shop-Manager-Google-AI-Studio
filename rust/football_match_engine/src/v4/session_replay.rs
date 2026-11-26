//! Deterministic replay and single/batch simulation entrypoints.

use rayon::prelude::*;

use crate::v4::error::V4Error;
use crate::v4::events::MatchResultV4;
use crate::v4::input::MatchInputV4;
use crate::v4::session::MatchSessionV4;
use crate::v4::session_commands::AppliedCommandV4;

/// Deterministically replays a previously controlled DES session from an immutable input
/// snapshot and its recorded `AppliedCommandV4` journal.
pub fn replay_v4_with_timeline(
    input: MatchInputV4,
    commands: &[AppliedCommandV4],
) -> Result<MatchResultV4, V4Error> {
    let mut session = MatchSessionV4::new(input)?;
    let mut next = 0usize;
    while !session.is_full_time() {
        while next < commands.len() && commands[next].applied_step_index == session.completed_beats {
            let recorded = &commands[next];
            if recorded.accepted_sim_time_ms != session.state.des_clock.sim_time_ms {
                return Err(V4Error::InvalidInput(format!(
                    "replay command {} acceptedSimTimeMs mismatch: recorded {}, actual {}",
                    next, recorded.accepted_sim_time_ms, session.state.des_clock.sim_time_ms
                )));
            }
            session.submit_command(recorded.command.clone())?;
            next += 1;
        }
        if next < commands.len() && commands[next].applied_step_index < session.completed_beats {
            return Err(V4Error::InvalidInput(format!(
                "replay command order mismatch at index {next}"
            )));
        }
        if session.stage.is_break() {
            session.begin_next_period_internal()?;
            continue;
        }
        session.step_one_des_event(false)?;
        session.completed_beats = session.completed_beats.saturating_add(1);
    }
    if next != commands.len() {
        return Err(V4Error::InvalidInput(format!(
            "{} commands have a simulation timestamp after full time",
            commands.len() - next
        )));
    }
    session.finished = true;
    Ok(session.build_result())
}

pub fn simulate_v4(input: MatchInputV4) -> Result<MatchResultV4, V4Error> {
    let mut session = MatchSessionV4::new(input)?;
    session.run_to_full_time()
}

pub fn simulate_batch_fast_v4(inputs: Vec<MatchInputV4>) -> Vec<Result<MatchResultV4, V4Error>> {
    inputs
        .into_par_iter()
        .map(|mut input| {
            input.simulation_mode = crate::v4::input::SimulationMode::FastBatch;
            simulate_v4(input)
        })
        .collect()
}


