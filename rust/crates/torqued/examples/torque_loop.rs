use openpilot_messaging::state::{Options, Poll, State};
use openpilot_torqued::{
    estimator::Estimator,
    loop_state::{self, TOPICS},
    numerics::Numerics,
    random::RandomState,
    wire,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    io::{self, BufRead},
    path::Path,
};
#[derive(Deserialize)]
struct Configuration {
    car: Vec<u8>,
    saved: Option<Vec<u8>>,
    simulation: bool,
    debug: bool,
}
#[derive(Deserialize)]
struct Frame {
    configuration: Option<Configuration>,
    time: f64,
    messages: Vec<Vec<u8>>,
}
fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("numerics directory required")?;
    let mut fit = Numerics::load(Path::new(&directory))?;
    let mut current = None;
    for line in io::stdin().lock().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if let Some(config) = frame.configuration {
            let mut estimator = Estimator::new(
                wire::car(&config.car)?,
                (false, true),
                RandomState::seeded(42),
            );
            wire::restore(&mut estimator, Some(&config.car), config.saved.as_deref())?;
            current = Some((
                estimator,
                State::new(
                    &TOPICS,
                    Options {
                        poll: Poll::One("livePose".to_owned()),
                        simulation: config.simulation,
                        ..Options::default()
                    },
                )?,
                config.debug,
            ));
        }
        let (estimator, state, debug) = current.as_mut().ok_or("missing configuration")?;
        state.update(frame.time, &frame.messages)?;
        let actions = loop_state::step(estimator, state)?;
        let packet = if actions.publish {
            Some(wire::encode(
                &estimator.message(&mut fit, *debug)?,
                0,
                actions.valid,
            )?)
        } else {
            None
        };
        let persisted = if actions.persist {
            Some(wire::encode(
                &estimator.message(&mut fit, true)?,
                0,
                actions.valid,
            )?)
        } else {
            None
        };
        println!(
            "{}",
            json!({"frame":state.frame(),"valid":actions.valid,"packet":packet,"persisted":persisted,"counts":estimator.buckets.counts(),"admitted":estimator.all_points.len()})
        );
    }
    Ok(())
}
