use openpilot_ui_framework::{
    animation::Bounce,
    assets::Texture,
    callback::Callback,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct CircleButton {
    pub binding: Option<crate::params::binding::Binding>,
    pub state: WidgetState,
    pub icon: Texture,
    pub offset: Point,
    pub red: bool,
    pub checked: Option<bool>,
    pub changed: Option<Callback<bool>>,
    pub scale: Bounce,
    backgrounds: [Texture; 5],
    indicators: Option<[Texture; 2]>,
}
impl CircleButton {
    pub fn new(
        icon: Texture,
        mut load: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let backgrounds = [
            "button_circle.png",
            "button_circle_pressed.png",
            "button_circle_disabled.png",
            "button_circle_red.png",
            "button_circle_red_pressed.png",
        ]
        .map(|name| load(&format!("icons_mici/buttons/{name}"), (180, 180)))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        let backgrounds = backgrounds
            .try_into()
            .map_err(|_| Error::Contract("circle background count"))?;
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: 180.0,
        };
        state.click_delay = Some(0.075);
        Ok(Self {
            binding: None,
            state,
            icon,
            offset: Point::default(),
            red: false,
            checked: None,
            changed: None,
            scale: Bounce::new(1.0, 0.1, 20.0, 2.0),
            backgrounds,
            indicators: None,
        })
    }
    pub fn enable_toggle(
        &mut self,
        mut load: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<(), Error> {
        self.checked = Some(false);
        self.indicators = Some([
            load("icons_mici/buttons/toggle_dot_disabled.png", (66, 66))?,
            load("icons_mici/buttons/toggle_dot_enabled.png", (66, 66))?,
        ]);
        Ok(())
    }
}
impl Widget for CircleButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        if let Some(checked) = &mut self.checked {
            *checked = !*checked;
            if let Some(callback) = &self.changed {
                callback.call(*checked);
            }
        }
        if let (Some(binding), Some(checked)) = (&self.binding, self.checked) {
            binding.write_bool(checked)?;
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let enabled = self.state.enabled.get();
        let pressed = self.state.is_pressed();
        let rect = self.state.rect;
        let index = if !enabled {
            2
        } else if self.red {
            if pressed {
                4
            } else {
                3
            }
        } else {
            usize::from(pressed)
        };
        let scale = self.scale.update(if pressed { 1.07 } else { 1.0 });
        let x = f64::from(rect.x) + f64::from(rect.width) * (1.0 - scale) / 2.0;
        let y = f64::from(rect.y) + f64::from(rect.height) * (1.0 - scale) / 2.0;
        self.backgrounds[index].draw(
            draw,
            Point {
                x: float(x),
                y: float(y),
            },
            float(scale),
            WHITE,
        )?;
        self.icon.draw(
            draw,
            Point {
                x: rect.x + (rect.width - self.icon.width) / 2.0 + self.offset.x,
                y: float(y + f64::from((rect.height - self.icon.height) / 2.0 + self.offset.y)),
            },
            1.0,
            u32::from_le_bytes([255, 255, 255, if enabled { 229 } else { 89 }]),
        )?;
        if let (Some(checked), Some(indicators)) = (self.checked, self.indicators) {
            indicators[usize::from(checked)].draw(
                draw,
                Point {
                    x: rect.x + (rect.width - indicators[1].width) / 2.0,
                    y: float(y + 5.0),
                },
                1.0,
                WHITE,
            )?;
        }
        Ok(RenderResult::None)
    }
}
