use crate::{
    config::Config,
    draw::{self, Draw, TextDraw},
    geometry::{Point, Rect},
    text::{self, Font, Measure},
    Error,
};
#[derive(Default, Debug, serde::Serialize)]
pub struct Spinner {
    pub rotation: f64,
    pub progress: Option<u8>,
    pub status: String,
    pub lines: Vec<String>,
}
impl Spinner {
    pub fn set_text(
        &mut self,
        text: &str,
        config: Config,
        measure: &impl Measure,
    ) -> Result<(), Error> {
        let text = text::trim(text);
        if !text.is_empty() && text.chars().all(crate::digits::is_digit) {
            if text.chars().count() > 4300 {
                return Err(Error::Integer);
            }
            let mut value = 0_u16;
            for digit in text.chars() {
                let digit = crate::digits::decimal(digit).ok_or(Error::Integer)?;
                value = (value * 10 + u16::from(digit)).min(101);
            }
            self.progress = Some(u8::try_from(value.min(100)).map_err(|_| Error::Integer)?);
            self.lines.clear();
        } else {
            self.status = text.into();
            if self.progress.is_none() {
                let tokens = config.spinner();
                self.lines = text::wrap(
                    text,
                    config.scaled_font(tokens.font),
                    config.width() - tokens.margin,
                    measure,
                );
            }
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        config: Config,
        ip: &str,
        dt: f32,
        draw: &mut impl Draw,
    ) -> Result<(), Error> {
        let t = config.spinner();
        let width = config.width();
        let height = config.height();

        let ip_size = draw.measure(Font::Pretendard, ip, config.scaled_font(t.ip_font), 0.0);
        draw.text(TextDraw {
            font: Font::Pretendard,
            text: ip,
            position: Point {
                x: (width / 2.0 - ip_size.x / 2.0).round_ties_even(),
                y: t.ip_top,
            },
            size: config.scaled_font(t.ip_font),
            spacing: 0.0,
            color: draw::IP,
        })?;
        let (spacing, center_y) = if self.lines.is_empty() {
            (t.center_gap, height / 2.0)
        } else {
            let total =
                t.texture + t.wrapped_gap + crate::number::float(self.lines.len()) * t.line_height;
            (t.wrapped_gap, (height - total) / 2.0 + t.texture / 2.0)
        };
        let y = center_y + t.texture / 2.0 + spacing;
        let center = Point {
            x: width / 2.0,
            y: center_y,
        };
        self.rotation = (self.rotation + 360.0 * f64::from(dt)) % 360.0;
        draw.texture(
            true,
            Rect {
                x: center.x,
                y: center.y,
                width: t.texture,
                height: t.texture,
            },
            Point {
                x: t.texture / 2.0,
                y: t.texture / 2.0,
            },
            crate::number::float(self.rotation),
        )?;
        draw.texture(
            false,
            Rect {
                x: center.x - t.texture / 2.0,
                y: center.y - t.texture / 2.0,
                width: t.texture,
                height: t.texture,
            },
            Point::default(),
            0.0,
        )?;
        if let Some(progress) = self.progress {
            let mut bar = Rect {
                x: center.x - t.bar_width / 2.0,
                y,
                width: t.bar_width,
                height: t.bar_height,
            };
            draw.rounded(bar, 1.0, draw::DARKGRAY)?;
            bar.width *= f32::from(progress) / 100.0;
            draw.rounded(bar, 1.0, draw::WHITE)?;
            if !self.status.is_empty() {
                let status = text::fit_single_line(
                    &self.status,
                    config.scaled_font(t.status_font),
                    width - t.margin,
                    draw,
                );
                let size = draw.measure(
                    Font::Pretendard,
                    &status,
                    config.scaled_font(t.status_font),
                    0.0,
                );
                draw.text(TextDraw {
                    font: Font::Pretendard,
                    text: &status,
                    position: Point {
                        x: (center.x - size.x / 2.0).round_ties_even(),
                        y: y - t.status_gap - size.y,
                    },
                    size: config.scaled_font(t.status_font),
                    spacing: 0.0,
                    color: draw::LIGHTGRAY,
                })?;
            }
        } else {
            for (index, line) in self.lines.iter().enumerate() {
                let size = draw.measure(Font::Normal, line, config.scaled_font(t.font), 0.0);
                draw.text(TextDraw {
                    font: Font::Normal,
                    text: line,
                    position: Point {
                        x: center.x - size.x / 2.0,
                        y: y + crate::number::float(index) * t.line_height,
                    },
                    size: config.scaled_font(t.font),
                    spacing: 0.0,
                    color: draw::WHITE,
                })?;
            }
        }
        Ok(())
    }
}
