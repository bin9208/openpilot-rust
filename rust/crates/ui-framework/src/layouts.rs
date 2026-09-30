use crate::{
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum VerticalAlignment {
    Top,
    #[default]
    Center,
    Bottom,
}
pub struct HBox {
    pub state: WidgetState,
    pub spacing: f32,
    pub alignment: VerticalAlignment,
}
impl Default for HBox {
    fn default() -> Self {
        Self {
            state: WidgetState::default(),
            spacing: 0.0,
            alignment: VerticalAlignment::Center,
        }
    }
}
impl Widget for HBox {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let mut offset = 0.0;
        for (index, child) in self
            .state
            .children
            .iter_mut()
            .filter(|child| child.state().visible.get())
            .enumerate()
        {
            let spacing = if index > 0 { self.spacing } else { 0.0 };
            let child_rect = child.state().rect;
            let x = rect.x + offset + spacing;
            offset += child_rect.width + spacing;
            let y = rect.y
                + match self.alignment {
                    VerticalAlignment::Top => 0.0,
                    VerticalAlignment::Center => (rect.height - child_rect.height) / 2.0,
                    VerticalAlignment::Bottom => rect.height - child_rect.height,
                };
            child.set_position(x, y);
            child.state_mut().parent_rect = Some(rect);
            child.render(frame, draw)?;
        }
        Ok(RenderResult::None)
    }
}
