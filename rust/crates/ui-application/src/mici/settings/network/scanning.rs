use super::{assets::Assets, loading::Loading};
use crate::mici::widgets::big_button::BigButton;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::Rect,
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub(super) struct Scanning {
    button: BigButton,
    loading: Loading,
}
impl Scanning {
    pub fn new(assets: &Assets) -> Result<Self, Error> {
        let mut button = assets.button("")?;
        button.value = "searching for networks".into();
        button.state.enabled = false.into();
        Ok(Self {
            button,
            loading: Loading::new(),
        })
    }
}
impl Widget for Scanning {
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
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let Self { button, loading } = self;
        button.paint_with(frame, draw, |button, frame, draw, y| {
            button.draw_content(frame, draw, y)?;
            let rect = button.state.rect;
            loading.set_position(
                rect.x + rect.width - loading.state().rect.width - 40.0,
                float(y + f64::from(rect.height) - f64::from(loading.state().rect.height) - 30.0),
            );
            loading.render(frame, draw)?;
            Ok(())
        })?;
        Ok(RenderResult::None)
    }
}
