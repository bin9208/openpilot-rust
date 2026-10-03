use crate::{
    context::{Action, Context},
    mici::widgets::big_button::BigButton,
    paint,
    services::egpu::{Backend, Check},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::sync::Arc;

pub(super) struct CheckButton {
    button: BigButton,
    context: Context,
    check: Check,
}
impl CheckButton {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        backend: Arc<dyn Backend>,
    ) -> Result<Self, Error> {
        let mut button = BigButton::new("check connection", |path, size| {
            paint::texture(canvas, path, size)
        })?;
        button.icon = Some(paint::texture(
            canvas,
            "icons_mici/settings/network/wifi_strength_full.png",
            (76, 56),
        )?);
        Ok(Self {
            button,
            context,
            check: Check::new(backend),
        })
    }
}
impl Widget for CheckButton {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if let Err(error) = self.check.poll() {
            self.context.actions.push(Action::Failure(error));
        }
        if !self.check.running() && self.button.value == "checking..." {
            self.button.set_rotate(false, frame.now);
            self.button.value = self
                .check
                .result
                .clone()
                .unwrap_or_else(|| "no errors".into());
        }
        self.button.state.enabled =
            (!self.context.ui.borrow().started && !self.check.running()).into();
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(position, frame, draw)?;
        if self.context.ui.borrow().started || self.check.running() {
            return Ok(());
        }
        self.check.start()?;
        self.button.value = "checking...".into();
        self.button.set_rotate(true, frame.now);
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.paint(frame, draw)
    }
}
