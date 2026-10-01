use crate::{
    draw::Draw,
    emoji::{self, Span},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self as text, float, Horizontal, Vertical},
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollState {
    #[default]
    Starting,
    Scrolling,
}
#[derive(PartialEq)]
struct CacheKey {
    text: String,
    width: f64,
    font: Font,
    size: f64,
    spacing: f64,
    padding: f64,
    line_height: f64,
    wrap: bool,
    elide: bool,
    scroll: bool,
}
#[derive(Clone)]
struct Line {
    text: String,
    size: Point,
    emojis: Vec<Span>,
}
pub struct UnifiedLabel {
    pub state: WidgetState,
    pub text: Property<String>,
    pub font: Font,
    pub size: f64,
    pub color: u32,
    pub horizontal: Horizontal,
    pub vertical: Vertical,
    pub padding: f64,
    pub max_width: Option<f64>,
    pub elide: bool,
    pub wrap: bool,
    pub scroll: bool,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub shimmer: bool,
    pub shimmer_start: f64,
    pub scroll_offset: f64,
    pub scroll_state: ScrollState,
    scroll_pause: Option<f64>,
    needs_scroll: bool,
    cache: Option<CacheKey>,
    lines: Vec<Line>,
    total_height: f64,
}
impl UnifiedLabel {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            state: WidgetState::default(),
            text: Property::Value(value.into()),
            font: Font::Normal,
            size: 60.0,
            color: u32::from_le_bytes([255, 255, 255, 229]),
            horizontal: Horizontal::Left,
            vertical: Vertical::Top,
            padding: 0.0,
            max_width: None,
            elide: true,
            wrap: true,
            scroll: false,
            line_height: 1.0,
            letter_spacing: 0.0,
            shimmer: false,
            shimmer_start: 0.0,
            scroll_offset: 0.0,
            scroll_state: ScrollState::Starting,
            scroll_pause: None,
            needs_scroll: false,
            cache: None,
            lines: Vec::new(),
            total_height: 0.0,
        }
    }
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0.0;
        self.scroll_pause = None;
        self.scroll_state = ScrollState::Starting;
    }
    pub fn reset_shimmer(&mut self, now: f64, offset: f64) {
        self.shimmer_start = now + offset;
    }
    pub fn text_width(&self) -> f64 {
        self.lines
            .iter()
            .map(|line| f64::from(line.size.x))
            .fold(0.0, f64::max)
    }
    pub fn content_height(&mut self, draw: &dyn Draw, max_width: f64) -> f64 {
        let width = if max_width > 0.0 {
            max_width
        } else {
            self.max_width
                .filter(|value| *value != 0.0)
                .unwrap_or(1000.0)
        };
        self.update_cache(draw, width);
        self.total_height
    }
    pub fn set_max_width(&mut self, draw: &dyn Draw, width: Option<f64>) {
        if self.max_width != width {
            self.max_width = width;
            self.cache = None;
            if let Some(width) = width {
                self.state.rect.width = float(width);
                self.state.rect.height = float(self.content_height(draw, width));
            }
        }
    }
    fn update_cache(&mut self, draw: &dyn Draw, width: f64) {
        let value = self.text.get();
        let spacing = self.size * self.letter_spacing;
        let key = CacheKey {
            text: value.clone(),
            width,
            font: self.font,
            size: self.size,
            spacing,
            padding: self.padding,
            line_height: self.line_height,
            wrap: self.wrap,
            elide: self.elide,
            scroll: self.scroll,
        };
        if self.cache.as_ref() == Some(&key) && !self.lines.is_empty() {
            return;
        }
        let content = (width - self.padding * 2.0).max(1.0);
        let lines = if self.wrap && !self.scroll {
            text::wrap(draw, self.font, &value, self.size, spacing, content)
        } else {
            value.split('\n').map(str::to_owned).collect()
        };
        let mut lines: Vec<_> = lines
            .into_iter()
            .map(|line| {
                if self.elide && !self.scroll {
                    text::elide(draw, self.font, &line, self.size, spacing, content, false)
                } else {
                    line
                }
            })
            .collect();
        if self.scroll {
            lines.truncate(1);
        }
        self.lines = lines
            .into_iter()
            .map(|value| {
                let size = if value.is_empty() {
                    Point {
                        x: 0.0,
                        y: float(self.size * draw.font_scale()),
                    }
                } else {
                    text::measure(draw, self.font, &value, self.size, spacing)
                };
                let emojis = emoji::spans(&value);
                Line {
                    text: value,
                    size,
                    emojis,
                }
            })
            .collect();
        if self.scroll {
            self.needs_scroll = self
                .lines
                .first()
                .is_some_and(|line| f64::from(line.size.x) > content);
        }
        self.total_height = self
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                f64::from(line.size.y)
                    * if index == 0 {
                        1.0
                    } else {
                        self.line_height * 0.9
                    }
            })
            .sum();
        self.cache = Some(key);
    }
    pub fn shimmer_alpha(&self, now: f64, x: f64, left: f64, width: f64) -> f64 {
        let sigma = width * 0.12;
        if sigma <= 0.0 {
            return 0.65;
        }
        let raw = (now - self.shimmer_start).rem_euclid(2.5) / 2.5;
        let clamped = (raw / 0.9).clamp(0.0, 1.0);
        let t = clamped * clamped * (3.0 - 2.0 * clamped);
        let margin = width * 0.3;
        let center = left + width + margin - t * (width + 2.0 * margin);
        let d = x - center;
        0.65 + 0.35 * (-0.5 * d * d / (sigma * sigma)).exp()
    }
    fn line(
        &self,
        draw: &mut dyn Draw,
        line: &Line,
        y: f64,
        offset: f64,
        now: f64,
    ) -> Result<(), Error> {
        let rect = self.state.rect;
        let x = f64::from(rect.x)
            + match self.horizontal {
                Horizontal::Left => self.padding,
                Horizontal::Center => (f64::from(rect.width) - f64::from(line.size.x)) / 2.0,
                Horizontal::Right => f64::from(rect.width) - f64::from(line.size.x) - self.padding,
            }
            + self.scroll_offset
            + offset;
        if self.shimmer {
            let width = self.text_width();
            let left = f64::from(rect.x)
                + match self.horizontal {
                    Horizontal::Left => self.padding,
                    Horizontal::Center => (f64::from(rect.width) - width) / 2.0,
                    Horizontal::Right => f64::from(rect.width) - self.padding - width,
                };
            let mut cursor = x;
            let mut color = self.color.to_le_bytes();
            let base = f64::from(color[3]) / 255.0;
            for c in line.text.chars() {
                let value = c.to_string();
                let char_width = f64::from(
                    text::measure(
                        draw,
                        self.font,
                        &value,
                        self.size,
                        self.size * self.letter_spacing,
                    )
                    .x,
                );
                color[3] = (255.0
                    * self.shimmer_alpha(now, cursor + char_width / 2.0, left, width)
                    * base)
                    .to_u8()
                    .ok_or(Error::Contract("shimmer alpha out of range"))?;
                text::draw_text(
                    draw,
                    self.font,
                    &value,
                    Point {
                        x: float(cursor),
                        y: float(y),
                    },
                    self.size,
                    0.0,
                    u32::from_le_bytes(color),
                )?;
                cursor += char_width + self.size * self.letter_spacing;
            }
            Ok(())
        } else {
            text::draw_emoji_spans(
                draw,
                &line.text,
                &line.emojis,
                Point {
                    x: float(x),
                    y: float(y),
                },
                text::TextStyle {
                    font: self.font,
                    size: self.size,
                    spacing: self.size * self.letter_spacing,
                    color: self.color,
                },
            )
        }
    }
}
impl Widget for UnifiedLabel {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        if self.shimmer {
            self.reset_shimmer(frame.now, 0.0);
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(RenderResult::None);
        }
        let width = self
            .max_width
            .map_or(f64::from(rect.width), |max| max.min(f64::from(rect.width)));
        self.update_cache(draw, width.trunc());
        let mut visible = Vec::new();
        let mut height = 0.0;
        let mut broke = false;
        for line in &self.lines {
            let needed = f64::from(line.size.y) * self.line_height * 0.9;
            if height + needed > f64::from(rect.height) && !visible.is_empty() {
                broke = true;
                break;
            }
            visible.push(line.clone());
            height += needed;
        }
        if broke && self.elide && !self.scroll {
            if let Some(last) = visible.last_mut() {
                last.text = text::elide(
                    draw,
                    self.font,
                    &last.text,
                    self.size,
                    self.size * self.letter_spacing,
                    (width - self.padding * 2.0).trunc().max(1.0),
                    true,
                );
                last.size = text::measure(
                    draw,
                    self.font,
                    &last.text,
                    self.size,
                    self.size * self.letter_spacing,
                );
            }
        }
        if visible.is_empty() {
            return Ok(RenderResult::None);
        }
        let total: f64 = visible
            .iter()
            .enumerate()
            .map(|(index, line)| {
                f64::from(line.size.y)
                    * if index == 0 {
                        1.0
                    } else {
                        self.line_height * 0.9
                    }
            })
            .sum();
        let mut y = f64::from(rect.y)
            + match self.vertical {
                Vertical::Top => 0.0,
                Vertical::Middle => (f64::from(rect.height) - total) / 2.0,
                Vertical::Bottom => f64::from(rect.height) - total,
            };
        if self.needs_scroll {
            draw.scissor(Some(Rect {
                x: rect.x,
                y: float(f64::from(rect.y) - self.size / 2.0),
                width: rect.width,
                height: float(f64::from(rect.height) + self.size),
            }))?;
        }
        for line in &visible {
            if self.needs_scroll {
                match self.scroll_state {
                    ScrollState::Starting => {
                        let pause = *self.scroll_pause.get_or_insert(frame.now + 2.0);
                        if frame.now >= pause {
                            self.scroll_state = ScrollState::Scrolling;
                            self.scroll_pause = None;
                        }
                    }
                    ScrollState::Scrolling => {
                        self.scroll_offset -= 0.8 / 60.0 * frame.target_fps;
                        if self.scroll_offset
                            <= -f64::from(line.size.x) - f64::from(rect.width) / 3.0
                        {
                            self.reset_scroll();
                        }
                    }
                }
            } else {
                self.reset_scroll();
            }
            self.line(draw, line, y, 0.0, frame.now)?;
            if self.needs_scroll && self.scroll_state != ScrollState::Starting {
                self.line(
                    draw,
                    line,
                    y,
                    f64::from(line.size.x) + f64::from(rect.width) / 3.0,
                    frame.now,
                )?;
            }
            y += f64::from(line.size.y) * self.line_height * 0.9;
        }
        if self.needs_scroll {
            let black = u32::from_le_bytes([0, 0, 0, 255]);
            draw.gradient(
                Rect {
                    x: (rect.x + rect.width - 20.0).trunc(),
                    y: rect.y.trunc(),
                    width: 20.0,
                    height: rect.height.trunc(),
                },
                [0, 0, black, black],
            )?;
            if self.scroll_state != ScrollState::Starting
                && self.scroll_offset + f64::from(visible[0].size.x) > 0.0
            {
                draw.gradient(
                    Rect {
                        x: rect.x.trunc(),
                        y: rect.y.trunc(),
                        width: 20.0,
                        height: rect.height.trunc(),
                    },
                    [black, black, 0, 0],
                )?;
            }
            draw.scissor(None)?;
        }
        Ok(RenderResult::None)
    }
}
