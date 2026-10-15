use openpilot_startup_ui::recovery::view::View;
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
struct Scene {
    #[serde(default)]
    native_repair: bool,
    width: i32,
    height: i32,
    status: String,
    detail: String,
    reason: String,
    error: String,
    address: String,
    page: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let root = PathBuf::from(&args[0]);
    let scene: Scene = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let output = PathBuf::from(&args[2]);
    let mut view = View::new(
        &root,
        (scene.width, scene.height),
        &scene.reason,
        &scene.error,
        true,
        scene.native_repair,
    )?;
    view.page = scene.page % view.pages.len();
    let areas = view.preview(&scene.status, &scene.detail, &scene.address, &output)?;
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec(
            &serde_json::json!({"pages":view.pages,"page":view.page,"areas":areas.map(|r|r.map(f64::from))}),
        )?,
    )?;
    Ok(())
}
