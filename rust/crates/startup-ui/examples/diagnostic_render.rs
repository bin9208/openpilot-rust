use openpilot_startup_ui::{
    config::Config,
    diagnostics::{Diagnostics, Options},
    draw::{Draw, TextDraw},
    geometry::{MouseEvent, Point, Rect},
    renderer::Renderer,
    text::Font,
};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, mode, output] = args.as_slice() else {
        return Err("diagnostic_render ROOT MODE OUTPUT".into());
    };
    let mut renderer = Renderer::new(
        Config {
            big: false,
            large_viewport: false,
            pc: true,
            scale: 1.0,
        },
        &Path::new(root).join("openpilot/selfdrive/assets"),
        false,
        "en",
    )?;
    let options = Options {
        burn_in: mode == "burn-in",
        show_touches: mode == "touches",
        grid: if mode == "grid" { 32 } else { 0 },
        ..Options::default()
    };
    let mut diagnostics = Diagnostics::new(&mut renderer, options, 20)?;
    let events = [
        MouseEvent {
            pos: Point { x: 70.0, y: 70.0 },
            slot: 0,
            pressed: true,
            released: false,
            down: true,
            time: 0.0,
        },
        MouseEvent {
            pos: Point { x: 90.0, y: 80.0 },
            slot: 0,
            pressed: false,
            released: false,
            down: true,
            time: 0.1,
        },
        MouseEvent {
            pos: Point { x: 120.0, y: 90.0 },
            slot: 0,
            pressed: false,
            released: false,
            down: true,
            time: 0.2,
        },
    ];
    renderer.begin();
    renderer.rounded(
        Rect {
            x: 20.0,
            y: 20.0,
            width: 110.0,
            height: 140.0,
        },
        0.0,
        0xff000000,
    )?;
    renderer.rounded(
        Rect {
            x: 140.0,
            y: 20.0,
            width: 110.0,
            height: 140.0,
        },
        0.0,
        0xff800000,
    )?;
    renderer.rounded(
        Rect {
            x: 260.0,
            y: 20.0,
            width: 110.0,
            height: 140.0,
        },
        0.0,
        0xffff0000,
    )?;
    renderer.text(TextDraw {
        font: Font::Normal,
        text: "Native diagnostics",
        position: Point { x: 30.0, y: 175.0 },
        size: renderer.config.scaled_font(32.0),
        spacing: 0.0,
        color: u32::MAX,
    })?;
    renderer.finish_content();
    diagnostics.overlays(&mut renderer, &events)?;
    renderer.screen_screenshot(Path::new(output))?;
    renderer.present();
    Ok(())
}
