use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_application::{
    mici::onroad::model_renderer::ModelRenderer as Small,
    onroad::model_renderer::ModelRenderer as Big, state::Status,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    geometry::Rect,
    keys::KeyboardInput,
    widget::{Frame, NavigationQueue, Widget},
};
use serde::Deserialize;
use serde_json::json;
use std::io::Write;
use std::{collections::BTreeMap, path::Path};
#[path = "support/model_context.rs"]
mod context;
#[path = "support/model_record.rs"]
mod record;
#[derive(Deserialize)]
pub struct Suite {
    config: Config,
    language: String,
    cases: Vec<Scene>,
}
#[derive(Deserialize)]
pub struct Scene {
    name: String,
    params: BTreeMap<String, Vec<u8>>,
    rect: Rect,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    ui: Ui,
    params: BTreeMap<String, Vec<u8>>,
    messages: Vec<Vec<u8>>,
    transform: Option<[[f64; 3]; 3]>,
    capture: bool,
}
#[derive(Deserialize)]
struct Ui {
    status: UiStatus,
    lat_active: bool,
    started_frame: i64,
    is_metric: bool,
    show_radar_info: i32,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UiStatus {
    Disengaged,
    Engaged,
    Override,
}
enum Model {
    Big(Box<Big>),
    Small(Box<Small>),
}
impl Model {
    fn widget(&mut self) -> &mut dyn Widget {
        match self {
            Self::Big(r) => r.as_mut(),
            Self::Small(r) => r.as_mut(),
        }
    }
    fn transform(&mut self, t: [[f64; 3]; 3]) {
        match self {
            Self::Big(r) => r.set_transform(t),
            Self::Small(r) => r.set_transform(t),
        }
    }
    fn snapshot(&self) -> serde_json::Value {
        match self {
            Self::Big(r) => json!({"common":r.common,"carrot":r.carrot,"settings":r.settings}),
            Self::Small(r) => {
                json!({"common":r.common,"filters":r.filters,"marking_codes":r.marking_codes,"marking_segments":r.marking_segments,"lead":r.lead,"lead_filter":r.lead_filter,"radar_items":r.radar_items,"gradient":{"colors":r.gradient.colors,"stops":r.gradient.stops}})
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output] = args.as_slice() else {
        return Err("model_render ROOT INPUT OUTPUT".into());
    };
    let suite: Suite = serde_json::from_slice(&std::fs::read(input)?)?;
    let root = Path::new(root);
    let output = Path::new(output);
    std::fs::create_dir_all(output)?;
    let assets = root.join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(suite.config, &assets, false, &suite.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let keyboard = KeyboardInput::default();
    let navigation = NavigationQueue::default();
    let mut results = Vec::new();
    for scene in &suite.cases {
        let context = context::context(
            root,
            scene,
            (&suite, &output.join(format!("{}-owned", scene.name))),
        )?;
        std::io::stderr().write_all(format!("MODEL_PARAMS_BEGIN {}\n", scene.name).as_bytes())?;
        let mut model = if suite.config.big {
            Model::Big(Box::new(Big::new(context.clone())?))
        } else {
            Model::Small(Box::new(Small::new(context.clone())?))
        };
        model.widget().set_rect(scene.rect);
        let mut rows = Vec::new();
        for (index, step) in scene.steps.iter().enumerate() {
            for (key, value) in &step.params {
                context.params.put(key, value)?;
            }
            {
                let mut ui = context.ui.borrow_mut();
                ui.started_frame = step.ui.started_frame;
                ui.lat_active = step.ui.lat_active;
                ui.realtime.value.is_metric = step.ui.is_metric;
                ui.slow.show_radar_info = step.ui.show_radar_info;
                ui.status = match step.ui.status {
                    UiStatus::Disengaged => Status::Disengaged,
                    UiStatus::Engaged => Status::Engaged,
                    UiStatus::Override => Status::Override,
                };
            }
            context
                .messages
                .borrow_mut()
                .state
                .update(step.now, &step.messages)?;
            if let Some(matrix) = step.transform {
                model.transform(matrix);
            }
            let frame = Frame {
                index: 0,
                now: step.now,
                monotonic: step.now,
                keyboard: &keyboard,
                navigation: &navigation,
                dt: 0.05,
                target_fps: 20.,
                awake: true,
                events: &[],
                last_event: Default::default(),
                cursor: Default::default(),
                wheel: 0.,
                show_touches: false,
            };
            canvas.renderer.begin();
            let mut draw = record::Recording {
                canvas: &mut canvas,
                commands: Default::default(),
            };
            model.widget().render(&frame, &mut draw)?;
            let commands = draw.commands.into_inner();
            rows.push(json!({"state":model.snapshot(),"commands":commands}));
            if step.capture {
                canvas
                    .renderer
                    .screenshot(&output.join(format!("{}-{index}.png", scene.name)))?;
            }
            canvas.renderer.end();
        }
        std::io::stderr().write_all(format!("MODEL_PARAMS_END {}\n", scene.name).as_bytes())?;
        results.push(json!({"name":scene.name,"rows":rows}));
    }
    std::fs::write(output.join("trace.json"), serde_json::to_vec(&results)?)?;
    Ok(())
}
