//! Network-owned parameterized keyboard, using independent Forms text primitives.
use super::controls::{Button, Event, SCALE};
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Input, InputFrame, Rect},
    ui_forms_reference::{assets, controls as c},
};
use openpilot_log_format::JsonValue;
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
    layout: String,
    caps: bool,
    last_shift: f64,
    password: bool,
    title: String,
    subtitle: String,
    min: usize,
    buttons: BTreeMap<String, Button>,
    cancel: Button,
    eye: Button,
    back_pressed: bool,
    back_time: f64,
    back_repeat: f64,
}
impl Keyboard {
    pub fn new(root: &Path, api: &mut dyn Canvas, shared: Option<Vec<(f64, f64)>>) -> Result<Self> {
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
            textures.push(if let Some(ref shared) = shared {
                shared[i]
            } else {
                assets::load(
                    api,
                    root,
                    i,
                    &format!("icons/{name}"),
                    f64::from(w),
                    f64::from(h),
                    true,
                )?
            });
        }
        let mut buttons = BTreeMap::new();
        for name in ["lowercase", "uppercase", "numbers", "specials"] {
            for row in layout(name) {
                for key in row {
                    let mut b = Button::new(
                        if icon(&key).is_some() { "" } else { &key },
                        if key == "->" { 1 } else { 7 },
                    );
                    b.touch.multi = true;
                    b.size = 85.;
                    buttons.insert(key, b);
                }
            }
        }
        let mut caps = Button::new("", 7);
        caps.touch.multi = true;
        buttons.insert("CAPS".into(), caps);
        Ok(Self {
            input: Input::new(64, false),
            textures,
            layout: "lowercase".into(),
            caps: false,
            last_shift: 0.,
            password: false,
            title: String::new(),
            subtitle: String::new(),
            min: 8,
            buttons,
            cancel: Button::new("Cancel", 0),
            eye: Button::new("", 3),
            back_pressed: false,
            back_time: 0.,
            back_repeat: 0.,
        })
    }
    pub fn text(&self) -> Result<JsonValue> {
        self.input.text_json()
    }
    pub fn set_text(&mut self, value: JsonValue, api: &mut dyn Canvas, now: f64) -> Result<()> {
        c::font(api, 0)?;
        self.input.set_value(value, api, now, SCALE)
    }
    pub fn textures(&self) -> Vec<(f64, f64)> {
        self.textures.clone()
    }
    pub fn title(&mut self, title: String, subtitle: String) {
        self.title = title;
        self.subtitle = subtitle;
    }
    pub fn clear(&mut self) {
        self.layout = "lowercase".into();
        self.caps = false;
        self.input.clear();
        self.back_pressed = false;
    }
    pub fn reset(&mut self, min: usize) {
        self.min = min;
        self.last_shift = 0.;
        self.back_time = 0.;
        self.back_repeat = 0.;
        self.clear();
    }
    fn key(&mut self, key: &str, api: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
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

    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        now: f64,
        chars: &[u32],
        events: &[Event],
    ) -> Result<Vec<bool>> {
        let rect = Rect {
            x: 50.,
            y: 50.,
            width: 2060.,
            height: 980.,
        };
        let style = |size| c::LabelStyle {
            size,
            scale: SCALE,
            padding: 20.,
            align: 0,
            rgba: c::color(255, 255, 255, 229),
            elide: false,
        };
        c::font(api, 1)?;
        c::label(
            api,
            &self.title,
            Rect {
                height: 95.,
                ..rect
            },
            style(90.),
        )?;
        c::font(api, 0)?;
        c::label(
            api,
            &self.subtitle,
            Rect {
                y: 145.,
                height: 60.,
                ..rect
            },
            style(55.),
        )?;
        let mut results = Vec::new();
        let cancelled = self
            .cancel
            .paint(
                api,
                Rect {
                    x: 1724.,
                    y: 50.,
                    width: 386.,
                    height: 125.,
                },
                events,
                Rect {
                    x: 1724.,
                    y: 50.,
                    width: 386.,
                    height: 125.,
                },
                true,
                None,
            )?
            .len();
        for _ in 0..cancelled {
            self.clear();
        }
        results.extend(std::iter::repeat_n(false, cancelled));
        let input = Rect {
            x: 75.,
            y: 210.,
            width: 2035.,
            height: 100.,
        };
        self.input.password(self.password);
        // InputBox consumes only the first queued character, exactly like original.
        let events_json = events
            .iter()
            .map(|e| serde_json::from_str(&e.original.to_json()?).map_err(Into::into))
            .collect::<Result<Vec<serde_json::Value>>>()?;
        self.input.frame(
            api,
            InputFrame {
                rect: Rect {
                    width: input.width - 100.,
                    ..input
                },
                now,
                scale: SCALE,
                key: 0,
                down: &[],
                character: chars.first().copied(),
                events: &events_json,
                debug: c::debug_enabled(),
            },
        )?;
        let eye = Rect {
            x: 2020.,
            y: 210.,
            width: 80.,
            height: 100.,
        };
        let id = usize::from(self.password);
        for _ in self.eye.paint(api, eye, events, eye, true, None)? {
            self.password = !self.password;
        }
        let (w, h) = self.textures[id];
        api.call(&format!(
            "texture {id} {} {} 1 4294967295",
            eye.x + (eye.width - w) / 2.,
            eye.y + (eye.height - h) / 2.
        ))?;
        api.call("line-ex 75 308 2110 308 3 4290624957")?;
        if !self.buttons["<-"].touch.pressed() {
            self.back_pressed = false;
        }
        if self.back_pressed && now - self.back_time > 0.5 && now - self.back_repeat > 0.07 {
            self.input.backspace(api, now, SCALE)?;
            self.back_repeat = now;
        }
        let rows = layout(&self.layout);
        let height = (rect.height - 345.) / 4.;
        let max = (rect.width - (rows[2].len() - 1) as f64 * 15.) / rows[2].len() as f64;
        for (row, values) in rows.iter().enumerate() {
            let width =
                ((rect.width - if row == 1 { 180. } else { 0. } - 15. * (values.len() - 1) as f64)
                    / values.len() as f64)
                    .min(max);
            let mut x = rect.x + if row == 1 { 90. } else { 0. };
            for (i, key) in values.iter().enumerate() {
                if i > 0 {
                    x += 15.;
                }
                let w = if key == " " {
                    width * 3. + 30.
                } else if key == "->" {
                    width * 2. + 15.
                } else {
                    width
                };
                let r = Rect {
                    x,
                    y: 350. + row as f64 * (height + 15.),
                    width: w,
                    height,
                }
                .cast();
                x += w;
                if key == "<-" && self.buttons["<-"].touch.pressed() && !self.back_pressed {
                    self.back_pressed = true;
                    self.back_time = now;
                    self.back_repeat = now;
                }
                let actual = if key == "SHIFT_ON" && self.caps {
                    "CAPS"
                } else {
                    key
                };
                let b = self.buttons.get_mut(actual).ok_or("network key missing")?;
                b.enabled = key != "->" || self.input.text_len() >= self.min;
                let icon = icon(actual).map(|id| (id, self.textures[id]));
                let releases = b.paint(api, r, events, r, true, icon)?;
                for _ in releases {
                    if key == "->" {
                        results.push(true);
                    } else {
                        self.key(actual, api, now, SCALE)?;
                    }
                }
            }
        }
        Ok(results)
    }
}
