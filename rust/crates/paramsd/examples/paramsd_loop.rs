use openpilot_messaging::state::{Options, Poll, State};
use openpilot_paramsd::{
    cache::{self, Store},
    estimator::Estimator,
    loop_state::LoopState,
    wire,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Frame {
    time: f64,
    messages: Vec<Vec<u8>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    car: Vec<u8>,
    seed: BTreeMap<String, Vec<u8>>,
    replay: bool,
    debug: bool,
    gps: String,
    simulation: bool,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
struct Memory {
    values: BTreeMap<String, Vec<u8>>,
    operations: Vec<(String, String, Option<Vec<u8>>)>,
}
impl Store for Memory {
    fn get(&mut self, key: &'static str) -> Option<Vec<u8>> {
        self.operations.push(("get".into(), key.into(), None));
        self.values
            .get(key)
            .filter(|value| !value.is_empty())
            .cloned()
    }
    fn put(&mut self, key: &'static str, bytes: Vec<u8>) {
        self.operations
            .push(("put".into(), key.into(), Some(bytes.clone())));
        self.values.insert(key.into(), bytes);
    }
    fn remove(&mut self, key: &'static str) {
        self.operations.push(("remove".into(), key.into(), None));
        self.values.remove(key);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut output = Vec::new();
    for case in request.cases {
        let car = wire::car(&case.car)?;
        let mut store = Memory {
            values: case.seed,
            operations: Vec::new(),
        };
        let mut logs = Vec::new();
        cache::migrate(&mut store, 1_234_567_890, &mut logs);
        let initial = cache::retrieve(&mut store, &car, case.replay, case.debug, &mut logs);
        let initial_bits =
            [initial.ratio, initial.stiffness, initial.offset_degrees].map(f64::to_bits);
        let covariance_bits = initial
            .covariance
            .as_ref()
            .map(|p| p.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
        let init = json!({"initial_bits":initial_bits,"covariance_bits":covariance_bits,"operations":store.operations,"logs":logs});
        let mut rows = Vec::new();
        if !case.frames.is_empty() {
            let estimator = Estimator::new(
                &car,
                initial.ratio,
                initial.stiffness,
                initial.offset_degrees.to_radians(),
                initial.covariance,
            )?;
            let mut state = LoopState {
                estimator,
                gps_service: case.gps.clone(),
                debug: case.debug,
            };
            let mut subscriber = State::new(
                &["livePose", "liveCalibration", "carState", &case.gps],
                Options {
                    simulation: case.simulation,
                    poll: Poll::One("livePose".into()),
                    ignore_alive: vec![case.gps.clone()],
                    ignore_valid: vec![case.gps.clone()],
                    ..Options::default()
                },
            )?;
            for frame in case.frames {
                subscriber.update(frame.time, &frame.messages)?;
                let result = state.step(&subscriber, 1_234_567_890)?;
                let snapshot = state.estimator.kf.snapshot()?;
                rows.push(
                    json!({"packet":result.packet,"cache":result.cache,"gps":result.gps,
                    "x_bits":snapshot.x.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                    "p_bits":snapshot.covariance.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                    "logs":std::mem::take(&mut state.estimator.logs)}),
                );
            }
        }
        output.push(json!({"name":case.name,"init":init,"rows":rows}));
    }
    let path = std::env::args_os().nth(1).ok_or("expected output path")?;
    serde_json::to_writer(std::fs::File::create(path)?, &output)?;
    Ok(())
}
