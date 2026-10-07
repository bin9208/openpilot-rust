use super::{Alert, Alerts};
use crate::paint::color;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self, float},
    widget::{Frame, Widget},
    Error,
};
impl Alerts {
    pub(super) fn draw_alert(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
        alert: &Alert,
    ) -> Result<(), Error> {
        let rect = self.state.rect;
        let alert_rect = if alert.size == 3 {
            rect
        } else {
            let height = match alert.size {
                1 => 271.0,
                2 => 420.0,
                _ => f64::from(rect.height),
            };
            Rect {
                x: float(f64::from(rect.x) + 40.0),
                y: float(f64::from(rect.y) + f64::from(rect.height) - height + 40.0),
                width: float(f64::from(rect.width) - 80.0),
                height: float(height - 80.0),
            }
        };
        let tint = match alert.status {
            1 => color(0xda, 0x6f, 0x25, 20),
            2 => color(0xc9, 0x22, 0x31, 20),
            _ => color(0x15, 0x15, 0x15, 20),
        };
        if alert.size == 3 {
            draw.rounded(alert_rect, 0.0, tint)?;
        } else {
            draw.rounded_segments(
                alert_rect,
                float(30.0 / (f64::from(alert_rect.width.min(alert_rect.height)) / 2.0)),
                10,
                tint,
                false,
            )?;
        }
        let rect = Rect {
            x: float(f64::from(alert_rect.x) + 60.0),
            y: float(f64::from(alert_rect.y) + 60.0),
            width: float(f64::from(alert_rect.width) - 120.0),
            height: float(f64::from(alert_rect.height) - 120.0),
        };
        match alert.size {
            1 => centered(draw, &alert.text1, rect, Font::Bold, 74.0, true)?,
            2 => {
                centered(draw, &alert.text1, rect, Font::Bold, 88.0, false)?;
                centered(
                    draw,
                    &alert.text2,
                    Rect {
                        y: float(f64::from(rect.y) + 133.0),
                        ..rect
                    },
                    Font::Normal,
                    66.0,
                    false,
                )?;
            }
            _ => {
                let long = alert.text1.chars().count() > 15;
                self.title.size = if long { 132.0 } else { 177.0 };
                self.title.text = alert.text1.clone().into();
                self.title.set_rect(Rect {
                    x: rect.x,
                    y: float(
                        f64::from(rect.y)
                            + if long || alert.text1.contains('\n') {
                                200.0
                            } else {
                                270.0
                            },
                    ),
                    width: rect.width,
                    height: 600.0,
                });
                self.title.render(frame, draw)?;
                self.subtitle.text = alert.text2.clone().into();
                self.subtitle.set_rect(Rect {
                    x: rect.x,
                    y: float(
                        f64::from(rect.y) + f64::from(rect.height)
                            - if long { 361.0 } else { 420.0 },
                    ),
                    width: rect.width,
                    height: 300.0,
                });
                self.subtitle.render(frame, draw)?;
            }
        }
        Ok(())
    }
}
fn centered(
    draw: &mut dyn Draw,
    text: &str,
    rect: Rect,
    font: Font,
    size: f64,
    vertical: bool,
) -> Result<(), Error> {
    let measured = text_layout::measure(draw, font, text, size, 0.0);
    text_layout::draw_text(
        draw,
        font,
        text,
        Point {
            x: float(f64::from(rect.x) + (f64::from(rect.width) - f64::from(measured.x)) / 2.0),
            y: float(
                f64::from(rect.y)
                    + if vertical {
                        (f64::from(rect.height) - f64::from(measured.y)) / 2.0
                    } else {
                        0.0
                    },
            ),
        },
        size,
        0.0,
        u32::MAX,
    )
}
