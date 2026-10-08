use super::*;
use openpilot_ui_framework::{
    draw::{BLACK, WHITE},
    text_layout,
};
impl InputDialog {
    pub(super) fn draw_input(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let rect = self.state.rect;
        let text = self.keyboard.text.clone();
        let candidate = self.keyboard.candidate().to_owned();
        let combined = format!("{text}{candidate}");
        let measured = if combined.is_empty() {
            self.hint.text.get()
        } else {
            combined
        };
        let size = text_layout::measure(draw, Font::Regular, &measured, 35.0, 0.0);
        let mut x = 10.0 + self.enter.width + 20.0;
        let field = Rect {
            x,
            y: rect.y + 15.0,
            width: rect.width - x * 2.0,
            height: size.y,
        };
        if size.x > field.width {
            x -= size.x - field.width;
        }
        draw.scissor(Some(field))?;
        paint::text(
            draw,
            Point { x, y: field.y },
            paint::Text {
                value: &text,
                font: Font::Regular,
                size: 35.0,
                spacing: 0.0,
                color: WHITE,
            },
        )?;
        if !candidate.is_empty() {
            let candidate_size = text_layout::measure(draw, Font::Regular, &candidate, 35.0, 0.0);
            paint::text(
                draw,
                Point {
                    x: (x + size.x).min(field.x + field.width) - candidate_size.x,
                    y: field.y,
                },
                paint::Text {
                    value: &candidate,
                    font: Font::Regular,
                    size: 35.0,
                    spacing: 0.0,
                    color: paint::color(255, 255, 255, 128),
                },
            )?;
        }
        draw.scissor(None)?;
        if size.x > field.width {
            draw.gradient(
                Rect {
                    width: 80.0,
                    ..field
                },
                [BLACK, 0, 0, BLACK],
            )?;
        }
        let alpha = (((frame.now * 6.0).sin() + 1.0) / 2.0 * 255.0)
            .to_u8()
            .ok_or(Error::Contract("input cursor alpha"))?;
        let cursor = if text.is_empty() {
            field.x - 6.0
        } else {
            (x + size.x + 3.0).min(field.x + field.width)
        };
        draw.rounded_segments(
            Rect {
                x: cursor,
                y: field.y,
                width: 4.0,
                height: size.y,
            },
            1.0,
            4,
            paint::color(255, 255, 255, alpha),
            false,
        )?;
        self.backspace_alpha
            .update(if text.is_empty() { 0.0 } else { 255.0 });
        if self.backspace_alpha.x > 1.0 {
            self.backspace.draw(
                draw,
                Point {
                    x: rect.width - self.backspace.width - 27.0,
                    y: rect.y + 14.0,
                },
                1.0,
                paint::color(
                    255,
                    255,
                    255,
                    self.backspace_alpha
                        .x
                        .to_u8()
                        .ok_or(Error::Contract("backspace alpha"))?,
                ),
            )?;
        }
        if text.is_empty() && !self.hint.text.get().is_empty() && candidate.is_empty() {
            self.hint.set_rect(Rect {
                width: rect.width - field.x - 20.0,
                ..field
            });
            self.hint.render(frame, draw)?;
        }
        self.left = Rect {
            x: rect.x,
            y: rect.y,
            width: field.x,
            height: rect.height - self.keyboard.height(),
        };
        self.right = Rect {
            x: field.x + field.width,
            y: rect.y,
            width: rect.width - (field.x + field.width),
            height: self.left.height,
        };
        self.enter_alpha
            .update(if text.chars().count() >= self.minimum_length {
                255.0
            } else {
                0.0
            });
        let alpha = self
            .enter_alpha
            .x
            .to_u8()
            .ok_or(Error::Contract("enter alpha"))?;
        let position = Point {
            x: rect.x + 10.0,
            y: rect.y,
        };
        self.enter
            .draw(draw, position, 1.0, paint::color(255, 255, 255, alpha))?;
        self.enter_disabled.draw(
            draw,
            position,
            1.0,
            paint::color(255, 255, 255, 255 - alpha),
        )?;
        Ok(())
    }
}
