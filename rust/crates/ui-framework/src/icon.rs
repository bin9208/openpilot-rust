use crate::{
    assets::Texture,
    draw::Draw,
    geometry::Point,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
pub struct Icon {
    pub state: WidgetState,
    pub texture: Texture,
    pub opacity: f64,
}
impl Icon {
    pub fn new(texture: Texture, opacity: f64) -> Self {
        let mut state = WidgetState::default();
        state.enabled = false.into();
        state.rect.width = texture.width;
        state.rect.height = texture.height;
        Self {
            state,
            texture,
            opacity,
        }
    }
}
impl Widget for Icon {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alpha = (self.opacity * 255.0)
            .to_u8()
            .ok_or(Error::Contract("icon opacity out of range"))?;
        self.texture.draw(
            draw,
            Point {
                x: self.state.rect.x,
                y: self.state.rect.y,
            },
            1.0,
            u32::from_le_bytes([255, 255, 255, alpha]),
        )?;
        Ok(RenderResult::None)
    }
}
