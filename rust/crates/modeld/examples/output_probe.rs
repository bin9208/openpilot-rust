use openpilot_cereal::log_capnp::{event, LaneChangeDirection, LaneChangeState};
use openpilot_modeld::{
    action::{self, Action, ActionInputs, PlanActionInput},
    derived_wire::{self, PoseTiming},
    driver_wire::{self, DriverTiming},
    model_wire::{self, ModelFrame, ModelTiming},
    parse::RawOutputs,
    prediction::{DriverPrediction, DrivingPrediction},
    publication::PublishState,
};
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error, fs, io::Write, path::PathBuf};

#[derive(Deserialize)]
struct Request {
    slices: BTreeMap<String, [usize; 2]>,
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Frame {
    Driving {
        input: PathBuf,
        timing: ModelTiming,
        pose: PoseTiming,
        action: ActionInputs,
        lane_change_state: u16,
        lane_change_direction: u16,
        raw: bool,
    },
    Driver {
        input: PathBuf,
        timing: DriverTiming,
        raw: bool,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("expected request.json")?);
    let output = PathBuf::from(args.next().ok_or("expected output directory")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let request: Request = serde_json::from_slice(&fs::read(&input)?)?;
    let base = input.parent().ok_or("request has no parent")?;
    fs::create_dir_all(&output)?;
    let mut messages = fs::File::create(output.join("messages.bin"))?;
    let mut actions = Vec::new();
    let mut previous = Action::default();
    let mut state = PublishState::default();
    for frame in request.frames {
        let source = match &frame {
            Frame::Driving { input, .. } | Frame::Driver { input, .. } => input,
        };
        let bytes = fs::read(base.join(source))?;
        if bytes.len() % 4 != 0 {
            return Err("model output size is not a multiple of f32".into());
        }
        let values: Vec<_> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        let raw_values = RawOutputs::new(&values, &request.slices)?;
        match frame {
            Frame::Driving {
                timing,
                pose,
                action,
                lane_change_state,
                lane_change_direction,
                raw,
                ..
            } => {
                let data = DrivingPrediction::parse(&raw_values)?;
                let selected = action::from_plan(
                    PlanActionInput {
                        plan: &data.plan,
                        direct_action: data.direct_action,
                    },
                    previous,
                    action,
                );
                actions.push(selected);
                let mut model = model_wire::build(
                    ModelFrame {
                        prediction: &data,
                        timing,
                        action: selected,
                        raw_predictions: raw.then_some(bytes.as_slice()),
                    },
                    &mut state,
                )?;
                {
                    let root = model.get_root::<event::Builder>()?;
                    let mut value = match root.which()? {
                        event::ModelV2(value) => value?,
                        _ => return Err("expected modelV2".into()),
                    };
                    let mut meta = value.reborrow().get_meta()?;
                    meta.set_lane_change_state(LaneChangeState::try_from(lane_change_state)?);
                    meta.set_lane_change_direction(LaneChangeDirection::try_from(
                        lane_change_direction,
                    )?);
                    let action = value.get_action()?.into_reader();
                    previous = Action {
                        desired_curvature: f64::from(action.get_desired_curvature()),
                        desired_acceleration: f64::from(action.get_desired_acceleration()),
                        desired_velocity: f64::from(action.get_desired_velocity()),
                        should_stop: action.get_should_stop(),
                    };
                }
                messages.write_all(&capnp::serialize::write_message_to_words(&model))?;
                messages.write_all(&derived_wire::driving(
                    model.get_root_as_reader::<event::Reader>()?,
                    timing.log_mono_time,
                )?)?;
                messages.write_all(&derived_wire::pose(&data, pose)?)?;
            }
            Frame::Driver { timing, raw, .. } => {
                let data = DriverPrediction::parse(&raw_values)?;
                messages.write_all(&driver_wire::encode(
                    &data,
                    timing,
                    if raw { &bytes } else { &[] },
                ))?;
            }
        }
    }
    fs::write(output.join("actions.json"), serde_json::to_vec(&actions)?)?;
    Ok(())
}
