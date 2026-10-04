use openpilot_startup_ui::{config::Config, draw::Draw, geometry::Rect, renderer::Renderer};
use openpilot_ui_application::qr::{texture::Texture, Correction};
use openpilot_ui_framework::canvas::Canvas;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, output] = args.as_slice() else {
        return Err("qr_render ROOT OUTPUT".into());
    };
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
    let mut texture = Texture::new(Correction::Medium);
    let mut states = Vec::new();
    for value in [
        Some("http://192.0.2.4:7000"),
        Some("http://192.0.2.4:7000"),
        None,
        Some("http://[2001:db8::1]:7000"),
        Some("http://192.0.2.5:7000"),
    ] {
        let changed = texture.set_data(value, &mut canvas)?;
        states.push(serde_json::json!({"changed":changed,"available":texture.available()}));
    }
    canvas.renderer.begin();
    canvas.rounded(
        Rect {
            x: 0.0,
            y: 0.0,
            width: 536.0,
            height: 240.0,
        },
        0.0,
        u32::from_le_bytes([38, 38, 38, 255]),
    )?;
    texture.draw(
        &mut canvas,
        Rect {
            x: 168.0,
            y: 20.0,
            width: 200.0,
            height: 200.0,
        },
    )?;
    canvas.renderer.screenshot(Path::new(output))?;
    canvas.renderer.end();
    texture.destroy();
    canvas.renderer.release_dynamic_textures();
    states.push(serde_json::json!({"available":texture.available()}));
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec_pretty(&states)?,
    )?;
    let late = canvas
        .renderer
        .dynamic_pixels(1, 1, &[255, 255, 255, 255])?;
    drop(canvas);
    drop(late);
    println!("PASS dynamic texture release, renderer shutdown and late handle drop");
    Ok(())
}
