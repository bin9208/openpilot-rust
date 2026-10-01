use super::*;
pub struct ToggleAction {
    pub state: WidgetState,
    pub toggle: Toggle,
}
impl ToggleAction {
    pub fn new(value: bool) -> Self {
        let mut state = WidgetState::default();
        state.rect.width = 160.0;
        Self {
            state,
            toggle: Toggle::new(value),
        }
    }
}
impl ItemAction for ToggleAction {
    fn width_hint(&self, _: &dyn Draw) -> f64 {
        f64::from(self.state.rect.width)
    }
}
impl Widget for ToggleAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        self.toggle.state.enabled = self.state.enabled.get().into();
        self.toggle.state.interaction_gate = valid(&self.state);
        self.toggle.set_rect(Rect {
            x: rect.x,
            y: rect.y + (rect.height - 80.0) / 2.0,
            width: rect.width,
            height: 80.0,
        });
        self.toggle.render(frame, draw)
    }
}
pub struct ButtonAction {
    pub state: WidgetState,
    pub text: Property<String>,
    pub value: Property<String>,
    pub button: Button,
    pressed: Rc<Cell<bool>>,
}
impl ButtonAction {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let mut state = WidgetState::default();
        state.rect.width = 250.0;
        let mut button = Button::new(&value);
        button.label.size = 35.0;
        button.label.padding = 0.0;
        button.set_style(ButtonStyle::ListAction);
        button.radius = 50.0;
        let pressed = Rc::new(Cell::new(false));
        let flag = pressed.clone();
        button.state.click = Some(Box::new(move || flag.set(true)));
        Self {
            state,
            text: value.into(),
            value: String::new().into(),
            button,
            pressed,
        }
    }
}
impl ItemAction for ButtonAction {
    fn width_hint(&self, draw: &dyn Draw) -> f64 {
        let value = self.value.get();
        if value.is_empty() {
            250.0
        } else {
            f64::from(text::measure(draw, Font::Normal, &value, 50.0, 0.0).x) + 270.0
        }
    }
}
impl Widget for ButtonAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        self.button.label.text = self.text.get().into();
        self.button.state.enabled = self.state.enabled.get().into();
        self.button.state.interaction_gate = valid(&self.state);
        self.button.set_rect(Rect {
            x: rect.x + rect.width - 250.0,
            y: rect.y + (rect.height - 100.0) / 2.0,
            width: 250.0,
            height: 100.0,
        });
        self.button.render(frame, draw)?;
        let value = self.value.get();
        if !value.is_empty() {
            gui_label(
                draw,
                Rect {
                    width: rect.width - 270.0,
                    ..rect
                },
                &value,
                text_style(VALUE_COLOR),
                (Horizontal::Left, Vertical::Middle),
                true,
            )?;
        }
        Ok(RenderResult::Bool(self.pressed.replace(false)))
    }
}
pub struct TextAction {
    pub state: WidgetState,
    pub text: Property<String>,
    pub color: u32,
}
impl TextAction {
    pub fn new(text: impl Into<String>, color: u32) -> Self {
        Self {
            state: WidgetState::default(),
            text: text.into().into(),
            color,
        }
    }
}
impl ItemAction for TextAction {
    fn width_hint(&self, draw: &dyn Draw) -> f64 {
        f64::from(text::measure(draw, Font::Normal, &self.text.get(), 50.0, 0.0).x) + 20.0
    }
}
impl Widget for TextAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        gui_label(
            draw,
            self.state.rect,
            &self.text.get(),
            text_style(self.color),
            (Horizontal::Right, Vertical::Middle),
            true,
        )?;
        Ok(RenderResult::Bool(false))
    }
}
