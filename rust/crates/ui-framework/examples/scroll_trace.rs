use openpilot_ui_framework::{
    geometry::{MouseEvent, Rect},
    scroll::ScrollPanel,
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Input {
    horizontal: bool,
    bounce: bool,
    tici: bool,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    bounds: Rect,
    content: f64,
    events: Vec<MouseEvent>,
    dt: f64,
    enabled: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin())?;
    let mut panel = ScrollPanel::new(input.horizontal, input.bounce, input.tici);
    let mut output = Vec::new();
    for frame in input.frames {
        panel.enabled = frame.enabled.into();
        let offset = panel.update(frame.bounds, frame.content, &frame.events, frame.dt);
        output.push(serde_json::json!({"offset":offset,"velocity":panel.velocity,"state":panel.state,"touch_valid":panel.touch_valid()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
