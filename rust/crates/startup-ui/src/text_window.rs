use crate::{
    config::Config,
    draw::{self, Draw, TextDraw},
    geometry::{MouseEvent, Point, Rect},
    scroll::Scroll,
    text::{self, Font, Measure},
    Error,
};
#[derive(Debug, serde::Serialize)]
pub struct TextWindow {
    pub lines: Vec<String>,
    pub scroll: Scroll,
    pub closed: bool,
    pressed: bool,
    tracking: bool,
}
impl TextWindow {
    pub fn new(text: &str, config: Config, measure: &impl Measure) -> Self {
        let t = config.text();
        let body = Self::body(config);
        let lines = text::wrap(text, config.scaled_font(t.font), body.width - 20.0, measure);
        let offset = -((crate::number::float(lines.len()) * t.line_height - body.height).max(0.0));
        let mut scroll = Scroll::default();
        scroll.offset = f64::from(offset);
        Self {
            lines,
            scroll,
            closed: false,
            pressed: false,
            tracking: false,
        }
    }
    pub fn body(config: Config) -> Rect {
        let t = config.text();
        let top = t.margin + t.ip_band;
        Rect {
            x: t.margin,
            y: top,
            width: config.width() - t.margin * 2.0,
            height: config.height() - top - t.margin,
        }
    }
    pub fn button(config: Config) -> Rect {
        let t = config.text();
        Rect {
            x: config.width() - t.margin - t.button_width - t.gap,
            y: config.height() - t.margin - t.button_height,
            width: t.button_width,
            height: t.button_height,
        }
    }
    pub fn render(
        &mut self,
        config: Config,
        ip: &str,
        events: &[MouseEvent],
        wheel: f32,
        draw: &mut impl Draw,
    ) -> Result<bool, Error> {
        let t = config.text();

        let ip_size = draw.measure(Font::NormalRaw, ip, t.ip_font, 1.0);
        draw.text(TextDraw {
            font: Font::Normal,
            text: ip,
            position: Point {
                x: config.width() / 2.0 - ip_size.x / 2.0,
                y: t.margin / 2.0,
            },
            size: config.scaled_font(t.ip_font),
            spacing: 1.0,
            color: draw::WHITE,
        })?;
        let body = Self::body(config);
        let scroll = self.scroll.update(
            body,
            crate::number::float(self.lines.len()) * t.line_height,
            events,
            wheel,
        );
        draw.scissor(Some(body))?;
        for (index, line) in self.lines.iter().enumerate() {
            let y = body.y + scroll + crate::number::float(index) * t.line_height;
            if y + t.line_height < body.y || y > body.y + body.height {
                continue;
            }
            draw.text(TextDraw {
                font: Font::Normal,
                text: line,
                position: Point { x: body.x, y },
                size: config.scaled_font(t.font),
                spacing: 0.0,
                color: draw::WHITE,
            })?;
        }
        draw.scissor(None)?;
        let button = Self::button(config);
        let roundness = 10.0 / (button.width.min(button.height) / 2.0);
        draw.rounded(button, roundness, draw::BLACK)?;
        draw.border(button, roundness, draw::WHITE)?;
        let label = if config.pc { "Exit" } else { "Reboot" };
        let mut labels = Vec::new();
        let mut remaining = label;
        while !remaining.is_empty() {
            let mut split = 1;
            for end in 1..=remaining.len() {
                if draw
                    .measure(
                        Font::Medium,
                        &remaining[..end],
                        config.scaled_font(t.font),
                        0.0,
                    )
                    .x
                    <= button.width - 40.0
                {
                    split = end;
                }
            }
            labels.push(&remaining[..split]);
            remaining = &remaining[split..];
        }
        let sizes: Vec<_> = labels
            .iter()
            .map(|label| draw.measure(Font::Medium, label, config.scaled_font(t.font), 0.0))
            .collect();
        let total_height: f32 = sizes.iter().map(|size| size.y).sum();
        let mut y = button.y + ((button.height - total_height) / 2.0).floor();
        for (label, size) in labels.iter().zip(sizes) {
            draw.text(TextDraw {
                font: Font::Medium,
                text: label,
                position: Point {
                    x: button.x + ((button.width - size.x) / 2.0).floor(),
                    y,
                },
                size: config.scaled_font(t.font),
                spacing: 0.0,
                color: draw::BUTTON_TEXT,
            })?;
            y += size.y;
        }
        let mut clicked = false;
        for event in events.iter().filter(|event| event.slot == 0) {
            let inside = button.contains(event.pos);
            if event.pressed {
                if inside {
                    self.pressed = true;
                    self.tracking = true;
                }
            } else if event.released {
                if self.pressed && inside {
                    clicked = true;
                    self.closed = true;
                }
                self.pressed = false;
                self.tracking = false;
            } else if inside {
                if self.tracking {
                    self.pressed = true;
                }
            } else {
                self.pressed = false;
            }
        }
        Ok(clicked)
    }
}
