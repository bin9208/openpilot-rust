use openpilot_control_policy::{
    identity::{Identity, Interface},
    similarity,
};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
struct Case {
    fingerprint: String,
    flags: u32,
    speed: f64,
    cruise: f64,
    angle: f64,
}
#[derive(Deserialize)]
struct Selection {
    fingerprint: String,
    firmware: String,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
    files: Vec<String>,
    selection: Vec<Selection>,
    similarity: Vec<[String; 2]>,
    neural_keys: Vec<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut cases = Vec::new();
    for case in request.cases {
        let identity = Identity::lookup(&case.fingerprint)?;
        let torque = if identity.interface == Interface::Gm
            && request.neural_keys.contains(&case.fingerprint)
        {
            "torque_from_lateral_accel_neural"
        } else if identity.interface == Interface::Gm && identity.siglin.is_some() {
            "torque_from_lateral_accel_siglin"
        } else {
            "torque_from_lateral_accel_linear"
        };
        cases.push(json!({"limits":identity.accel_limits(case.flags, case.speed, case.cruise)?,"feedforward":identity.steer_feedforward(case.angle,case.speed),"torque":torque}));
    }
    let selection: Vec<_> = request
        .selection
        .iter()
        .map(|q| similarity::select(&request.files, &q.fingerprint, &q.firmware))
        .collect();
    let similarity: Vec<_> = request
        .similarity
        .iter()
        .map(|[a, b]| similarity::ratio(a, b))
        .collect();
    let path = std::env::args_os().nth(1).ok_or("output")?;
    serde_json::to_writer(
        std::fs::File::create(path)?,
        &json!({"cases":cases,"selection":selection,"similarity":similarity}),
    )?;
    Ok(())
}
