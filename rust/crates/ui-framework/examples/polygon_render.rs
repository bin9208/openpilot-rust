use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{Point, Rect},
    polygon::{self, Fill, Gradient},
};
use serde::Deserialize;
use std::path::Path;
#[derive(Deserialize)]
struct Scene {
    points: Vec<Point>,
    origin: Rect,
    mode: String,
    #[serde(default)]
    expect_error: bool,
    colors: Vec<u32>,
    stops: Vec<f64>,
    start: (f64, f64),
    end: (f64, f64),
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("polygon_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(
        Config {
            big: false,
            large_viewport: false,
            pc: true,
            scale: 1.0,
        },
        &assets,
        false,
        "en",
    )?;
    let mut canvas = Canvas::new(renderer, &assets);
    let gradient = Gradient::new(scene.start, scene.end, scene.colors.clone(), scene.stops);
    let mut failed = false;
    for index in 0..3 {
        canvas.renderer.begin();
        let result = match scene.mode.as_str() {
            "gradient" => polygon::polygon(
                &mut canvas,
                &scene.points,
                (scene.origin, Fill::Gradient(&gradient)),
            ),
            "color" => polygon::polygon(
                &mut canvas,
                &scene.points,
                (scene.origin, Fill::Color(scene.colors[0])),
            ),
            "solid" => polygon::solid(&mut canvas, &scene.points, scene.colors[0]),
            _ => return Err("unknown polygon mode".into()),
        };
        if let Err(error) = result {
            if scene.expect_error {
                failed = true;
            } else {
                return Err(error.into());
            }
        }
        if index == 2 {
            canvas.renderer.screenshot(Path::new(output))?;
        }
        canvas.renderer.end();
    }
    canvas.renderer.cleanup_polygon()?;
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec(
            &serde_json::json!({"failed":failed,"strip":polygon::triangulate(&scene.points),"stops":gradient.stops,"colors":gradient.colors}),
        )?,
    )?;
    Ok(())
}
