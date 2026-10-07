use openpilot_ui_framework::{
    draw::Draw,
    list::{ItemAction, ToggleAction},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
pub(super) struct Control {
    pub action: ToggleAction,
    pub checked: Rc<Cell<bool>>,
}
impl ItemAction for Control {
    fn width_hint(&self, draw: &dyn Draw) -> f64 {
        self.action.width_hint(draw)
    }
}
impl Widget for Control {
    fn state(&self) -> &WidgetState {
        &self.action.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.action.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.action.toggle.set_value(self.checked.get());
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.action.paint(frame, draw)
    }
}
