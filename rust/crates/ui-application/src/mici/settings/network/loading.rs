use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub(super) struct Loading {
    state: WidgetState,
}
impl Loading {
    pub fn new() -> Self {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 27.0,
        };
        Self { state }
    }
}
impl Widget for Loading {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let x = (f64::from(rect.x) + f64::from(rect.width) / 2.0).trunc();
        let base = (f64::from(rect.y) + f64::from(rect.height) - 8.0).trunc();
        for i in 0..3 {
            let y =
                (base + (((frame.now - f64::from(i) * 0.2) * 4.0).sin() * 11.2).min(0.0)).trunc();
            let alpha = (255.0 * 0.45
                + ((base - y) / 11.2).clamp(0.0, 1.0) * (255.0 * 0.9 - 255.0 * 0.45))
                .to_u8()
                .ok_or(Error::Contract("loading alpha out of range"))?;
            draw.circle(
                Point {
                    x: float(x + f64::from(i - 1) * 24.0),
                    y: float(y),
                },
                8.0,
                u32::from_le_bytes([255, 255, 255, alpha]),
            )?;
        }
        Ok(RenderResult::None)
    }
}
