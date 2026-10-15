use openpilot_signal_observer::pixels::{dark_scene, illuminated_green, lamp_evidence_with, Rgb};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    rgb: Rgb,
    #[serde(rename = "box")]
    bounds: [i32; 4],
    conservative_green: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let output: Vec<_> = cases
        .iter()
        .map(|i| Ok(json!({"dark":dark_scene(&i.rgb)?,"evidence":lamp_evidence_with(&i.rgb,i.bounds,i.conservative_green)?,"illuminated_green":illuminated_green(&i.rgb,i.bounds)?})))
        .collect::<Result<_, openpilot_opencv_runtime::Error>>()?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
