//! Independent original labels/buttons/dialog geometry. No candidate widgets.
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Rect},
    ui_forms_reference::html::Html,
};
thread_local! {static DEBUG: std::cell::Cell<bool> = const {std::cell::Cell::new(false)};}
pub fn debug_enabled() -> bool {
    DEBUG.get()
}
pub fn set_debug(enabled: bool) {
    DEBUG.set(enabled);
}
pub fn debug_rect(api: &mut dyn Canvas, rect: Rect) -> Result<()> {
    if DEBUG.get() {
        api.call(&format!(
            "outline {} {} {} {} 4281805286",
            rect.x.trunc(),
            rect.y.trunc(),
            rect.width.trunc().max(1.0),
            rect.height.trunc().max(1.0)
        ))?;
    }
    Ok(())
}
pub fn color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    u32::from_le_bytes([r, g, b, a])
}
pub fn font(api: &mut dyn Canvas, id: u8) -> Result<()> {
    api.call(&format!("font-select {id}"))
}
pub struct LabelStyle {
    pub size: f64,
    pub scale: f64,
    pub padding: f64,
    pub align: u8,
    pub rgba: u32,
    pub elide: bool,
}
pub fn label(api: &mut dyn Canvas, text: &str, rect: Rect, style: LabelStyle) -> Result<()> {
    label_codepoints(
        api,
        &text.chars().map(u32::from).collect::<Vec<_>>(),
        rect,
        style,
    )
}
pub fn label_codepoints(
    api: &mut dyn Canvas,
    text: &[u32],
    rect: Rect,
    style: LabelStyle,
) -> Result<()> {
    let LabelStyle {
        size,
        scale,
        padding,
        align,
        rgba,
        elide,
    } = style;
    let width = rect.width - padding * 2.0;
    let lines = if elide {
        let mut display = text.to_vec();
        if crate::ui_forms_input::text_width_codepoints(api, text, size, scale)? > width {
            let chars = text.to_vec();
            let (mut left, mut right) = (0, chars.len());
            while left < right {
                let mid = (left + right) / 2;
                let candidate = [&chars[..mid], &[46, 46, 46]].concat();
                if crate::ui_forms_input::text_width_codepoints(api, &candidate, size, scale)?
                    <= width
                {
                    left = mid + 1;
                } else {
                    right = mid;
                }
            }
            display = if left > 0 {
                [&chars[..left - 1], &[46, 46, 46]].concat()
            } else {
                vec![46, 46, 46]
            };
        }
        vec![display]
    } else {
        Html::wrap_codepoints(api, text, size, width.round_ties_even(), scale)?
    };
    let mut sizes = vec![];
    for line in &lines {
        sizes.push(crate::ui_forms_input::text_extent_codepoints(
            api, line, size, scale,
        )?);
    }
    let total = sizes.iter().map(|v| v.1).sum::<f64>();
    let total = if total == 0.0 { size * scale } else { total };
    let mut y = f64::from((rect.y + ((rect.height - total) / 2.0).floor()) as f32);
    for (line, (width, height)) in lines.iter().zip(sizes) {
        let x = match align {
            0 => rect.x + padding,
            1 => rect.x + ((rect.width - width) / 2.0).floor(),
            _ => rect.x + rect.width - width - padding,
        };
        let mut cursor = f64::from(x as f32);
        let mut ordinary = Vec::new();
        let mut pictograph = String::new();
        for point in line.iter().copied().map(Some).chain(std::iter::once(None)) {
            let pictographic = point
                .and_then(char::from_u32)
                .is_some_and(crate::ui_forms_input::emoji);
            if pictographic {
                if pictograph.is_empty() {
                    api.call(&format!(
                        "text {} {cursor} {y} {} {rgba}",
                        crate::ui_forms_input::hex_codepoints(&ordinary)?,
                        size * scale
                    ))?;
                    cursor = f64::from(
                        (cursor
                            + crate::ui_forms_input::text_width_codepoints(
                                api, &ordinary, size, scale,
                            )?) as f32,
                    );
                    ordinary.clear();
                }
                pictograph.push(
                    point
                        .and_then(char::from_u32)
                        .ok_or("invalid emoji point")?,
                );
            } else {
                if !pictograph.is_empty() {
                    api.color_emoji(&pictograph, cursor, y, size * scale, rgba)?;
                    cursor = f64::from((cursor + size * scale) as f32);
                    pictograph.clear();
                }
                if let Some(point) = point {
                    ordinary.push(point);
                }
            }
        }
        api.call(&format!(
            "text {} {cursor} {y} {} {rgba}",
            crate::ui_forms_input::hex_codepoints(&ordinary)?,
            size * scale
        ))?;
        y = f64::from((y + if height == 0.0 { size * scale } else { height }) as f32);
    }
    debug_rect(api, rect)
}
#[derive(Default)]
pub struct Touch {
    pressed: bool,
    tracking: bool,
    slots: [(bool, bool); 2],
    pub multi: bool,
}
impl Touch {
    pub fn multi_touch() -> Self {
        Self {
            multi: true,
            ..Self::default()
        }
    }
    pub fn pressed(&self) -> bool {
        self.pressed || (self.multi && self.slots.iter().any(|v| v.0))
    }
    pub fn update(&mut self, rect: Rect, events: &[serde_json::Value]) -> Result<bool> {
        let mut clicked = false;
        for e in events {
            let raw = &e["slot"];
            let numeric = match raw {
                serde_json::Value::Bool(v) => Some(f64::from(u8::from(*v))),
                serde_json::Value::Number(v) => v.as_f64(),
                _ => None,
            };
            if !self.multi && numeric != Some(0.0) {
                continue;
            }
            let x = e["pos"]["x"].as_f64().ok_or("Forms touch x")?;
            let y = e["pos"]["y"].as_f64().ok_or("Forms touch y")?;
            let inside =
                x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
            if e["pressed"] == true && !inside {
                continue;
            }
            if raw.as_number().is_some_and(serde_json::Number::is_f64) {
                return Err("original floating touch slot cannot index widget state".into());
            }
            let numeric =
                numeric.ok_or("original noninteger touch slot cannot index widget state")?;
            let index = if numeric < 0.0 {
                numeric + 2.0
            } else {
                numeric
            };
            if !(0.0..2.0).contains(&index) || index.fract() != 0.0 {
                return Err("original touch slot index outside widget state".into());
            }
            let slot = index as usize;
            let (mut pressed, mut tracking) = if slot == 0 {
                (self.pressed, self.tracking)
            } else {
                self.slots[slot]
            };
            if e["pressed"] == true {
                pressed = true;
                tracking = true;
            } else if e["released"] == true {
                clicked |= pressed && inside;
                pressed = false;
                tracking = false;
            } else if inside {
                if tracking {
                    pressed = true;
                }
            } else {
                pressed = false;
            }
            if slot == 0 {
                self.pressed = pressed;
                self.tracking = tracking;
            } else {
                self.slots[slot] = (pressed, tracking);
            }
        }
        Ok(clicked)
    }
}

fn python_truth(value: &openpilot_log_format::JsonValue) -> bool {
    use openpilot_log_format::JsonView;
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v != "0" && v != "-0",
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
pub struct Button {
    touch: Touch,
    pub text: String,
    raw_text: Option<openpilot_log_format::JsonValue>,
    pub primary: bool,
    pub enabled: bool,
    pub size: f64,
    pub padding: f64,
    pub align: u8,
    pub font: u8,
    pub elide: bool,
    pub radius: f64,
    pub style: u8,
}
impl Button {
    pub fn multi_touch(&mut self) {
        self.touch.multi = true;
    }
    pub fn new(text: &str, primary: bool) -> Self {
        Self {
            touch: Touch::default(),
            text: text.into(),
            raw_text: None,
            primary,
            enabled: true,
            size: 60.0,
            padding: 20.0,
            align: 1,
            font: 2,
            elide: false,
            radius: 10.0,
            style: 0,
        }
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        events: &[serde_json::Value],
    ) -> Result<bool> {
        self.render_clipped(api, rect, rect, scale, events, true)
    }
    pub fn render_clipped(
        &mut self,
        api: &mut dyn Canvas,
        rect: Rect,
        hit: Rect,
        scale: f64,
        events: &[serde_json::Value],
        valid: bool,
    ) -> Result<bool> {
        let bg = if !self.enabled {
            color(51, 51, 51, 255)
        } else if self.touch.pressed() {
            if self.primary {
                color(48, 73, 244, 255)
            } else if self.style == 7 {
                color(51, 51, 51, 255)
            } else {
                color(74, 74, 74, 255)
            }
        } else if self.primary {
            color(70, 91, 234, 255)
        } else if self.style == 5 {
            color(57, 57, 57, 255)
        } else if self.style == 7 {
            color(68, 68, 68, 255)
        } else {
            color(51, 51, 51, 255)
        };
        api.call(&format!(
            "rounded {} {} {} {} {} 10 {bg}",
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            self.radius / (rect.width.min(rect.height) / 2.0)
        ))?;
        font(api, self.font)?;
        let text = match &self.raw_text {
            None => self.text.chars().map(u32::from).collect::<Vec<_>>(),
            Some(value) => match value.view() {
                openpilot_log_format::JsonView::Text(points) => points.to_vec(),
                _ if !python_truth(value) => Vec::new(),
                _ => return Err("original Button label text has no split method".into()),
            },
        };
        label_codepoints(
            api,
            &text,
            rect,
            LabelStyle {
                size: self.size,
                scale,
                padding: self.padding,
                align: self.align,
                rgba: color(
                    if self.style == 7 { 221 } else { 228 },
                    if self.style == 7 { 221 } else { 228 },
                    if self.style == 7 { 221 } else { 228 },
                    if self.enabled { 255 } else { 51 },
                ),
                elide: self.elide,
            },
        )?;
        debug_rect(api, rect)?;
        Ok(if self.enabled {
            if valid {
                self.touch.update(hit, events)?
            } else {
                let multi = self.touch.multi;
                self.touch = Touch::default();
                self.touch.multi = multi;
                false
            }
        } else {
            false
        })
    }
}
pub struct Confirm {
    text: Vec<u32>,
    raw_text: Option<openpilot_log_format::JsonValue>,
    label_width: f64,
    cancel: openpilot_log_format::JsonValue,
    rich: bool,
    html: Html,
    confirm: Button,
    cancel_button: Button,
}
impl Confirm {
    pub fn new_codepoints(text: &[u32], cancel: &[u32], rich: bool) -> Result<Self> {
        Ok(Self {
            text: text.to_vec(),
            raw_text: None,
            label_width: 0.0,
            cancel: openpilot_log_format::JsonValue::codepoints(cancel.to_vec())
                .ok_or("invalid cancel text point")?,
            rich,
            html: Html::new_codepoints(text, 50.0, true)?,
            confirm: Button::new("Continue", true),
            cancel_button: {
                let mut b = Button::new("", false);
                b.raw_text = Some(
                    openpilot_log_format::JsonValue::codepoints(cancel.to_vec())
                        .ok_or("invalid cancel text point")?,
                );
                b
            },
        })
    }
    pub fn set_value(
        &mut self,
        value: openpilot_log_format::JsonValue,
        api: &mut dyn Canvas,
        scale: f64,
    ) -> Result<()> {
        use openpilot_log_format::JsonView;
        if self.rich {
            let JsonView::Text(points) = value.view() else {
                return Err("Forms HTML requires Python text".into());
            };
            return self.html.parse_codepoints(points);
        }
        self.raw_text = Some(value);
        // Label.set_text immediately updates layout against its current rect.
        // Before first rendering that width is zero; later setters can encode
        // a word or reject a truthy non-string before a same-frame restore.
        let points = self.plain_points(self.label_width)?;
        font(api, 1)?;
        Html::wrap_codepoints(
            api,
            &points,
            70.0,
            self.label_width.round_ties_even(),
            scale,
        )?;
        Ok(())
    }
    fn plain_points(&self, width: f64) -> Result<Vec<u32>> {
        let Some(value) = &self.raw_text else {
            return Ok(self.text.clone());
        };
        if matches!(
            value.view(),
            openpilot_log_format::JsonView::Array(_) | openpilot_log_format::JsonView::Object(_)
        ) {
            return Err("Forms Python label cache key is unhashable".into());
        }
        if width <= 0.0 || !python_truth(value) {
            return Ok(Vec::new());
        }
        match value.view() {
            openpilot_log_format::JsonView::Text(points) => Ok(points.to_vec()),
            _ => Err("Forms Python label requires split on text".into()),
        }
    }
    pub fn set_cancel_value(&mut self, value: openpilot_log_format::JsonValue) -> Result<()> {
        let value = if matches!(value.view(), openpilot_log_format::JsonView::Null) {
            openpilot_log_format::JsonValue::text("Cancel")
        } else {
            value
        };
        self.cancel = value.clone();
        self.cancel_button.raw_text = Some(value);
        Ok(())
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        width: f64,
        height: f64,
        scale: f64,
        keys: &[i64],
        events: &[serde_json::Value],
    ) -> Result<usize> {
        let m = if self.rich { 100.0 } else { 200.0 };
        let rect = Rect {
            x: m,
            y: m,
            width: width - m * 2.0,
            height: height - m * 2.0,
        };
        api.call(&format!(
            "rect {} {} {} {} {}",
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            color(27, 27, 27, 255)
        ))?;
        let content = Rect {
            x: rect.x + 50.0,
            y: rect.y + 10.0,
            width: rect.width - 100.0,
            height: rect.height - 230.0,
        }
        .cast();
        if self.rich {
            let h = self.html.height(api, content.width.trunc(), scale)?;
            api.call(&format!(
                "clip {} {} {} {}",
                content.x.trunc(),
                content.y.trunc(),
                content.width.trunc(),
                content.height.trunc()
            ))?;
            self.html.render(
                api,
                Rect {
                    height: h,
                    ..content
                }
                .cast(),
                scale,
            )?;
            api.call("unclip")?;
            debug_rect(api, content)?;
        } else {
            self.label_width = content.width;
            font(api, 1)?;
            label_codepoints(
                api,
                &self.plain_points(content.width)?,
                content,
                LabelStyle {
                    size: 70.0,
                    scale,
                    padding: 0.0,
                    align: 1,
                    rgba: color(201, 201, 201, 255),
                    elide: false,
                },
            )?;
        }
        let mut pops = usize::from(keys.contains(&257) || keys.contains(&256));
        let y = rect.y + rect.height - 210.0;
        if python_truth(&self.cancel) {
            let w = ((rect.width - 150.0) / 2.0).floor();
            pops += usize::from(
                self.confirm.render(
                    api,
                    Rect {
                        x: rect.x + rect.width - w - 50.0,
                        y,
                        width: w,
                        height: 160.0,
                    }
                    .cast(),
                    scale,
                    events,
                )?,
            );
            pops += usize::from(
                self.cancel_button.render(
                    api,
                    Rect {
                        x: rect.x + 50.0,
                        y,
                        width: w,
                        height: 160.0,
                    }
                    .cast(),
                    scale,
                    events,
                )?,
            );
        } else {
            pops += usize::from(
                self.confirm.render(
                    api,
                    Rect {
                        x: rect.x + 50.0,
                        y,
                        width: rect.width - 100.0,
                        height: 160.0,
                    }
                    .cast(),
                    scale,
                    events,
                )?,
            );
        }
        Ok(pops)
    }
}

#[derive(Default)]
pub struct Scroll {
    pub offset: f64,
    velocity: f64,
    initialized: bool,
    dragging: bool,
    last_y: f64,
    start_y: f64,
    last_time: f64,
}
impl Scroll {
    pub fn update(&mut self, rect: Rect, content: f64, events: &[serde_json::Value]) -> f64 {
        let max = (content - rect.height).max(0.0);
        for e in events {
            if e["slot"].as_i64() != Some(0) {
                continue;
            }
            let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
            let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
            let time = e["time"].as_f64().unwrap_or(0.0);
            let inside =
                x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
            if !self.dragging {
                if inside {
                    if e["pressed"] == true {
                        self.start_y = y;
                        if self.velocity.abs() > 120.0 {
                            self.dragging = true;
                            self.initialized = false;
                        }
                    }
                    if e["down"] == true && (y - self.start_y).abs() > 12.0 {
                        self.dragging = true;
                        self.initialized = false;
                    }
                }
            } else if e["released"] == true {
                self.dragging = false;
            } else {
                let mut delta = y - self.last_y;
                if self.offset > 0.0 || self.offset < -max {
                    delta /= 3.0;
                }
                self.offset += delta;
                let dt = time - self.last_time;
                if dt > 0.0 {
                    let v = delta / dt;
                    self.velocity = if self.initialized {
                        0.5 * self.velocity + 0.5 * v
                    } else {
                        self.initialized = true;
                        v
                    };
                }
            }
            self.last_time = time;
            self.last_y = y;
        }
        if !self.dragging {
            let out = self.offset > 0.0 || self.offset < -max;
            if self.velocity.abs() > 2.0 {
                let friction = (-5.0f64 * 0.05).exp();
                self.velocity *= if out { friction.powi(2) } else { friction };
            } else {
                self.velocity = 0.0;
            }
            if out {
                self.offset = (1.0 - 0.05 / 0.15) * self.offset
                    + (0.05 / 0.15) * if self.offset > 0.0 { 0.0 } else { -max };
            }
            self.offset += self.velocity / 20.0;
        } else if events.is_empty() {
            self.velocity *= 0.5;
        }
        if self.offset.abs() < 0.01 {
            self.offset = 0.0;
        } else if (self.offset + max).abs() < 0.01 {
            self.offset = -max;
        }
        self.offset
    }
    pub fn valid(&self) -> bool {
        !self.dragging && self.velocity.abs() < 120.0
    }
}
pub struct Options {
    title: openpilot_log_format::JsonValue,
    pub selection: openpilot_log_format::JsonValue,
    buttons: Vec<Button>,
    cancel: Button,
    select: Button,
    scroll: Scroll,
}
impl Options {
    pub fn new_codepoints(title: &[u32]) -> Self {
        let buttons = ["First", "Second", "Third", "Fourth"]
            .iter()
            .map(|s| {
                let mut b = Button::new(s, false);
                b.padding = 50.0;
                b.align = 0;
                b.elide = true;
                b
            })
            .collect();
        Self {
            title: openpilot_log_format::JsonValue::codepoints(title.to_vec())
                .expect("valid Python text points"),
            selection: openpilot_log_format::JsonValue::text("First"),
            buttons,
            cancel: Button::new("Cancel", false),
            select: Button::new("Select", true),
            scroll: Scroll::default(),
        }
    }
    pub fn set_value(&mut self, value: openpilot_log_format::JsonValue) {
        self.title = value;
    }
    pub fn set_selection(&mut self, value: openpilot_log_format::JsonValue) {
        self.selection = value;
    }
    pub fn snapshot_json(&self) -> Result<openpilot_log_format::JsonValue> {
        crate::ui_forms_input::snapshot_object(&[("selection", self.selection.clone())])
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        events: &[serde_json::Value],
    ) -> Result<usize> {
        let dialog = Rect {
            x: rect.x + 50.0,
            y: rect.y + 50.0,
            width: rect.width - 100.0,
            height: rect.height - 100.0,
        }
        .cast();
        api.call(&format!(
            "rounded {} {} {} {} 0.02 20 {}",
            dialog.x,
            dialog.y,
            dialog.width,
            dialog.height,
            color(30, 30, 30, 255)
        ))?;
        let content = Rect {
            x: dialog.x + 50.0,
            y: dialog.y + 50.0,
            width: dialog.width - 100.0,
            height: dialog.height - 100.0,
        }
        .cast();
        font(api, 1)?;
        let title = match self.title.view() {
            openpilot_log_format::JsonView::Text(points) => points.to_vec(),
            _ => return Err("original Options title cannot be measured as text".into()),
        };
        gui_label_codepoints(
            api,
            &title,
            Rect {
                height: 70.0,
                ..content
            },
            70.0,
            scale,
            0,
            color(255, 255, 255, 229),
        )?;
        let options = Rect {
            x: content.x,
            y: content.y + 120.0,
            width: content.width,
            height: content.height - 330.0,
        }
        .cast();
        let offset = self.scroll.update(options, 640.0, events);
        api.call(&format!(
            "clip {} {} {} {}",
            options.x.trunc(),
            options.y.trunc(),
            options.width.trunc(),
            options.height.trunc()
        ))?;
        for (i, b) in self.buttons.iter_mut().enumerate() {
            b.primary = self.selection.text_eq(&b.text);
            let r = Rect {
                x: options.x,
                y: options.y + i as f64 * 160.0 + offset,
                width: options.width,
                height: 135.0,
            }
            .cast();
            let hit = intersection(r, options);
            let clicked = b.render_clipped(api, r, hit, scale, events, self.scroll.valid())?;
            if clicked {
                self.selection = openpilot_log_format::JsonValue::text(&b.text);
            }
        }
        api.call("unclip")?;
        debug_rect(api, options)?;
        let y = content.y + content.height - 160.0;
        let w = (content.width - 50.0) / 2.0;
        let mut pops = usize::from(
            self.cancel.render(
                api,
                Rect {
                    x: content.x,
                    y,
                    width: w,
                    height: 160.0,
                }
                .cast(),
                scale,
                events,
            )?,
        );
        self.select.enabled = !self.selection.text_eq("First");
        pops += usize::from(
            self.select.render(
                api,
                Rect {
                    x: content.x + w + 50.0,
                    y,
                    width: w,
                    height: 160.0,
                }
                .cast(),
                scale,
                events,
            )?,
        );
        Ok(pops)
    }
}
pub fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect {
        x,
        y,
        width: ((a.x + a.width).min(b.x + b.width) - x).max(0.0),
        height: ((a.y + a.height).min(b.y + b.height) - y).max(0.0),
    }
    .cast()
}
pub fn gui_label(
    api: &mut dyn Canvas,
    text: &str,
    rect: Rect,
    size: f64,
    scale: f64,
    align: u8,
    rgba: u32,
) -> Result<()> {
    gui_label_codepoints(
        api,
        &text.chars().map(u32::from).collect::<Vec<_>>(),
        rect,
        size,
        scale,
        align,
        rgba,
    )
}
pub fn gui_label_codepoints(
    api: &mut dyn Canvas,
    text: &[u32],
    rect: Rect,
    size: f64,
    scale: f64,
    align: u8,
    rgba: u32,
) -> Result<()> {
    let mut text = text.to_vec();
    let mut w = crate::ui_forms_input::text_width_codepoints(api, &text, size, scale)?;
    if w > rect.width {
        let chars = text.to_vec();
        let (mut left, mut right) = (0, chars.len());
        while left < right {
            let mid = (left + right) / 2;
            let candidate = [&chars[..mid], &[46, 46, 46]].concat();
            if crate::ui_forms_input::text_width_codepoints(api, &candidate, size, scale)?
                <= rect.width
            {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        text = if left > 0 {
            [&chars[..left - 1], &[46, 46, 46]].concat()
        } else {
            vec![46, 46, 46]
        };
        w = crate::ui_forms_input::text_width_codepoints(api, &text, size, scale)?;
    }
    let h = api
        .measure_vector(&crate::ui_forms_input::Input::utf8(&text)?, size * scale)?
        .1;
    let x = rect.x
        + match align {
            1 => (rect.width - w) / 2.0,
            2 => rect.width - w,
            _ => 0.0,
        };
    let y = rect.y + (rect.height - h) / 2.0;
    api.call(&format!(
        "text {} {x} {y} {} {rgba}",
        crate::ui_forms_input::hex_codepoints(&text)?,
        size * scale
    ))
}
