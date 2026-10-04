use super::DebugPlot;
use crate::{
    onroad::model_renderer::math,
    paint::{self, color, Text},
};
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{Point, Rect},
    text::Font,
    text_layout::float,
    Error,
};
impl DebugPlot {
    pub(super) fn paint_plot(&self, draw: &mut dyn Draw, title: &str) -> Result<(), Error> {
        let rect = self.state.rect;
        let x = f64::from(rect.x);
        let y = f64::from(rect.y);
        let width = math::integer(f64::from(rect.width))?;
        let height = math::integer(f64::from(rect.height))?;
        let x0 = math::integer(x)?;
        let y0 = math::integer(y)?;
        let x1 = math::integer(x + f64::from(rect.width))?;
        let y1 = math::integer(y + f64::from(rect.height))?;
        let grid = color(60, 60, 60, 120);
        for gx in ((x0 - x0.rem_euclid(100))..=x1).step_by(100) {
            draw.integer_line((gx, y0), (gx, y1), grid)?;
        }
        for gy in ((y0 - y0.rem_euclid(100))..=y1).step_by(100) {
            draw.integer_line((x0, gy), (x1, gy), grid)?;
        }
        draw.rounded_segments(
            Rect {
                x: float(f64::from(x0)),
                y: float(f64::from(y0)),
                width: float(f64::from(x1 - x0)),
                height: float(f64::from(y1 - y0)),
            },
            0.0,
            0,
            color(0, 0, 0, 70),
            false,
        )?;
        paint::text(
            draw,
            Point {
                x: float(f64::from(x0 + 10)),
                y: float(f64::from(y0 + 8)),
            },
            Text {
                value: title,
                font: Font::Display,
                size: 26.0,
                color: u32::MAX,
                spacing: 0.0,
            },
        )?;
        draw.default_text(
            &format!(
                "min={:.2}  max={:.2}",
                self.samples.minimum, self.samples.maximum
            ),
            (x0 + 10, y0 + 30),
            22,
            color(200, 200, 200, 255),
        )?;
        let colors = [
            color(255, 220, 0, 255),
            color(0, 255, 0, 255),
            color(255, 165, 0, 255),
        ];
        for (series, color) in colors.into_iter().enumerate() {
            if self.samples.size < 2 {
                continue;
            }
            let plot_y = y + 46.0;
            let plot_height = f64::from((height - 46).max(60));
            let dx = (f64::from(width.max(1)) / 299.0).max(1.0);
            let range = self.samples.maximum - self.samples.minimum;
            let ratio = if range < 1e-6 {
                plot_height
            } else {
                plot_height / range
            };
            let mut previous = None;
            for index in 0..self.samples.size {
                let value = self.samples.value(series, self.samples.size - 1 - index);
                let point = (
                    x + f64::from(index) * dx,
                    plot_y + plot_height - (value - self.samples.minimum) * ratio,
                );
                if let Some((px, py)) = previous {
                    for offset in -1..=1 {
                        draw.integer_line(
                            (math::integer(px)?, math::integer(py)? + offset),
                            (math::integer(point.0)?, math::integer(point.1)? + offset),
                            color,
                        )?;
                    }
                }
                previous = Some(point);
            }
            let (px, py) = previous.ok_or(Error::Contract("debug plot series missing"))?;
            let label = format!("{:.2}", self.samples.value(series, 0));
            let mut lx = math::integer(px + 12.0)?;
            let mut ly = math::integer(py + if series > 0 { 30.0 } else { 0.0 })?;
            let left = x0 + 6;
            let right = x1 - 6;
            let top = y0 + 6;
            let bottom = y1 - 6;
            let text_width = draw.measure_default(&label, 30)?;
            if lx + text_width > right {
                lx = left.max(right - text_width);
            }
            if lx < left {
                lx = left;
            }
            if ly + 30 > bottom {
                ly = top.max(bottom - 30);
            }
            if ly < top {
                ly = top;
            }
            draw.default_text(&label, (lx, ly), 30, color)?;
        }
        Ok(())
    }
}
