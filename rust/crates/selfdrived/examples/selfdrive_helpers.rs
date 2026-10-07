use openpilot_selfdrived::helpers::{
    camera_packets, ActuationInput, ExcessiveActuationCheck, Pose, PoseCalibrator,
};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Calibration {
    rpy: [f64; 3],
    calibrated: bool,
}

#[derive(Deserialize)]
struct Cameras {
    wide: bool,
    disable_dm: i32,
    simulation: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum NonFinite {
    Nan,
    Inf,
    NegInf,
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    reset: bool,
    calibration: Option<Calibration>,
    pose: Pose,
    actuation: Option<ActuationInput>,
    cameras: Option<Cameras>,
    nonfinite_roll: Option<NonFinite>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut calibrator = PoseCalibrator::default();
    let mut actuation = ExcessiveActuationCheck::default();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let mut request: Request = serde_json::from_str(&line?)?;
        if let Some(value) = request.nonfinite_roll {
            request
                .actuation
                .as_mut()
                .ok_or("nonfinite roll needs actuation")?
                .roll = match value {
                NonFinite::Nan => f64::NAN,
                NonFinite::Inf => f64::INFINITY,
                NonFinite::NegInf => f64::NEG_INFINITY,
            };
        }
        if request.reset {
            calibrator = PoseCalibrator::default();
            actuation = ExcessiveActuationCheck::default();
        }
        if let Some(calibration) = request.calibration {
            calibrator.feed(calibration.rpy, calibration.calibrated);
        }
        let pose = calibrator.build(&request.pose);
        let (excessive, error) = match request
            .actuation
            .map(|input| actuation.update(&input, &pose))
            .transpose()
        {
            Ok(value) => (value.flatten(), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let cameras = request
            .cameras
            .map(|input| camera_packets(input.wide, input.disable_dm, input.simulation));
        serde_json::to_writer(
            &mut output,
            &json!({"calibrator":calibrator,"pose":pose,"actuation":actuation,"excessive":excessive,"error":error,"cameras":cameras}),
        )?;
        writeln!(output)?;
    }
    Ok(())
}
