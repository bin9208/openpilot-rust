mod blindspot;
pub mod carrot;
pub mod colors;
pub mod common;
pub mod drawing;
pub mod hsv;
pub mod input;
mod lanes;
pub mod math;
mod path;
mod path_animation;
mod path_end;
mod path_modes;
mod path_overlay;
mod path_sampling;
pub mod points;
pub mod projection;
mod radar;
pub mod settings;
mod tire;
use crate::{context::Context, render_diagnostics::RenderDiagnostics, Error};
use carrot::Carrot;
use common::Common;
use openpilot_ui_framework::{
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetState},
};
use settings::Settings;

pub struct ModelRenderer {
    widget: WidgetState,
    context: Context,
    pub common: Common,
    pub carrot: Carrot,
    pub settings: Settings,
    diagnostics: Option<RenderDiagnostics>,
}
impl ModelRenderer {
    pub fn new(context: Context) -> Result<Self, Error> {
        Ok(Self {
            widget: WidgetState::default(),
            common: Common::new(&context)?,
            context,
            carrot: Carrot::default(),
            settings: Settings::default(),
            diagnostics: None,
        })
    }
    pub fn set_transform(&mut self, transform: [[f64; 3]; 3]) {
        self.common.projection.set_transform(transform);
        self.common.transform_dirty = true;
    }
    fn render_model(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        let context = self.context.clone();
        let messages = context.messages.borrow();
        let input = input::Input::new(&messages.state)?;
        if !input.current(context.ui.borrow().started_frame)? {
            return Ok(());
        }
        self.common.header(&input, self.widget.rect)?;
        if input.updated("modelV2")? {
            self.common.raw(input.model)?;
        }
        if self.common.path.raw.is_empty() {
            return Ok(());
        }
        self.common.transform_dirty = false;
        let mut timing = match self.diagnostics.take() {
            Some(t) => t,
            None => RenderDiagnostics::new("uiModel")?,
        };
        timing.start();
        let result = (|| {
            timing.call("path", || self.draw_path(&input, frame.now, draw))?;
            timing.call("lanes", || self.draw_lanes(&input, draw))?;
            timing.call("blindspot", || self.draw_blindspot(&input, draw))?;
            timing.call("radar", || self.draw_radar(&input, draw))?;
            timing.finish()
        })();
        self.diagnostics = Some(timing);
        result
    }
}
impl Widget for ModelRenderer {
    fn state(&self) -> &WidgetState {
        &self.widget
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.widget
    }
    fn paint(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, openpilot_ui_framework::Error> {
        self.render_model(frame, draw)?;
        Ok(RenderResult::None)
    }
}

#[cfg(test)]
mod tests;
