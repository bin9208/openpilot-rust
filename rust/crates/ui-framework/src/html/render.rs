use super::*;
impl Widget for HtmlRenderer {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let mut y = f64::from(rect.y);
        let width = f64::from(rect.width) - 40.0;
        for element in &self.elements {
            if element.kind == ElementType::Br {
                y += element.bottom;
                continue;
            }
            y += element.top;
            if y > f64::from(rect.y + rect.height) {
                break;
            }
            for line in text::wrap(
                draw,
                element.font,
                &element.content,
                element.size,
                0.0,
                width.trunc(),
            ) {
                let step = element.size * draw.font_scale() * element.line_height;
                if y < f64::from(rect.y) - element.size * draw.font_scale() {
                    y += step;
                    continue;
                }
                if y > f64::from(rect.y + rect.height) {
                    break;
                }
                let x = if self.center {
                    f64::from(rect.x)
                        + (f64::from(rect.width)
                            - f64::from(
                                text::measure(draw, element.font, &line, element.size, 0.0).x,
                            ))
                            / 2.0
                } else {
                    f64::from(rect.x) + f64::from((element.indent - 1).max(0)) * 40.0
                };
                text::draw_text(
                    draw,
                    element.font,
                    &line,
                    Point {
                        x: float(x + 20.0),
                        y: float(y),
                    },
                    element.size,
                    0.0,
                    self.color,
                )?;
                y += step;
            }
            y += element.bottom;
        }
        Ok(RenderResult::Float(y - f64::from(rect.y)))
    }
}
