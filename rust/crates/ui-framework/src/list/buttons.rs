use super::*;
pub struct DualButtonAction {
    pub state: WidgetState,
    pub left: Button,
    pub right: Button,
}
impl DualButtonAction {
    pub fn new(left: impl Into<String>, right: impl Into<String>) -> Self {
        let mut left = Button::new(left);
        left.label.padding = 0.0;
        let mut right = Button::new(right);
        right.label.padding = 0.0;
        right.set_style(ButtonStyle::Danger);
        Self {
            state: WidgetState::default(),
            left,
            right,
        }
    }
}
impl ItemAction for DualButtonAction {
    fn width_hint(&self, _: &dyn Draw) -> f64 {
        0.0
    }
}
impl Widget for DualButtonAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let width = (rect.width - 30.0) / 2.0;
        let y = rect.y + (rect.height - 120.0) / 2.0;
        let mut left = Rect {
            x: rect.x,
            y,
            width,
            height: 120.0,
        };
        let mut right = Rect {
            x: rect.x + width + 30.0,
            y,
            width,
            height: 120.0,
        };
        if !self.left.state.visible.get() {
            right.x = rect.x;
            right.width = rect.width;
        } else if !self.right.state.visible.get() {
            left.width = rect.width;
        }
        let gate = valid(&self.state);
        self.left.state.interaction_gate = gate;
        self.right.state.interaction_gate = gate;
        self.left.set_rect(left);
        self.right.set_rect(right);
        self.left.render(frame, draw)?;
        self.right.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
pub struct MultipleButtonAction {
    pub state: WidgetState,
    pub buttons: Vec<Property<String>>,
    pub button_width: f64,
    pub selected: usize,
    pub callback: Option<Callback<usize>>,
}
impl MultipleButtonAction {
    pub fn new(buttons: Vec<Property<String>>, width: f64, selected: usize) -> Self {
        let mut state = WidgetState::default();
        state.rect.width = float(
            buttons.len().to_f64().unwrap_or(f64::INFINITY) * width
                + (buttons.len().to_f64().unwrap_or(f64::INFINITY) - 1.0) * 20.0,
        );
        Self {
            state,
            buttons,
            button_width: width,
            selected,
            callback: None,
        }
    }
    pub fn set_selected(&mut self, index: usize) {
        if index < self.buttons.len() {
            self.selected = index;
        }
    }
    fn button_rect(&self, index: usize) -> Result<Rect, Error> {
        let rect = self.state.rect;
        Ok(Rect {
            x: float(
                f64::from(rect.x)
                    + index
                        .to_f64()
                        .ok_or(Error::Contract("button index overflow"))?
                        * (self.button_width + 20.0),
            ),
            y: rect.y + (rect.height - 100.0) / 2.0,
            width: float(self.button_width),
            height: 100.0,
        })
    }
}
impl ItemAction for MultipleButtonAction {
    fn width_hint(&self, _: &dyn Draw) -> f64 {
        f64::from(self.state.rect.width)
    }
}
impl Widget for MultipleButtonAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        for (index, value) in self.buttons.iter().enumerate() {
            let rect = self.button_rect(index)?;
            let enabled = self.state.enabled.get();
            let pressed = rect.contains(frame.cursor) && enabled && self.state.is_pressed();
            let mut background = if index == self.selected {
                [51, 171, 76, 255]
            } else if pressed {
                [74, 74, 74, 255]
            } else {
                [57, 57, 57, 255]
            };
            if !enabled {
                background[3] = 150;
            }
            draw.rounded_segments(rect, 1.0, 20, u32::from_le_bytes(background), false)?;
            let value = value.get();
            let size = text::measure(draw, Font::Medium, &value, 40.0, 0.0);
            text::draw_text(
                draw,
                Font::Medium,
                &value,
                Point {
                    x: rect.x + (rect.width - size.x) / 2.0,
                    y: rect.y + (100.0 - size.y) / 2.0,
                },
                40.0,
                0.0,
                if enabled {
                    u32::from_le_bytes([228, 228, 228, 255])
                } else {
                    u32::from_le_bytes([150, 150, 150, 255])
                },
            )?;
        }
        Ok(RenderResult::None)
    }
    fn mouse_release(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        for index in 0..self.buttons.len() {
            if self.button_rect(index)?.contains(position) {
                self.selected = index;
                if let Some(callback) = &self.callback {
                    callback.call(index);
                }
            }
        }
        Ok(())
    }
}
