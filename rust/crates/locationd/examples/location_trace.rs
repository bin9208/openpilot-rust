use openpilot_locationd::{estimator::Estimator, wire};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Case {
    name: String,
    reset_time: Option<f64>,
    events: Vec<Vec<u8>>,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut output = Vec::new();
    for case in request.cases {
        let mut estimator = Estimator::new(true)?;
        if let Some(time) = case.reset_time {
            estimator.kf.reset_default(Some(time))?;
        }
        let mut rows = Vec::new();
        for packet in case.events {
            let event = wire::decode(&packet)?;
            let result = estimator.handle(event.log_time_ns as f64 * 1e-9, event.input)?;
            let pose = estimator.pose(true, true, true)?;
            let state = estimator.kf.snapshot()?;
            rows.push(json!({"result":result as i32, "x":state.x, "p":state.covariance,
                "packet":wire::encode(&pose,1234567890)?, "logs":std::mem::take(&mut estimator.logs)}));
        }
        output.push(json!({"name":case.name,"rows":rows}));
    }
    let path = std::env::args_os().nth(1).ok_or("expected output path")?;
    serde_json::to_writer(std::fs::File::create(path)?, &output)?;
    Ok(())
}
