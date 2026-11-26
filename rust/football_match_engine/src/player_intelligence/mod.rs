//! Player Intelligence 4.0: read-only policy, deterministic intentions.
//! Behavioral policy is versioned separately from the original extraction baseline.
//! No scheduler, score mutation, ball ownership or fatigue integration here.
#[path = "../v4/pi_adapter/decision.rs"]
mod decision;
#[path = "../v4/pi_adapter/goalkeeper.rs"]
mod goalkeeper;
mod intent;
mod off_ball;
mod defensive;
#[path = "../v4/pi_adapter/tackle.rs"]
mod tackle;
#[path = "../v4/pi_adapter/pass_forecast.rs"]
pub(crate) mod pass_forecast;
pub(crate) use tackle::evaluate_tackle;
pub(crate) use defensive::{assign_defense, DefensiveRole, DefenderObservation, DefensiveIntent};
mod support;
pub(crate) use support::{ObservedPlayer, SupportObservation, select_coordinated_support_target, count_open_outlets};
mod timing;
mod version;
mod utility;
mod on_ball;
pub(crate) use on_ball::carry_target;

pub use decision::{perceive_evaluate_and_decide, perceive_evaluate_and_decide_des};
pub(crate) use decision::{observe_pass_risk,reconsider_short_pass};
#[cfg(test)]
pub(crate) use decision::decision_uncertainty;
pub use goalkeeper::choose_gk_distribution;
pub use intent::{CandidateOption, CarryIntentV4, DecisionPlanV4, PassKindV4, PlannedActionV4};
pub use off_ball::{select_movement_target, MovementObservation};
pub use timing::evaluate_one_touch_intent;
pub use version::{PI_MODULE_VERSION, PI_POLICY_ID, PI_BASELINE_COMMIT};
