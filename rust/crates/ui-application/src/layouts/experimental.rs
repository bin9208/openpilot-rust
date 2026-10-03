//! Source: selfdrive/ui/widgets/exp_mode_button.py (MIT).
use crate::{
    context::Context,
    paint::{self, Text},
    params::Read,
};
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::{Draw, RoundedOutline, BLACK, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct ExperimentalModeButton {
    pub state: WidgetState,
    context: Context,
    pub experimental: bool,
    chill: Texture,
    experimental_icon: Texture,
}
impl ExperimentalModeButton {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let experimental = context.params.boolean("ExperimentalMode")?;
        Ok(Self {
            state: WidgetState::default(),
            context,
            experimental,
            chill: paint::texture(canvas, "icons/couch.png", (80, 80))?,
            experimental_icon: paint::texture(canvas, "icons/experimental_grey.png", (80, 80))?,
        })
    }
    pub fn refresh(&mut self) -> Result<(), crate::Error> {
        self.experimental = self.context.params.boolean("ExperimentalMode")?;
        Ok(())
    }
}
impl Widget for ExperimentalModeButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        if let Err(error) = self.refresh() {
            self.context
                .actions
                .push(crate::context::Action::Failure(error));
        }
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let alpha = if self.state.is_pressed() { 204 } else { 255 };
        let (start, end) = if self.experimental {
            (
                paint::color(255, 155, 63, alpha),
                paint::color(219, 56, 34, alpha),
            )
        } else {
            (
                paint::color(20, 255, 171, alpha),
                paint::color(35, 149, 255, alpha),
            )
        };
        let integer_rect = Rect {
            x: rect.x.trunc(),
            y: rect.y.trunc(),
            width: rect.width.trunc(),
            height: rect.height.trunc(),
        };
        draw.scissor(Some(integer_rect))?;
        let result = (|| {
            draw.gradient(integer_rect, [start, start, end, end])?;
            draw.rounded_outline(
                rect,
                RoundedOutline {
                    roundness: 0.19,
                    segments: 10,
                    thickness: 5.0,
                    color: BLACK,
                },
            )
        })();
        draw.scissor(None)?;
        result?;
        let x = rect.x + rect.width - 130.0;
        draw.line(
            Point { x, y: rect.y },
            Point {
                x,
                y: rect.y + rect.height,
            },
            3.0,
            paint::color(0, 0, 0, 77),
        )?;
        let text = self.context.tr(if self.experimental {
            "EXPERIMENTAL MODE ON"
        } else {
            "CHILL MODE ON"
        });
        let font_scale = draw.font_scale();
        paint::text(
            draw,
            Point {
                x: (rect.x + 25.0).trunc(),
                y: float(
                    (f64::from(rect.y) + f64::from(rect.height) / 2.0
                        - (45.0 * font_scale / 2.0).floor())
                    .trunc(),
                ),
            },
            Text {
                value: &text,
                font: Font::Normal,
                size: 45.0,
                spacing: 0.0,
                color: BLACK,
            },
        )?;
        paint::image(
            draw,
            paint::Image {
                texture: if self.experimental {
                    self.experimental_icon
                } else {
                    self.chill
                },
                rect: Rect {
                    x: rect.x + rect.width - 105.0,
                    y: rect.y + (rect.height - 80.0) / 2.0,
                    width: 80.0,
                    height: 80.0,
                },
                origin: Point::default(),
                rotation: 0.0,
                tint: WHITE,
            },
        )?;
        Ok(RenderResult::None)
    }
}
