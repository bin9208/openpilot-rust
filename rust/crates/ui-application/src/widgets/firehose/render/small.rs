use super::*;
impl Firehose {
    pub(crate) fn render_small(&self, draw: &mut dyn Draw, offset: f64) -> Result<(), Error> {
        let rect = self.state.rect;
        let x = rect.x + 40.0;
        let width = rect.width - 80.0;
        let mut y = f64::from(rect.y) + 40.0 + offset;
        paint::text(
            draw,
            Point { x, y: float(y) },
            Text {
                value: &self.context.tr(copy::TITLE),
                font: Font::Bold,
                size: 64.0,
                spacing: 0.0,
                color: WHITE,
            },
        )?;
        y += (64.0 * draw.font_scale()).trunc() + 20.0;
        y = self.block(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 0.0,
            },
            Text {
                value: &self.context.tr(copy::DESCRIPTION),
                font: Font::Regular,
                size: 36.0,
                spacing: 0.0,
                color: WHITE,
            },
        )? + 20.0;
        Self::separator(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 2.0,
            },
        )?;
        y += 20.0;
        let (status, color) = self.status()?;
        y = self.block(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 0.0,
            },
            Text {
                value: &status,
                font: Font::Bold,
                size: 48.0,
                spacing: 0.0,
                color,
            },
        )? + 20.0;
        if let Some(contribution) = self.contribution()? {
            y = self.block(
                draw,
                Rect {
                    x,
                    y: float(y),
                    width,
                    height: 0.0,
                },
                Text {
                    value: &contribution,
                    font: Font::Bold,
                    size: 42.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )? + 20.0;
        }
        Self::separator(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 2.0,
            },
        )?;
        y += 20.0;
        y = self.block(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 0.0,
            },
            Text {
                value: &self.context.tr(copy::INSTRUCTIONS_INTRO),
                font: Font::Regular,
                size: 32.0,
                spacing: 0.0,
                color: LIGHT_GRAY,
            },
        )? + 20.0;
        y = self.block(
            draw,
            Rect {
                x,
                y: float(y),
                width,
                height: 0.0,
            },
            Text {
                value: &self.context.tr(copy::FAQ_HEADER),
                font: Font::Bold,
                size: 44.0,
                spacing: 0.0,
                color: LIGHT_GRAY,
            },
        )? + 20.0;
        for (question, answer) in copy::FAQ_ITEMS {
            y = self.block(
                draw,
                Rect {
                    x,
                    y: float(y),
                    width,
                    height: 0.0,
                },
                Text {
                    value: &self.context.tr(question),
                    font: Font::Bold,
                    size: 32.0,
                    spacing: 0.0,
                    color: LIGHT_GRAY,
                },
            )?;
            y = self.block(
                draw,
                Rect {
                    x,
                    y: float(y),
                    width,
                    height: 0.0,
                },
                Text {
                    value: &self.context.tr(answer),
                    font: Font::Regular,
                    size: 32.0,
                    spacing: 0.0,
                    color: LIGHT_GRAY,
                },
            )? + 20.0;
        }
        Ok(())
    }
}
