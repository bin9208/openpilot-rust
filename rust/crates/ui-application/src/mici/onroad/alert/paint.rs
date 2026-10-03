use super::{icons::Layout, Alerts};
use crate::{onroad::alert::Alert, paint::color};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::Rect,
    text_layout::{float, Horizontal},
    widget::{Frame, Widget},
    Error,
};
impl Alerts {
    pub(super) fn background(&self, draw: &mut dyn Draw, alert: &Alert) -> Result<(), Error> {
        let (r, g, b) = match alert.status {
            1 => (255, 115, 0),
            2 => (255, 0, 21),
            _ => (0, 0, 0),
        };
        let solid = color(
            r,
            g,
            b,
            (255.0 * 0.90 * self.alpha.x)
                .to_u8()
                .ok_or(Error::Contract("invalid alert background opacity"))?,
        );
        let clear = color(r, g, b, 0);
        let rect = self.state.rect;
        let height = match alert.alert_type.split('/').next().unwrap_or_default() {
            "preLaneChangeLeft" | "preLaneChangeRight" | "laneChange" => {
                (f64::from(rect.height) * 0.583).round_ties_even()
            }
            "laneChangeBlocked" => (f64::from(rect.height) * 0.833).round_ties_even(),
            _ => f64::from(rect.height).trunc(),
        };
        let fill = (height * 0.2).round_ties_even();
        draw.rounded(
            Rect {
                x: rect.x.trunc(),
                y: rect.y.trunc(),
                width: rect.width.trunc(),
                height: float(fill),
            },
            0.0,
            solid,
        )?;
        draw.gradient(
            Rect {
                x: rect.x.trunc(),
                y: float((f64::from(rect.y) + fill).trunc()),
                width: rect.width.trunc(),
                height: float((height - fill).trunc()),
            },
            [solid, clear, clear, solid],
        )
    }
    pub(super) fn text(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
        alert: &Alert,
        layout: &Layout,
    ) -> Result<(), Error> {
        let title = alert
            .text1
            .to_lowercase()
            .replace("calibrating: ", "calibrating:\n");
        let length = title.chars().count();
        let second = length <= 16;
        let mut size = if length <= 12 {
            60.0
        } else if length <= 16 {
            50.0
        } else {
            40.0
        };
        if layout.icon.is_some() {
            size -= 10.0;
        }
        let alignment = if layout.left() {
            Horizontal::Right
        } else {
            Horizontal::Left
        };
        self.title.text = title.into();
        self.title.size = size;
        self.title.horizontal = alignment;
        self.title.color = color(
            255,
            255,
            255,
            (255.0 * 0.9 * self.alpha.x)
                .to_u8()
                .ok_or(Error::Contract("invalid alert text opacity"))?,
        );
        self.title.set_rect(Rect {
            y: float(f64::from(layout.rect.y) - if size >= 70.0 { 11.0 } else { 4.0 }),
            ..layout.rect
        });
        self.title.render(frame, draw)?;
        let subtitle = alert.text2.to_lowercase();
        if (second || alert.size != 1) && !subtitle.is_empty() {
            let y = f64::from(self.title.state.rect.y)
                + self
                    .title
                    .content_height(draw, f64::from(layout.rect.width.trunc()))
                - 4.0;
            self.subtitle.size = if subtitle.chars().count() > 24 {
                32.0
            } else if subtitle.chars().count() > 16 {
                36.0
            } else {
                40.0
            };
            self.subtitle.text = subtitle.into();
            self.subtitle.horizontal = alignment;
            self.subtitle.color = color(
                255,
                255,
                255,
                (255.0 * 0.65 * self.alpha.x)
                    .to_u8()
                    .ok_or(Error::Contract("invalid alert subtitle opacity"))?,
            );
            self.subtitle.set_rect(Rect {
                y: float(y),
                height: float(f64::from(layout.rect.height) - y),
                ..layout.rect
            });
            self.subtitle.render(frame, draw)?;
        }
        Ok(())
    }
}
