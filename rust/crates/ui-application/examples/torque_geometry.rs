use openpilot_ui_application::mici::onroad::torque_bar::geometry::{Arc, ArcCache};
use serde::{Deserialize, Serialize};
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    arc: Arc,
    #[serde(default)]
    reset: bool,
}
#[derive(Serialize)]
struct ObservedPoint {
    x: f64,
    y: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = match args.as_slice() {
        [_] => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            input
        }
        [_, flag, value] if flag == "--json" => value.clone(),
        _ => return Err("torque_geometry [--json INPUT] or JSON on stdin".into()),
    };
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let mut cache = ArcCache::default();
    let mut outputs = Vec::new();
    for step in steps {
        if step.reset {
            cache = ArcCache::default();
        }
        outputs.push(
            cache
                .points(step.arc)?
                .into_iter()
                .map(|point| ObservedPoint {
                    x: f64::from(point.x),
                    y: f64::from(point.y),
                })
                .collect::<Vec<_>>(),
        );
    }
    println!("{}", serde_json::to_string(&outputs)?);
    Ok(())
}
