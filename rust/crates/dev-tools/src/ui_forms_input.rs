//! Independent InputBox policy translated from the pinned Python widget.
//! This module never imports the candidate UI framework.
use crate::seal::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub fn cast(self) -> Self {
        Self {
            x: f64::from(self.x as f32),
            y: f64::from(self.y as f32),
            width: f64::from(self.width as f32),
            height: f64::from(self.height as f32),
        }
    }
}
pub trait Canvas {
    fn pixel_scale(&self) -> f64 {
        1.0
    }
    fn call(&mut self, command: &str) -> Result<()>;
    fn measure(&mut self, text: &str, size: f64) -> Result<f64>;
    fn color_emoji(&mut self, _text: &str, _x: f64, _y: f64, _size: f64, _rgba: u32) -> Result<()> {
        Err("Forms color emoji canvas unavailable".into())
    }
    fn measure_vector(&mut self, text: &str, size: f64) -> Result<(f64, f64)> {
        Ok((
            self.measure(text, size)?,
            if text.is_empty() {
                0.0
            } else {
                f64::from(size as f32)
            },
        ))
    }
}
pub struct InputFrame<'a> {
    pub rect: Rect,
    pub now: f64,
    pub scale: f64,
    pub key: i64,
    pub down: &'a [i64],
    pub character: Option<u32>,
    pub events: &'a [serde_json::Value],
    pub debug: bool,
}
pub fn hex(text: &str) -> String {
    if text.is_empty() {
        "-".into()
    } else {
        text.as_bytes().iter().map(|v| format!("{v:02x}")).collect()
    }
}
pub fn emoji(c: char) -> bool {
    // Union of the original overlapping regex character ranges.
    matches!(u32::from(c),0x200d|0x2300..=0x23ff|0x2600..=0x2bff|0x3030|0xfe0f|0x1f1e0..=0x1f1ff|0x1f300..=0x1f5ff|0x1f600..=0x1f64f|0x1f680..=0x1f6ff|0x1f700..=0x1f77f|0x1f900..=0x1f9ff|0x1fa70..=0x1faff)
}
pub fn text_extent(
    canvas: &mut dyn Canvas,
    text: &str,
    size: f64,
    scale: f64,
) -> Result<(f64, f64)> {
    let mut normal = String::new();
    let mut groups = 0;
    let mut previous = false;
    for c in text.chars() {
        let found = emoji(c);
        if found {
            if !previous {
                groups += 1;
            }
        } else {
            normal.push(c);
        }
        previous = found;
    }
    let (width, height) = canvas.measure_vector(&normal, size * scale)?;
    if groups == 0 {
        return Ok((width, height));
    }
    Ok((
        f64::from((width + f64::from(groups) * size * scale) as f32),
        if height == 0.0 {
            f64::from((size * scale) as f32)
        } else {
            height
        },
    ))
}
pub fn text_width(canvas: &mut dyn Canvas, text: &str, size: f64, scale: f64) -> Result<f64> {
    Ok(text_extent(canvas, text, size, scale)?.0)
}

/// Assemble JSON as Python codepoints without accidentally joining adjacent
/// surrogate escapes into a scalar during an internal serialize/parse cycle.
pub fn json_codepoints(value: &openpilot_log_format::JsonValue) -> Result<Vec<u32>> {
    use openpilot_log_format::JsonView;
    enum Part {
        Node(openpilot_log_format::JsonValue),
        Token(String),
        Text(Vec<u32>),
    }
    let mut stack = vec![Part::Node(value.clone())];
    let mut result = Vec::new();
    while let Some(part) = stack.pop() {
        match part {
            Part::Token(token) => result.extend(token.chars().map(u32::from)),
            Part::Text(points) => {
                result.push(34);
                for point in points {
                    if point == 34 || point == 92 || point < 32 {
                        let character = char::from_u32(point).ok_or("Forms invalid JSON point")?;
                        let escaped = serde_json::to_string(&character.to_string())?;
                        result.extend(escaped[1..escaped.len() - 1].chars().map(u32::from));
                    } else {
                        result.push(point);
                    }
                }
                result.push(34);
            }
            Part::Node(value) => match value.view() {
                JsonView::Text(points) => stack.push(Part::Text(points.to_vec())),
                JsonView::Array(values) => {
                    result.push(91);
                    stack.push(Part::Token("]".into()));
                    for (index, value) in values.into_iter().enumerate().rev() {
                        stack.push(Part::Node(value));
                        if index > 0 {
                            stack.push(Part::Token(",".into()));
                        }
                    }
                }
                JsonView::Object(values) => {
                    result.push(123);
                    stack.push(Part::Token("}".into()));
                    for (index, (key, value)) in values.into_iter().enumerate().rev() {
                        stack.push(Part::Node(value));
                        stack.push(Part::Token(":".into()));
                        stack.push(Part::Text(key.to_vec()));
                        if index > 0 {
                            stack.push(Part::Token(",".into()));
                        }
                    }
                }
                _ => result.extend(value.to_json()?.chars().map(u32::from)),
            },
        }
    }
    Ok(result)
}
pub fn snapshot_object(
    fields: &[(&str, openpilot_log_format::JsonValue)],
) -> Result<openpilot_log_format::JsonValue> {
    let mut result = vec![123];
    for (index, (name, value)) in fields.iter().enumerate() {
        if index > 0 {
            result.push(44);
        }
        result.extend(serde_json::to_string(name)?.chars().map(u32::from));
        result.push(58);
        result.extend(json_codepoints(value)?);
    }
    result.push(125);
    Ok(openpilot_log_format::JsonValue::parse_codepoints(&result)?)
}
pub fn text_extent_codepoints(
    canvas: &mut dyn Canvas,
    text: &[u32],
    size: f64,
    scale: f64,
) -> Result<(f64, f64)> {
    text_extent(canvas, &Input::utf8(text)?, size, scale)
}
pub fn text_width_codepoints(
    canvas: &mut dyn Canvas,
    text: &[u32],
    size: f64,
    scale: f64,
) -> Result<f64> {
    Ok(text_extent_codepoints(canvas, text, size, scale)?.0)
}
pub fn hex_codepoints(text: &[u32]) -> Result<String> {
    Ok(hex(&Input::utf8(text)?))
}
pub fn python_space(point: u32) -> bool {
    char::from_u32(point).is_some_and(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
/// Single-touch widgets skip nonzero slots before hit testing. Python's 0.0
/// compares equal to zero, but fails as a list index only when state is used;
/// a press beginning outside the widget never reaches that indexing branch.
pub fn single_touch_event(event: &serde_json::Value, rect: Rect) -> Result<bool> {
    let raw = &event["slot"];
    let numeric = match raw {
        serde_json::Value::Bool(value) => Some(f64::from(u8::from(*value))),
        serde_json::Value::Number(value) => value.as_f64(),
        _ => None,
    };
    if numeric != Some(0.0) {
        return Ok(false);
    }
    let x = event["pos"]["x"].as_f64().ok_or("Forms touch x")?;
    let y = event["pos"]["y"].as_f64().ok_or("Forms touch y")?;
    let inside =
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
    if event["pressed"] == true && !inside {
        return Ok(false);
    }
    Ok(true)
}
pub fn single_touch_index(event: &serde_json::Value) -> Result<()> {
    if event["slot"]
        .as_number()
        .is_some_and(serde_json::Number::is_f64)
    {
        return Err("original floating touch slot cannot index widget state".into());
    }
    Ok(())
}
pub fn python_truth(value: &openpilot_log_format::JsonValue) -> bool {
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
pub fn json_array(
    values: &[openpilot_log_format::JsonValue],
) -> Result<openpilot_log_format::JsonValue> {
    let mut points = vec![91];
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            points.push(44);
        }
        points.extend(json_codepoints(value)?);
    }
    points.push(93);
    Ok(openpilot_log_format::JsonValue::parse_codepoints(&points)?)
}
#[derive(Default)]
pub struct Input {
    pub text: Vec<u32>,
    list_text: Option<Vec<openpilot_log_format::JsonValue>>,
    pub cursor: usize,
    pub cursor_repr: Option<serde_json::Value>,
    pub offset: f64,
    pub show: bool,
    max: Option<i64>,
    password: bool,
    blink: u32,
    last_key: i64,
    key_time: u64,
    visible: f64,
    font_size: f64,
    last_char: f64,
    tracking: bool,
    pressed: bool,
}
impl Input {
    pub fn text_len(&self) -> usize {
        self.list_text.as_ref().map_or(self.text.len(), Vec::len)
    }
    fn remove_text(&mut self, index: usize) {
        if let Some(list) = &mut self.list_text {
            list.remove(index);
        } else {
            self.text.remove(index);
        }
    }
    pub fn set_value(
        &mut self,
        value: openpilot_log_format::JsonValue,
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        use openpilot_log_format::JsonView;
        match value.view() {
            JsonView::Text(points) => self.set_codepoints(points, canvas, now, scale),
            JsonView::Array(mut values) => {
                let limit = match self.max {
                    Some(max) if max < 0 => {
                        values.len().saturating_sub(max.unsigned_abs() as usize)
                    }
                    Some(max) => (max as usize).min(values.len()),
                    None => values.len(),
                };
                values.truncate(limit);
                self.text.clear();
                self.cursor = values.len();
                self.list_text = Some(values);
                self.cursor_repr = None;
                self.offset(canvas, now, scale)
            }
            _ => Err("Forms Python input value cannot be sliced".into()),
        }
    }
    /// Python accepts None as a slice end when setting initial text, but
    /// comparing its max against a later character insertion raises TypeError.
    pub fn none_limit(&mut self) {
        self.max = None;
    }
    pub fn password(&mut self, value: bool) {
        self.password = value;
    }
    pub fn add(&mut self, text: &str, canvas: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        self.add_codepoints(
            &text.chars().map(u32::from).collect::<Vec<_>>(),
            canvas,
            now,
            scale,
        )
    }
    pub fn add_codepoints(
        &mut self,
        text: &[u32],
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        if text.iter().any(|&point| point > 0x10ffff) {
            return Err("Forms invalid Python codepoint".into());
        }
        if (self.text_len() as i128)
            < i128::from(self.max.ok_or("Forms cannot compare text length to None")?)
        {
            if self.list_text.is_some() {
                return Err("Forms cannot concatenate Python list and text".into());
            }
            if self
                .cursor_repr
                .as_ref()
                .and_then(serde_json::Value::as_number)
                .is_some_and(serde_json::Number::is_f64)
            {
                return Err("Forms insertion cannot slice with float".into());
            }
            self.text
                .splice(self.cursor..self.cursor, text.iter().copied());
            self.cursor(self.cursor as i64 + 1, canvas, now, scale)?;
            if self.password {
                self.last_char = now;
            }
        }
        Ok(())
    }
    pub fn add_value(
        &mut self,
        value: openpilot_log_format::JsonValue,
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        if (self.text_len() as i128)
            >= i128::from(self.max.ok_or("Forms cannot compare text length to None")?)
        {
            return Ok(());
        }
        match value.view() {
            openpilot_log_format::JsonView::Text(points) if self.list_text.is_none() => {
                self.add_codepoints(points, canvas, now, scale)
            }
            openpilot_log_format::JsonView::Array(values) if self.list_text.is_some() => {
                if self
                    .cursor_repr
                    .as_ref()
                    .and_then(serde_json::Value::as_number)
                    .is_some_and(serde_json::Number::is_f64)
                {
                    return Err("Forms insertion cannot slice with float".into());
                }
                self.list_text
                    .as_mut()
                    .unwrap()
                    .splice(self.cursor..self.cursor, values);
                self.cursor(self.cursor as i64 + 1, canvas, now, scale)?;
                if self.password {
                    self.last_char = now;
                }
                Ok(())
            }
            _ => Err("Forms Python input values cannot be concatenated".into()),
        }
    }
    pub fn backspace(&mut self, canvas: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        self.process(259, canvas, now, scale)
    }
    pub fn new(max: i64, password: bool) -> Self {
        Self {
            max: Some(max),
            password,
            ..Self::default()
        }
    }
    /// Python strings retain lone surrogate codepoints. Encoding is deferred
    /// until the source would pass a particular string/slice to CFFI.
    pub fn display_codepoints(&self, now: f64) -> Result<Vec<u32>> {
        if !self.password {
            if self.list_text.is_some() {
                return Err("Forms Python list cannot be encoded as display text".into());
            }
            return Ok(self.text.clone());
        }
        let mut text = vec![u32::from('•'); self.text_len()];
        if now - self.last_char < 1.5 && !text.is_empty() {
            let at = self.cursor.saturating_sub(1);
            if at < text.len() {
                if let Some(values) = &self.list_text {
                    let openpilot_log_format::JsonView::Text(points) = values[at].view() else {
                        return Err(
                            "Forms cannot concatenate password mask and non-text item".into()
                        );
                    };
                    text.splice(at..at + 1, points.iter().copied());
                } else {
                    text[at] = self.text[at];
                }
            }
        }
        Ok(text)
    }
    pub fn utf8(points: &[u32]) -> Result<String> {
        points
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .ok_or_else(|| "Forms Python text cannot encode lone surrogate as UTF8".into())
    }
    pub fn display(&self, now: f64) -> Result<String> {
        Self::utf8(&self.display_codepoints(now)?)
    }
    pub fn text_json(&self) -> Result<openpilot_log_format::JsonValue> {
        if let Some(values) = &self.list_text {
            return json_array(values);
        }
        openpilot_log_format::JsonValue::codepoints(self.text.clone())
            .ok_or_else(|| "Forms invalid Python text codepoint".into())
    }
    pub fn display_json(&self, now: f64) -> Result<openpilot_log_format::JsonValue> {
        if !self.password {
            return self.text_json();
        }
        openpilot_log_format::JsonValue::codepoints(self.display_codepoints(now)?)
            .ok_or_else(|| "Forms invalid display codepoint".into())
    }
    fn offset(&mut self, canvas: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        if self.visible == 0.0 {
            return Ok(());
        }
        // Plain list values remain unencoded until a nonempty cursor prefix
        // is measured; password masking itself can still fail before this.
        let display = if self.cursor == 0 && !self.password && self.list_text.is_some() {
            Vec::new()
        } else {
            self.display_codepoints(now)?
        };
        let x = if self.cursor > 0 {
            text_width(
                canvas,
                &Self::utf8(&display[..self.cursor.min(display.len())])?,
                self.font_size,
                scale,
            )?
        } else {
            0.0
        };
        let visible = self.visible - 20.0;
        if x < self.offset {
            self.offset = (x - 10.0).max(0.0);
        } else if x > self.offset + visible {
            self.offset = x - visible + 10.0;
        }
        Ok(())
    }
    pub fn set_text(
        &mut self,
        text: &str,
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        self.set_codepoints(
            &text.chars().map(u32::from).collect::<Vec<_>>(),
            canvas,
            now,
            scale,
        )
    }
    pub fn set_codepoints(
        &mut self,
        text: &[u32],
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        if text.iter().any(|&point| point > 0x10ffff) {
            return Err("Forms invalid Python codepoint".into());
        }
        self.list_text = None;
        self.text = text.to_vec();
        let limit = match self.max {
            Some(max) if max < 0 => self.text_len().saturating_sub(max.unsigned_abs() as usize),
            Some(max) => (max as usize).min(self.text_len()),
            None => self.text_len(),
        };
        self.text.truncate(limit);
        self.cursor = self.text_len();
        self.cursor_repr = None;
        self.offset(canvas, now, scale)
    }
    pub fn cursor_value(
        &mut self,
        value: &serde_json::Value,
        canvas: &mut dyn Canvas,
        now: f64,
        scale: f64,
    ) -> Result<()> {
        let number = match value {
            serde_json::Value::Bool(v) => f64::from(u8::from(*v)),
            serde_json::Value::Number(v) => v.as_f64().ok_or("Forms cursor number")?,
            _ => return Err("Forms cursor comparison requires number".into()),
        };
        if number < 0.0 || number > self.text_len() as f64 {
            return Ok(());
        }
        if value.as_number().is_some_and(serde_json::Number::is_f64) {
            self.cursor = number as usize;
            self.cursor_repr = Some(value.clone());
            self.blink = 0;
            self.show = true;
            if number > 0.0 && self.visible != 0.0 {
                return Err("Forms cursor cannot slice with float".into());
            }
        } else {
            self.cursor(number as i64, canvas, now, scale)?;
            if value.is_boolean() {
                self.cursor_repr = Some(value.clone());
            }
        }
        Ok(())
    }
    pub fn cursor(&mut self, at: i64, canvas: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        if at >= 0 && (at as usize) <= self.text_len() {
            self.cursor = at as usize;
            self.cursor_repr = None;
            self.blink = 0;
            self.show = true;
            self.offset(canvas, now, scale)?;
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        self.list_text = None;
        self.text.clear();
        self.cursor = 0;
        self.cursor_repr = None;
        self.offset = 0.0;
    }
    fn process(&mut self, key: i64, canvas: &mut dyn Canvas, now: f64, scale: f64) -> Result<()> {
        let float_cursor = self
            .cursor_repr
            .as_ref()
            .and_then(serde_json::Value::as_number)
            .is_some_and(serde_json::Number::is_f64);
        if float_cursor {
            match key {
                263 if self.cursor > 0 => {
                    let value = self.cursor_repr.as_ref().unwrap().as_f64().unwrap() - 1.0;
                    return self.cursor_value(&serde_json::json!(value), canvas, now, scale);
                }
                262 if self.cursor < self.text_len() => {
                    let value = self.cursor_repr.as_ref().unwrap().as_f64().unwrap() + 1.0;
                    return self.cursor_value(&serde_json::json!(value), canvas, now, scale);
                }
                259 if self.cursor > 0 => {
                    return Err("Forms deletion cannot slice with float".into())
                }
                261 if self.cursor < self.text_len() => {
                    return Err("Forms deletion cannot slice with float".into())
                }
                _ => {}
            }
        }
        match key {
            263 if self.cursor > 0 => self.cursor(self.cursor as i64 - 1, canvas, now, scale)?,
            262 if self.cursor < self.text_len() => {
                self.cursor(self.cursor as i64 + 1, canvas, now, scale)?
            }
            259 if self.cursor > 0 => {
                self.remove_text(self.cursor - 1);
                self.cursor(self.cursor as i64 - 1, canvas, now, scale)?;
            }
            261 if self.cursor < self.text_len() => {
                self.remove_text(self.cursor);
                self.cursor(self.cursor as i64, canvas, now, scale)?;
            }
            268 => self.cursor(0, canvas, now, scale)?,
            269 => self.cursor(self.text_len() as i64, canvas, now, scale)?,
            _ => {}
        }
        Ok(())
    }
    pub fn frame(&mut self, canvas: &mut dyn Canvas, frame: InputFrame<'_>) -> Result<()> {
        let InputFrame {
            rect,
            now,
            scale,
            key,
            down,
            character,
            events,
            debug,
        } = frame;
        self.visible = rect.width;
        self.font_size = 80.0;
        canvas.call(&format!(
            "rect {} {} {} {} 4278190080",
            rect.x, rect.y, rect.width, rect.height
        ))?;
        if key != 0 {
            self.process(key, canvas, now, scale)?;
            if matches!(key, 263 | 262 | 259 | 261) {
                self.last_key = key;
                self.key_time = 0;
            }
        } else if self.last_key != 0 {
            if down.contains(&self.last_key) {
                self.key_time += 1;
                if self.key_time > 30 && self.key_time.is_multiple_of(4) {
                    self.process(self.last_key, canvas, now, scale)?;
                }
            } else {
                self.last_key = 0;
            }
        }
        if let Some(c) = character.filter(|&c| c >= 32) {
            if (self.text_len() as i128)
                < i128::from(self.max.ok_or("Forms cannot compare text length to None")?)
            {
                if self
                    .cursor_repr
                    .as_ref()
                    .and_then(serde_json::Value::as_number)
                    .is_some_and(serde_json::Number::is_f64)
                {
                    return Err("Forms insertion cannot slice with float".into());
                }
                if self.list_text.is_some() {
                    return Err("Forms cannot concatenate Python list and text".into());
                }
                self.text.insert(self.cursor, c);
                self.cursor(self.cursor as i64 + 1, canvas, now, scale)?;
                if self.password {
                    self.last_char = now;
                }
            }
        }
        self.blink += 1;
        if self.blink >= 30 {
            self.show = !self.show;
            self.blink = 0;
        }
        if self
            .cursor_repr
            .as_ref()
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|v| {
                self.cursor_repr
                    .as_ref()
                    .unwrap()
                    .as_number()
                    .is_some_and(serde_json::Number::is_f64)
                    && v > 0.0
            })
            && self.text_len() != 0
        {
            return Err("Forms cursor cannot slice with float".into());
        }
        canvas.call(&format!(
            "clip {} {} {} {}",
            (rect.x + 8.0).trunc(),
            rect.y.trunc(),
            (rect.width - 16.0).trunc(),
            rect.height.trunc()
        ))?;
        // CFFI encodes this whole display string at draw_text_ex, after the
        // original rectangle and clipping setup. Measurements above encode
        // only the cursor prefix actually consumed by that operation.
        let display = self.display(now)?;
        canvas.call(&format!(
            "text {} {} {} {} 4294967295",
            hex(&display),
            (rect.x + 10.0 - self.offset).trunc(),
            (rect.y + rect.height / 2.0 - 80.0 * scale / 2.0).trunc(),
            80.0 * scale
        ))?;
        if self.show {
            let text = display.chars().take(self.cursor).collect::<String>();
            let x = rect.x
                + 10.0
                + if !display.is_empty() && self.cursor > 0 {
                    text_width(canvas, &text, 80.0, scale)?
                } else {
                    0.0
                }
                - self.offset;
            let height = 80.0 * scale + 4.0;
            let y = rect.y + rect.height / 2.0 - height / 2.0;
            canvas.call(&format!(
                "line {} {} {} {} 4294967295",
                x.trunc(),
                y.trunc(),
                x.trunc(),
                (y + height).trunc()
            ))?;
        }
        canvas.call("unclip")?;
        if debug {
            canvas.call(&format!(
                "outline {} {} {} {} 4281805286",
                rect.x.trunc(),
                rect.y.trunc(),
                rect.width.trunc().max(1.0),
                rect.height.trunc().max(1.0)
            ))?;
        }
        // Widget dispatch occurs after drawing; snapshots observe the click result.
        for event in events {
            let slot = &event["slot"];
            let float_zero = slot.as_number().is_some_and(serde_json::Number::is_f64)
                && slot.as_f64() == Some(0.0);
            if slot.as_i64() != Some(0) && slot.as_bool() != Some(false) && !float_zero {
                continue;
            }
            let x = event["pos"]["x"].as_f64().ok_or("forms mouse x")?;
            let y = event["pos"]["y"].as_f64().ok_or("forms mouse y")?;
            let inside =
                x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
            // Python first compares slot != 0 numerically, then indexes its
            // per-slot lists lazily. A floating zero passes the comparison but
            // cannot index; an ignored nonzero slot never reaches this stage.
            let pressed = match &event["pressed"] {
                serde_json::Value::Bool(v) => *v,
                serde_json::Value::Null => false,
                serde_json::Value::Number(v) => v.as_f64() != Some(0.0),
                serde_json::Value::String(v) => !v.is_empty(),
                serde_json::Value::Array(v) => !v.is_empty(),
                serde_json::Value::Object(v) => !v.is_empty(),
            };
            if float_zero && (!pressed || inside) {
                return Err("original float mouse slot cannot index widget state".into());
            }
            if pressed {
                if inside {
                    self.pressed = true;
                    self.tracking = true;
                }
            } else if event["released"] == true {
                if self.pressed && inside {
                    let display = self.display(now)?;
                    let mut best = 0;
                    let mut distance = f64::INFINITY;
                    let relative = x - (rect.x + 10.0) + self.offset;
                    for i in 0..=self.text_len() {
                        let w = text_width(
                            canvas,
                            &display.chars().take(i).collect::<String>(),
                            80.0,
                            scale,
                        )?;
                        if (relative - w).abs() < distance {
                            distance = (relative - w).abs();
                            best = i;
                        }
                    }
                    self.cursor(best as i64, canvas, now, scale)?;
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
        Ok(())
    }
    pub fn snapshot_json(&self, now: f64) -> Result<openpilot_log_format::JsonValue> {
        use openpilot_log_format::JsonValue;
        snapshot_object(&[
            ("text", self.text_json()?),
            (
                "cursor",
                JsonValue::parse(
                    &self
                        .cursor_repr
                        .clone()
                        .unwrap_or_else(|| self.cursor.into())
                        .to_string(),
                )?,
            ),
            (
                "offset",
                JsonValue::parse(&serde_json::to_string(&self.offset)?)?,
            ),
            (
                "show_cursor",
                JsonValue::parse(if self.show { "true" } else { "false" })?,
            ),
            ("display", self.display_json(now)?),
        ])
    }
    pub fn snapshot(&self, now: f64) -> Result<serde_json::Value> {
        Ok(serde_json::from_str(&self.snapshot_json(now)?.to_json()?)?)
    }
}
