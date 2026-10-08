use openpilot_ui_framework::{
    draw::{Draw, TextDraw},
    geometry::{MouseEvent, Point, Rect},
    text::{Font, Measure},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Input {
    frames: Vec<InputFrame>,
}
#[derive(Deserialize)]
struct InputFrame {
    now: f64,
    events: Vec<MouseEvent>,
    enabled: bool,
    visible: bool,
    awake: bool,
    valid: bool,
}
struct Probe {
    state: WidgetState,
    calls: Vec<String>,
}
impl Widget for Probe {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.calls
            .push(format!("paint:{}", self.state.is_pressed()));
        Ok(RenderResult::None)
    }
    fn mouse_press(&mut self, _: Point, _: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        self.calls.push("press".into());

        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.calls.push("release".into());
        self.state.release(frame.now);

        Ok(())
    }
    fn mouse_event(
        &mut self,
        event: MouseEvent,
        _: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.calls.push(format!("event:{}", event.slot));

        Ok(())
    }
}
struct NoDraw;
impl Measure for NoDraw {
    fn measure(&self, _: Font, _: &str, _: f32, _: f32) -> Point {
        Point::default()
    }
}
impl Draw for NoDraw {
    fn text(&mut self, _: TextDraw<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn rounded(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn border(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn texture(&mut self, _: bool, _: Rect, _: Point, _: f32) -> Result<(), Error> {
        Ok(())
    }
    fn scissor(&mut self, _: Option<Rect>) -> Result<(), Error> {
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin())?;
    let mut state = WidgetState::default();
    state.rect = Rect {
        x: 10.0,
        y: 10.0,
        width: 100.0,
        height: 100.0,
    };
    state.parent_rect = Some(Rect {
        x: 0.0,
        y: 0.0,
        width: 90.0,
        height: 90.0,
    });
    state.multi_touch = true;
    state.click_delay = Some(0.1);
    let mut probe = Probe {
        state,
        calls: Vec::new(),
    };
    let mut output = Vec::new();
    for input in input.frames {
        probe.state.enabled = input.enabled.into();
        probe.state.visible = input.visible.into();
        let valid = input.valid;
        probe.state.touch_valid = Some(Box::new(move || valid));
        let frame = Frame {
            index: 0,
            now: input.now,
            monotonic: 0.0,
            keyboard: &Default::default(),
            navigation: &Default::default(),
            dt: 0.05,
            target_fps: 20.0,
            awake: input.awake,
            events: &input.events,
            last_event: input.events.last().copied().unwrap_or_default(),
            cursor: openpilot_ui_framework::geometry::Point::default(),
            wheel: 0.0,
            show_touches: false,
        };
        probe.render(&frame, &mut NoDraw)?;
        output.push(serde_json::json!({"calls":std::mem::take(&mut probe.calls),"pressed":probe.state.is_pressed()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
