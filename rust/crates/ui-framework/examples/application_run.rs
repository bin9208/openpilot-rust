use openpilot_startup_ui::{config::Config, diagnostics::Options};
use openpilot_ui_framework::{
    application::{Application, ApplicationConfig, Tick},
    button::Button,
    draw::Draw,
    geometry::Rect,
    inputbox::InputBox,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{cell::Cell, path::Path, rc::Rc};
struct Demo {
    state: WidgetState,
    input: InputBox,
    button: Button,
    clicked: Rc<Cell<bool>>,
}
impl Demo {
    fn new() -> Self {
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        let mut button = Button::new("Close");
        button.label.size = 30.0;
        button.state.click = Some(Box::new(move || flag.set(true)));
        Self {
            state: WidgetState::default(),
            input: InputBox::new(255, false),
            button,
            clicked,
        }
    }
}
impl Widget for Demo {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if !frame.events.is_empty() {
            eprintln!("fixture input {:?}", frame.events);
        }
        self.input.set_rect(Rect {
            x: 20.0,
            y: 30.0,
            width: 496.0,
            height: 100.0,
        });
        self.input.render(frame, draw)?;
        self.button.set_rect(Rect {
            x: 190.0,
            y: 160.0,
            width: 160.0,
            height: 60.0,
        });
        self.button.render(frame, draw)?;
        if self.clicked.get() {
            frame.navigation.push(NavigationRequest::Close);
        }
        Ok(RenderResult::None)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, mode, output] = args.as_slice() else {
        return Err("application_run ROOT MODE OUTPUT".into());
    };
    let mut app = Application::new(ApplicationConfig {
        graphics: Config {
            big: false,
            large_viewport: false,
            pc: true,
            scale: 1.0,
        },
        assets: Path::new(root).join("openpilot/selfdrive/assets"),
        language: "en".into(),
        title: "Rust framework lifecycle".into(),
        dimensions: None,
        fps: 20,
        diagnostics: Options::from_environment()?,
    })?;
    let demo = WidgetHandle::new(Demo::new());
    app.push(demo.clone());
    let ticks = Rc::new(Cell::new(0));
    let count = ticks.clone();
    let tick = Tick::new(move || count.set(count.get() + 1));
    app.add_tick(tick.clone());
    app.add_tick(tick);
    let mut skipped = 0;
    let mut calls = 0;
    if mode == "dynamic" {
        app.diagnostics.record_dir = Path::new(output).with_extension("videos");
        app.toggle_recording()?;
        app.diagnostics.start_recording(&mut app.canvas.renderer)?;
    }
    while !app.is_closed() {
        app.should_render = mode != "paused" || calls >= 2;
        if !app.render(|_, canvas| {
            canvas.renderer.screenshot(Path::new(output))?;
            Ok(())
        })? {
            skipped += 1;
        }
        calls += 1;
        if calls >= 400 {
            return Err("application fixture did not close".into());
        }
        if mode == "paused" && app.frame_count() >= 4 {
            app.request_close();
        }
    }
    if mode == "dynamic" {
        app.diagnostics.stop_recording()?;
        app.diagnostics.stop_recording()?;
    }
    app.diagnostics.close_recording()?;
    let text = demo.get::<Demo>()?.input.text();
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"text":text,"frames":app.frame_count(),"ticks":ticks.get(),"skipped":skipped,"closed":app.is_closed()}),
        )?,
    )?;
    Ok(())
}
