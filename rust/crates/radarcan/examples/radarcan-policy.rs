#[path = "radarcan_policy/base.rs"]
mod base;
#[path = "radarcan_policy/batches.rs"]
mod batches;
#[path = "radarcan_policy/cluster.rs"]
mod cluster;
#[path = "radarcan_policy/constructor.rs"]
mod constructor;
#[path = "radarcan_policy/decoder.rs"]
mod decoder;
#[path = "radarcan_policy/decoder_hyundai.rs"]
mod decoder_hyundai;
#[path = "radarcan_policy/decoder_parser.rs"]
mod decoder_parser;
#[path = "radarcan_policy/decoder_settings.rs"]
mod decoder_settings;
#[path = "radarcan_policy/filters.rs"]
mod filters;
#[path = "radarcan_policy/parser.rs"]
mod parser;
#[path = "radarcan_policy/runtime.rs"]
mod runtime;
#[path = "radarcan_policy/runtime_input.rs"]
mod runtime_input;
#[path = "radarcan_policy/runtime_io.rs"]
mod runtime_io;

use openpilot_radarcan::{databases::Databases, numerics::Numerics, scalar::bits};
use serde_json::{json, Value};
use std::{error::Error, io::Read, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args().nth(1).ok_or("output path required")?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let requests: Vec<Value> = serde_json::from_str(&input)?;
    let requires_numerics = requests.iter().any(|case| {
        matches!(
            case["op"].as_str(),
            Some("track" | "weights" | "base" | "decoder" | "cluster" | "runtime")
        )
    });
    let mut numerics = if requires_numerics {
        Some(Numerics::load(Path::new(
            &std::env::args()
                .nth(2)
                .ok_or("numerical artifact required")?,
        ))?)
    } else {
        None
    };
    let mut results = Vec::new();
    for request in requests {
        let mut stdout = String::new();
        let result = match request["op"].as_str() {
            Some("constructor") => constructor::trace(
                request.clone(),
                &mut Databases::new(std::env::args().nth(3).ok_or("DBC assets required")?.into()),
                &mut stdout,
            )?,
            Some("runtime") => runtime::trace(
                request.clone(),
                &mut Databases::new(std::env::args().nth(3).ok_or("DBC assets required")?.into()),
                numerics.as_mut().ok_or("numerics absent")?,
                &mut stdout,
            )?,
            Some("cluster") => {
                cluster::trace(request.clone(), numerics.as_ref().ok_or("numerics absent")?)?
            }
            Some("decoder") => decoder::trace(
                request.clone(),
                &mut Databases::new(std::env::args().nth(3).ok_or("DBC assets required")?.into()),
                numerics.as_mut().ok_or("numerics absent")?,
                &mut stdout,
            )?,
            Some("batches") => batches::trace(request.clone())?,
            Some("lead_filter") => filters::lead(request.clone())?,
            Some("parser_sets") => parser::trace(request.clone(), &mut stdout)?,
            Some("base") => base::trace(
                request.clone(),
                numerics.as_mut().ok_or("numerics absent")?,
                &mut stdout,
            )?,
            Some("track") => {
                filters::track(request.clone(), numerics.as_mut().ok_or("numerics absent")?)?
            }
            Some("weights") => {
                let mut output = serde_json::Map::new();
                for count in request["counts"].as_array().ok_or("counts absent")? {
                    let count = count.as_u64().ok_or("invalid count")? as usize;
                    let weights = numerics
                        .as_mut()
                        .ok_or("numerics absent")?
                        .jerk_weights(count)?;
                    output.insert(count.to_string(), json!({"weights":weights,
                        "bits":weights.iter().map(|v| bits([("",*v)])[""].clone()).collect::<Vec<_>>()}));
                }
                Value::Object(output)
            }
            _ => return Err("unsupported policy operation".into()),
        };
        results.push(json!({"name":request["name"],"result":result,"stdout":stdout,"stderr":""}));
    }
    std::fs::write(output, serde_json::to_vec(&results)?)?;
    Ok(())
}
