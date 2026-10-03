use crate::{controller::Command, longitudinal::State, Error};
fn number(value: f32) -> Result<String, Error> {
    if value.is_nan() {
        return Ok("nan".into());
    }
    if value.is_infinite() {
        return Ok(if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into());
    }
    let mut text = String::new();
    openpilot_runtime_core::python_float::write_float(f64::from(value), &mut text)?;
    Ok(text)
}
fn dictionary(command: &Command, state: State) -> Result<String, Error> {
    let state = match state {
        State::Off => "off",
        State::Pid => "pid",
        State::Stopping => "stopping",
        State::Starting => "starting",
    };
    Ok(format!("{{'gas': 0.0, 'brake': 0.0, 'torque': {}, 'steeringAngleDeg': {}, 'accel': {}, 'longControlState': '{}', 'speed': 0.0, 'curvature': {}, 'torqueOutputCan': 0.0, 'jerk': {}, 'aTarget': {}}}", number(command.torque)?, number(command.angle)?, number(command.accel)?, state, number(command.curvature)?, number(command.jerk)?, number(command.target)?))
}
pub fn apply(command: &mut Command, state: State) -> Result<(), Error> {
    for name in [
        "torque",
        "steeringAngleDeg",
        "accel",
        "curvature",
        "jerk",
        "aTarget",
    ] {
        let value = match name {
            "torque" => command.torque,
            "steeringAngleDeg" => command.angle,
            "accel" => command.accel,
            "curvature" => command.curvature,
            "jerk" => command.jerk,
            _ => command.target,
        };
        if value.is_finite() {
            continue;
        }
        command.errors.push(format!(
            "actuators.{name} not finite {}",
            dictionary(command, state)?
        ));
        match name {
            "torque" => command.torque = 0.,
            "steeringAngleDeg" => command.angle = 0.,
            "accel" => command.accel = 0.,
            "curvature" => command.curvature = 0.,
            "jerk" => command.jerk = 0.,
            _ => command.target = 0.,
        }
    }
    Ok(())
}
