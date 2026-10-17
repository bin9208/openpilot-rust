//! Independent original Mici keyboard key identity, filters and layout policy.
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas, Rect},
    ui_forms_reference::{assets, controls as c},
};
use serde_json::json;
use std::path::Path;
#[derive(Clone)]
struct Bounce {
    x: f64,
    v: f64,
    rc: f64,
}
impl Bounce {
    fn new(x: f64, rc: f64) -> Self {
        Self { x, v: 0.0, rc }
    }
    fn update(&mut self, target: f64) -> f64 {
        let a = 0.05 / (self.rc + 0.05);
        self.x = (1.0 - a) * self.x + a * target;
        self.v += (target - self.x) * 2.0 * 3.0 * 0.05;
        self.v *= 1.0 - 0.05 / (0.15 + 0.05);
        if self.v.abs() < 1e-3 {
            self.v = 0.0;
        }
        self.x += self.v;
        self.x
    }
}
#[derive(Clone)]
struct Key {
    value: String,
    kind: u8,
    rect: Rect,
    original: (f64, f64),
    x: Bounce,
    y: Bounce,
    size: Bounce,
    alpha: Bounce,
    initialized: bool,
}
impl Key {
    fn new(value: &str, kind: u8) -> Self {
        Self {
            value: value.into(),
            kind,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            original: (0.0, 0.0),
            x: Bounce::new(0.0, 0.065),
            y: Bounce::new(0.0, 0.065),
            size: Bounce::new(if kind == 1 { 24.0 } else { 42.0 }, 0.065),
            alpha: Bounce::new(1.0, 0.075 * 0.65),
            initialized: false,
        }
    }
    fn position(&mut self, x: f64, y: f64, parent: f64, smooth: bool) {
        let y = y - parent;
        if !self.initialized {
            self.x.x = x;
            self.y.x = y;
            self.original = (f64::from(x as f32), f64::from((y + 10.0) as f32));
            self.initialized = true;
        }
        if !smooth {
            self.x.x = x;
            self.y.x = y;
        }
        self.rect.x = f64::from(self.x.update(x) as f32);
        self.rect.y = f64::from((parent + self.y.update(y)) as f32);
    }
}
fn interp(x: f64, a: f64, b: f64, lo: f64, hi: f64) -> f64 {
    if x <= a {
        lo
    } else if x >= b {
        hi
    } else {
        lo + (x - a) / (b - a) * (hi - lo)
    }
}
pub struct Mici {
    keys: Vec<Key>,
    layers: Vec<Vec<Vec<usize>>>,
    layer: usize,
    caps: u8,
    space: usize,
    capskey: usize,
    numeric: usize,
    numeric2: usize,
    abc: usize,
    special: usize,
    closest: Option<usize>,
    distance: f64,
    pub selected: Option<f64>,
    pub unselect: Option<f64>,
    pub dragging: bool,
    touch_started: bool,
    release_started: bool,
    tracking: bool,
    pressed: bool,
    initialized: bool,
    text: std::rc::Rc<std::cell::RefCell<openpilot_log_format::JsonValue>>,
    auto_return: openpilot_log_format::JsonValue,
    bg: Bounce,
    selected_filter: f64,
    rect: Rect,
    images: Vec<(f64, f64)>,
}
impl Mici {
    pub fn new_codepoints(
        root: &Path,
        text: &[u32],
        auto: &str,
        api: &mut dyn Canvas,
    ) -> Result<Self> {
        let mut keys = vec![];
        let mut layers = vec![];
        for rows in [
            ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
            ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
            ["1234567890", "-/:;()$&@\"", "~.,?!'#%"],
            ["1234567890", "`[]{}^*+=_", "\\|<>¥€£•"],
        ] {
            let mut rows_out = vec![];
            for row in rows {
                let mut ids = vec![];
                for ch in row.chars() {
                    ids.push(keys.len());
                    keys.push(Key::new(&ch.to_string(), 0));
                }
                rows_out.push(ids);
            }
            layers.push(rows_out);
        }
        let mut control = |value: &str, kind: u8| {
            let id = keys.len();
            keys.push(Key::new(value, kind));
            id
        };
        let space = control(" ", 2);
        let capskey = control("", 3);
        let numeric = control("123", 1);
        let numeric2 = control("123", 1);
        let abc = control("abc", 1);
        let special = control("#+=", 1);
        for l in &mut layers[..2] {
            l[2].insert(0, capskey);
            l[2].push(numeric);
        }
        for l in &mut layers {
            l[1].push(space);
        }
        for l in &mut layers[2..] {
            l[2].push(abc);
        }
        layers[2][2].insert(0, special);
        layers[3][2].insert(0, numeric2);
        let mut images = vec![];
        for (i, (path, w, h, keep)) in [
            ("space.png", 43.0, 14.0, true),
            ("caps_lower.png", 38.0, 33.0, true),
            ("caps_upper.png", 38.0, 33.0, true),
            ("caps_lock.png", 39.0, 38.0, true),
            ("keyboard_background.png", 520.0, 170.0, false),
        ]
        .into_iter()
        .enumerate()
        {
            images.push(assets::load(
                api,
                root,
                i,
                &format!("icons_mici/settings/keyboard/{path}"),
                w,
                h,
                keep,
            )?);
        }
        Ok(Self {
            keys,
            layers,
            layer: 0,
            caps: 0,
            space,
            capskey,
            numeric,
            numeric2,
            abc,
            special,
            closest: None,
            distance: f64::INFINITY,
            selected: None,
            unselect: None,
            dragging: false,
            touch_started: false,
            release_started: false,
            tracking: false,
            pressed: false,
            initialized: false,
            text: std::rc::Rc::new(std::cell::RefCell::new(
                openpilot_log_format::JsonValue::codepoints(text.to_vec())
                    .ok_or("Mici invalid Python text")?,
            )),
            auto_return: openpilot_log_format::JsonValue::text(auto),
            bg: Bounce::new(1.0, 0.065),
            selected_filter: 0.0,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            images,
        })
    }
    pub fn set_auto_return(&mut self, value: openpilot_log_format::JsonValue) {
        self.auto_return = value;
    }
    fn auto_contains(&self, text: &str) -> Result<bool> {
        use openpilot_log_format::{JsonValue, JsonView};
        let needle = JsonValue::text(text);
        Ok(match self.auto_return.view() {
            JsonView::Text(points) => {
                let needle = text.chars().map(u32::from).collect::<Vec<_>>();
                needle.is_empty() || points.windows(needle.len()).any(|v| v == needle)
            }
            JsonView::Array(values) => values
                .iter()
                .any(|v| crate::control_tools_policy_check::json::equal(v, &needle)),
            JsonView::Object(values) => values
                .iter()
                .any(|(key, _)| key.iter().copied().eq(text.chars().map(u32::from))),
            _ => return Err("original Mici auto-return value is not iterable".into()),
        })
    }
    pub fn set_value(&mut self, value: openpilot_log_format::JsonValue) {
        self.text = std::rc::Rc::new(std::cell::RefCell::new(value));
    }
    pub fn text_handle(&self) -> std::rc::Rc<std::cell::RefCell<openpilot_log_format::JsonValue>> {
        std::rc::Rc::clone(&self.text)
    }
    fn append_points(&mut self, text: &[u32]) -> Result<()> {
        use openpilot_log_format::{JsonValue, JsonView};
        let previous = self.text.borrow().clone();
        match previous.view() {
            JsonView::Text(points) => {
                let mut points = points.to_vec();
                points.extend(text);
                self.set_value(JsonValue::codepoints(points).ok_or("invalid Python text")?);
            }
            JsonView::Array(mut values) => {
                for &point in text {
                    values.push(JsonValue::codepoints(vec![point]).ok_or("invalid Python char")?);
                }
                // Python list += str extends in place. Every prior snapshot
                // retaining this list observes the same mutation at final write.
                *self.text.borrow_mut() = crate::ui_forms_input::json_array(&values)?;
            }
            _ => return Err("original Mici text does not support string concatenation".into()),
        }
        Ok(())
    }
    pub fn backspace(&mut self) -> Result<()> {
        use openpilot_log_format::{JsonValue, JsonView};
        let previous = self.text.borrow().clone();
        if !crate::ui_forms_input::python_truth(&previous) {
            return Ok(());
        }
        let value = match previous.view() {
            JsonView::Text(points) => JsonValue::codepoints(points[..points.len() - 1].to_vec())
                .ok_or("invalid Python text")?,
            JsonView::Array(values) => {
                crate::ui_forms_input::json_array(&values[..values.len() - 1])?
            }
            _ => return Err("original Mici text does not support slicing".into()),
        };
        self.set_value(value);
        Ok(())
    }
    pub fn space(&mut self) -> Result<()> {
        self.append_points(&[32])
    }
    fn set_layer(&mut self, layer: usize) {
        let old = self.layers[self.layer].clone();
        let new = self.layers[layer].clone();
        for (current, row) in old.iter().zip(&new) {
            for i in 0..current.len().max(row.len()) {
                let source = current[i.min(current.len() - 1)];
                let target = row[i.min(row.len() - 1)];
                let p = self.keys[source].rect;
                self.keys[target].position(p.x, p.y, self.rect.y, false);
            }
        }
        self.layer = layer;
    }
    fn uppercase(&mut self, cycle: bool) {
        if !cycle {
            self.set_layer(0);
            self.caps = 0;
        } else {
            self.set_layer(1);
            if self.caps < 2 {
                self.caps += 1;
            } else {
                self.uppercase(false);
            }
        }
    }
    fn closest(&self, x: f64, y: f64) -> (Option<usize>, f64) {
        let (mut selected, mut distance) = (None, f64::INFINITY);
        for row in &self.layers[self.layer] {
            for &id in row {
                let p = self.keys[id].original;
                let d = (p.0 - x).abs() + (p.1 - (y - self.rect.y)).abs();
                if d < distance
                    && (self.closest.is_none()
                        || self.closest == Some(id)
                        || d < self.distance - 5.0)
                {
                    selected = Some(id);
                    distance = d;
                }
            }
        }
        (selected, distance)
    }
    fn event(&mut self, e: &serde_json::Value, now: f64) {
        let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
        let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
        let top = self.rect.y + self.rect.height - 170.0;
        if e["pressed"] == true {
            self.touch_started = y > top;
            self.release_started = false;
            self.dragging = self.touch_started;
            self.closest = None;
            self.distance = f64::INFINITY;
            self.selected = None;
            self.unselect = None;
        } else if e["released"] == true {
            self.release_started = self.touch_started;
            self.touch_started = false;
            self.dragging = false;
            if y <= top {
                self.closest = None;
                self.distance = f64::INFINITY;
            }
        }
        if e["down"] == true && self.dragging {
            let (c, d) = self.closest(x, y);
            self.closest = c;
            self.distance = d;
            if self.selected.is_none() {
                self.selected = Some(now);
            }
            if y <= top {
                self.closest = None;
                self.distance = f64::INFINITY;
            }
        }
    }
    fn release(&mut self, now: f64) -> Result<()> {
        if !self.release_started {
            self.closest = None;
            self.distance = f64::INFINITY;
            self.selected = None;
            self.unselect = None;
            self.dragging = false;
            self.touch_started = false;
            return Ok(());
        }
        if let Some(id) = self.closest {
            if id == self.capskey {
                self.uppercase(true);
            } else if id == self.numeric || id == self.numeric2 {
                self.set_layer(2);
            } else if id == self.abc {
                self.uppercase(false);
            } else if id == self.special {
                self.set_layer(3);
            } else {
                let value = self.keys[id].value.clone();
                self.append_points(&value.chars().map(u32::from).collect::<Vec<_>>())?;
                if self.caps == 1 {
                    self.uppercase(false);
                }
                if self.auto_contains(&value)? && matches!(self.layer, 2 | 3) {
                    self.uppercase(false);
                }
            }
        }
        self.unselect = Some(if now - self.selected.unwrap_or(0.0) < 0.075 {
            now + 0.075
        } else {
            now
        });
        self.release_started = false;
        self.dragging = false;
        self.touch_started = false;
        Ok(())
    }
    fn layout(&mut self, layer: usize, bgx: f64, bgy: f64, api: &mut dyn Canvas) -> Result<()> {
        let keyrect = Rect {
            x: bgx,
            y: bgy,
            width: 520.0,
            height: 170.0,
        }
        .cast();
        for (row_index, row) in self.layers[layer].clone().into_iter().enumerate() {
            let padding = if row_index == 1 { 33.0 } else { 44.0 };
            for (index, id) in row.iter().copied().enumerate() {
                let mut x = keyrect.x
                    + padding
                    + index as f64 * ((keyrect.width - padding * 2.0) / (row.len() - 1) as f64);
                let mut y = keyrect.y + 33.0 + row_index as f64 * ((keyrect.height - 66.0) / 2.0);
                let (alpha, size) = if let Some(selected) = self.closest {
                    if id == selected {
                        y = (y - 120.0).max(40.0);
                        x += interp(x, self.rect.x, self.rect.x + self.rect.width, 100.0, -100.0);
                        api.call(&format!(
                            "circle-gradient {} {} 128 {} 0",
                            x,
                            y,
                            c::color(0, 0, 0, (self.selected_filter * 225.0) as u8)
                        ))?;
                        (1.0, 128.0)
                    } else {
                        let (dx, dy) = (
                            self.keys[id].original.0 - self.keys[selected].original.0,
                            self.keys[id].original.1 - self.keys[selected].original.1,
                        );
                        let distance =
                            0.941246 * dx.abs().max(dy.abs()) + 0.41 * dx.abs().min(dy.abs());
                        let inv = 1.0 / if distance == 0.0 { 1.0 } else { distance };
                        let push = interp(distance, 0.0, 250.0, 20.0, 0.0);
                        x += dx * inv * push;
                        y += dy * inv * push;
                        (
                            interp(distance, 0.0, 100.0, 1.0, 0.35),
                            interp(distance, 0.0, 150.0, 84.0, 42.0),
                        )
                    }
                } else {
                    (1.0, 42.0)
                };
                let key = &mut self.keys[id];
                key.alpha.update(alpha);
                key.size.update(if key.kind == 1 {
                    size * (24.0 / 42.0)
                } else {
                    size
                });
                key.position(x, y, self.rect.y, true);
            }
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        now: f64,
        events: &[serde_json::Value],
    ) -> Result<()> {
        self.rect = rect;
        let a = 0.05 / (0.075 * 0.65 + 0.05);
        self.selected_filter =
            (1.0 - a) * self.selected_filter + a * if self.closest.is_some() { 1.0 } else { 0.0 };
        if self.unselect.is_some_and(|t| now > t) {
            self.closest = None;
            self.distance = f64::INFINITY;
            self.unselect = None;
            self.selected = None;
        }
        let bgx = rect.x + (rect.width - 520.0) / 2.0;
        let bgy = rect.y + rect.height - 170.0;
        let bgscale = self.bg.update(if self.closest.is_some() {
            1.0307692307692307
        } else {
            1.0
        });
        api.call(&format!(
            "texture-pro 4 {} {bgy} {} 170 4294967295",
            rect.x + rect.width / 2.0 - 520.0 * bgscale / 2.0,
            520.0 * bgscale
        ))?;
        if !self.initialized {
            for layer in 0..4 {
                self.layout(layer, bgx, bgy, api)?;
            }
            self.initialized = true;
        }
        self.layout(self.layer, bgx, bgy, api)?;
        for row in &self.layers[self.layer] {
            for &id in row {
                let key = &self.keys[id];
                let alpha = (255.0 * key.alpha.x).trunc().min(255.0) as u8;
                if key.kind >= 2 {
                    let texture = if id == self.space {
                        0
                    } else {
                        match self.caps {
                            0 => 1,
                            1 => 2,
                            _ => 3,
                        }
                    };
                    let (w, h) = self.images[texture];
                    let s = interp(key.size.x, 42.0, 84.0, 1.0, 1.5);
                    let y = if id == self.space {
                        key.rect.y
                    } else {
                        key.rect.y - h * s / 2.0
                    };
                    api.call(&format!(
                        "texture-pro {texture} {} {y} {} {} {}",
                        key.rect.x - w * s / 2.0,
                        w * s,
                        h * s,
                        c::color(255, 255, 255, alpha)
                    ))?;
                } else {
                    c::font(api, if key.kind == 1 { 1 } else { 3 })?;
                    let size = key.size.x.round_ties_even();
                    let (w, h) = api.measure_vector(&key.value, size * scale)?;
                    api.call(&format!(
                        "text {} {} {} {} {}",
                        hex(&key.value),
                        key.rect.x - w / 2.0,
                        key.rect.y - h / 2.0,
                        size * scale,
                        c::color(255, 255, 255, alpha)
                    ))?;
                }
                c::debug_rect(api, key.rect)?;
            }
        }
        for e in events {
            if !crate::ui_forms_input::single_touch_event(e, rect)? {
                continue;
            }
            let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
            let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
            let inside =
                x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
            if e["pressed"] == true {
                if inside {
                    crate::ui_forms_input::single_touch_index(e)?;
                    self.pressed = true;
                    self.tracking = true;
                    self.event(e, now);
                }
            } else if e["released"] == true {
                self.event(e, now);
                crate::ui_forms_input::single_touch_index(e)?;
                if self.pressed && inside {
                    self.release(now)?;
                }
                self.pressed = false;
                self.tracking = false;
            } else if inside {
                crate::ui_forms_input::single_touch_index(e)?;
                if self.tracking {
                    self.pressed = true;
                    self.event(e, now);
                }
            } else {
                crate::ui_forms_input::single_touch_index(e)?;
                self.pressed = false;
                self.event(e, now);
            }
        }
        Ok(())
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let caps = ["Lower", "Upper", "Lock"][usize::from(self.caps)];
        let layer = ["Lower", "Upper", "Special", "SuperSpecial"][self.layer];
        json!({"text":"","candidate":if self.dragging{self.closest.filter(|id|self.keys[*id].kind==0).map(|id|self.keys[id].value.clone()).unwrap_or_default()}else{String::new()},"caps":caps,"layer":layer,"selected_at":self.selected,"unselect_at":self.unselect,"dragging":self.dragging,"keys":self.layers[self.layer].iter().flatten().map(|id|{let k=&self.keys[*id];json!({"value":k.value,"rect":k.rect,"original":{"x":k.original.0,"y":k.original.1},"size":k.size.x,"alpha":k.alpha.x})}).collect::<Vec<_>>()})
    }

    pub fn snapshot_json(&self) -> Result<openpilot_log_format::JsonValue> {
        use crate::ui_forms_input::{json_codepoints, snapshot_object};
        use openpilot_log_format::{JsonValue, JsonView};
        let ordinary = JsonValue::parse(&serde_json::to_string(&self.snapshot())?)?;
        let mut keys = vec![91];
        let key_values = ordinary.get("keys").ok_or("Mici keys missing")?;
        let JsonView::Array(values) = key_values.view() else {
            return Err("Mici keys not list".into());
        };
        for (index, key) in values.iter().enumerate() {
            if index > 0 {
                keys.push(44);
            }
            let rect = key.get("rect").ok_or("Mici rect missing")?;
            let rect = snapshot_object(&[
                ("x", rect.get("x").unwrap()),
                ("y", rect.get("y").unwrap()),
                ("width", rect.get("width").unwrap()),
                ("height", rect.get("height").unwrap()),
            ])?;
            let original = key.get("original").unwrap();
            let original = snapshot_object(&[
                ("x", original.get("x").unwrap()),
                ("y", original.get("y").unwrap()),
            ])?;
            let key = snapshot_object(&[
                ("value", key.get("value").unwrap()),
                ("rect", rect),
                ("original", original),
                ("size", key.get("size").unwrap()),
                ("alpha", key.get("alpha").unwrap()),
            ])?;
            keys.extend(json_codepoints(&key)?);
        }
        keys.push(93);
        let keys = JsonValue::parse_codepoints(&keys)?;
        snapshot_object(&[
            ("text", self.text.borrow().clone()),
            ("candidate", ordinary.get("candidate").unwrap()),
            ("caps", ordinary.get("caps").unwrap()),
            ("layer", ordinary.get("layer").unwrap()),
            ("selected_at", ordinary.get("selected_at").unwrap()),
            ("unselect_at", ordinary.get("unselect_at").unwrap()),
            ("dragging", ordinary.get("dragging").unwrap()),
            ("keys", keys),
        ])
    }
}
