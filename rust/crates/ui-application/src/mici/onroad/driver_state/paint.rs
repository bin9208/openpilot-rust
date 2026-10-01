use super::DriverState;
use crate::paint::{self, color};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    assets::Texture,
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    Error,
};
fn alpha(value: f64) -> u32 {
    color(255, 255, 255, (255.0 * value).to_u8().unwrap_or(0))
}
fn image(draw: &mut dyn Draw, texture: Texture, position: Point, tint: u32) -> Result<(), Error> {
    paint::image(
        draw,
        paint::Image {
            texture,
            rect: Rect {
                x: position.x,
                y: position.y,
                width: texture.width,
                height: texture.height,
            },
            tint,
            origin: Point::default(),
            rotation: 0.0,
        },
    )
}
impl DriverState {
    pub(super) fn draw(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        let rect = self.state.rect;
        let centered = |t: Texture| Point {
            x: float(f64::from(rect.x) + f64::from(rect.width - t.width) / 2.0),
            y: float(f64::from(rect.y) + f64::from(rect.height - t.height) / 2.0),
        };
        image(
            draw,
            self.icons[3],
            Point {
                x: rect.x,
                y: rect.y,
            },
            alpha(self.fade.value()),
        )?;
        image(
            draw,
            self.icons[0],
            centered(self.icons[0]),
            alpha(0.9 * self.fade.value()),
        )?;
        if !self.active() {
            return Ok(());
        }
        if !self.lines {
            let texture = self.icons[1];
            paint::image(
                draw,
                paint::Image {
                    texture,
                    rect: Rect {
                        x: rect.x + rect.width / 2.0,
                        y: rect.y + rect.height / 2.0,
                        width: texture.width,
                        height: texture.height,
                    },
                    origin: Point {
                        x: texture.width / 2.0,
                        y: texture.height / 2.0,
                    },
                    rotation: float(self.rotation.value() - 90.0),
                    tint: alpha(self.fade.value() * (1.0 - self.center.value())),
                },
            )?;
            let mut position = centered(self.icons[2]);
            position.x = position.x.trunc();
            position.y = position.y.trunc();
            image(
                draw,
                self.icons[2],
                position,
                alpha(self.fade.value() * self.center.value()),
            )?;
        } else {
            for (index, filter) in self.angles.iter_mut().enumerate() {
                let angle = f64::from(
                    u32::try_from(index).map_err(|_| Error::Contract("driver angle overflow"))?,
                ) * 5.0;
                let difference = (angle - self.rotation.value()).rem_euclid(360.0) - 180.0;
                let mut target = f64::from(difference.abs() <= 25.0 && self.data.detected);
                if self.looking_center {
                    target += self.center.value().clamp(0.0, 1.0) * (0.45 - target);
                }
                let value = filter.update(target);
                let length =
                    (value.clamp(0.0, 1.0) * f64::from(rect.width) / 6.0).round_ties_even();
                let offset = f64::from(rect.width) / 2.0 - length * 2.0;
                let x = f64::from(rect.x)
                    + f64::from(rect.width) / 2.0
                    + (offset + length) * angle.to_radians().cos();
                let y = f64::from(rect.y)
                    + f64::from(rect.height) / 2.0
                    + (offset + length) * angle.to_radians().sin();
                if value > 0.01 {
                    draw.line(
                        Point {
                            x: float(x),
                            y: float(y),
                        },
                        Point {
                            x: float(x + length * angle.to_radians().cos()),
                            y: float(y + length * angle.to_radians().sin()),
                        },
                        12.0,
                        if self.looking_center {
                            color(166, 166, 166, 255)
                        } else {
                            color(0, 255, 64, 255)
                        },
                    )?;
                }
            }
        }
        Ok(())
    }
}
