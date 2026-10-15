use crate::{context::Context, paint::color, state::messages};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    animation::Filter,
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct TrafficLight {
    state: WidgetState,
    context: Context,
    current: i32,
    signal: crate::signal_status::Status,
    green_start: Option<f64>,
    alpha_filter: Filter,
}
impl TrafficLight {
    pub fn new(context: Context) -> Self {
        Self {
            state: WidgetState::default(),
            context,
            current: 0,
            signal: Default::default(),
            green_start: None,
            alpha_filter: Filter::new(0.0, 1.0, 60.0),
        }
    }
    pub fn visible(&self) -> bool {
        self.alpha_filter.x > 0.05 || self.signal.assisted()
    }
    pub fn opacity(&self) -> f64 {
        self.alpha_filter.x
    }
}
impl Widget for TrafficLight {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.signal =
            crate::signal_status::Status::from_messages(&self.context.messages.borrow().state)?;
        if self.signal.paused {
            self.current = 3;
            self.green_start = None;
            self.alpha_filter.x = 1.;
            return Ok(());
        }
        if self.current == 3 {
            self.alpha_filter.x = 0.;
        }
        let traffic =
            messages::longitudinal_plan(&self.context.messages.borrow().state)?.get_traffic_state();
        let now = (self.context.now_monotonic)();
        let visible = match traffic {
            1 => {
                self.current = 1;
                self.green_start = None;
                true
            }
            2 => {
                if self.current != 2 {
                    self.green_start = Some(now);
                }
                self.current = 2;
                self.green_start
                    .is_some_and(|start| start != 0.0 && now - start <= 2.0)
            }
            _ => {
                self.current = 0;
                self.green_start = None;
                false
            }
        };
        self.alpha_filter.update(f64::from(visible));
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alpha = self.alpha_filter.x.clamp(0.0, 1.0);
        if alpha <= 0.01 && !self.signal.assisted() {
            return Ok(RenderResult::None);
        }
        let alpha = (255.0 * alpha)
            .to_u8()
            .ok_or(Error::Contract("invalid traffic light alpha"))?;
        let (top, bottom) = if self.current == 1 {
            (color(255, 80, 80, alpha), color(255, 0, 0, alpha))
        } else {
            (color(120, 255, 120, alpha), color(0, 255, 0, alpha))
        };
        let rect = self.state.rect;
        let content_x = openpilot_ui_framework::text_layout::float(
            f64::from(rect.x) + f64::from(rect.width) - 60.0,
        );
        let center_x = f64::from(content_x) + 60.0 - 24.0;
        let center_y = f64::from(rect.y) + 24.0;
        if self.current == 3 {
            crate::signal_status::draw_pause(draw, center_x, center_y, 24.)?;
            return Ok(RenderResult::None);
        }
        super::confidence_ball::circle_gradient(
            draw,
            f64::from(content_x) + 60.0 - 24.0,
            f64::from(rect.y) + 24.0,
            24.0,
            top,
            bottom,
        )?;
        self.signal.draw_assist(draw, center_x, center_y, 24.)?;
        Ok(RenderResult::None)
    }
}
