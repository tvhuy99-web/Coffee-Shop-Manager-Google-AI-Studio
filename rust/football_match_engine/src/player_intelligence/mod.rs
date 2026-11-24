//! Player Intelligence 4.0: read-only policy, deterministic intentions.
//! Extraction profile preserves pre-PI behavior and RNG ordering exactly.
//! No scheduler, score mutation, ball ownership or fatigue integration here.
mod decision;
mod goalkeeper;
mod intent;
mod off_ball;
mod timing;
mod version;

pub use decision::{perceive_evaluate_and_decide, perceive_evaluate_and_decide_des};
pub use goalkeeper::choose_gk_distribution;
pub use intent::{CandidateOption, DecisionPlanV4, PassKindV4, PlannedActionV4};
pub use off_ball::{select_movement_target, MovementObservation};
pub use timing::evaluate_one_touch_intent;
pub use version::{PI_MODULE_VERSION, PI_POLICY_ID, PI_BASELINE_COMMIT};
