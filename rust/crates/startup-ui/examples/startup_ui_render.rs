use openpilot_startup_ui::{
    config::Config, geometry::MouseEvent, renderer::Renderer, spinner::Spinner,
    text_window::TextWindow,
};
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
struct Scene {
    config: Config,
    kind: String,
    #[serde(default)]
    updates: Vec<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    language: String,
    ip: String,
    #[serde(default)]
    events: Vec<MouseEvent>,
    #[serde(default)]
    wheel: f32,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let root = PathBuf::from(&args[1]);
    let scene: Scene = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let output = PathBuf::from(&args[3]);
    let mut renderer = Renderer::new(
        scene.config,
        &root.join("openpilot/selfdrive/assets"),
        scene.kind == "spinner",
        &scene.language,
    )?;
    let state = if scene.kind == "spinner" {
        let mut spinner = Spinner::default();
        for update in &scene.updates {
            spinner.set_text(update, scene.config, &renderer)?;
        }
        spinner.rotation = 45.0;
        for _ in 0..3 {
            renderer.begin();
            spinner.render(scene.config, &scene.ip, 0.0, &mut renderer)?;
            renderer.screenshot(&output)?;
            renderer.end();
        }
        serde_json::to_value(&spinner)?
    } else {
        let mut viewer = TextWindow::new(&scene.text, scene.config, &renderer);
        for frame in 0..3 {
            renderer.begin();
            viewer.render(
                scene.config,
                &scene.ip,
                if frame == 0 { &scene.events } else { &[] },
                if frame == 0 { scene.wheel } else { 0.0 },
                &mut renderer,
            )?;
            renderer.screenshot(&output)?;
            renderer.end();
        }
        serde_json::to_value(&viewer)?
    };
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&state)?,
    )?;
    Ok(())
}
