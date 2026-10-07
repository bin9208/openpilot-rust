use crate::{mici::widgets::big_button::BigButton, paint};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};

pub(super) struct Button {
    pub button: BigButton,
    pub target: WidgetHandle,
}
pub(super) struct Icon<'a> {
    pub path: &'a str,
    pub size: (i32, i32),
}
impl Button {
    pub fn new(
        canvas: &mut Canvas,
        title: &str,
        icon: Icon<'_>,
        target: WidgetHandle,
    ) -> Result<Self, Error> {
        let mut button = BigButton::new(title, |path, size| paint::texture(canvas, path, size))?;
        button.icon = Some(paint::texture(canvas, icon.path, icon.size)?);
        button.font_size = Some(64.0);
        Ok(Self { button, target })
    }
}
impl Widget for Button {
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
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(position, frame, draw)?;
        frame
            .navigation
            .push(NavigationRequest::Push(self.target.clone()));
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.paint(frame, draw)
    }
}
