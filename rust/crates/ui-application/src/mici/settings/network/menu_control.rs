use super::menu_model::MenuModel;
use crate::mici::widgets::big_button::BigButton;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::Rect,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::rc::Rc;
pub(super) enum ControlKind {
    Tether,
    Meter,
}
pub(super) struct Control {
    pub button: BigButton,
    pub model: Rc<MenuModel>,
    pub kind: ControlKind,
}
impl Widget for Control {
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
        self.button.paint(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        match self.kind {
            ControlKind::Tether => self.button.set_checked(self.model.tether_checked.get()),
            ControlKind::Meter => self.button.value = self.model.meter_value.borrow().clone(),
        }
        self.button.render(frame, draw)
    }
}
