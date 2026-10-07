use openpilot_xiaoge::lane::Head;
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Input {
    predictions: PathBuf,
    prototypes: PathBuf,
    confidence: f32,
    iou: f32,
}

fn floats(path: PathBuf) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    if bytes.len() % 4 != 0 {
        return Err("incomplete f32 tensor".into());
    }
    bytes
        .chunks_exact(4)
        .map(|bytes| Ok(f32::from_le_bytes(bytes.try_into()?)))
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input: Vec<Input> = serde_json::from_str(&input)?;
    let output = input
        .into_iter()
        .map(|case| {
            let predictions = floats(case.predictions)?;
            let prototypes = floats(case.prototypes)?;
            let head = Head::new(&predictions, &prototypes)?;
            let candidates = head.candidates(case.confidence, case.iou)?;
            let result = openpilot_xiaoge::lane::select(&candidates)?;
            Ok(json!({"candidates": candidates, "result": result}))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    std::io::stdout().flush()?;
    Ok(())
}
