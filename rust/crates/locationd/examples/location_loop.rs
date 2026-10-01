use openpilot_locationd::{loop_state::LoopState, wire};
use openpilot_messaging::state::{Options, Poll, State};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Frame {
    time: f64,
    messages: Vec<Vec<u8>>,
    acceleration: Vec<Vec<u8>>,
    gyroscope: Vec<Vec<u8>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    simulation: bool,
    seed: Option<Vec<u8>>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut output = Vec::new();
    for case in request.cases {
        let mut state = LoopState::new(true, case.simulation)?;
        let mut subscriber = State::new(
            &["carState", "liveCalibration", "cameraOdometry"],
            Options {
                simulation: case.simulation,
                poll: Poll::One("cameraOdometry".into()),
                ..Options::default()
            },
        )?;
        if let Some(bytes) = case.seed {
            let seed = wire::seed(&bytes)?;
            state.estimator.kf.reset(None, &seed.x, &seed.covariance)?;
        }
        let mut rows = Vec::new();
        for frame in case.frames {
            subscriber.update(frame.time, &frame.messages)?;
            let acceleration = frame
                .acceleration
                .iter()
                .map(|bytes| wire::decode(bytes))
                .collect::<Result<Vec<_>, _>>()?;
            let gyroscope = frame
                .gyroscope
                .iter()
                .map(|bytes| wire::decode(bytes))
                .collect::<Result<Vec<_>, _>>()?;
            let result = state.step(&subscriber, &acceleration, &gyroscope, || frame.time)?;
            let mut packets = Vec::new();
            let mut printed = String::new();
            if let Some(result) = result {
                packets.push(wire::encode(&result.pose, (frame.time * 1e9) as u64)?);
                if let Some(text) = result.diagnostic {
                    printed = text + "\n";
                }
            }
            let snapshot = state.estimator.kf.snapshot()?;
            rows.push(json!({"initialized":state.initialized,"invalid":state.invalid,"sensor_valid":state.sensor_valid,
                "sensor_alive":state.sensor_alive,"sensor_received":state.sensor_received,"x":snapshot.x,"p":snapshot.covariance,
                "packets":packets,"logs":std::mem::take(&mut state.estimator.logs),"printed":printed}));
        }
        output.push(json!({"name":case.name,"rows":rows}));
    }
    let path = std::env::args_os().nth(1).ok_or("expected output path")?;
    serde_json::to_writer(std::fs::File::create(path)?, &output)?;
    Ok(())
}
