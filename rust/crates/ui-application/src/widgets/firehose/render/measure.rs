use super::*;
use num_traits::ToPrimitive;
impl Firehose {
    fn measured(&self, draw: &dyn Draw, text: Text<'_>, width: f64) -> Result<f64, Error> {
        let lines = text_layout::wrap(draw, text.font, text.value, text.size, 0.0, width);
        let count = lines
            .len()
            .to_f64()
            .ok_or(Error::Contract("firehose line count overflow"))?;
        Ok((count * text.size * draw.font_scale()).trunc())
    }
    pub(crate) fn measure_small(&self, draw: &dyn Draw) -> Result<f64, Error> {
        let width = (f64::from(self.state.rect.width) - 80.0).trunc();
        let mut y = 40.0 + (72.0 * draw.font_scale()).trunc() + 20.0;
        y += self.measured(
            draw,
            Text {
                value: &self.context.tr(copy::DESCRIPTION),
                font: Font::Regular,
                size: 36.0,
                spacing: 0.0,
                color: WHITE,
            },
            width,
        )? + 20.0;
        y += 22.0;
        let (status, _) = self.status()?;
        y += self.measured(
            draw,
            Text {
                value: &status,
                font: Font::Bold,
                size: 48.0,
                spacing: 0.0,
                color: WHITE,
            },
            width,
        )? + 20.0;
        if let Some(contribution) = self.contribution()? {
            y += self.measured(
                draw,
                Text {
                    value: &contribution,
                    font: Font::Bold,
                    size: 42.0,
                    spacing: 0.0,
                    color: WHITE,
                },
                width,
            )? + 20.0;
        }
        y += 22.0;
        y += self.measured(
            draw,
            Text {
                value: &self.context.tr(copy::INSTRUCTIONS_INTRO),
                font: Font::Regular,
                size: 32.0,
                spacing: 0.0,
                color: WHITE,
            },
            width,
        )? + 20.0;
        y += self.measured(
            draw,
            Text {
                value: &self.context.tr(copy::FAQ_HEADER),
                font: Font::Bold,
                size: 44.0,
                spacing: 0.0,
                color: WHITE,
            },
            width,
        )? + 20.0;
        for (question, answer) in copy::FAQ_ITEMS {
            y += self.measured(
                draw,
                Text {
                    value: &self.context.tr(question),
                    font: Font::Bold,
                    size: 32.0,
                    spacing: 0.0,
                    color: WHITE,
                },
                width,
            )?;
            y += self.measured(
                draw,
                Text {
                    value: &self.context.tr(answer),
                    font: Font::Regular,
                    size: 32.0,
                    spacing: 0.0,
                    color: WHITE,
                },
                width,
            )? + 20.0;
        }
        Ok(y + 40.0)
    }
}
