use crate::{
    context::Context,
    paint::{self, color, Image},
    params::Read,
    state::messages,
};
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct ExpButton {
    state: WidgetState,
    context: Context,
    experimental: bool,
    engageable: bool,
    held_mode: Option<bool>,
    hold_end: Option<f64>,
    wheel: Texture,
    experiment: Texture,
}
impl ExpButton {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        button_size: i32,
        icon_size: i32,
    ) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: float(f64::from(button_size)),
            height: float(f64::from(button_size)),
        };
        Ok(Self {
            state,
            context,
            experimental: false,
            engageable: false,
            held_mode: None,
            hold_end: None,
            wheel: paint::texture(canvas, "img_chffr_wheel.png", (icon_size, icon_size))?,
            experiment: paint::texture(canvas, "icons/experimental.png", (icon_size, icon_size))?,
        })
    }
    pub fn experimental(&self) -> bool {
        self.experimental
    }
    pub fn held_mode(&self) -> Option<bool> {
        self.held_mode
    }
    pub fn hold_end(&self) -> Option<f64> {
        self.hold_end
    }
    fn held_or_actual(&mut self) -> bool {
        let now = (self.context.now_monotonic)();
        if self.hold_end.is_some_and(|end| end != 0.0 && now < end) {
            return self.held_mode.unwrap_or(false);
        }
        if self.hold_end.is_some_and(|end| end != 0.0 && now >= end) {
            self.hold_end = None;
            self.held_mode = None;
        }
        self.experimental
    }
}
impl Widget for ExpButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn set_rect(&mut self, rect: Rect) {
        self.state.rect.x = rect.x;
        self.state.rect.y = rect.y;
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let messages = self.context.messages.borrow();
        let state = messages::selfdrive_state(&messages.state)?;
        self.experimental = state.get_experimental_mode();
        self.engageable = state.get_engageable() || state.get_enabled();
        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        if self.context.params.boolean("ExperimentalModeConfirmed")?
            && self.context.ui.borrow().slow.has_longitudinal_control
        {
            let mode = !self.experimental;
            self.context.params.put_bool("ExperimentalMode", mode)?;
            self.held_mode = Some(mode);
            self.hold_end = Some((self.context.now_monotonic)() + 2.0);
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let cx = (f64::from(rect.x) + (f64::from(rect.width) / 2.0).floor()).trunc();
        let cy = (f64::from(rect.y) + (f64::from(rect.height) / 2.0).floor()).trunc();
        let tint = color(
            255,
            255,
            255,
            if self.state.is_pressed() || !self.engageable {
                180
            } else {
                255
            },
        );
        let texture = if self.held_or_actual() {
            self.experiment
        } else {
            self.wheel
        };
        draw.circle(
            Point {
                x: float(cx),
                y: float(cy),
            },
            rect.width / 2.0,
            color(0, 0, 0, 166),
        )?;
        paint::image(
            draw,
            Image {
                texture,
                rect: Rect {
                    x: float(cx - f64::from(texture.width) / 2.0),
                    y: float(cy - f64::from(texture.height) / 2.0),
                    width: texture.width,
                    height: texture.height,
                },
                tint,
                origin: Point::default(),
                rotation: 0.0,
            },
        )?;
        Ok(RenderResult::None)
    }
}
