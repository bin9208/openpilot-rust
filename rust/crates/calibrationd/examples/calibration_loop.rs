use openpilot_calibrationd::{loop_state, parameters, wire, Calibrator, Limits, Seed};
use openpilot_messaging::state::{Options, Poll, State};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    io::{self, BufRead},
};

#[derive(Deserialize)]
struct Configuration {
    saved: Option<Vec<u8>>,
    not_car: bool,
    simulation: bool,
    mici: bool,
}

#[derive(Deserialize)]
struct Frame {
    configuration: Option<Configuration>,
    time: f64,
    timestamp: u64,
    messages: Vec<Vec<u8>>,
    trim: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut current = None;
    for line in io::stdin().lock().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if let Some(configuration) = frame.configuration {
            let seed = configuration
                .saved
                .map_or_else(Seed::default, |bytes| wire::saved(&bytes).0);
            let mut calibrator = Calibrator::new(
                if configuration.mici {
                    Limits::mici()
                } else {
                    Limits::standard()
                },
                seed,
            )?;
            calibrator.not_car = configuration.not_car;
            current = Some((
                calibrator,
                State::new(
                    &["cameraOdometry", "carState"],
                    Options {
                        poll: Poll::One("cameraOdometry".to_owned()),
                        simulation: configuration.simulation,
                        ..Options::default()
                    },
                )?,
            ));
        }
        let (calibrator, state) = current.as_mut().ok_or("missing configuration")?;
        let timeout = if state.frame() == -1 { 0 } else { 100 };
        state.update(frame.time, &frame.messages)?;
        let trim = if state.topic("cameraOdometry")?.updated {
            parameters::parse_float(frame.trim.as_bytes())? * 0.01
        } else {
            0.0
        };
        let result = loop_state::step(calibrator, state, trim)?;
        let packet = if result.publish {
            Some(wire::encode(calibrator, frame.timestamp, result.valid)?)
        } else {
            None
        };
        let persisted = if result.update.persist {
            Some(wire::encode(calibrator, frame.timestamp, true)?)
        } else {
            None
        };
        println!(
            "{}",
            serde_json::to_string(&json!({"frame": state.frame(), "timeout": timeout,
            "publish": result.publish, "valid": result.valid, "accepted": result.update.rpy.is_some(),
            "persisted": persisted, "packet": packet, "idx": calibrator.idx, "block_idx": calibrator.block_idx}))?
        );
    }
    Ok(())
}
