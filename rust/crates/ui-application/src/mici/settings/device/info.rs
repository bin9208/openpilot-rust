use crate::{context::Context, paint, params::Read};
use openpilot_ui_framework::{
    draw::Draw,
    geometry::Rect,
    text::Font,
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub(super) struct Info {
    state: WidgetState,
    labels: [UnifiedLabel; 4],
}
impl Info {
    pub fn new(context: &Context) -> Result<Self, Error> {
        let mut labels = [
            UnifiedLabel::new("device ID"),
            UnifiedLabel::new(context.params.string("DongleId")?),
            UnifiedLabel::new("serial"),
            UnifiedLabel::new(context.params.string("HardwareSerial")?),
        ];
        for (index, label) in labels.iter_mut().enumerate() {
            label.wrap = false;
            label.size = if index % 2 == 0 { 48.0 } else { 32.0 };
            label.font = if index % 2 == 0 {
                Font::Display
            } else {
                Font::Regular
            };
            if index % 2 == 1 {
                label.color = paint::color(255, 255, 255, 149);
                if label.text.get().is_empty() {
                    label.text = "N/A".to_owned().into();
                }
            }
        }
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 360.0,
            height: 180.0,
        };
        Ok(Self { state, labels })
    }
}
impl Widget for Info {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        for (label, offset) in self.labels.iter_mut().zip([-10.0, 43.0, 84.0, 136.0]) {
            label.set_max_width(draw, Some(340.0));
            label.set_position(self.state.rect.x + 20.0, self.state.rect.y + offset);
            label.render(frame, draw)?;
        }
        Ok(RenderResult::None)
    }
}
