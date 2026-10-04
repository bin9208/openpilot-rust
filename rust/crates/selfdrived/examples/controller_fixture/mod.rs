pub mod compress;
pub mod effects;
use openpilot_cereal::car_capnp::car_params;
use openpilot_messaging::state::State;
use openpilot_selfdrived::controller::{
    car_frame::CarFrame,
    config::{Config, Mode},
    Controller, Error,
};
use openpilot_ui_framework::multilang::Multilang;
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::Path};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Init {
        cp: Vec<u8>,
        mode: Mode,
        language: Option<String>,
        #[serde(default)]
        health_simulation: bool,
    },
    Step {
        current: Option<Vec<u8>>,
        messages: Vec<Vec<u8>>,
        now: f64,
        lagging: bool,
    },
    Events {
        current: Vec<u8>,
        messages: Vec<Vec<u8>>,
        now: f64,
        lagging: bool,
    },
    Sample {
        current: Vec<u8>,
        messages: Vec<Vec<u8>>,
        now: f64,
    },
    Alerts {
        current: Vec<u8>,
    },
    Publish,
    ParamsCycle,
    Parameter {
        key: String,
        bytes: Option<Vec<u8>>,
        directory: bool,
    },
    Streams {
        values: Vec<u16>,
    },
    Advance {
        current: Option<Vec<u8>>,
        messages: Vec<Vec<u8>>,
        now: f64,
        dt: f64,
        lagging: bool,
        count: u32,
    },
}
#[derive(Default)]
pub struct Fixture {
    pub controller: Option<Controller>,
    pub state: Option<State>,
}
#[derive(Serialize)]
pub struct Health<'a> {
    pub frame: i64,
    pub topics: &'a [openpilot_messaging::state::Topic],
    pub ignore_alive: &'a [String],
    pub ignore_valid: &'a [String],
    pub ignore_frequency: &'a [String],
}
impl Fixture {
    pub fn health(&self) -> Option<Health<'_>> {
        self.state.as_ref().map(|state| Health {
            frame: state.frame(),
            topics: state.topics(),
            ignore_alive: state.ignore_alive(),
            ignore_valid: state.ignore_valid(),
            ignore_frequency: state.ignore_frequency(),
        })
    }
    pub fn dispatch(
        &mut self,
        request: Request,
        trace: &mut effects::Trace<'_>,
        messages: &mut effects::Messages,
    ) -> Result<(), Error> {
        match request {
            Request::Init {
                cp,
                mode,
                language,
                health_simulation,
            } => {
                self.controller = None;
                self.state = None;
                let cp = capnp::serialize::read_message(
                    Cursor::new(cp),
                    capnp::message::ReaderOptions::new(),
                )?;
                let config = Config::read(cp.get_root::<car_params::Reader<'_>>()?)?;
                let language = Multilang::new(
                    &Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../../openpilot/selfdrive/ui/translations"),
                    language.as_deref(),
                )?;
                let controller = Controller::new(config, (mode, language), trace)?;
                let (names, mut options) = controller.subscription();
                options.simulation = health_simulation;
                self.state = Some(State::new(&names, options)?);
                self.controller = Some(controller);
            }
            Request::Step {
                current,
                messages: inputs,
                now,
                lagging,
            } => {
                trace.now = now;
                let (controller, state) = self.parts()?;
                state.update(now, &inputs)?;
                let car = CarFrame::read(
                    current.unwrap_or_else(|| controller.previous_bytes().to_vec()),
                )?;
                controller.step((&car, state, lagging), trace, messages)?;
            }
            Request::Events {
                current,
                messages: inputs,
                now,
                lagging,
            } => {
                trace.now = now;
                let (controller, state) = self.parts()?;
                state.update(now, &inputs)?;
                let car = CarFrame::read(current)?;
                controller.update_events(car.state()?, state, lagging, trace)?;
            }
            Request::Sample {
                current,
                messages: inputs,
                now,
            } => {
                trace.now = now;
                let (controller, state) = self.parts()?;
                state.update(now, &inputs)?;
                let car = CarFrame::read(current)?;
                controller.data_sample(car.state()?, state, trace)?;
            }
            Request::Alerts { current } => {
                let (controller, state) = self.parts()?;
                let car = CarFrame::read(current)?;
                controller.update_alerts(car.state()?, state, trace)?;
            }
            Request::Publish => {
                let (controller, state) = self.parts()?;
                controller.publish(state, trace, messages)?;
            }
            Request::ParamsCycle => self
                .controller
                .as_mut()
                .ok_or(Error::Contract("controller not initialized"))?
                .params_cycle(trace)?,
            Request::Parameter {
                key,
                bytes,
                directory,
            } => parameter(&trace.directory, (&key, bytes, directory))?,
            Request::Streams { values } => {
                trace.streams = values
                    .into_iter()
                    .map(|value| match value {
                        0 => Ok(openpilot_msgq::VisionStream::Road),
                        1 => Ok(openpilot_msgq::VisionStream::Driver),
                        2 => Ok(openpilot_msgq::VisionStream::WideRoad),
                        3 => Ok(openpilot_msgq::VisionStream::Map),
                        _ => Err(Error::Contract("unknown VisionStream")),
                    })
                    .collect::<Result<_, Error>>()?
            }
            Request::Advance {
                current,
                messages: inputs,
                now,
                dt,
                lagging,
                count,
            } => {
                for index in 0..count {
                    self.dispatch(
                        Request::Step {
                            current: current.clone(),
                            messages: inputs.clone(),
                            now: now + f64::from(index) * dt,
                            lagging,
                        },
                        trace,
                        messages,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn parts(&mut self) -> Result<(&mut Controller, &mut State), Error> {
        Ok((
            self.controller
                .as_mut()
                .ok_or(Error::Contract("controller not initialized"))?,
            self.state
                .as_mut()
                .ok_or(Error::Contract("state not initialized"))?,
        ))
    }
}
fn parameter(root: &Path, input: (&str, Option<Vec<u8>>, bool)) -> Result<(), Error> {
    let (key, bytes, directory) = input;
    let path = root.join(key);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir(&path)?,
        Ok(_) => std::fs::remove_file(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    if directory {
        std::fs::create_dir(path)?;
    } else if let Some(bytes) = bytes {
        std::fs::write(path, bytes)?;
    }
    Ok(())
}
