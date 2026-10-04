use super::*;
use crate::paint::{self, Text};
use openpilot_ui_framework::{
    draw::WHITE,
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self, float},
};
mod big;
mod measure;
mod small;
const GRAY: u32 = paint::color(68, 68, 68, 255);
const LIGHT_GRAY: u32 = paint::color(228, 228, 228, 255);
impl Firehose {
    fn block(&self, draw: &mut dyn Draw, rect: Rect, value: Text<'_>) -> Result<f64, Error> {
        let lines = text_layout::wrap(
            draw,
            value.font,
            value.value,
            value.size,
            0.0,
            f64::from(rect.width),
        );
        let mut y = f64::from(rect.y);
        for line in lines {
            paint::text(
                draw,
                Point {
                    x: rect.x,
                    y: float(y),
                },
                Text {
                    value: &line,
                    ..value
                },
            )?;
            let step = value.size * draw.font_scale();
            y += if self.context.big { step } else { step.trunc() };
        }
        Ok(if self.context.big {
            y.round_ties_even()
        } else {
            y
        })
    }
    fn contribution(&self) -> Result<Option<String>, Error> {
        let count = self
            .count
            .lock()
            .map_err(|_| Error::Contract("firehose count poisoned"))?;
        count::contribution(&self.context, &count)
    }
    fn separator(draw: &mut dyn Draw, rect: Rect) -> Result<(), Error> {
        draw.rounded(
            Rect {
                height: 2.0,
                ..rect
            },
            0.0,
            GRAY,
        )
    }
}
