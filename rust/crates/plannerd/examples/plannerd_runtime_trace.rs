use openpilot_messaging::state::State;
use openpilot_plannerd::{
    config::Config,
    native_parameters,
    parameters::Parameters,
    platform::Clock,
    runtime::{Output, Planner, SERVICES},
    Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Request {
    artifact: PathBuf,
    car_params: PathBuf,
    parameters: BTreeMap<String, String>,
    output: PathBuf,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    time: f64,
    wall_time: f64,
    packets: Vec<PathBuf>,
    parameters: BTreeMap<String, String>,
}
struct Store {
    values: BTreeMap<String, String>,
    operations: Vec<[String; 2]>,
}
impl Parameters for Store {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        self.operations.push(["get_int".into(), key.into()]);
        native_parameters::integer(self.values.get(key).map_or(&[], |value| value.as_bytes()))
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        self.operations.push(["get_float".into(), key.into()]);
        native_parameters::float(self.values.get(key).map_or(&[], |value| value.as_bytes()))
    }
}
struct Time {
    mono: f64,
    wall: f64,
}
impl Clock for Time {
    fn monotonic(&self) -> f64 {
        self.mono
    }
    fn wall(&self) -> f64 {
        self.wall
    }
    fn thread_cpu(&self) -> f64 {
        0.
    }
}
struct Capture {
    path: PathBuf,
    messages: Vec<(String, PathBuf)>,
    logs: Vec<(String, String)>,
    total: usize,
}
impl Output for Capture {
    fn send(&mut self, name: &'static str, bytes: &[u8]) -> Result<(), Error> {
        let path = self.path.join(format!("{:05}-{name}.bin", self.total));
        self.total += 1;
        std::fs::write(&path, bytes)?;
        self.messages.push((name.into(), path));
        Ok(())
    }
    fn warning(&mut self, message: String) -> Result<(), Error> {
        self.logs.push(("warning".into(), message));
        Ok(())
    }
    fn info(&mut self, message: String) -> Result<(), Error> {
        self.logs.push(("info".into(), message));
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let request: Request = serde_json::from_str(&raw)?;
    std::fs::create_dir(&request.output)?;
    let config = Config::decode(&std::fs::read(&request.car_params)?)?;
    let mut store = Store {
        values: request.parameters,
        operations: vec![["get".into(), "CarParams".into()]],
    };
    let (mut planner, mut state) =
        Planner::load_with(config, &request.artifact, &mut store, |options| {
            Ok(State::new(&SERVICES, options)?)
        })?;
    let mut capture = Capture {
        path: request.output,
        messages: Vec::new(),
        logs: Vec::new(),
        total: 0,
    };
    let mut output = Vec::new();
    for frame in request.frames {
        store.values.extend(frame.parameters);
        let messages = frame
            .packets
            .iter()
            .map(std::fs::read)
            .collect::<Result<Vec<_>, _>>()?;
        state.update(frame.time, &messages)?;
        let clock = Time {
            mono: frame.time,
            wall: frame.wall_time,
        };
        let tick = planner.process(&state, &mut store, &clock, &mut capture)?;
        output.push(json!({"messages":capture.messages,"logs":capture.logs,"parameters":store.operations,
            "trigger":match tick.trigger { openpilot_cereal::log_capnp::longitudinal_plan::PlanningTrigger::ModelV2 => "modelV2",
                openpilot_cereal::log_capnp::longitudinal_plan::PlanningTrigger::LiveTracks => "liveTracks" },
            "longitudinal_run":tick.longitudinal_run,"model_updated":tick.model_updated}));
        store.operations.clear();
        capture.logs.clear();
        capture.messages.clear();
    }
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, serde_json::to_vec(&output)?)?;
    } else {
        serde_json::to_writer(io::stdout().lock(), &output)?;
    }
    Ok(())
}
