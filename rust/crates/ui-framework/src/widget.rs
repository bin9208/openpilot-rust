use crate::{
    draw::Draw,
    geometry::{MouseEvent, Point, Rect},
    Error,
};

mod handles;
mod state;
mod types;
pub use handles::{NavigationQueue, NavigationRequest, WeakWidgetHandle, WidgetHandle};
pub use state::WidgetState;
pub use types::{DialogResult, Property, RenderResult};
pub struct Frame<'a> {
    pub index: u64,
    pub now: f64,
    pub monotonic: f64,
    pub keyboard: &'a crate::keys::KeyboardInput,
    pub navigation: &'a NavigationQueue,
    pub dt: f64,
    pub target_fps: f64,
    pub awake: bool,
    pub events: &'a [MouseEvent],
    pub last_event: MouseEvent,
    pub cursor: Point,
    pub wheel: f64,
    pub show_touches: bool,
}

pub trait Widget: std::any::Any {
    fn state(&self) -> &WidgetState;
    fn state_mut(&mut self) -> &mut WidgetState;
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error>;
    fn update(&mut self, _frame: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        Ok(())
    }
    fn layout(&mut self, _frame: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        Ok(())
    }
    fn layout_changed(&mut self) {}
    fn mouse_press(
        &mut self,
        _position: Point,
        _frame: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn mouse_release(
        &mut self,
        _position: Point,
        frame: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state_mut().release(frame.now);

        Ok(())
    }
    fn mouse_event(
        &mut self,
        _event: MouseEvent,
        _frame: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state_mut().children {
            child.show(frame);
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state_mut().children {
            child.hide(frame);
        }
    }
    fn set_rect(&mut self, rect: Rect) {
        let old = self.state().rect;
        self.state_mut().rect = rect;
        if old.x != rect.x
            || old.y != rect.y
            || old.width != rect.width
            || old.height != rect.height
        {
            self.layout_changed();
        }
    }
    fn set_parent_rect(&mut self, rect: Rect) {
        self.state_mut().parent_rect = Some(rect);
    }
    fn back_enabled(&self) -> bool {
        true
    }
    fn dismiss_navigation(&mut self, callback: Option<Box<dyn FnOnce()>>, frame: &Frame<'_>) {
        frame.navigation.push(NavigationRequest::Pop(callback));
    }
    fn take_navigation(&mut self) -> Option<NavigationRequest> {
        None
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.set_rect(Rect {
            x,
            y,
            ..self.state().rect
        });
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        render_widget(self, frame, draw)
    }
    fn process_mouse_events(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let hit = self.state().hit_rect();
        let valid = self.state().interaction_gate
            && self
                .state()
                .touch_valid
                .as_ref()
                .is_none_or(|valid| valid());
        for event in frame.events {
            let slot = usize::from(event.slot);
            if slot >= 2 || (!self.state().multi_touch && slot != 0) {
                continue;
            }
            let inside = hit.contains(event.pos);
            if event.pressed && valid {
                if inside {
                    self.mouse_press(event.pos, frame, draw)?;
                    self.state_mut().pressed[slot] = true;
                    self.state_mut().tracking[slot] = true;
                    self.mouse_event(*event, frame, draw)?;
                }
            } else if !valid {
                self.state_mut().pressed[slot] = false;
                self.state_mut().tracking[slot] = false;
            } else if event.released {
                self.mouse_event(*event, frame, draw)?;
                if self.state().pressed[slot] && inside {
                    self.mouse_release(event.pos, frame, draw)?;
                }
                self.state_mut().pressed[slot] = false;
                self.state_mut().tracking[slot] = false;
            } else if inside {
                if self.state().tracking[slot] {
                    self.state_mut().pressed[slot] = true;
                    self.mouse_event(*event, frame, draw)?;
                }
            } else {
                self.state_mut().pressed[slot] = false;
                self.mouse_event(*event, frame, draw)?;
            }
        }
        Ok(())
    }
}

pub fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = (a.x + a.width).min(b.x + b.width) - x;
    let height = (a.y + a.height).min(b.y + b.height) - y;
    if width <= 0.0 || height <= 0.0 {
        Rect::default()
    } else {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn render_widget<W: Widget + ?Sized>(
    widget: &mut W,
    frame: &Frame<'_>,
    draw: &mut dyn Draw,
) -> Result<RenderResult, Error> {
    widget.update(frame, draw)?;
    widget.state_mut().expire(frame.now);
    if !widget.state().visible.get() {
        return Ok(RenderResult::None);
    }
    widget.layout(frame, draw)?;
    let result = widget.paint(frame, draw)?;
    if frame.show_touches {
        let rect = widget.state().rect;
        draw.rectangle_lines(
            Rect {
                x: rect.x.trunc(),
                y: rect.y.trunc(),
                width: rect.width.trunc().max(1.0),
                height: rect.height.trunc().max(1.0),
            },
            u32::from_le_bytes([230, 41, 55, 255]),
        )?;
    }
    if widget.state().enabled.get() && widget.state().was_awake {
        widget.process_mouse_events(frame, draw)?;
    } else {
        let state = widget.state_mut();
        state.pressed = [false; 2];
        state.tracking = [false; 2];
    }
    widget.state_mut().was_awake = frame.awake;
    Ok(result)
}
