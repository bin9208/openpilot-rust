mod support;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{MouseEvent, Rect},
    navigation::NavWidget,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use serde::Deserialize;
use std::{cell::Cell, rc::Rc};
#[derive(Deserialize)]
struct Input {
    frames: Vec<InputFrame>,
}
#[derive(Deserialize)]
struct InputFrame {
    now: f64,
    events: Vec<MouseEvent>,
    show: bool,
    dismiss: bool,
    enabled: bool,
    back: bool,
}
#[derive(Default)]
struct Empty(WidgetState);
impl Widget for Empty {
    fn state(&self) -> &WidgetState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.0
    }
    fn paint(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<RenderResult, Error> {
        Ok(RenderResult::None)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin())?;
    let mut widget = NavWidget::new(Box::new(Empty::default()), 20.0, 240.0);
    widget.set_rect(Rect {
        x: 0.0,
        y: 0.0,
        width: 536.0,
        height: 240.0,
    });
    let back = Rc::new(Cell::new(true));
    let view = back.clone();
    widget.back_enabled = Box::new(move || view.get());
    let back_count = Rc::new(Cell::new(0));
    let count = back_count.clone();
    widget.set_back_callback(Box::new(move || count.set(count.get() + 1)));
    let shown = Rc::new(Cell::new(0));
    let dismissed = Rc::new(Cell::new(0));
    let mut pops = 0;
    let mut last = MouseEvent::default();
    let mut output = Vec::new();
    for input in input.frames {
        if let Some(event) = input.events.last() {
            last = *event;
        }
        let frame = Frame {
            now: input.now,
            monotonic: 0.0,
            keyboard: &Default::default(),
            navigation: &Default::default(),
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events: &input.events,
            last_event: last,
            cursor: openpilot_ui_framework::geometry::Point::default(),
            wheel: 0.0,
            show_touches: false,
        };
        back.set(input.back);
        widget.state.enabled = input.enabled.into();
        if input.show {
            let count = shown.clone();
            widget.on_shown = Some(Box::new(move || count.set(count.get() + 1)));
            widget.show(&frame);
        }
        if input.dismiss {
            let count = dismissed.clone();
            widget.dismiss(Some(Box::new(move || count.set(count.get() + 1))));
        }
        widget.render(&frame, &mut support::NoDraw)?;
        if let Some(NavigationRequest::Pop(callback)) = widget.take_navigation() {
            pops += 1;
            if let Some(callback) = callback {
                callback();
            }
        }
        output.push(serde_json::json!({"y":widget.state.rect.y,"position":widget.motion.position.position.x,"velocity":widget.motion.position.velocity.x,"bar_y":widget.motion.bar_position.x,"bar_alpha":widget.motion.bar_alpha.x,"dragging":widget.motion.dragging_down,"playing":widget.motion.playing_dismiss,"pops":pops,"back":back_count.get(),"shown":shown.get(),"dismissed":dismissed.get()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
