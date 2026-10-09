use crate::{Error, Value};
use openpilot_params::Params;

pub fn status(params: Option<&Params>) -> Result<Value, Error> {
    let fresh = super::fresh::reopen(params)?;
    let parsed = fresh.as_ref().and_then(|params| {
        let bytes = params.get("CalibrationParams").ok()??;
        let message =
            capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), Default::default())
                .ok()?;
        let event = message
            .get_root::<openpilot_cereal::log_capnp::event::Reader<'_>>()
            .ok()?;
        let openpilot_cereal::log_capnp::event::Which::LiveCalibration(Ok(calibration)) =
            event.which().ok()?
        else {
            return None;
        };
        if calibration.get_cal_status().ok()?
            == openpilot_cereal::log_capnp::live_calibration_data::Status::Uncalibrated
        {
            return None;
        }
        let rpy = calibration.get_rpy_calib().ok()?;
        if rpy.len() < 3 {
            return None;
        }
        Some((
            f64::from(rpy.get(1)).to_degrees(),
            f64::from(rpy.get(2)).to_degrees(),
        ))
    });
    let rounded = |value: f64| Value::Float(format!("{value:.1}").parse().unwrap_or(value));
    Ok(match parsed {
        Some((pitch, yaw)) => Value::object([
            ("calibrated", Value::Bool(true)),
            ("pitch", rounded(pitch)),
            ("yaw", rounded(yaw)),
        ]),
        None => Value::object([
            ("calibrated", Value::Bool(false)),
            ("pitch", Value::Null),
            ("yaw", Value::Null),
        ]),
    })
}
