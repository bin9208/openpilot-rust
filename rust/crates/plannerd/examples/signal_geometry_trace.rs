use openpilot_plannerd::signal::geometry::Geometry;
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    geometry: Geometry,
    #[serde(rename = "box")]
    bounds: [f64; 4],
    distance: [f64; 2],
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let values:Vec<_>=cases.iter().map(|i|json!({"range":i.geometry.housing_range(i.bounds,3.5),"night":i.geometry.night_support(i.bounds),"alignment":i.geometry.alignment(i.bounds,i.distance)})).collect();
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
