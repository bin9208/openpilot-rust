use crate::{
    assets::Texture,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self as text, float, Horizontal, Vertical},
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};

pub struct Label {
    pub state: WidgetState,
    pub text: Property<String>,
    pub font: Font,
    pub size: f64,
    pub horizontal: Horizontal,
    pub vertical: Vertical,
    pub padding: f64,
    pub color: u32,
    pub icon: Option<Texture>,
    pub elide: bool,
    pub line_scale: f64,
}
impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            state: WidgetState::default(),
            text: Property::Value(text.into()),
            font: Font::Normal,
            size: 60.0,
            horizontal: Horizontal::Center,
            vertical: Vertical::Middle,
            padding: 0.0,
            color: u32::from_le_bytes([255, 255, 255, 229]),
            icon: None,
            elide: false,
            line_scale: 1.0,
        }
    }
}
impl Widget for Label {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let value = self.text.get();
        let rect = self.state.rect;
        let width = f64::from(rect.width) - self.padding * 2.0;
        let lines = if self.elide {
            vec![text::elide(
                draw,
                self.font,
                &value,
                self.size,
                0.0,
                width - self.icon.map_or(0.0, |icon| f64::from(icon.width) + 15.0),
                false,
            )]
        } else {
            text::wrap(
                draw,
                self.font,
                &value,
                self.size,
                0.0,
                width.round_ties_even(),
            )
        };
        let sizes: Vec<_> = lines
            .iter()
            .map(|line| text::measure(draw, self.font, line, self.size, 0.0))
            .collect();
        let first = sizes.first().copied().unwrap_or_default();
        let sum: f64 = sizes.iter().map(|size| f64::from(size.y)).sum();
        let total = if sum == 0.0 {
            self.size * draw.font_scale()
        } else {
            sum
        };
        let mut position = Point {
            x: rect.x,
            y: if self.vertical == Vertical::Middle {
                float(f64::from(rect.y) + ((f64::from(rect.height) - total) / 2.0).floor())
            } else {
                rect.y
            },
        };
        if let Some(icon) = self.icon {
            let y = rect.y + (rect.height - icon.height) / 2.0;
            let x = if lines.is_empty() {
                rect.x + (rect.width - icon.width) / 2.0
            } else {
                match self.horizontal {
                    Horizontal::Left => {
                        position.x = icon.width + 15.0;
                        rect.x + float(self.padding)
                    }
                    Horizontal::Center => {
                        position.x = icon.width + 15.0;
                        rect.x + (rect.width - icon.width - 15.0 - first.x) / 2.0
                    }
                    Horizontal::Right => {
                        rect.x + rect.width - first.x - float(self.padding) - 15.0 - icon.width
                    }
                }
            };
            icon.draw(draw, Point { x, y }, 1.0, WHITE)?;
        }
        for (line, size) in lines.iter().zip(sizes) {
            let x = position.x
                + match self.horizontal {
                    Horizontal::Left => float(self.padding),
                    Horizontal::Center => ((rect.width - size.x) / 2.0).floor(),
                    Horizontal::Right => rect.width - size.x - float(self.padding),
                };
            text::draw_emoji_line(
                draw,
                self.font,
                line,
                Point { x, y: position.y },
                self.size,
                0.0,
                self.color,
            )?;
            position.y += float(
                if size.y == 0.0 {
                    self.size * draw.font_scale()
                } else {
                    f64::from(size.y)
                } * self.line_scale,
            );
        }
        Ok(RenderResult::None)
    }
}

pub fn gui_label(
    draw: &mut dyn Draw,
    rect: Rect,
    value: &str,
    style: text::TextStyle,
    alignment: (Horizontal, Vertical),
    elide: bool,
) -> Result<(), Error> {
    let text::TextStyle {
        font, size, color, ..
    } = style;
    let value = if elide {
        text::elide(draw, font, value, size, 0.0, f64::from(rect.width), false)
    } else {
        value.to_owned()
    };
    let measured = text::measure(draw, font, &value, size, 0.0);
    let x = rect.x
        + match alignment.0 {
            Horizontal::Left => 0.0,
            Horizontal::Center => (rect.width - measured.x) / 2.0,
            Horizontal::Right => rect.width - measured.x,
        };
    let y = rect.y
        + match alignment.1 {
            Vertical::Top => 0.0,
            Vertical::Middle => (rect.height - measured.y) / 2.0,
            Vertical::Bottom => rect.height - measured.y,
        };
    text::draw_text(draw, font, &value, Point { x, y }, size, 0.0, color)
}
