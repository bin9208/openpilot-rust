use openpilot_radarcan::{clustering, numerics::Numerics, Error};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Request {
    previous: Vec<[f64; 3]>,
    current: Vec<[f64; 3]>,
    max_distance: f64,
}

pub fn trace(request: Value, numerics: &Numerics) -> Result<Value, Error> {
    let request: Request = serde_json::from_value(request)?;
    let result = clustering::correlate(
        &request.previous,
        &request.current,
        request.max_distance,
        numerics,
    )?;
    if request.previous.is_empty() || request.current.is_empty() {
        return Ok(json!({"labels":result.labels}));
    }
    let width = request.previous.len();
    Ok(json!({"labels":result.labels,
        "dot":result.dot.chunks(width).collect::<Vec<_>>(),
        "dot_bits":result.dot.iter().map(|v| v.to_le_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>()).collect::<Vec<_>>(),
        "distance":result.distance.chunks(width).collect::<Vec<_>>(),
        "distance_bits":result.distance.iter().map(|v| v.to_le_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>()).collect::<Vec<_>>() }))
}
