mod paint;
pub mod policy;
use crate::context::Context;
use openpilot_ui_framework::{
    draw::Draw,
    label::Label,
    text::Font,
    text_layout::{Horizontal, Vertical},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub use policy::{Alert, Input, Policy};
pub struct Alerts {
    state: WidgetState,
    context: Context,
    policy: Policy,
    title: Label,
    subtitle: Label,
}
impl Alerts {
    pub fn new(context: Context) -> Self {
        let mut title = Label::new("");
        title.font = Font::Bold;
        title.horizontal = Horizontal::Center;
        title.vertical = Vertical::Top;
        let mut subtitle = Label::new("");
        subtitle.size = 88.0;
        subtitle.horizontal = Horizontal::Center;
        subtitle.vertical = Vertical::Top;
        let policy = Policy::new(false, |text| context.tr(text));
        Self {
            state: WidgetState::default(),
            context,
            policy,
            title,
            subtitle,
        }
    }
    pub fn current(&self) -> Result<Option<Alert>, Error> {
        Ok(self.policy.get(&Input::read(&self.context)?))
    }
}
impl Widget for Alerts {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if let Some(alert) = self.current()? {
            self.draw_alert(frame, draw, &alert)?;
        }
        Ok(RenderResult::None)
    }
}
