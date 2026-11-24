//! Player Intelligence 4.0: read-only policy, deterministic intentions.
//! Behavioral policy is versioned separately from the original extraction baseline.
//! No scheduler, score mutation, ball ownership or fatigue integration here.
mod decision;
mod goalkeeper;
mod intent;
mod off_ball;
mod timing;
mod version;
mod utility;
mod on_ball;
pub(crate) use on_ball::carry_target;

pub use decision::{perceive_evaluate_and_decide, perceive_evaluate_and_decide_des};
pub use goalkeeper::choose_gk_distribution;
pub use intent::{CandidateOption, CarryIntentV4, DecisionPlanV4, PassKindV4, PlannedActionV4};
pub use off_ball::{select_movement_target, MovementObservation};
pub use timing::evaluate_one_touch_intent;
pub use version::{PI_MODULE_VERSION, PI_POLICY_ID, PI_BASELINE_COMMIT};
