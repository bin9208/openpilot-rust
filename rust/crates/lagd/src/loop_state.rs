use crate::{
    estimator::{Estimator, Evaluation},
    wire, Error,
};
use openpilot_messaging::state::State;
pub const TOPICS: [&str; 5] = [
    "livePose",
    "liveCalibration",
    "carState",
    "controlsState",
    "carControl",
];
pub struct Actions {
    pub valid: bool,
    pub publish: bool,
    pub persist: bool,
    pub evaluation: Option<Evaluation>,
}
pub fn step(estimator: &mut Estimator, state: &State) -> Result<Actions, Error> {
    let valid = state.all_checks(&[])?;
    if valid {
        let mut topics = state
            .topics()
            .iter()
            .filter(|topic| topic.updated)
            .collect::<Vec<_>>();
        // Stable timestamp sorting preserves the original subscription insertion order on ties.
        topics.sort_by_key(|topic| topic.log_mono_time);
        for topic in topics {
            let (time, input) = wire::input(topic.event()?)?;
            estimator.motion.handle(time, input);
        }
        estimator.update_points();
    }
    let publish = state.frame() % 5 == 0;
    let evaluation = if publish {
        estimator.update_estimate()?
    } else {
        None
    };
    Ok(Actions {
        valid,
        publish,
        persist: publish && state.frame() % 1200 == 0,
        evaluation,
    })
}
