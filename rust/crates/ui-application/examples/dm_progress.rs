use openpilot_ui_application::mici::layouts::dm_progress::{Input, Progress};
use openpilot_ui_framework::geometry::Rect;
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    input: Input,
    #[serde(default)]
    show: bool,
    value: Option<f64>,
    right_hand_drive: bool,
    rect: Option<Rect>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    let steps: Vec<Step> = serde_json::from_str(&text)?;
    let mut state = Progress::default();
    let mut output = Vec::new();
    for step in steps {
        if step.show {
            state.show();
        }
        if let Some(value) = step.value {
            state.value = value;
        }
        state.update(&step.input);
        let r = state.ring(
            step.rect.unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                width: 536.0,
                height: 240.0,
            }),
            step.right_hand_drive,
        );
        output.push(serde_json::json!({"progress":state.value,"enabled":state.good_enabled,"ring":[r.center.x,r.center.y,r.inner,r.outer,r.start,r.end],"segments":r.segments,"color":r.color}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
