use super::*;
use crate::mici::widgets::{
    big_button::{BigButton, Kind},
    circle_button::CircleButton,
};
use openpilot_ui_framework::geometry::Rect;
pub(super) enum Body {
    Big(Box<BigButton>),
    Circle(Box<CircleButton>),
}
pub(super) struct Control {
    pub body: Body,
    pub policy: Rc<Policy>,
    pub key: Key,
}
impl Control {
    fn inner(&self) -> &dyn Widget {
        match &self.body {
            Body::Big(button) => button.as_ref(),
            Body::Circle(button) => button.as_ref(),
        }
    }
    fn inner_mut(&mut self) -> &mut dyn Widget {
        match &mut self.body {
            Body::Big(button) => button.as_mut(),
            Body::Circle(button) => button.as_mut(),
        }
    }
}
impl Widget for Control {
    fn state(&self) -> &WidgetState {
        self.inner().state()
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        self.inner_mut().state_mut()
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.inner_mut().set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.inner_mut().set_rect(rect);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.inner_mut().paint(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let checked = self.policy.checked[self.key.index()].get();
        match &mut self.body {
            Body::Big(button) => button.kind = Kind::Toggle(checked),
            Body::Circle(button) => button.checked = Some(checked),
        }
        self.inner_mut().render(frame, draw)
    }
}
