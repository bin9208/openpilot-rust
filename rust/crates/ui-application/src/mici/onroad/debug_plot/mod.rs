pub mod data;
mod paint;
pub mod samples;
use crate::{context::Context, params::Read};
use openpilot_ui_framework::{
    draw::Draw,
    geometry::Rect,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct DebugPlot {
    state: WidgetState,
    context: Context,
    pub samples: samples::Samples,
    mode_previous: i32,
}
impl DebugPlot {
    pub fn new(context: Context, viewport: Rect) -> Self {
        let mut state = WidgetState::default();
        state.rect = viewport;
        Self {
            state,
            context,
            samples: samples::Samples::default(),
            mode_previous: -1,
        }
    }
    pub fn mode_previous(&self) -> i32 {
        self.mode_previous
    }
}
impl Widget for DebugPlot {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let mode = self.context.params.integer("ShowPlotMode")?;
        if mode == 0 {
            return Ok(RenderResult::None);
        }
        let context = self.context.clone();
        let messages = context.messages.borrow();
        if !messages
            .state
            .topic("carState")
            .map_err(crate::Error::from)?
            .alive
            || !messages
                .state
                .topic("longitudinalPlan")
                .map_err(crate::Error::from)?
                .alive
        {
            return Ok(RenderResult::None);
        }
        if mode != self.mode_previous {
            self.samples = samples::Samples::default();
            self.mode_previous = mode;
        }
        let (values, title) = data::data(&messages.state, mode)?;
        self.samples.sample(frame.now, values);
        self.paint_plot(draw, title)?;
        Ok(RenderResult::None)
    }
}
