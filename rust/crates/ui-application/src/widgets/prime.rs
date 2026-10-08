use crate::{
    context::Context,
    paint::{self, Label, Text},
};
use openpilot_ui_framework::{
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self, float},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct PrimeWidget {
    pub state: WidgetState,
    context: Context,
}
impl PrimeWidget {
    pub fn new(context: Context) -> Self {
        Self {
            state: WidgetState::default(),
            context,
        }
    }
}
impl Widget for PrimeWidget {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let background = paint::color(51, 51, 51, 255);
        if self.context.prime.is_prime() {
            draw.rounded_segments(
                Rect {
                    height: 230.0,
                    ..rect
                },
                0.1,
                10,
                background,
                false,
            )?;
            let position = Point {
                x: rect.x + 56.0,
                y: rect.y + 40.0,
            };
            paint::text(
                draw,
                position,
                Text {
                    value: &self.context.tr("✓ SUBSCRIBED"),
                    font: Font::Bold,
                    size: 41.0,
                    spacing: 0.0,
                    color: paint::color(134, 255, 78, 255),
                },
            )?;
            paint::text(
                draw,
                Point {
                    y: position.y + 61.0,
                    ..position
                },
                Text {
                    value: &self.context.tr("comma prime"),
                    font: Font::Bold,
                    size: 75.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )?;
        } else {
            draw.rounded_segments(rect, 0.025, 10, background, false)?;
            let x = rect.x + 80.0;
            let y = rect.y + 90.0;
            let width = rect.width - 160.0;
            paint::label(
                draw,
                Rect {
                    x,
                    y,
                    width,
                    height: 90.0,
                },
                Label {
                    font: Font::Bold,
                    ..Label::new(&self.context.tr("Upgrade Now"), 75.0)
                },
            )?;
            let desc_y = y + 140.0;
            let desc = text_layout::wrap(
                draw,
                Font::Normal,
                &self
                    .context
                    .tr("Become a comma prime member at connect.comma.ai"),
                56.0,
                0.0,
                f64::from(width).trunc(),
            )
            .join("\n");
            let size = text_layout::measure(draw, Font::Normal, &desc, 56.0, 0.0);
            paint::text(
                draw,
                Point { x, y: desc_y },
                Text {
                    value: &desc,
                    font: Font::Normal,
                    size: 56.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )?;
            let features_y = desc_y + size.y + 50.0;
            paint::label(
                draw,
                Rect {
                    x,
                    y: features_y,
                    width,
                    height: 50.0,
                },
                Label {
                    font: Font::Bold,
                    ..Label::new(&self.context.tr("PRIME FEATURES:"), 41.0)
                },
            )?;
            for (index, feature) in [
                "Remote access",
                "24/7 LTE connectivity",
                "1 year of drive storage",
                "Remote snapshots",
            ]
            .into_iter()
            .enumerate()
            {
                use num_traits::ToPrimitive;
                let item_y = f64::from(features_y)
                    + 80.0
                    + index
                        .to_f64()
                        .ok_or(Error::Contract("feature index overflow"))?
                        * 65.0;
                paint::label(
                    draw,
                    Rect {
                        x,
                        y: float(item_y),
                        width: 100.0,
                        height: 60.0,
                    },
                    Label {
                        color: paint::color(70, 91, 234, 255),
                        ..Label::new("✓", 50.0)
                    },
                )?;
                paint::label(
                    draw,
                    Rect {
                        x: x + 60.0,
                        y: float(item_y),
                        width: width - 60.0,
                        height: 60.0,
                    },
                    Label::new(&self.context.tr(feature), 50.0),
                )?;
            }
        }
        Ok(RenderResult::None)
    }
}
