//! Compare real final-source text fallback/measurements and complete RGBA renders.
use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_framework::{canvas::Canvas, geometry::Point, text::Font, text_layout};
use serde::Deserialize;
use std::path::Path;
#[derive(Deserialize)]
struct Row {
    font: String,
    text: String,
    measured: [f32; 2],
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output, language] = args.as_slice() else {
        return Err("font_render ROOT INPUT OUTPUT LANGUAGE".into());
    };
    let rows: Vec<Row> = serde_json::from_slice(&std::fs::read(input)?)?;
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
        language,
    )?;
    let mut canvas = Canvas::new(renderer, &assets);
    std::fs::create_dir_all(output)?;
    let mut results = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let font = match row.font.as_str() {
            "normal" => Font::Normal,
            "bold" => Font::Bold,
            "display" => Font::Display,
            _ => return Err("unknown font".into()),
        };
        canvas.renderer.begin();
        let measured = text_layout::measure(&canvas, font, &row.text, 32., 0.);
        text_layout::draw_text(
            &mut canvas,
            font,
            &row.text,
            Point { x: 10., y: 50. },
            32.,
            0.,
            0xffffffff,
        )?;
        canvas
            .renderer
            .screenshot(&Path::new(output).join(format!("{index}.png")))?;
        canvas.renderer.end();
        results
            .push(serde_json::json!({"measured":[measured.x,measured.y],"expected":row.measured}));
    }
    std::fs::write(
        Path::new(output).join("actual.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
