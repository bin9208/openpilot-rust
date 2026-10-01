use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::widgets::{
    carrot_web::CarrotWeb, prime::PrimeWidget, setup::SetupWidget,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::{Point, Rect},
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, Widget},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};
#[path = "support/context.rs"]
mod context;
#[derive(Deserialize)]
pub struct Scene {
    kind: String,
    config: Config,
    language: String,
    rect: Rect,
    frames: u32,
    prime: i32,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    address: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("product_render ROOT SCENE OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    let context = context::context(root, &scene, &output.with_extension("owned"))?;
    let assets = root.join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, &scene.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let mut widget: Box<dyn Widget> = match scene.kind.as_str() {
        "ssh" => Box::new(openpilot_ui_application::widgets::ssh::SshAction::new(context.clone(), &mut canvas)?),
        "prime" => Box::new(PrimeWidget::new(context.clone())),
        "setup" => Box::new(SetupWidget::new(context.clone())),
        "pairing" if !scene.config.big => {
            let mut widget = openpilot_ui_application::mici::widgets::pairing::Pairing::new(
                context.clone(),
                &mut canvas,
            )?;
            widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
            Box::new(widget.navigation(context.clone(), &canvas))
        }
        "pairing" => {
            let mut widget = openpilot_ui_application::widgets::pairing::Pairing::new(
                context.clone(),
                &mut canvas,
            )?;
            widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
            Box::new(widget)
        }
        "carrot-web" => Box::new(CarrotWeb::new(context.clone())),
        _ => return Err("unknown product kind".into()),
    };
    widget.set_rect(scene.rect);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    for index in 0..scene.frames {
        let now = f64::from(index) / 20.0;
        let frame = Frame {
            now,
            monotonic: now,
            keyboard: &keyboard,
            navigation: &navigation,
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events: &[],
            last_event: Default::default(),
            cursor: Point::default(),
            wheel: 0.0,
            show_touches: false,
        };
        if index == 0 {
            widget.show(&frame);
        }
        canvas.renderer.begin();
        widget.render(&frame, &mut canvas)?;
        results.push(serde_json::json!({"prime":context.prime.get()}));
        if index + 1 == scene.frames {
            canvas.renderer.screenshot(output)?;
        }
        canvas.renderer.end();
    }
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
