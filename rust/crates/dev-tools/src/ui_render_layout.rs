//! Independent Label/UnifiedLabel/Button/Toggle drawing policy from pinned source.
use crate::{
    seal::Result,
    ui_forms_input::{emoji, hex, Rect},
};
use serde_json::{json, Value};
pub trait Api {
    fn call(&mut self, command: &str) -> Result<()>;
    fn measure(&mut self, text: &str, size: f64, spacing: f64) -> Result<(f64, f64)>;
    fn emoji(&mut self, text: &str, x: f64, y: f64, size: f64, rgba: u32) -> Result<()>;
}
pub fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn number(p: &Value, key: &str, default: f64) -> Result<f64> {
    match p.get(key) {
        None => Ok(default),
        Some(Value::Bool(v)) => Ok(f64::from(u8::from(*v))),
        Some(v) => v
            .as_f64()
            .ok_or_else(|| format!("render {key} must numeric").into()),
    }
}
fn word<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    p.get(key)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| format!("render {key} must string").into())
        })
        .unwrap_or(Ok(default))
}
fn enabled(p: &Value, key: &str, default: bool) -> bool {
    p.get(key).map(truth).unwrap_or(default)
}
fn f32v(v: f64) -> f64 {
    f64::from(v as f32)
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    u32::from_le_bytes([r, g, b, a])
}
pub fn extent(
    api: &mut dyn Api,
    text: &str,
    size: f64,
    scale: f64,
    spacing: f64,
) -> Result<(f64, f64)> {
    let mut normal = String::new();
    let mut groups = 0;
    let mut was = false;
    for c in text.chars() {
        let found = emoji(c);
        if found {
            if !was {
                groups += 1;
            }
        } else {
            normal.push(c);
        }
        was = found;
    }
    let (w, h) = api.measure(
        &normal,
        size * scale,
        (spacing * 10000.0).round_ties_even() / 10000.0,
    )?;
    Ok(if groups == 0 {
        (w, h)
    } else {
        (
            f32v(w + f64::from(groups) * size * scale),
            if h == 0.0 { f32v(size * scale) } else { h },
        )
    })
}
fn width(api: &mut dyn Api, text: &str, size: f64, scale: f64, spacing: f64) -> Result<f64> {
    Ok(extent(api, text, size, scale, spacing)?.0)
}
fn long_word(
    api: &mut dyn Api,
    word: &str,
    size: f64,
    scale: f64,
    spacing: f64,
    max: f64,
) -> Result<Vec<String>> {
    let mut remaining = word.chars().collect::<Vec<_>>();
    let mut out = vec![];
    while !remaining.is_empty() {
        if width(
            api,
            &remaining.iter().collect::<String>(),
            size,
            scale,
            spacing,
        )? <= max
        {
            out.push(remaining.iter().collect());
            break;
        }
        let (mut left, mut right, mut best) = (1, remaining.len(), 1);
        while left <= right {
            let mid = (left + right) / 2;
            if width(
                api,
                &remaining[..mid].iter().collect::<String>(),
                size,
                scale,
                spacing,
            )? <= max
            {
                best = mid;
                left = mid + 1;
            } else {
                right = mid - 1;
            }
        }
        out.push(remaining[..best].iter().collect());
        remaining.drain(..best);
    }
    Ok(out)
}
fn wrap(
    api: &mut dyn Api,
    text: &str,
    size: f64,
    scale: f64,
    spacing: f64,
    max: f64,
) -> Result<Vec<String>> {
    if text.is_empty() || max <= 0.0 {
        return Ok(vec![]);
    }
    let mut lines = vec![];
    for paragraph in text.split('\n') {
        let python_space = |c: char| crate::ui_forms_input::python_space(u32::from(c));
        if paragraph.trim_matches(python_space).is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph
            .split(python_space)
            .filter(|word| !word.is_empty())
        {
            if width(api, word, size, scale, spacing)? > max {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                lines.extend(long_word(api, word, size, scale, spacing, max)?);
                continue;
            }
            let candidate = if line.is_empty() {
                word.into()
            } else {
                format!("{line} {word}")
            };
            if width(api, &candidate, size, scale, spacing)? <= max {
                line = candidate;
            } else {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                line = word.into();
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    Ok(lines)
}
fn elide(
    api: &mut dyn Api,
    text: &str,
    size: f64,
    scale: f64,
    spacing: f64,
    max: f64,
    force: bool,
) -> Result<String> {
    let w = width(api, text, size, scale, spacing)?;
    if w <= max {
        if !force {
            return Ok(text.into());
        }
        if w + width(api, "...", size, scale, spacing)? <= max {
            return Ok(format!("{text}..."));
        }
    }
    let chars = text.chars().collect::<Vec<_>>();
    let (mut left, mut right) = (0, chars.len());
    while left < right {
        let mid = (left + right) / 2;
        let candidate = chars[..mid].iter().collect::<String>() + "...";
        if width(api, &candidate, size, scale, spacing)? <= max {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    Ok(if left > 0 {
        chars[..left - 1].iter().collect::<String>() + "..."
    } else {
        "...".into()
    })
}
#[allow(clippy::too_many_arguments)]
fn ordinary_line(
    api: &mut dyn Api,
    text: &str,
    at: (f64, f64),
    size: f64,
    scale: f64,
    spacing: f64,
    color: u32,
    empty_calls: bool,
) -> Result<()> {
    let (mut x, y) = (f32v(at.0), f32v(at.1));
    let mut ordinary = String::new();
    let mut pictograph = String::new();
    // A separate end marker preserves embedded NUL in each C text segment.
    for c in text.chars().map(Some).chain(std::iter::once(None)) {
        if c.is_some_and(emoji) {
            if pictograph.is_empty() {
                if empty_calls || !ordinary.is_empty() {
                    api.call(&format!(
                        "text-spaced {} {x} {y} {} {spacing} {color}",
                        hex(&ordinary),
                        size * scale
                    ))?;
                }
                x = f32v(x + width(api, &ordinary, size, scale, spacing)?);
                ordinary.clear();
            }
            pictograph.push(c.unwrap());
        } else {
            if !pictograph.is_empty() {
                api.emoji(&pictograph, x, y, size * scale, color)?;
                x = f32v(x + size * scale);
                pictograph.clear();
            }
            if let Some(c) = c {
                ordinary.push(c);
            }
        }
    }
    if empty_calls || !ordinary.is_empty() {
        api.call(&format!(
            "text-spaced {} {x} {y} {} {spacing} {color}",
            hex(&ordinary),
            size * scale
        ))?;
    }
    Ok(())
}
#[derive(Default)]
struct Touch {
    pressed: bool,
    tracking: bool,
}
impl Touch {
    fn update(&mut self, r: Rect, events: &[super::scene_json::Event]) -> Result<usize> {
        let mut clicks = 0;
        for e in events {
            if !e.primary() {
                continue;
            }
            let (x, y) = e.position()?;
            let inside = x >= r.x
                && x <= f64::from(r.x as f32 + r.width as f32)
                && y >= r.y
                && y <= f64::from(r.y as f32 + r.height as f32);
            let press = e.pressed;
            let release = e.released;
            if (!press || inside) && e.float_slot() {
                return Err("original float touch slot cannot index widget state".into());
            }
            if press {
                if inside {
                    self.pressed = true;
                    self.tracking = true;
                }
            } else if release {
                if self.pressed && inside {
                    clicks += 1;
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
        Ok(clicks)
    }
}
pub struct Widget {
    kind: String,
    text: String,
    raw_text: Option<openpilot_log_format::JsonValue>,
    rect: Rect,
    p: Value,
    enabled: bool,
    font: usize,
    touch: Touch,
    value: openpilot_log_format::JsonValue,
    progress: f64,
    target: f64,
    cached: Vec<(String, (f64, f64))>,
    needs_scroll: bool,
    scroll: f64,
    pause: Option<f64>,
    scrolling: bool,
}
impl Widget {
    pub fn new(kind: &str, text: &str, rect: Rect, p: Value) -> Result<Self> {
        if !matches!(kind, "label" | "unified" | "button" | "toggle") {
            return Err("original render widget kind missing".into());
        }
        let font = if matches!(kind, "label" | "unified") {
            match word(&p, "font", "normal")? {
                "normal" => 0,
                "medium" => 1,
                "bold" => 2,
                "semi_bold" => 3,
                "display" => 4,
                "regular" => 5,
                "pretendard" => 6,
                "unifont" => 7,
                _ => return Err("original render font selector missing".into()),
            }
        } else if kind == "button" {
            1
        } else {
            0
        };
        if kind == "button"
            && !matches!(
                word(&p, "style", "normal")?,
                "normal" | "primary" | "danger" | "list" | "border"
            )
        {
            return Err("original button style missing".into());
        }
        if matches!(kind, "label" | "unified")
            && (!matches!(
                word(
                    &p,
                    "horizontal",
                    if kind == "label" { "center" } else { "left" }
                )?,
                "left" | "center" | "right"
            ) || !matches!(
                word(
                    &p,
                    "vertical",
                    if kind == "label" { "middle" } else { "top" }
                )?,
                "top" | "middle" | "bottom"
            ))
        {
            return Err("original render alignment selector missing".into());
        }
        if kind == "unified" {
            // The source multiplies line_height by float0.9 immediately,
            // even before any render frame consumes its cache.
            number(&p, "line_height", 1.0)?;
        }
        let value = p.get("value").cloned().unwrap_or(Value::Bool(false));
        let progress = if truth(&value) { 1.0 } else { 0.0 };
        Ok(Self {
            kind: kind.into(),
            text: text.into(),
            raw_text: None,
            rect: if kind == "toggle" {
                Rect {
                    width: 160.0,
                    height: 80.0,
                    ..rect
                }
                .cast()
            } else {
                rect.cast()
            },
            enabled: enabled(&p, "enabled", true),
            p,
            font,
            touch: Touch::default(),
            value: openpilot_log_format::JsonValue::parse(&serde_json::to_string(&value)?)?,
            progress,
            target: progress,
            cached: vec![],
            needs_scroll: false,
            scroll: 0.0,
            pause: None,
            scrolling: false,
        })
    }
    pub fn set_raw_values(
        &mut self,
        text: openpilot_log_format::JsonValue,
        value: openpilot_log_format::JsonValue,
    ) -> Result<()> {
        // Label's constructor calls wrap_text even against its initial zero
        // rectangle. Its cache hashes containers before the width short circuit.
        if matches!(self.kind.as_str(), "label" | "button")
            && matches!(
                text.view(),
                openpilot_log_format::JsonView::Array(_)
                    | openpilot_log_format::JsonView::Object(_)
            )
        {
            return Err("original label cache key is unhashable".into());
        }
        self.raw_text = Some(text);
        self.progress = if crate::ui_forms_input::python_truth(&value) {
            1.0
        } else {
            0.0
        };
        self.target = self.progress;
        self.value = value;
        Ok(())
    }
    pub fn initialize(&mut self, api: &mut dyn Api, scale: f64) -> Result<()> {
        if matches!(self.kind.as_str(), "label" | "button") && enabled(&self.p, "elide", false) {
            api.call(&format!("font-select {}", self.font))?;
            self.prepare_text()?;
            let size = number(&self.p, "size", 60.0)?;
            // Label.__init__ computes its initial elision before set_rect.
            elide(api, &self.text, size, scale, 0.0, 0.0, false)?;
        }
        Ok(())
    }
    fn prepare_text(&mut self) -> Result<()> {
        use openpilot_log_format::JsonView;
        let Some(value) = &self.raw_text else {
            return Ok(());
        };
        if matches!(self.kind.as_str(), "label" | "button") {
            let padding = if self.kind == "button" {
                20.0
            } else {
                number(&self.p, "padding", 0.0)?
            };
            if !enabled(&self.p, "elide", false)
                && (self.rect.width - padding * 2.0).round_ties_even() <= 0.0
            {
                self.text.clear();
                return Ok(());
            }
        }
        self.text = match value.view() {
            JsonView::Text(points) => crate::ui_forms_input::Input::utf8(points)?,
            JsonView::Null => String::new(),
            _ if self.kind == "unified" => crate::ui_forms_check::repr(value),
            _ if !crate::ui_forms_input::python_truth(value) => String::new(),
            _ => return Err("original label requires Python text".into()),
        };
        Ok(())
    }
    fn alignment(&self, plain: bool) -> Result<(u8, u8)> {
        let h = match word(&self.p, "horizontal", if plain { "center" } else { "left" })? {
            "left" => 0,
            "center" => 1,
            "right" => 2,
            _ => return Err("original horizontal selector missing".into()),
        };
        let v = match word(&self.p, "vertical", if plain { "middle" } else { "top" })? {
            "top" => 0,
            "middle" => 1,
            "bottom" => 2,
            _ => return Err("original vertical selector missing".into()),
        };
        Ok((h, v))
    }
    pub fn render(
        &mut self,
        api: &mut dyn Api,
        scale: f64,
        now: f64,
        events: &[super::scene_json::Event],
    ) -> Result<()> {
        api.call(&format!("font-select {}", self.font))?;
        if self.kind != "unified" || (self.rect.width > 0.0 && self.rect.height > 0.0) {
            self.prepare_text()?;
        }
        match self.kind.as_str() {
            "label" => self.label(api, scale, 229)?,
            "unified" => self.unified(api, scale, now)?,
            "button" => self.button(api, scale)?,
            "toggle" => self.toggle(api)?,
            _ => unreachable!(),
        }
        if std::env::var_os("SHOW_TOUCHES").as_deref() == Some(std::ffi::OsStr::new("1")) {
            api.call(&format!(
                "outline {} {} {} {} 4281805286",
                self.rect.x.trunc(),
                self.rect.y.trunc(),
                self.rect.width.trunc().max(1.0),
                self.rect.height.trunc().max(1.0)
            ))?;
        }
        if self.enabled {
            let clicks = self.touch.update(self.rect, events)?;
            if self.kind == "toggle" {
                // Each release invokes Toggle._handle_mouse_release in order.
                // Even counts still convert an arbitrary initial value to bool.
                for _ in 0..clicks {
                    self.value = openpilot_log_format::JsonValue::parse(
                        if crate::ui_forms_input::python_truth(&self.value) {
                            "false"
                        } else {
                            "true"
                        },
                    )?;
                    self.target = if crate::ui_forms_input::python_truth(&self.value) {
                        1.0
                    } else {
                        0.0
                    };
                }
            }
        } else {
            self.touch = Touch::default();
        }
        Ok(())
    }
    fn label(&self, api: &mut dyn Api, scale: f64, alpha: u8) -> Result<()> {
        let r = self.rect;
        let size = number(&self.p, "size", 60.0)?;
        let padding = number(&self.p, "padding", 0.0)?;
        let (h, v) = self.alignment(true)?;
        let lines = if enabled(&self.p, "elide", false) {
            vec![elide(
                api,
                &self.text,
                size,
                scale,
                0.0,
                r.width - padding * 2.0,
                false,
            )?]
        } else {
            wrap(
                api,
                &self.text,
                size,
                scale,
                0.0,
                (r.width - padding * 2.0).round_ties_even(),
            )?
        };
        let mut measured = vec![];
        for line in &lines {
            measured.push(extent(api, line, size, scale, 0.0)?);
        }
        let total = measured.iter().map(|v| v.1).sum::<f64>();
        let total = if total == 0.0 { size * scale } else { total };
        let mut y = f32v(if v == 1 {
            r.y + ((r.height - total) / 2.0).floor()
        } else {
            r.y
        });
        for (line, (w, height)) in lines.iter().zip(measured) {
            let x = f32v(match h {
                0 => r.x + padding,
                1 => r.x + ((r.width - w) / 2.0).floor(),
                _ => r.x + r.width - w - padding,
            });
            ordinary_line(
                api,
                line,
                (x, y),
                size,
                scale,
                0.0,
                rgba(255, 255, 255, alpha),
                true,
            )?;
            y = f32v(y + if height == 0.0 { size * scale } else { height });
        }
        Ok(())
    }
    fn button(&self, api: &mut dyn Api, scale: f64) -> Result<()> {
        let style = word(&self.p, "style", "normal")?;
        let (bg, text) = if !self.enabled {
            (rgba(51, 51, 51, 255), rgba(228, 228, 228, 51))
        } else {
            (
                match (style, self.touch.pressed) {
                    ("normal", false) => rgba(51, 51, 51, 255),
                    ("normal", true) => rgba(74, 74, 74, 255),
                    ("primary", false) => rgba(70, 91, 234, 255),
                    ("primary", true) => rgba(48, 73, 244, 255),
                    ("danger", false) => rgba(226, 44, 44, 255),
                    ("danger", true) => rgba(255, 36, 36, 255),
                    ("list", false) => rgba(57, 57, 57, 255),
                    ("list", true) => rgba(74, 74, 74, 74),
                    ("border", _) => rgba(0, 0, 0, 255),
                    _ => return Err("original button style missing".into()),
                },
                rgba(228, 228, 228, 255),
            )
        };
        let r = self.rect;
        let roundness = 10.0 / (r.width.min(r.height) / 2.0);
        api.call(&format!(
            "rounded {} {} {} {} {roundness} 10 {bg}",
            r.x, r.y, r.width, r.height
        ))?;
        if style == "border" {
            api.call(&format!(
                "rounded-outline {} {} {} {} {roundness} 10 2 4294967295",
                r.x, r.y, r.width, r.height
            ))?;
        }
        let size = number(&self.p, "size", 60.0)?;
        let lines = wrap(
            api,
            &self.text,
            size,
            scale,
            0.0,
            (r.width - 40.0).round_ties_even(),
        )?;
        let mut measures = vec![];
        for line in &lines {
            measures.push(extent(api, line, size, scale, 0.0)?);
        }
        let total = measures.iter().map(|v| v.1).sum::<f64>();
        let total = if total == 0.0 { size * scale } else { total };
        let mut y = f32v(r.y + ((r.height - total) / 2.0).floor());
        for (line, (w, h)) in lines.iter().zip(measures) {
            ordinary_line(
                api,
                line,
                (r.x + ((r.width - w) / 2.0).floor(), y),
                size,
                scale,
                0.0,
                text,
                true,
            )?;
            y = f32v(y + if h == 0.0 { size * scale } else { h });
        }
        Ok(())
    }
    fn toggle(&mut self, api: &mut dyn Api) -> Result<()> {
        if (self.progress - self.target).abs() > 0.01 {
            self.progress += (if self.progress < self.target {
                1.0
            } else {
                -1.0
            }) * 0.4;
            self.progress = self.progress.clamp(0.0, 1.0);
        }
        let (from, to, knob) = if self.enabled {
            ([57., 57., 57.], [51., 171., 76.], rgba(255, 255, 255, 255))
        } else {
            ([57., 57., 57.], [34., 119., 34.], rgba(136, 136, 136, 255))
        };
        let c = rgba(
            (from[0] + (to[0] - from[0]) * self.progress) as u8,
            (from[1] + (to[1] - from[1]) * self.progress) as u8,
            (from[2] + (to[2] - from[2]) * self.progress) as u8,
            255,
        );
        let r = self.rect;
        api.call(&format!(
            "rounded {} {} 150 60 1 10 {c}",
            r.x + 5.0,
            r.y + 10.0
        ))?;
        api.call(&format!(
            "circle {} {} 40 {knob}",
            (r.x + 40.0 + 80.0 * self.progress).trunc(),
            (r.y + 40.0).trunc()
        ))
    }
    fn unified(&mut self, api: &mut dyn Api, scale: f64, now: f64) -> Result<()> {
        let r = self.rect;
        if r.width <= 0.0 || r.height <= 0.0 {
            return Ok(());
        }
        let size = number(&self.p, "size", 60.0)?;
        let spacing = size * number(&self.p, "letter_spacing", 0.0)?;
        let padding = number(&self.p, "padding", 0.0)?;
        let scroll = enabled(&self.p, "scroll", false);
        let eliding = !scroll && enabled(&self.p, "elide", true);
        let wrap_text = !scroll && enabled(&self.p, "wrap", true);
        let line_height = number(&self.p, "line_height", 1.0)? * 0.9;
        let (h, v) = self.alignment(false)?;
        let content = (r.width.trunc() - padding * 2.0).max(1.0);
        if self.cached.is_empty() {
            let mut lines = if wrap_text {
                wrap(api, &self.text, size, scale, spacing, content)?
            } else {
                self.text.split('\n').map(str::to_owned).collect()
            };
            if scroll {
                lines.truncate(1);
            }
            for mut line in lines {
                if eliding {
                    line = elide(api, &line, size, scale, spacing, content, false)?;
                }
                let extent = if line.is_empty() {
                    (0.0, f32v(size * scale))
                } else {
                    extent(api, &line, size, scale, spacing)?
                };
                self.cached.push((line, extent));
            }
            self.needs_scroll = scroll && self.cached.first().is_some_and(|v| v.1 .0 > content);
        }
        let mut visible = vec![];
        let mut used = 0.0;
        for (line, extent) in &self.cached {
            if used + extent.1 * line_height > r.height && !visible.is_empty() {
                break;
            }
            visible.push((line.clone(), *extent));
            used += extent.1 * line_height;
        }
        if visible.len() < self.cached.len() && eliding && !visible.is_empty() {
            let last = visible.last_mut().unwrap();
            last.0 = elide(
                api,
                &last.0,
                size,
                scale,
                spacing,
                (r.width - padding * 2.0).trunc().max(1.0),
                true,
            )?;
            last.1 = extent(api, &last.0, size, scale, spacing)?;
        }
        let total = visible
            .iter()
            .enumerate()
            .map(|(i, (_, e))| e.1 * if i == 0 { 1.0 } else { line_height })
            .sum::<f64>();
        let mut y = match v {
            0 => r.y,
            1 => r.y + (r.height - total) / 2.0,
            _ => r.y + r.height - total,
        };
        if self.needs_scroll {
            api.call(&format!(
                "clip {} {} {} {}",
                r.x.trunc(),
                (r.y - size / 2.0).trunc(),
                r.width.trunc(),
                (r.height + size).trunc()
            ))?;
        }
        for (line, e) in visible {
            if self.needs_scroll {
                if !self.scrolling {
                    if self.pause.is_none() {
                        self.pause = Some(now + 2.0);
                    }
                    if now >= self.pause.unwrap() {
                        self.scrolling = true;
                        self.pause = None;
                    }
                } else {
                    self.scroll -= 0.8 / 60.0 * 20.0;
                    if self.scroll <= -e.0 - r.width / 3.0 {
                        self.scroll = 0.0;
                        self.scrolling = false;
                    }
                }
            } else {
                self.scroll = 0.0;
                self.pause = None;
                self.scrolling = false;
            }
            let x = match h {
                0 => r.x + padding,
                1 => r.x + (r.width - e.0) / 2.0,
                _ => r.x + r.width - e.0 - padding,
            } + self.scroll;
            self.unified_line(api, &line, (x, y), size, scale, spacing, h, now)?;
            if self.needs_scroll && self.scrolling {
                self.unified_line(
                    api,
                    &line,
                    (x + e.0 + r.width / 3.0, y),
                    size,
                    scale,
                    spacing,
                    h,
                    now,
                )?;
            }
            y += e.1 * line_height;
        }
        if self.needs_scroll {
            api.call(&format!(
                "gradient-h {} {} 20 {} 0 4278190080",
                (r.x + r.width - 20.0).trunc(),
                r.y.trunc(),
                r.height.trunc()
            ))?;
            if self.scrolling && self.scroll + self.cached.first().map_or(0.0, |v| v.1 .0) > 0.0 {
                api.call(&format!(
                    "gradient-h {} {} 20 {} 4278190080 0",
                    r.x.trunc(),
                    r.y.trunc(),
                    r.height.trunc()
                ))?;
            }
            api.call("unclip")?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn unified_line(
        &self,
        api: &mut dyn Api,
        line: &str,
        at: (f64, f64),
        size: f64,
        scale: f64,
        spacing: f64,
        h: u8,
        now: f64,
    ) -> Result<()> {
        if !enabled(&self.p, "shimmer", false) {
            return ordinary_line(
                api,
                line,
                at,
                size,
                scale,
                spacing,
                rgba(255, 255, 255, 229),
                false,
            );
        }
        let max = self.cached.iter().map(|v| v.1 .0).fold(0.0, f64::max);
        let r = self.rect;
        let padding = number(&self.p, "padding", 0.0)?;
        let left = match h {
            0 => r.x + padding,
            1 => r.x + (r.width - max) / 2.0,
            _ => r.x + r.width - padding - max,
        };
        let sigma = max * 0.12;
        let t = ((now.rem_euclid(2.5) / 2.5) / 0.9).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let center = left + max + max * 0.3 - t * (max + 2.0 * max * 0.3);
        let mut x = at.0;
        for ch in line.chars() {
            let w = width(api, &ch.to_string(), size, scale, spacing)?;
            let d = x + w / 2.0 - center;
            let shimmer = if sigma <= 0.0 {
                0.65
            } else {
                0.65 + 0.35 * (-0.5 * d * d / (sigma * sigma)).exp()
            };
            let color = rgba(255, 255, 255, (255.0 * shimmer * (229.0 / 255.0)) as u8);
            api.call(&format!(
                "text-spaced {} {x} {} {} 0 {color}",
                hex(&ch.to_string()),
                at.1,
                size * scale
            ))?;
            x += w + spacing;
        }
        Ok(())
    }
    pub fn state_json(&self) -> Result<openpilot_log_format::JsonValue> {
        use openpilot_log_format::JsonValue;
        match self.kind.as_str() {
            "unified" => Ok(JsonValue::parse(&serde_json::to_string(
                &json!({"width":self.cached.iter().map(|v|v.1.0).fold(0.0,f64::max),"scroll_offset":self.scroll}),
            )?)?),
            "toggle" => crate::ui_forms_input::snapshot_object(&[
                ("value", self.value.clone()),
                (
                    "progress",
                    JsonValue::parse(&serde_json::to_string(&self.progress)?)?,
                ),
            ]),
            _ => Ok(JsonValue::parse("null")?),
        }
    }
}
