use super::*;
impl Firehose {
    pub(crate) fn render_big(&mut self, draw: &mut dyn Draw, offset: f64) -> Result<f64, Error> {
        let rect = self.state.rect;
        let x = (f64::from(rect.x) + 40.0).trunc();
        let mut y = (f64::from(rect.y) + 40.0 + offset).trunc();
        let width = (f64::from(rect.width) - 80.0).trunc();
        let title = self.context.tr(copy::TITLE);
        let size = text_layout::measure(draw, Font::Medium, &title, 100.0, 0.0);
        paint::text(
            draw,
            Point {
                x: rect.x + (rect.width - size.x) / 2.0,
                y: float(y),
            },
            Text {
                value: &title,
                font: Font::Medium,
                size: 100.0,
                spacing: 0.0,
                color: WHITE,
            },
        )?;
        y += 200.0;
        y = self.block(
            draw,
            Rect {
                x: float(x),
                y: float(y),
                width: float(width),
                height: 0.0,
            },
            Text {
                value: &self.context.tr(copy::DESCRIPTION),
                font: Font::Normal,
                size: 45.0,
                spacing: 0.0,
                color: WHITE,
            },
        )? + 60.0;
        Self::separator(
            draw,
            Rect {
                x: float(x),
                y: float(y),
                width: float(width),
                height: 2.0,
            },
        )?;
        y += 50.0;
        let (status, color) = self.status()?;
        y = self.block(
            draw,
            Rect {
                x: float(x),
                y: float(y),
                width: float(width),
                height: 0.0,
            },
            Text {
                value: &status,
                font: Font::Bold,
                size: 60.0,
                spacing: 0.0,
                color,
            },
        )? + 40.0;
        if let Some(contribution) = self.contribution()? {
            y = self.block(
                draw,
                Rect {
                    x: float(x),
                    y: float(y),
                    width: float(width),
                    height: 0.0,
                },
                Text {
                    value: &contribution,
                    font: Font::Bold,
                    size: 52.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )? + 40.0;
        }
        Self::separator(
            draw,
            Rect {
                x: float(x),
                y: float(y),
                width: float(width),
                height: 2.0,
            },
        )?;
        y += 50.0;
        y = self.block(
            draw,
            Rect {
                x: float(x),
                y: float(y),
                width: float(width),
                height: 0.0,
            },
            Text {
                value: &self.context.tr(copy::INSTRUCTIONS),
                font: Font::Normal,
                size: 40.0,
                spacing: 0.0,
                color: LIGHT_GRAY,
            },
        )?;
        Ok((y - self.legacy.offset + 40.0).round_ties_even())
    }
}
