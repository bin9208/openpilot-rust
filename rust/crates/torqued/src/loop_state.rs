use crate::{estimator::Estimator, wire, Error};
use openpilot_messaging::state::State;
pub const TOPICS: [&str; 6] = [
    "carControl",
    "carOutput",
    "carState",
    "liveCalibration",
    "livePose",
    "liveDelay",
];
pub struct Actions {
    pub valid: bool,
    pub publish: bool,
    pub persist: bool,
}
pub fn step(estimator: &mut Estimator, state: &State) -> Result<Actions, Error> {
    let valid = state.all_checks(&[])?;
    if valid {
        for name in TOPICS {
            let topic = state.topic(name)?;
            if topic.updated {
                estimator.handle(wire::input(topic.event()?)?)?;
            }
        }
    }
    Ok(Actions {
        valid,
        publish: state.frame() % 5 == 0,
        persist: state.frame() % 240 == 0,
    })
}
