pub mod geometry;
use crate::{
    context::Context,
    onroad::model_renderer::{hsv, math},
    paint::color,
    state::{messages, Status},
};
use geometry::{Arc, ArcCache};
use openpilot_ui_framework::{
    animation::Filter,
    draw::{Draw, PolygonPaint},
    geometry::Point,
    polygon,
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::cell::RefCell;
thread_local! {
    static ARCS: RefCell<ArcCache> = RefCell::default();
}
pub struct TorqueBar {
    state: WidgetState,
    context: Context,
    demo: bool,
    torque: Filter,
    opacity: Filter,
}
impl TorqueBar {
    pub fn new(context: Context, fps: f64, demo: bool) -> Self {
        Self {
            state: WidgetState::default(),
            context,
            demo,
            torque: Filter::new(0.0, 0.1, fps),
            opacity: Filter::new(0.0, 0.1, fps),
        }
    }
    pub fn update_filter(&mut self, value: f64) {
        self.torque.update(value);
    }
    pub fn value(&self) -> f64 {
        self.torque.x
    }
    pub fn opacity(&self) -> f64 {
        self.opacity.x
    }
}
impl Widget for TorqueBar {
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
        let messages = self.context.messages.borrow();
        let controls = messages::controls_state(&messages.state)?;
        let value =
            if matches!(
            controls.get_lateral_control_state().which().map_err(crate::Error::from)?,
            openpilot_cereal::log_capnp::controls_state::lateral_control_state::Which::AngleState(_)
        ) {
                let car = messages::car_state(&messages.state)?;
                let speed = f64::from(car.get_v_ego());
                let actual = f64::from(controls.get_curvature()) * speed.powi(2);
                let desired = f64::from(controls.get_desired_curvature()) * speed.powi(2);
                let difference = desired - actual;
                let compensation =
                    f64::from(messages::live_parameters(&messages.state)?.get_roll())
                        * 9.81
                        * math::interp(speed, &[5.0, 15.0], &[0.0, 1.0])?;
                let lateral = actual - compensation;
                let max = self
                    .context
                    .ui
                    .borrow()
                    .slow
                    .car
                    .map_or(3.0, |car| car.max_lateral_accel);
                if messages::car_control(&messages.state)?.get_lat_active() {
                    math::clip((lateral + difference) / max, -1.0, 1.0)
                } else {
                    0.0
                }
            } else {
                -f64::from(
                    messages::car_output(&messages.state)?
                        .get_actuators_output()
                        .map_err(crate::Error::from)?
                        .get_torque(),
                )
            };
        self.torque.update(value);
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let status = self.context.ui.borrow().status;
        self.opacity
            .update(f64::from(self.demo || status != Status::Disengaged));
        let alpha = self.opacity.x;
        let torque = self.torque.x;
        let absolute = torque.abs();
        let offset = math::interp(absolute, &[0.5, 1.0], &[22.0, 26.0])?;
        let height = math::interp(absolute, &[0.5, 1.0], &[14.0, 56.0])?;
        let engaged = self.demo || status == Status::Engaged;
        let background_alpha = if engaged {
            math::interp(absolute, &[0.5, 1.0], &[0.25, 0.5])?
        } else {
            0.15
        };
        let background = color(255, 255, 255, math::byte(255.0 * background_alpha * alpha)?);
        let rect = self.state.rect;
        let cx = f64::from(rect.x) + f64::from(rect.width) / 2.0 + 8.0;
        let cy = f64::from(rect.y) + f64::from(rect.height) + 1200.0 - offset;
        let span = alpha * 12.7;
        let arc = Arc {
            cx,
            cy,
            radius: 1200.0 + height / 2.0,
            thickness: height,
            start: -90.0 - span / 2.0,
            end: -90.0 + span / 2.0,
        };
        let background_points = ARCS.with(|cache| cache.borrow_mut().points(arc))?;
        polygon::polygon(
            draw,
            &background_points,
            (rect, polygon::Fill::Color(background)),
        )?;
        let points = ARCS.with(|cache| {
            cache.borrow_mut().points(Arc {
                start: -90.0,
                end: -90.0 + span / 2.0 * torque,
                ..arc
            })
        })?;
        let edge = background_points
            .iter()
            .map(|point| point.x)
            .reduce(|a, b| {
                if (torque < 0.0 && b < a) || (torque >= 0.0 && b > a) {
                    b
                } else {
                    a
                }
            })
            .ok_or(Error::Contract("empty torque arc"))?;
        let end_normalized = (float(cx * (1.0 - 0.65)) + edge * 0.65) / rect.width;
        let end_x = rect.x + end_normalized * rect.width;
        let start_x = float(f64::from(rect.x) + cx / f64::from(rect.width) * f64::from(rect.width));
        let fade = (absolute - 0.75).max(0.0) * 4.0;
        let white = color(255, 255, 255, math::byte(255.0 * 0.9 * alpha)?);
        let yellow = color(255, 200, 0, math::byte(255.0 * alpha)?);
        let orange = color(255, 115, 0, math::byte(255.0 * alpha)?);
        let colors = if engaged {
            [
                hsv::blend(white, yellow, fade)?,
                hsv::blend(white, orange, fade)?,
            ]
        } else {
            [color(255, 255, 255, math::byte(255.0 * 0.35 * alpha)?); 2]
        };
        draw.shaded_strip(
            &polygon::triangulate(&points),
            PolygonPaint::Gradient {
                start: Point {
                    x: start_x,
                    y: rect.y,
                },
                end: Point {
                    x: end_x,
                    y: rect.y,
                },
                colors: &colors,
                stops: &[0.0, 1.0],
            },
        )?;
        if absolute < 0.5 {
            draw.circle(
                Point {
                    x: float(cx.trunc()),
                    y: float(
                        (f64::from(rect.y) + f64::from(rect.height) - offset - height / 2.0)
                            .trunc(),
                    ),
                },
                5.0,
                color(182, 182, 182, math::byte(255.0 * 0.9 * alpha)?),
            )?;
        }
        Ok(RenderResult::None)
    }
}
