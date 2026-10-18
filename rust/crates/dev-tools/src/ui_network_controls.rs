//! Network-owned controls: per-release dispatch and source button styles.
use super::model::field;
use crate::ui_forms_reference::controls as c;
use crate::{
    control_tools_policy_check::json,
    seal::Result,
    ui_forms_input::{python_truth, Canvas, Rect},
};
use openpilot_log_format::{JsonValue, JsonView};
pub const SCALE: f64 = 1.242;
pub fn numeric(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Bool(v) => Ok(f64::from(u8::from(v))),
        JsonView::Integer(v) => {
            let n = v.parse::<f64>()?;
            if n.is_finite() {
                Ok(n)
            } else {
                Err("network integer C float overflow".into())
            }
        }
        JsonView::Float(v) => Ok(v),
        _ => Err("network numeric value required".into()),
    }
}
#[derive(Clone)]
pub struct Event {
    pub original: JsonValue,
    pub x: JsonValue,
    pub y: JsonValue,
    pub slot: JsonValue,
    pub pressed: bool,
    pub released: bool,
    pub down: bool,
    pub time: JsonValue,
}
impl Event {
    pub fn parse(v: JsonValue) -> Result<Self> {
        let pos = field(&v, "pos")?;
        Ok(Self {
            x: field(&pos, "x")?,
            y: field(&pos, "y")?,
            slot: field(&v, "slot")?,
            pressed: python_truth(&field(&v, "pressed")?),
            released: python_truth(&field(&v, "released")?),
            down: python_truth(&field(&v, "down")?),
            time: field(&v, "time")?,
            original: v,
        })
    }
    pub fn primary(&self) -> bool {
        json::equal(&self.slot, &JsonValue::parse("0").unwrap())
    }
    pub fn position(&self) -> Result<(f64, f64)> {
        Ok((
            f64::from(numeric(&self.x)? as f32),
            f64::from(numeric(&self.y)? as f32),
        ))
    }
    fn index(&self) -> Result<usize> {
        let index = match self.slot.view() {
            JsonView::Bool(v) => i64::from(u8::from(v)),
            JsonView::Integer(v) => v.parse::<i64>()?,
            _ => return Err("network touch index is not integer".into()),
        };
        match index {
            -2 | 0 => Ok(0),
            -1 | 1 => Ok(1),
            _ => Err("network touch index out of range".into()),
        }
    }
}
pub fn inside(rect: Rect, x: f64, y: f64) -> bool {
    x >= rect.x
        && x <= f64::from(rect.x as f32 + rect.width as f32)
        && y >= rect.y
        && y <= f64::from(rect.y as f32 + rect.height as f32)
}
#[derive(Default)]
pub struct Touch {
    pressed: [bool; 2],
    tracking: [bool; 2],
    pub multi: bool,
}
impl Touch {
    pub fn pressed(&self) -> bool {
        self.pressed.iter().any(|v| *v)
    }
    pub fn reset(&mut self) {
        self.pressed = [false; 2];
        self.tracking = [false; 2];
    }
    pub fn update(&mut self, rect: Rect, events: &[Event], valid: bool) -> Result<Vec<usize>> {
        let mut released = Vec::new();
        for (at, e) in events.iter().enumerate() {
            if !self.multi && !e.primary() {
                continue;
            }
            let (x, y) = e.position()?;
            let hit = inside(rect, x, y);
            if e.pressed && valid {
                if hit {
                    let index = e.index()?;
                    self.pressed[index] = true;
                    self.tracking[index] = true;
                }
            } else if !valid {
                let index = e.index()?;
                self.pressed[index] = false;
                self.tracking[index] = false;
            } else if e.released {
                let index = e.index()?;
                if self.pressed[index] && hit {
                    released.push(at)
                }
                self.pressed[index] = false;
                self.tracking[index] = false;
            } else if hit {
                let index = e.index()?;
                if self.tracking[index] {
                    self.pressed[index] = true;
                }
            } else {
                let index = e.index()?;
                self.pressed[index] = false;
            }
        }
        Ok(released)
    }
}
pub struct Button {
    pub text: Vec<u32>,
    pub style: u8,
    pub size: f64,
    pub font: u8,
    pub padding: f64,
    pub align: u8,
    pub radius: f64,
    pub enabled: bool,
    pub touch: Touch,
    label_touch: Touch,
}
impl Button {
    pub fn new(text: &str, style: u8) -> Self {
        Self {
            text: text.chars().map(u32::from).collect(),
            style,
            size: 60.,
            font: 2,
            padding: 20.,
            align: 1,
            radius: 10.,
            enabled: true,
            touch: Touch::default(),
            label_touch: Touch::default(),
        }
    }
    pub fn paint(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        events: &[Event],
        hit: Rect,
        valid: bool,
        icon: Option<(usize, (f64, f64))>,
    ) -> Result<Vec<usize>> {
        let (bg, fg) = if !self.enabled {
            if self.style == 9 {
                (c::color(0, 0, 0, 0), c::color(255, 255, 255, 255))
            } else {
                (c::color(51, 51, 51, 255), c::color(228, 228, 228, 51))
            }
        } else {
            let press = self.touch.pressed();
            let (bg, fg) = match self.style {
                1 => (
                    if press {
                        (48, 73, 244, 255)
                    } else {
                        (70, 91, 234, 255)
                    },
                    (228, 228, 228, 255),
                ),
                3 => ((0, 0, 0, 255), (0, 0, 0, 255)),
                5 => (
                    if press {
                        (74, 74, 74, 74)
                    } else {
                        (57, 57, 57, 255)
                    },
                    (228, 228, 228, 255),
                ),
                7 => (
                    if press {
                        (51, 51, 51, 255)
                    } else {
                        (68, 68, 68, 255)
                    },
                    (221, 221, 221, 255),
                ),
                8 => (
                    if press {
                        (130, 130, 130, 255)
                    } else {
                        (189, 189, 189, 255)
                    },
                    (51, 51, 51, 255),
                ),
                9 => ((0, 0, 0, 0), (255, 255, 255, 255)),
                _ => (
                    if press {
                        (74, 74, 74, 255)
                    } else {
                        (51, 51, 51, 255)
                    },
                    (228, 228, 228, 255),
                ),
            };
            (
                c::color(bg.0, bg.1, bg.2, bg.3),
                c::color(fg.0, fg.1, fg.2, fg.3),
            )
        };
        api.call(&format!(
            "rounded {} {} {} {} {} 10 {bg}",
            r.x,
            r.y,
            r.width,
            r.height,
            self.radius / (r.width.min(r.height) / 2.)
        ))?;
        if let Some((id, (w, h))) = icon {
            api.call(&format!(
                "texture {id} {} {} 1 4294967295",
                r.x + (r.width - w) / 2.,
                r.y + (r.height - h) / 2.
            ))?;
        } else {
            c::font(api, self.font)?;
            c::label_codepoints(
                api,
                &self.text,
                r,
                c::LabelStyle {
                    size: self.size,
                    scale: SCALE,
                    padding: self.padding,
                    align: self.align,
                    rgba: fg,
                    elide: false,
                },
            )?;
        }
        // Label is a child Widget and observes primary events before its button.
        self.label_touch.update(r, events, true)?;
        c::debug_rect(api, r)?;
        if self.enabled {
            self.touch.update(hit, events, valid)
        } else {
            self.touch.reset();
            Ok(Vec::new())
        }
    }
}
pub fn label(
    api: &mut dyn Canvas,
    text: &JsonValue,
    r: Rect,
    size: f64,
    font: u8,
    align: u8,
    color: u32,
) -> Result<()> {
    let points = match text.view() {
        JsonView::Text(v) => v.to_vec(),
        _ if !python_truth(text) => vec![],
        _ => return Err("network label requires text".into()),
    };
    c::font(api, font)?;
    c::label_codepoints(
        api,
        &points,
        r,
        c::LabelStyle {
            size,
            scale: SCALE,
            padding: 0.,
            align,
            rgba: color,
            elide: false,
        },
    )
}
pub fn gui_label(
    api: &mut dyn Canvas,
    text: &str,
    r: Rect,
    size: f64,
    align: u8,
    color: u32,
) -> Result<()> {
    c::font(api, 0)?;
    c::gui_label_codepoints(
        api,
        &text.chars().map(u32::from).collect::<Vec<_>>(),
        r,
        size,
        SCALE,
        align,
        color,
    )
}
