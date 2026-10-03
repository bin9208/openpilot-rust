use super::Preview;
use crate::paint::{self, color};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    assets::Texture,
    draw::{Draw, RoundedOutline},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Horizontal, Vertical},
    Error,
};
fn image(draw: &mut dyn Draw, texture: Texture, x: f64, y: f64, tint: u32) -> Result<(), Error> {
    paint::image(
        draw,
        paint::Image {
            texture,
            rect: Rect {
                x: float(x),
                y: float(y),
                width: texture.width,
                height: texture.height,
            },
            tint,
            origin: Point::default(),
            rotation: 0.0,
        },
    )
}
impl Preview {
    pub(super) fn overlay(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let data = &self.driver.data;
        if !data.detected {
            return Ok(());
        }
        let [face_x, face_y] = data.position.as_slice() else {
            return Err(Error::Contract("driver face position must have two values"));
        };
        let std = data
            .deviation
            .first()
            .zip(data.deviation.get(1))
            .map(|(x, y)| x.max(*y))
            .ok_or(Error::Contract("driver face deviation missing"))?;
        let alpha = if std > 0.15 {
            (0.7 - (std - 0.15) * 3.5).max(0.0)
        } else {
            0.7
        };
        let rect = self.state.rect;
        let x = f64::from(rect.x)
            + f64::from(rect.width) / 2.0
            + ((1080.0 - 1714.0 * face_x) - 1080.0) * 1.25 * (f64::from(rect.width) / 2160.0);
        let tici_y =
            -135.0 + (504.0 + face_x.abs() * 112.0) + (1205.0 - face_x.abs() * 724.0) * face_y;
        let y = f64::from(rect.y)
            + f64::from(rect.height) / 2.0
            + (tici_y - 540.0) * 1.25 * (f64::from(rect.height) / 1080.0);
        draw.rounded_outline(
            Rect {
                x: float(x - 37.5),
                y: float(y - 37.5),
                width: 75.0,
                height: 75.0,
            },
            RoundedOutline {
                roundness: float(35.0 / 75.0 / 2.0),
                segments: 3,
                thickness: 3.0,
                color: color(255, 255, 255, (alpha * 255.0).to_u8().unwrap_or(0)),
            },
        )?;
        if self.setup {
            return Ok(());
        }
        for (index, probability) in data.eyes.iter().enumerate() {
            let x = f64::from(rect.x) + 10.0 + if index == 1 { 89.0 } else { 0.0 };
            image(
                draw,
                self.eyes[1],
                x,
                f64::from(rect.y) + 10.0,
                color(
                    255,
                    255,
                    255,
                    (255.0 * (1.0 - probability)).to_u8().unwrap_or(0),
                ),
            )?;
            image(
                draw,
                self.eyes[0],
                x,
                f64::from(rect.y) + 10.0,
                color(255, 255, 255, (255.0 * probability).to_u8().unwrap_or(0)),
            )?;
        }
        image(
            draw,
            self.eyes[2],
            f64::from(rect.x) + 6.0,
            f64::from(rect.y),
            color(70, 80, 161, (255.0 * data.glasses).to_u8().unwrap_or(0)),
        )
    }
    pub(super) fn awareness(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let text = format!("Awareness: {}%", self.driver.data.awareness);
        let rect = self.state.rect;
        for (rect, color) in [
            (
                Rect {
                    x: rect.x + 2.0,
                    y: rect.y + 2.0,
                    ..rect
                },
                color(0, 0, 0, 180),
            ),
            (rect, color(255, 255, 255, 229)),
        ] {
            paint::label(
                draw,
                rect,
                paint::Label {
                    text: &text,
                    font: Font::Medium,
                    horizontal: Horizontal::Right,
                    vertical: Vertical::Top,
                    color,
                    ..paint::Label::new("", 44.0)
                },
            )?;
        }
        Ok(())
    }
}
