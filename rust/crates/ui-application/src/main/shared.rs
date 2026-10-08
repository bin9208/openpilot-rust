use openpilot_ui_framework::{
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
pub(super) struct Shared {
    state: WidgetState,
    handle: WidgetHandle,
    failure: Option<Error>,
}
impl Shared {
    pub fn new(handle: WidgetHandle) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = handle.borrow()?.state().rect;
        Ok(Self {
            state,
            handle,
            failure: None,
        })
    }
}
impl Widget for Shared {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        match self.handle.borrow_mut() {
            Ok(mut widget) => widget.show(frame),
            Err(error) => self.failure = Some(error),
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        match self.handle.borrow_mut() {
            Ok(mut widget) => widget.hide(frame),
            Err(error) => self.failure = Some(error),
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.handle.borrow_mut()?.paint(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        let mut widget = self.handle.borrow_mut()?;
        widget.set_rect(self.state.rect);
        widget.state_mut().parent_rect = self.state.parent_rect;
        widget.state_mut().interaction_gate = self.state.interaction_gate;
        widget.state_mut().enabled = self.state.enabled.get().into();
        widget.render(frame, draw)
    }
}
