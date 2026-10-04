mod drawing;
mod geometry;
mod gradient;
mod leads;
mod radar;
use crate::{
    context::Context,
    onroad::model_renderer::{common::Common, input::Input, points::ScreenPoint},
    Error,
};
use openpilot_ui_framework::{
    draw::Draw,
    polygon::Gradient,
    widget::{Frame, RenderResult, Widget, WidgetState},
};
use serde::Serialize;

#[derive(Serialize)]
pub struct LeadRectangle {
    pub corners: [ScreenPoint; 4],
    pub color: u32,
}
#[derive(Serialize)]
pub struct RadarItem {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub color: u32,
    pub star: bool,
}
#[derive(Serialize)]
pub struct Filters {
    pub throttle: f64,
    pub acceleration: f32,
    pub acceleration_slow: f32,
    pub torque: f64,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            throttle: 1.,
            acceleration: 0.,
            acceleration_slow: 0.,
            torque: 0.,
        }
    }
}
pub struct ModelRenderer {
    widget: WidgetState,
    context: Context,
    pub common: Common,
    pub filters: Filters,
    pub marking_codes: [i16; 4],
    pub marking_segments: [Vec<Vec<openpilot_ui_framework::geometry::Point>>; 4],
    pub lead: Option<LeadRectangle>,
    pub lead_filter: Option<ScreenPoint>,
    pub radar_items: Vec<RadarItem>,
    pub gradient: Gradient,
}
impl ModelRenderer {
    pub fn new(context: Context) -> Result<Self, Error> {
        Ok(Self {
            widget: WidgetState::default(),
            common: Common::new(&context)?,
            context,
            filters: Filters::default(),
            marking_codes: [-1; 4],
            marking_segments: std::array::from_fn(|_| Vec::new()),
            lead: None,
            lead_filter: None,
            radar_items: Vec::new(),
            gradient: Gradient::new((0., 1.), (0., 0.), Vec::new(), Vec::new()),
        })
    }
    pub fn set_transform(&mut self, transform: [[f64; 3]; 3]) {
        self.common.projection.set_transform(transform);
        self.common.transform_dirty = true;
    }
    fn render_model(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        let context = self.context.clone();
        let messages = context.messages.borrow();
        let input = Input::new(&messages.state)?;
        let alpha = 0.05 / (0.1 + 0.05);
        self.filters.torque = (1. - alpha) * self.filters.torque
            + alpha * (-f64::from(input.output.get_actuators_output()?.get_torque()));
        if !input.current(context.ui.borrow().started_frame)? {
            return Ok(());
        }
        self.common.header(&input, self.widget.rect)?;
        let radar = input.valid("radarState")?;
        let leads = self.common.longitudinal && radar;
        if input.updated("modelV2")? || input.updated("radarState")? || self.common.transform_dirty
        {
            if input.updated("modelV2")? {
                self.common.raw(input.model)?;
            }
            if self.common.path.raw.is_empty() {
                return Ok(());
            }
            self.update_model(&input)?;
            if leads {
                self.update_lead(&input)?;
            }
            if context.ui.borrow().slow.show_radar_info > 0 && radar {
                self.update_radar(&input, draw)?;
            } else {
                self.radar_items.clear();
            }
            self.common.transform_dirty = false;
        }
        self.draw_lanes(draw)?;
        if context.ui.borrow().status != crate::state::Status::Disengaged {
            self.draw_path(&input, draw)?;
        }
        self.draw_blindspots(&input, draw)?;
        if leads {
            self.draw_lead(draw)?;
        }
        if context.ui.borrow().slow.show_radar_info > 0 {
            self.draw_radar(draw)?;
        }
        Ok(())
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
        _frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, openpilot_ui_framework::Error> {
        self.render_model(draw)?;
        Ok(RenderResult::None)
    }
}
