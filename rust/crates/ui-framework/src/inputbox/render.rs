use super::*;
impl Widget for InputBox {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        self.visible_width = f64::from(rect.width);
        if self.pending_offset {
            self.update_offset(draw, frame.monotonic);
            self.pending_offset = false;
        }
        draw.rounded(rect, 0.0, self.color)?;
        self.keyboard(frame, draw);
        self.blink += 1;
        if self.blink >= 30 {
            self.show_cursor = !self.show_cursor;
            self.blink = 0;
        }
        let value = self.display_text(frame.monotonic);
        draw.scissor(Some(Rect {
            x: (rect.x + 8.0).trunc(),
            y: rect.y.trunc(),
            width: (rect.width - 16.0).trunc(),
            height: rect.height.trunc(),
        }))?;
        let font_scale = draw.font_scale();
        text::draw_text(
            draw,
            Font::Normal,
            &value,
            Point {
                x: float(f64::from(rect.x) + 10.0 - self.offset).trunc(),
                y: float(
                    f64::from(rect.y) + f64::from(rect.height) / 2.0
                        - self.font_size * font_scale / 2.0,
                )
                .trunc(),
            },
            self.font_size,
            0.0,
            self.text_color,
        )?;
        if self.show_cursor {
            let mut x = f64::from(rect.x) + 10.0;
            if !value.is_empty() && self.cursor > 0 {
                x += f64::from(
                    text::measure(
                        draw,
                        Font::Normal,
                        &value.chars().take(self.cursor).collect::<String>(),
                        self.font_size,
                        0.0,
                    )
                    .x,
                );
            }
            x -= self.offset;
            let height = self.font_size * draw.font_scale() + 4.0;
            let y = f64::from(rect.y) + f64::from(rect.height) / 2.0 - height / 2.0;
            draw.line(
                Point {
                    x: float(x).trunc(),
                    y: float(y).trunc(),
                },
                Point {
                    x: float(x).trunc(),
                    y: float(y + height).trunc(),
                },
                1.0,
                WHITE,
            )?;
        }
        draw.scissor(None)?;
        Ok(RenderResult::None)
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.text.is_empty() {
            self.set_cursor(0, draw, frame.monotonic);
            return Ok(());
        }
        let display = self.display_text(frame.monotonic);
        let chars: Vec<_> = display.chars().collect();
        let x = f64::from(position.x) - f64::from(self.state.rect.x) - 10.0 + self.offset;
        let mut best = 0;
        let mut distance = f64::INFINITY;
        for index in 0..=self.text.len() {
            let value: String = chars[..index.min(chars.len())].iter().collect();
            let width = f64::from(text::measure(draw, Font::Normal, &value, self.font_size, 0.0).x);
            let delta = (x - width).abs();
            if delta < distance {
                distance = delta;
                best = index;
            }
        }
        self.set_cursor(best, draw, frame.monotonic);
        Ok(())
    }
}
