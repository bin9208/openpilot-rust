use capnp::message::Builder;
use openpilot_card::{
    firmware::Firmware,
    state_helpers::{self, Blinkers, SpeedFilter, SteeringPressed},
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use openpilot_cereal::car_capnp::{car_params, car_state};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Case {
    Params {
        candidate: String,
        torque: Option<TorqueOptions>,
        nnff: bool,
        disable_min: bool,
        not_car: bool,
        angle: bool,
        firmware: Vec<Firmware>,
    },
    State {
        steps: Vec<Step>,
    },
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Step {
    Speed {
        value: f64,
    },
    Lamp {
        time: u32,
        left: bool,
        right: bool,
    },
    Stalk {
        time: u32,
        left: bool,
        right: bool,
    },
    Pressed {
        pressed: bool,
        minimum: u32,
    },
    Gear {
        value: Option<String>,
    },
    Wheels {
        values: [f64; 4],
        factor: f64,
        unit: f64,
    },
    Buttons {
        pcm: bool,
        events: Vec<(u16, bool)>,
    },
}

fn trace(case: Case, settings: &Params) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match case {
        Case::Params {
            candidate,
            torque,
            nnff,
            disable_min,
            not_car,
            angle,
            firmware,
        } => {
            settings.put_bool("NNFF", nnff)?;
            settings.put_bool("DisableMinSteerSpeed", disable_min)?;
            let mut params = match vehicle_params::baseline(&candidate) {
                Ok(params) => params,
                Err(vehicle_params::Error::MissingTorque(_)) => {
                    return Ok(json!({"error":"missing_torque"}));
                }
                Err(vehicle_params::Error::UnknownPlatform(_)) => {
                    return Ok(json!({"error":"unknown_platform"}));
                }
                Err(error) => return Err(error.into()),
            };
            let mut cp = params.get_root::<car_params::Builder>()?;
            cp.set_not_car(not_car);
            if angle {
                cp.set_steer_control_type(car_params::SteerControlType::Angle);
            }
            if let Some(torque) = torque {
                vehicle_params::configure_torque(
                    &candidate,
                    cp.reborrow().get_lateral_tuning(),
                    torque,
                )?;
            }
            vehicle_params::finish(
                cp,
                settings,
                FinishOptions {
                    firmware: &firmware,
                },
            )?;
            let bytes = capnp::serialize::write_message_to_words(&params);
            let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../opendbc_repo/opendbc/car/torque_data")
                .canonicalize()?;
            let prints = vehicle_params::parameter_diagnostics(vehicle_params::DiagnosticInput {
                params: params.get_root_as_reader()?,
                firmware: &firmware,
                settings,
                assets: &assets,
            })?;
            let model = settings
                .get("NNFFModelName")?
                .map(String::from_utf8)
                .transpose()?;
            json!({"wire":bytes,"model":model,"prints":prints})
        }
        Case::State { steps } => {
            let mut speed = SpeedFilter::new()?;
            let mut blinkers = Blinkers::default();
            let mut pressed_filter = SteeringPressed::default();
            let mut output = Vec::new();
            for step in steps {
                output.push(match step {
                    Step::Speed { value } => json!(speed.update(value)),
                    Step::Lamp { time, left, right } => json!(blinkers.lamp(time, left, right)),
                    Step::Stalk { time, left, right } => json!(blinkers.stalk(time, left, right)),
                    Step::Pressed { pressed, minimum } => {
                        json!(pressed_filter.update(pressed, minimum))
                    }
                    Step::Gear { value } => {
                        json!(state_helpers::parse_gear(value.as_deref()) as u16)
                    }
                    Step::Wheels {
                        values,
                        factor,
                        unit,
                    } => json!(
                        state_helpers::wheel_speeds(values, f64::from(factor as f32), unit)
                            .map(|value| value as f32)
                    ),
                    Step::Buttons { pcm, events } => {
                        let mut message = Builder::new_default();
                        let mut buttons = message
                            .init_root::<car_state::Builder>()
                            .init_button_events(u32::try_from(events.len())?);
                        for (index, &(kind, pressed)) in events.iter().enumerate() {
                            let mut button = buttons.reborrow().get(u32::try_from(index)?);
                            button.set_type(car_state::button_event::Type::try_from(kind)?);
                            button.set_pressed(pressed);
                        }
                        json!(state_helpers::button_enable(pcm, buttons.into_reader())?)
                    }
                });
            }
            json!(output)
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    let root = Path::new(&path)
        .parent()
        .ok_or("missing output directory")?
        .join("params");
    let mut output = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let settings = Params::open(&root, &format!("case{index}"))?;
        output.push(trace(case, &settings)?);
    }
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
