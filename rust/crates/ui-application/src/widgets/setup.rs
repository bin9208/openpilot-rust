use crate::{
    context::{Action, Context, Page, Panel},
    paint::{self, Text},
};
use openpilot_ui_framework::{
    button::{Button, ButtonStyle},
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    label::Label,
    text::Font,
    text_layout::{self, float},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct SetupWidget {
    pub state: WidgetState,
    context: Context,
    pair: Button,
    open: Button,
    firehose: Label,
}
impl SetupWidget {
    pub fn new(context: Context) -> Self {
        let mut pair = Button::new("");
        pair.label.text = context.text("Pair device");
        pair.set_style(ButtonStyle::Primary);
        let queue = context.actions.clone();
        pair.state.click = Some(Box::new(move || queue.push(Action::PairingCheck)));
        let mut open = Button::new("");
        open.label.text = context.text("Open");
        open.set_style(ButtonStyle::Primary);
        let queue = context.actions.clone();
        open.state.click = Some(Box::new(move || {
            queue.push(Action::Open(Page::Settings(Panel::Firehose)))
        }));
        let mut firehose = Label::new("");
        firehose.text = context.text("🔥 Firehose Mode 🔥");
        firehose.font = Font::Medium;
        firehose.size = 64.0;
        Self {
            state: WidgetState::default(),
            context,
            pair,
            open,
            firehose,
        }
    }
}
impl Widget for SetupWidget {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let background = paint::color(51, 51, 51, 255);
        if !self.context.prime.is_paired() {
            draw.rounded_segments(rect, 0.03, 20, background, false)?;
            let x = rect.x + 64.0;
            let mut y = f64::from(rect.y) + 48.0;
            let width = rect.width - 128.0;
            paint::text(
                draw,
                Point { x, y: float(y) },
                Text {
                    value: &self.context.tr("Finish Setup"),
                    font: Font::Bold,
                    size: 75.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )?;
            y += 113.0;
            let lines=text_layout::wrap(draw,Font::Normal,&self.context.tr("Pair your device with comma connect (connect.comma.ai) and claim your comma prime offer."),50.0,0.0,f64::from(width).trunc());
            for line in lines {
                paint::text(
                    draw,
                    Point { x, y: float(y) },
                    Text {
                        value: &line,
                        font: Font::Normal,
                        size: 50.0,
                        spacing: 0.0,
                        color: WHITE,
                    },
                )?;
                y += 50.0 * draw.font_scale();
            }
            self.pair.set_rect(Rect {
                x,
                y: float(y + 30.0),
                width,
                height: 200.0,
            });
            self.pair.render(frame, draw)?;
        } else {
            draw.rounded_segments(
                Rect {
                    height: 500.0,
                    ..rect
                },
                0.04,
                20,
                background,
                false,
            )?;
            let x = rect.x + 56.0;
            let mut y = f64::from(rect.y) + 40.0;
            let width = rect.width - 112.0;
            self.firehose.set_rect(Rect {
                x: rect.x,
                y: float(y),
                width: rect.width,
                height: 64.0,
            });
            self.firehose.render(frame, draw)?;
            y += 64.0 + 42.0;
            let lines = text_layout::wrap(
                draw,
                Font::Normal,
                &self.context.tr(
                    "Maximize your training data uploads to improve openpilot's driving models.",
                ),
                40.0,
                0.0,
                f64::from(width).trunc(),
            );
            for line in lines {
                paint::text(
                    draw,
                    Point { x, y: float(y) },
                    Text {
                        value: &line,
                        font: Font::Normal,
                        size: 40.0,
                        spacing: 0.0,
                        color: WHITE,
                    },
                )?;
                y += 40.0 * draw.font_scale();
            }
            y += 42.0;
            self.open.set_rect(Rect {
                x,
                y: float(y),
                width,
                height: 112.0,
            });
            self.open.render(frame, draw)?;
        }
        Ok(RenderResult::None)
    }
}
