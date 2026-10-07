use openpilot_ui_application::onroad::driver_state::geometry::Geometry;
use openpilot_ui_framework::geometry::Rect;
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    orientation: [f64; 3],
    active: bool,
    rhd: bool,
    rect: Rect,
    #[serde(default)]
    reset: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let mut state = Geometry::default();
    let mut outputs = Vec::new();
    for step in steps {
        if step.reset {
            state = Geometry::default();
        }
        state.update(step.orientation, step.active, step.rhd, step.rect);
        outputs.push(serde_json::to_value(&state)?);
    }
    println!("{}", serde_json::to_string(&outputs)?);
    Ok(())
}
