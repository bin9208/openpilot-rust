use openpilot_control_tools::{
    joystickd::{Command, Config, Controller, Input, LongState},
    Error,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
struct Request {
    config: Config,
    inputs: Vec<Input>,
}
#[derive(Serialize)]
struct Control {
    enabled: bool,
    lat_active: bool,
    long_active: bool,
    cancel: bool,
    resume: bool,
    lead_distance_bars: u8,
    long_state: LongState,
    actuators: [u32; 4],
}
impl From<Command> for Control {
    fn from(command: Command) -> Self {
        Self {
            enabled: command.enabled,
            lat_active: command.lat_active,
            long_active: command.long_active,
            cancel: command.cancel,
            resume: command.resume,
            lead_distance_bars: 2,
            long_state: command.long_state,
            actuators: [
                command.accel,
                command.torque,
                command.steering_angle,
                command.curvature,
            ]
            .map(f32::to_bits),
        }
    }
}
#[derive(Default, Serialize)]
struct Output {
    control: Option<Control>,
    curvature: Option<u32>,
    error: Option<&'static str>,
}

fn source_error(error: Error) -> Result<&'static str, Error> {
    match error {
        Error::Policy(openpilot_control_policy::Error::Contract("division by zero")) => {
            Ok("ZeroDivisionError")
        }
        Error::Contract("missing longitudinal joystick axis" | "missing lateral joystick axis") => {
            Ok("IndexError")
        }
        error => Err(error),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let requests: Vec<Request> = serde_json::from_str(&text)?;
    let mut rows = Vec::with_capacity(requests.len());
    for request in requests {
        let owner = Controller::new(request.config);
        let mut output = Vec::with_capacity(request.inputs.len());
        for input in request.inputs {
            let mut row = Output::default();
            match owner.control(&input) {
                Ok(control) => {
                    row.control = Some(control.into());
                    match owner.curvature(&input) {
                        Ok(curvature) => row.curvature = Some(curvature.to_bits()),
                        Err(error) => row.error = Some(source_error(error)?),
                    }
                }
                Err(error) => row.error = Some(source_error(error)?),
            }
            let failed = row.error.is_some();
            output.push(row);
            if failed {
                break;
            }
        }
        rows.push(output);
    }
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &rows)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
