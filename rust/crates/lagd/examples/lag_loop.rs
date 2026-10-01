use openpilot_lagd::{
    estimator::Estimator,
    loop_state::{self, TOPICS},
    message,
    settings::Settings,
    wire,
};
use openpilot_messaging::state::{Options, Poll, State};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead};
#[derive(Deserialize)]
struct Configuration {
    car: Vec<u8>,
    saved: Option<Vec<u8>>,
    previous: Option<Vec<u8>>,
}
#[derive(Deserialize)]
struct Frame {
    configuration: Option<Configuration>,
    now: f64,
    messages: Vec<Vec<u8>>,
    debug: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut current = None;
    for line in io::stdin().lock().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if let Some(config) = frame.configuration {
            let car = wire::car(&config.car)?;
            let mut estimator = Estimator::new(Settings::default(), car.actuator_delay)?;
            let mut removed = false;
            if let Some(bytes) = config.saved.filter(|value| !value.is_empty()) {
                match wire::saved(&bytes, config.previous.as_deref().unwrap_or_default(), &car) {
                    Ok((lag, blocks)) => estimator.reset(lag, blocks)?,
                    Err(_) => removed = true,
                }
            }
            current = Some((
                estimator,
                State::new(
                    &TOPICS,
                    Options {
                        poll: Poll::One("livePose".into()),
                        simulation: true,
                        ..Options::default()
                    },
                )?,
                removed,
            ));
        }
        let (estimator, state, removed) = current.as_mut().ok_or("configuration required")?;
        state.update(frame.now, &frame.messages)?;
        let action = loop_state::step(estimator, state)?;
        let bytes = if action.publish {
            Some(wire::encode(
                &message::packet(estimator, frame.debug)?,
                123456789,
                action.valid,
            )?)
        } else {
            None
        };
        println!(
            "{}",
            json!({"frame":state.frame(),"valid":action.valid,"publish":action.publish,"persist":action.persist,"packet":bytes,"removed":removed,"state":{"okay":estimator.points.okay(),"last":estimator.points.rows.back(),"last_estimate":estimator.last_estimate_t,"block_idx":estimator.blocks.block_idx,"idx":estimator.blocks.idx,"valid_blocks":estimator.blocks.valid_blocks,"recovery":estimator.motion.recovery_times}})
        );
    }
    Ok(())
}
