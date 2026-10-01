use crate::{
    draw::{Draw, BLACK, WHITE},
    geometry::{Point, Rect},
    keys,
    text::Font,
    text_layout::{self as text, float},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct InputBox {
    pub state: WidgetState,
    pub max_length: usize,
    pub password: bool,
    pub font_size: f64,
    pub color: u32,
    pub text_color: u32,
    text: Vec<char>,
    cursor: usize,
    blink: u32,
    show_cursor: bool,
    last_key: i32,
    key_time: u32,
    offset: f64,
    visible_width: f64,
    last_char_time: f64,
    pending_offset: bool,
}
impl InputBox {
    pub fn new(max_length: usize, password: bool) -> Self {
        Self {
            state: WidgetState::default(),
            max_length,
            password,
            font_size: 80.0,
            color: BLACK,
            text_color: WHITE,
            text: Vec::new(),
            cursor: 0,
            blink: 0,
            show_cursor: false,
            last_key: 0,
            key_time: 0,
            offset: 0.0,
            visible_width: 0.0,
            last_char_time: 0.0,
            pending_offset: false,
        }
    }
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }
    pub fn len(&self) -> usize {
        self.text.len()
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn show_cursor(&self) -> bool {
        self.show_cursor
    }
    pub fn set_text(&mut self, value: &str, draw: &dyn Draw, now: f64) {
        self.text = value.chars().take(self.max_length).collect();
        self.cursor = self.text.len();
        self.update_offset(draw, now);
        self.pending_offset = false;
    }
    pub fn replace_text(&mut self, value: &str) {
        self.text = value.chars().take(self.max_length).collect();
        self.cursor = self.text.len();
        self.pending_offset = self.visible_width != 0.0;
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.offset = 0.0;
    }
    pub fn set_cursor(&mut self, position: usize, draw: &dyn Draw, now: f64) {
        if position <= self.text.len() {
            self.cursor = position;
            self.blink = 0;
            self.show_cursor = true;
            self.update_offset(draw, now);
        }
    }
    pub fn display_text(&self, now: f64) -> String {
        if !self.password {
            return self.text();
        }
        let mut masked = vec!['•'; self.text.len()];
        if now - self.last_char_time < 1.5 && !self.text.is_empty() {
            let last = self.cursor.saturating_sub(1);
            if last < self.text.len() {
                masked[last] = self.text[last];
            }
        }
        masked.iter().collect()
    }
    fn update_offset(&mut self, draw: &dyn Draw, now: f64) {
        if self.visible_width == 0.0 {
            return;
        }
        let display = self.display_text(now);
        let cursor = if self.cursor > 0 {
            f64::from(
                text::measure(
                    draw,
                    Font::Normal,
                    &display.chars().take(self.cursor).collect::<String>(),
                    self.font_size,
                    0.0,
                )
                .x,
            )
        } else {
            0.0
        };
        let width = self.visible_width - 20.0;
        if cursor < self.offset {
            self.offset = (cursor - 10.0).max(0.0);
        } else if cursor > self.offset + width {
            self.offset = cursor - width + 10.0;
        }
    }
    pub fn add(&mut self, value: &str, draw: &dyn Draw, now: f64) -> bool {
        if self.text.len() >= self.max_length {
            return false;
        }
        self.text.splice(self.cursor..self.cursor, value.chars());
        self.set_cursor(self.cursor + 1, draw, now);
        if self.password {
            self.last_char_time = now;
        }
        true
    }
    pub fn backspace(&mut self, draw: &dyn Draw, now: f64) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.text.remove(self.cursor - 1);
        self.set_cursor(self.cursor - 1, draw, now);
        true
    }
    pub fn delete(&mut self, draw: &dyn Draw, now: f64) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.text.remove(self.cursor);
        self.set_cursor(self.cursor, draw, now);
        true
    }
    pub fn key(&mut self, key: i32, draw: &dyn Draw, now: f64) {
        match key {
            keys::LEFT => {
                if self.cursor > 0 {
                    self.set_cursor(self.cursor - 1, draw, now);
                }
            }
            keys::RIGHT => {
                if self.cursor < self.text.len() {
                    self.set_cursor(self.cursor + 1, draw, now);
                }
            }
            keys::BACKSPACE => {
                self.backspace(draw, now);
            }
            keys::DELETE => {
                self.delete(draw, now);
            }
            keys::HOME => self.set_cursor(0, draw, now),
            keys::END => self.set_cursor(self.text.len(), draw, now),
            _ => {}
        }
    }
    fn keyboard(&mut self, frame: &Frame<'_>, draw: &dyn Draw) {
        let key = frame.keyboard.key();
        if key != 0 {
            self.key(key, draw, frame.monotonic);
            if matches!(
                key,
                keys::LEFT | keys::RIGHT | keys::BACKSPACE | keys::DELETE
            ) {
                self.last_key = key;
                self.key_time = 0;
            }
        } else if self.last_key != 0 {
            if frame.keyboard.down.contains(&self.last_key) {
                self.key_time = self.key_time.saturating_add(1);
                if self.key_time > 30 && self.key_time.is_multiple_of(4) {
                    self.key(self.last_key, draw, frame.monotonic);
                }
            } else {
                self.last_key = 0;
            }
        }
        if let Some(character) = frame.keyboard.character().filter(|c| u32::from(*c) >= 32) {
            self.add(&character.to_string(), draw, frame.monotonic);
        }
    }
}

mod render;
