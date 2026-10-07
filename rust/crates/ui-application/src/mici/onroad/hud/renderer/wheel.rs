use super::*;
use crate::onroad::model_renderer::math;
use crate::{
    onroad::{hud::common::WHITE, model_renderer::hsv},
    paint::{color, Image},
};
use openpilot_ui_framework::geometry::{Point, Rect};
impl Hud {
    pub(super) fn draw_wheel(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let wheel = if self.critical {
            self.wheel_critical
        } else {
            self.wheel
        };
        self.wheel_alpha.update(255.0 * 0.95);
        self.wheel_y.update(0.0);
        let x = (f64::from(self.state.rect.x) + 18.0 + f64::from(wheel.width) / 2.0).trunc();
        let y =
            (f64::from(self.state.rect.y) + 18.0 + f64::from(wheel.height) / 2.0 + self.wheel_y.x)
                .trunc();
        let scale = math::interp(self.torque.x.abs(), &[0.5, 1.0], &[1.0, 1.5])?;
        let (width, height, margin) = (
            f64::from(wheel.width) * scale,
            f64::from(wheel.height) * scale,
            25.0 * scale,
        );
        self.turn.set_rect(Rect {
            x: math::float(x - width / 2.0 - margin),
            y: math::float(y - height / 2.0 - margin),
            width: math::float(width + margin * 2.0),
            height: math::float(height + margin * 2.0),
        });
        self.turn.render(frame, draw)?;
        let alpha = math::byte(self.wheel_alpha.x)?;
        let tint = if self.context.ui.borrow().lat_active {
            hsv::blend(
                color(0, 255, 0, alpha),
                color(255, 115, 0, alpha),
                math::clip((self.torque.x.abs() - 0.75) * 4.0, 0.0, 1.0),
            )?
        } else {
            color(230, 230, 230, alpha)
        };
        let messages = self.context.messages.borrow();
        let rotation = -messages::car_state(&messages.state)?.get_steering_angle_deg();
        let destination = Rect {
            x: math::float(x),
            y: math::float(y),
            width: math::float(width),
            height: math::float(height),
        };
        let origin = Point {
            x: math::float(width / 2.0),
            y: math::float(height / 2.0),
        };
        paint::image(
            draw,
            Image {
                texture: wheel,
                rect: destination,
                tint,
                rotation,
                origin,
            },
        )?;
        paint::image(
            draw,
            Image {
                texture: self.wheel_cap,
                rect: destination,
                tint: WHITE,
                rotation,
                origin,
            },
        )?;
        if self.critical {
            let rect = Rect {
                x: math::float(
                    x - f64::from(self.exclamation.width) / 2.0
                        + f64::from(wheel.width) / 2.0
                        + 10.0,
                ),
                y: math::float(y - f64::from(self.exclamation.height) / 2.0),
                width: self.exclamation.width,
                height: self.exclamation.height,
            };
            paint::image(
                draw,
                Image {
                    texture: self.exclamation,
                    rect,
                    tint: WHITE,
                    rotation: 0.0,
                    origin: Point::default(),
                },
            )?;
        } else if self.debug_speed
            || messages::controls_state(&messages.state)?.get_active_lane_line()
        {
            paint::image(
                draw,
                Image {
                    texture: self.wheel_lane,
                    rect: Rect {
                        x: math::float(x - f64::from(self.wheel_lane.width) / 2.0),
                        y: math::float(y - f64::from(self.wheel_lane.height) / 2.0 - 3.0),
                        width: self.wheel_lane.width,
                        height: self.wheel_lane.height,
                    },
                    tint,
                    rotation: 0.0,
                    origin: Point::default(),
                },
            )?;
        }
        drop(messages);
        self.side(draw, wheel, [x, y])
    }
}
