use openpilot_radard::{
    in_lane::InLaneCornerTracker,
    model::{Model, VisionLead},
    point::Point,
    stationary_lane::StoppingLaneCornerTracker,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Read};
#[derive(Deserialize)]
struct Action {
    owner: usize,
    kind: String,
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    time: f64,
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    ego: f64,
    model: Model,
    points: Vec<Point>,
    vision: Option<VisionLead>,
    measurement: Option<f64>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = String::new();
    io::stdin().read_to_string(&mut bytes)?;
    let mut stopping = std::collections::HashMap::<usize, StoppingLaneCornerTracker>::new();
    let mut moving = std::collections::HashMap::<usize, InLaneCornerTracker>::new();
    let mut output = Vec::<Value>::new();
    for raw in serde_json::from_str::<Vec<Value>>(&bytes)? {
        if raw.get("reset").and_then(Value::as_bool) == Some(true) {
            let id = usize::try_from(raw["owner"].as_u64().ok_or("owner")?)?;
            if raw["kind"] == "stopping" {
                stopping.entry(id).or_default().reset();
            } else {
                moving.entry(id).or_default().reset();
            }
            output.push(Value::Null);
            continue;
        }
        let a: Action = serde_json::from_value(raw)?;
        if a.kind == "stopping" {
            let owner = stopping.entry(a.owner).or_default();
            let result = owner.update(a.time, a.ego, &a.model, &a.points, a.vision);
            output.push(json!({"output":result,"state":owner}));
        } else {
            let owner = moving.entry(a.owner).or_default();
            let result = owner.update(a.time, a.ego, &a.model, &a.points, a.vision, a.measurement);
            output.push(json!({"output":result,"state":owner}));
        }
    }
    serde_json::to_writer(io::stdout().lock(), &output)?;
    Ok(())
}
