use super::*;
impl Widget for BigButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.position_base = Some(Point { x, y });
        self.state.rect.x = x;
        self.state.rect.y = y;
    }
    fn set_rect(&mut self, rect: Rect) {
        self.position_base = None;
        self.state.rect = rect;
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        match &mut self.kind {
            Kind::Toggle(value) | Kind::Multiple { checked: value, .. } => {
                *value = !*value;
                if let Some(callback) = &self.changed {
                    callback.call(*value);
                }
            }
            Kind::Button | Kind::Grey => {}
        }
        if let Kind::Multiple { options, .. } = &self.kind {
            let index = options
                .iter()
                .position(|v| v == &self.value)
                .ok_or(Error::Contract("multi-toggle selection missing"))?;
            self.value = options[(index + 1) % options.len()].clone();
            if let Some(callback) = &self.selected {
                callback.call(self.value.clone());
            }
        }
        if let Some(binding) = &self.binding {
            match &self.kind {
                Kind::Toggle(checked) => binding.write_bool(*checked)?,
                Kind::Multiple { options, .. } => binding.write_int(
                    options
                        .iter()
                        .position(|value| value == &self.value)
                        .ok_or(Error::Contract("UI option missing"))?,
                )?,
                Kind::Button | Kind::Grey => {}
            }
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.paint_button(frame, draw)?;
        Ok(RenderResult::None)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let gate = self.state.interaction_gate;
        if self.state.parent_rect.is_some()
            && self.grow_until.is_some_and(|until| frame.now < until)
        {
            self.state.interaction_gate = false;
        }
        let result = openpilot_ui_framework::widget::render_widget(self, frame, draw);
        self.state.interaction_gate = gate;
        result
    }
}
