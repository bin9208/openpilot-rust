//! Independent original four-layout keyboard and InputBox integration.
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Input, InputFrame, Rect},
    ui_forms_reference::controls::{self as c, Button, LabelStyle, Touch},
};
use std::{collections::BTreeMap, path::Path};
fn layout(name: &str) -> Vec<Vec<String>> {
    let rows: Vec<Vec<&str>> = match name {
        "lowercase" => vec![
            vec!["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
            vec!["a", "s", "d", "f", "g", "h", "j", "k", "l"],
            vec!["SHIFT_OFF", "z", "x", "c", "v", "b", "n", "m", "<-"],
            vec!["123", "/", "-", " ", ".", "->"],
        ],
        "uppercase" => vec![
            vec!["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"],
            vec!["A", "S", "D", "F", "G", "H", "J", "K", "L"],
            vec!["SHIFT_ON", "Z", "X", "C", "V", "B", "N", "M", "<-"],
            vec!["123", "/", "-", " ", ".", "->"],
        ],
        "numbers" => vec![
            vec!["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
            vec!["-", "/", ":", ";", "(", ")", "$", "&", "@", "\""],
            vec!["#+=", "_", ",", "?", "!", "`", "<-"],
            vec!["ABC", " ", ".", "->"],
        ],
        _ => vec![
            vec!["[", "]", "{", "}", "#", "%", "^", "*", "+", "="],
            vec!["_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"],
            vec!["123", "-", ",", "?", "!", "'", "<-"],
            vec!["ABC", " ", ".", "->"],
        ],
    };
    rows.into_iter()
        .map(|r| r.into_iter().map(str::to_owned).collect())
        .collect()
}
fn icon(key: &str) -> Option<usize> {
    Some(match key {
        "<-" => 2,
        "SHIFT_OFF" => 3,
        "SHIFT_ON" => 4,
        "CAPS" => 5,
        "->" => 6,
        _ => return None,
    })
}
pub struct Keyboard {
    input: Input,
    textures: Vec<(f64, f64)>,
    pub layout: String,
    caps: bool,
    last_shift: f64,
    password: bool,
    toggle: bool,
    buttons: BTreeMap<String, Button>,
    icon_touches: BTreeMap<String, Touch>,
    cancel: Button,
    eye: Touch,
    back_pressed: bool,
    back_time: f64,
    back_repeat: f64,
}
impl Keyboard {
    pub fn new(
        root: &Path,
        text: &str,
        p: &serde_json::Value,
        api: &mut dyn Canvas,
        scale: f64,
    ) -> Result<Self> {
        let mut textures = Vec::new();
        for (i, (name, w, h)) in [
            ("eye_open.png", 81, 54),
            ("eye_closed.png", 81, 54),
            ("backspace.png", 80, 80),
            ("shift.png", 80, 80),
            ("shift-fill.png", 80, 80),
            ("capslock-fill.png", 80, 80),
            ("arrow-right.png", 80, 80),
        ]
        .into_iter()
        .enumerate()
        {
            textures.push(super::assets::load(
                api,
                root,
                i,
                &format!("icons/{name}"),
                f64::from(w),
                f64::from(h),
                true,
            )?);
        }
        let mut input = Input::new(255, false);
        input.set_text(text, api, 0.0, scale)?;
        let mut buttons = BTreeMap::new();
        let mut touches = BTreeMap::new();
        for name in ["lowercase", "uppercase", "numbers", "specials"] {
            for row in layout(name) {
                for key in row {
                    if icon(&key).is_some() {
                        touches.insert(key, Touch::multi_touch());
                    } else {
                        let mut b = Button::new(&key, false);
                        b.multi_touch();
                        b.size = 85.0;
                        b.style = 7;
                        buttons.insert(key, b);
                    }
                }
            }
        }
        touches.insert("CAPS".into(), Touch::multi_touch());
        Ok(Self {
            input,
            textures,
            layout: "lowercase".into(),
            caps: false,
            last_shift: 0.0,
            password: p["password"] == true,
            toggle: p["toggle"] == true,
            buttons,
            icon_touches: touches,
            cancel: Button::new("Cancel", false),
            eye: Touch::default(),
            back_pressed: false,
            back_time: 0.0,
            back_repeat: 0.0,
        })
    }
    pub fn set_value(
        &mut self,
        value: openpilot_log_format::JsonValue,
        api: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        c::font(api, 0)?;
        self.input.set_value(value, api, now, scale)
    }
    pub fn clear(&mut self) {
        self.layout = "lowercase".into();
        self.caps = false;
        self.input.clear();
        self.back_pressed = false;
    }
    pub fn key(&mut self, key: &str, api: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        c::font(api, 0)?;
        match key {
            "CAPS" | "ABC" => {
                self.caps = false;
                self.layout = "lowercase".into();
            }
            "SHIFT_OFF" => {
                self.last_shift = now;
                self.layout = "uppercase".into();
            }
            "SHIFT_ON" => {
                if now - self.last_shift < 0.5 {
                    self.caps = true;
                } else {
                    self.layout = "lowercase".into();
                }
            }
            "123" => self.layout = "numbers".into(),
            "#+=" => self.layout = "specials".into(),
            "<-" => self.input.backspace(api, now, scale)?,
            _ => {
                self.input.add(key, api, now, scale)?;
                if !self.caps && self.layout == "uppercase" {
                    self.layout = "lowercase".into();
                }
            }
        }
        Ok(())
    }
    pub fn key_value(
        &mut self,
        value: openpilot_log_format::JsonValue,
        api: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        if let openpilot_log_format::JsonView::Text(points) = value.view() {
            return self.key_codepoints(points, api, now, scale);
        }
        c::font(api, 0)?;
        self.input.add_value(value, api, now, scale)?;
        if !self.caps && self.layout == "uppercase" {
            self.layout = "lowercase".into();
        }
        Ok(())
    }
    pub fn key_codepoints(
        &mut self,
        key: &[u32],
        api: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        // Dispatch names are ASCII. Unknown Python strings are inserted as-is,
        // including surrogate codepoints; Input owns the later encoding boundary.
        if let Ok(text) = Input::utf8(key) {
            return self.key(&text, api, now, scale);
        }
        c::font(api, 0)?;
        self.input.add_codepoints(key, api, now, scale)?;
        if !self.caps && self.layout == "uppercase" {
            self.layout = "lowercase".into();
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        now: f64,
        keys: &[i64],
        down: &[i64],
        chars: &[u32],
        events: &[serde_json::Value],
    ) -> Result<usize> {
        let rect = Rect {
            x: rect.x + 50.0,
            y: rect.y + 50.0,
            width: rect.width - 100.0,
            height: rect.height - 100.0,
        }
        .cast();
        c::font(api, 1)?;
        c::label(
            api,
            "Keyboard Input",
            Rect {
                height: 95.0,
                ..rect
            },
            LabelStyle {
                size: 90.0,
                scale,
                padding: 20.0,
                align: 0,
                rgba: c::color(255, 255, 255, 229),
                elide: false,
            },
        )?;
        c::font(api, 0)?;
        c::label(
            api,
            "Type your text below",
            Rect {
                y: rect.y + 95.0,
                height: 60.0,
                ..rect
            }
            .cast(),
            LabelStyle {
                size: 55.0,
                scale,
                padding: 20.0,
                align: 0,
                rgba: c::color(255, 255, 255, 229),
                elide: false,
            },
        )?;
        let mut pops = 0;
        let cancel_count = self.cancel.render_count(
            api,
            Rect {
                x: rect.x + rect.width - 386.0,
                y: rect.y,
                width: 386.0,
                height: 125.0,
            }
            .cast(),
            scale,
            events,
        )?;
        for _ in 0..cancel_count {
            self.clear();
            pops += 1;
        }
        let input = Rect {
            x: rect.x + 25.0,
            y: rect.y + 160.0,
            width: rect.width - 25.0,
            height: 100.0,
        }
        .cast();
        c::font(api, 0)?;
        self.input.password(self.toggle && self.password);
        self.input.frame(
            api,
            InputFrame {
                rect: if self.toggle {
                    Rect {
                        width: input.width - 100.0,
                        ..input
                    }
                    .cast()
                } else {
                    input
                },
                now,
                scale,
                key: keys.first().copied().unwrap_or(0),
                down,
                character: chars.first().copied(),
                events,
                debug: c::debug_enabled(),
            },
        )?;
        if self.toggle {
            let eye = Rect {
                x: input.x + input.width - 90.0,
                y: input.y,
                width: 80.0,
                height: input.height,
            }
            .cast();
            api.call(&format!(
                "rounded {} {} {} {} {} 10 4278190080",
                eye.x,
                eye.y,
                eye.width,
                eye.height,
                10.0 / (eye.width.min(eye.height) / 2.0)
            ))?;
            let id = if self.password { 1 } else { 0 };
            for _ in self.eye.releases(eye, events)? {
                self.password = !self.password;
            }
            c::debug_rect(api, eye)?;
            c::debug_rect(api, eye)?;
            let (eye_w, eye_h) = self.textures[id];
            api.call(&format!(
                "texture {id} {} {} 1 4294967295",
                eye.x + (eye.width - eye_w) / 2.0,
                eye.y + (eye.height - eye_h) / 2.0
            ))?;
        }
        api.call(&format!(
            "line-ex {} {} {} {} 3 {}",
            input.x,
            input.y + input.height - 2.0,
            input.x + input.width,
            input.y + input.height - 2.0,
            c::color(189, 189, 189, 255)
        ))?;
        if !self.icon_touches["<-"].pressed() {
            self.back_pressed = false;
        }
        if self.back_pressed && now - self.back_time > 0.5 && now - self.back_repeat > 0.07 {
            self.input.backspace(api, now, scale)?;
            self.back_repeat = now;
        }
        let rows = layout(&self.layout);
        let height = (rect.height - 345.0) / 4.0;
        let max = (rect.width - (rows[2].len() - 1) as f64 * 15.0) / rows[2].len() as f64;
        for (row, values) in rows.iter().enumerate() {
            let width = ((rect.width
                - if row == 1 { 180.0 } else { 0.0 }
                - 15.0 * (values.len() - 1) as f64)
                / values.len() as f64)
                .min(max);
            let mut x = rect.x + if row == 1 { 90.0 } else { 0.0 };
            for (i, key) in values.iter().enumerate() {
                if i > 0 {
                    x += 15.0;
                }
                let w = if key == " " {
                    width * 3.0 + 30.0
                } else if key == "->" {
                    width * 2.0 + 15.0
                } else {
                    width
                };
                let r = Rect {
                    x,
                    y: rect.y + 300.0 + row as f64 * (height + 15.0),
                    width: w,
                    height,
                }
                .cast();
                x += w;
                let enabled = key != "->" || self.input.text_len() >= 8;
                if key == "<-" && self.icon_touches["<-"].pressed() && !self.back_pressed {
                    self.back_pressed = true;
                    self.back_time = now;
                    self.back_repeat = now;
                }
                let actual = if key == "SHIFT_ON" && self.caps {
                    "CAPS"
                } else {
                    key
                };
                let clicked = if let Some(id) = icon(actual) {
                    let touch = self.icon_touches.get_mut(actual).unwrap();
                    let bg = if !enabled {
                        c::color(51, 51, 51, 255)
                    } else if key == "->" {
                        if touch.pressed() {
                            c::color(48, 73, 244, 255)
                        } else {
                            c::color(70, 91, 234, 255)
                        }
                    } else if touch.pressed() {
                        c::color(51, 51, 51, 255)
                    } else {
                        c::color(68, 68, 68, 255)
                    };
                    api.call(&format!(
                        "rounded {} {} {} {} {} 10 {bg}",
                        r.x,
                        r.y,
                        r.width,
                        r.height,
                        10.0 / (r.width.min(r.height) / 2.0)
                    ))?;
                    let (icon_w, icon_h) = self.textures[id];
                    api.call(&format!(
                        "texture {id} {} {} 1 4294967295",
                        r.x + (r.width - icon_w) / 2.0,
                        r.y + (r.height - icon_h) / 2.0
                    ))?;
                    c::debug_rect(api, r)?;
                    c::debug_rect(api, r)?;
                    if enabled {
                        touch.releases(r, events)?.len()
                    } else {
                        0
                    }
                } else {
                    let b = self.buttons.get_mut(actual).unwrap();
                    b.enabled = enabled;
                    b.render_count(api, r, scale, events)?
                };
                for _ in 0..clicked {
                    if key == "->" {
                        pops += 1;
                    } else {
                        self.key(actual, api, now, scale)?;
                    }
                }
            }
        }
        Ok(pops)
    }
    pub fn snapshot_json(&self, now: f64) -> Result<openpilot_log_format::JsonValue> {
        use openpilot_log_format::JsonValue;
        crate::ui_forms_input::snapshot_object(&[
            ("text", self.input.text_json()?),
            ("layout", JsonValue::text(&self.layout)),
            (
                "caps",
                JsonValue::parse(if self.caps { "true" } else { "false" })?,
            ),
            ("cursor", JsonValue::parse(&self.input.cursor.to_string())?),
            ("display", self.input.display_json(now)?),
        ])
    }
}
