use openpilot_paramsd::{estimator::Estimator, types::diagonal, wire};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Operation {
    packet: Option<Vec<u8>>,
    state: Option<Vec<u64>>,
    covariance: Option<Vec<u64>>,
    time: Option<f64>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    car: Vec<u8>,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut output = Vec::new();
    for case in request.cases {
        let car = wire::car(&case.car)?;
        let mut learner = Estimator::new(&car, car.ratio, 1., 0., None)?;
        let mut rows = Vec::new();
        for operation in case.operations {
            if let Some(packet) = operation.packet {
                learner.handle(wire::decode(&packet)?)?;
            }
            if let Some(state) = operation.state {
                let x: Vec<_> = state.into_iter().map(f64::from_bits).collect();
                let p: Vec<_> = operation
                    .covariance
                    .ok_or("missing covariance")?
                    .into_iter()
                    .map(f64::from_bits)
                    .collect();
                learner.kf.reset(operation.time, &x, &diagonal(&p))?;
            }
            let msg = learner.parameters(true, true)?;
            let state = learner.kf.snapshot()?;
            rows.push(json!({"x_bits":state.x.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                "p_bits":state.covariance.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                "active":learner.active, "time":if state.time.is_nan() {None} else {Some(state.time)},
                "observed_bits":learner.observed.map(f64::to_bits), "packet":wire::encode(&msg,1234567890)?,
                "logs":std::mem::take(&mut learner.logs)}));
        }
        output.push(json!({"name":case.name,"rows":rows}));
    }
    let path = std::env::args_os().nth(1).ok_or("expected output path")?;
    serde_json::to_writer(std::fs::File::create(path)?, &output)?;
    Ok(())
}
