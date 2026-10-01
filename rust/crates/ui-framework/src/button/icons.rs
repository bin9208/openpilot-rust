use super::*;
pub struct IconButton {
    pub state: WidgetState,
    pub texture: Texture,
    pub opacity: Filter,
}
impl IconButton {
    pub fn new(texture: Texture, fps: f64) -> Self {
        let mut state = WidgetState::default();
        state.rect.width = texture.width;
        state.rect.height = texture.height;
        Self {
            state,
            texture,
            opacity: Filter::new(1.0, 0.1, fps),
        }
    }
    pub fn set_opacity(&mut self, opacity: f64, smooth: bool) {
        if smooth {
            self.opacity.update(opacity);
        } else {
            self.opacity.x = opacity;
        }
    }
}
impl Widget for IconButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alpha = |value: f64| {
            value
                .to_u8()
                .ok_or(Error::Contract("icon opacity out of range"))
        };
        let color = if !self.state.enabled.get() {
            rgba(255, 255, 255, alpha(255.0 * 0.9 * 0.35 * self.opacity.x)?)
        } else if self.state.is_pressed() {
            rgba(180, 180, 180, alpha(150.0 * self.opacity.x)?)
        } else {
            WHITE
        };
        let rect = self.state.rect;
        self.texture.draw(
            draw,
            Point {
                x: rect.x + (rect.width - self.texture.width) / 2.0,
                y: rect.y + (rect.height - self.texture.height) / 2.0,
            },
            1.0,
            color,
        )?;
        Ok(RenderResult::None)
    }
}
pub struct SmallCircleIconButton {
    pub state: WidgetState,
    pub icon: Texture,
    pub normal: Texture,
    pub pressed: Texture,
    pub disabled: Texture,
    pub opacity: Filter,
}
impl SmallCircleIconButton {
    pub fn new(
        icon: Texture,
        normal: Texture,
        pressed: Texture,
        disabled: Texture,
        fps: f64,
    ) -> Self {
        let mut state = WidgetState::default();
        state.rect.width = 100.0;
        state.rect.height = 100.0;
        Self {
            state,
            icon,
            normal,
            pressed,
            disabled,
            opacity: Filter::new(1.0, 0.1, fps),
        }
    }
}
impl Widget for SmallCircleIconButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alpha = (255.0 * self.opacity.x)
            .to_u8()
            .ok_or(Error::Contract("icon opacity out of range"))?;
        let enabled = self.state.enabled.get();
        let bg = if !enabled {
            self.disabled
        } else if self.state.is_pressed() {
            self.pressed
        } else {
            self.normal
        };
        let icon_alpha = if enabled {
            alpha
        } else {
            (f64::from(alpha) * 0.35)
                .to_u8()
                .ok_or(Error::Contract("icon opacity out of range"))?
        };
        let rect = self.state.rect;
        bg.draw(
            draw,
            Point {
                x: rect.x,
                y: rect.y,
            },
            1.0,
            rgba(255, 255, 255, alpha),
        )?;
        self.icon.draw(
            draw,
            Point {
                x: rect.x + (rect.width - self.icon.width) / 2.0,
                y: rect.y + (rect.height - self.icon.height) / 2.0,
            },
            1.0,
            rgba(255, 255, 255, icon_alpha),
        )?;
        Ok(RenderResult::None)
    }
}
