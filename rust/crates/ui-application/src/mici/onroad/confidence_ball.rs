use crate::{
    context::Context,
    paint::color,
    state::{messages, Status},
};
use openpilot_ui_framework::{
    animation::Filter,
    draw::{Draw, Ring},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct ConfidenceBall {
    state: WidgetState,
    context: Context,
    demo: bool,
    confidence: Filter,
}
impl ConfidenceBall {
    pub fn new(context: Context, fps: f64, demo: bool) -> Self {
        Self {
            state: WidgetState::default(),
            context,
            demo,
            confidence: Filter::new(-0.5, 0.5, fps),
        }
    }
    pub fn update_filter(&mut self, value: f64) {
        self.confidence.update(value);
    }
    pub fn value(&self) -> f64 {
        self.confidence.x
    }
}
impl Widget for ConfidenceBall {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if self.demo {
            return Ok(());
        }
        let value = if self.context.ui.borrow().status == Status::Disengaged {
            -0.5
        } else {
            let messages = self.context.messages.borrow();
            let predictions = messages::model(&messages.state)?
                .get_meta()
                .map_err(crate::Error::from)?
                .get_disengage_predictions()
                .map_err(crate::Error::from)?;
            let brake = predictions
                .get_brake_disengage_probs()
                .map_err(crate::Error::from)?
                .iter()
                .map(f64::from)
                .reduce(|a, b| if b > a { b } else { a })
                .unwrap_or(1.0);
            let steer = predictions
                .get_steer_override_probs()
                .map_err(crate::Error::from)?
                .iter()
                .map(f64::from)
                .reduce(|a, b| if b > a { b } else { a })
                .unwrap_or(1.0);
            (1.0 - brake) * (1.0 - steer)
        };
        self.confidence.update(value);
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let status = if self.demo {
            Status::Engaged
        } else {
            self.context.ui.borrow().status
        };
        let (top, bottom) = match status {
            Status::Engaged => {
                if self.confidence.x > 0.5 {
                    (color(0, 255, 204, 255), color(0, 255, 38, 255))
                } else if self.confidence.x > 0.2 {
                    (color(255, 200, 0, 255), color(255, 115, 0, 255))
                } else {
                    (color(255, 0, 21, 255), color(255, 0, 89, 255))
                }
            }
            Status::Override => (u32::MAX, color(82, 82, 82, 255)),
            Status::Disengaged => (color(50, 50, 50, 255), color(13, 13, 13, 255)),
        };
        let rect = self.state.rect;
        let content_x = float(f64::from(rect.x) + f64::from(rect.width) - 60.0);
        circle_gradient(
            draw,
            f64::from(content_x) + 60.0 - 24.0,
            f64::from(rect.y) + (1.0 - self.confidence.x) * (f64::from(rect.height) - 48.0) + 24.0,
            24.0,
            top,
            bottom,
        )?;
        Ok(RenderResult::None)
    }
}

pub(super) fn circle_gradient(
    draw: &mut dyn Draw,
    x: f64,
    y: f64,
    radius: f64,
    top: u32,
    bottom: u32,
) -> Result<(), Error> {
    draw.gradient(
        Rect {
            x: float((x - radius).trunc()),
            y: float((y - radius).trunc()),
            width: float(radius * 2.0),
            height: float(radius * 2.0),
        },
        [top, bottom, bottom, top],
    )?;
    draw.ring(Ring {
        center: Point {
            x: float(x.trunc()),
            y: float(y.trunc()),
        },
        inner: float(radius),
        outer: float((radius * std::f64::consts::SQRT_2).ceil() + 1.0),
        start: 0.0,
        end: 360.0,
        segments: 20,
        color: color(0, 0, 0, 255),
    })
}
