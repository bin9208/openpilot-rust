use openpilot_ui_application::{
    context::Context, onroad::driver_preview::DriverPreview, state::messages,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct Preview {
    state: WidgetState,
    context: Context,
    preview: DriverPreview,
}
impl Preview {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Ok(Self {
            state: WidgetState::default(),
            preview: DriverPreview::new(context.clone(), canvas, "rustvision")?,
            context,
        })
    }
}
impl Widget for Preview {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alert = {
            let sm = self.context.messages.borrow();
            u16::from(
                messages::selfdrive_state(&sm.state)?
                    .get_alert_size()
                    .map_err(openpilot_ui_application::Error::from)?,
            ) != 0
        };
        self.preview
            .draw_onroad(self.state.rect, alert, frame.monotonic, draw)?;
        Ok(RenderResult::None)
    }
}
