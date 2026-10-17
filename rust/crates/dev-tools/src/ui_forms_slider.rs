//! Original larger-slider state/filter/resource and shimmer policy.
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Rect, hex},
    ui_forms_reference::{assets, controls as c, html::Html},
};
use std::path::Path;
pub struct Slider {
    text: Vec<u32>,
    raw_text: Option<openpilot_log_format::JsonValue>,
    position: f64,
    scroll: f64,
    start: f64,
    dragging: bool,
    press: Option<f64>,
    confirmed: f64,
    called: bool,
    scale: f64,
    velocity: f64,
    tracking: bool,
    pressed: bool,
    images: Vec<(f64, f64)>,
    pub count: usize,
}
impl Slider {
    pub fn new_codepoints(
        root: &Path,
        text: &[u32],
        green: bool,
        api: &mut dyn Canvas,
    ) -> Result<Self> {
        let stem = if green {
            "slider_green_rounded_rectangle"
        } else {
            "slider_black_rounded_rectangle"
        };
        let mut images = vec![];
        for (i, (name, w, h)) in [
            ("slider_bg_larger".into(), 520.0, 115.0),
            (stem.into(), 180.0, 115.0),
            (format!("{stem}_pressed"), 180.0, 115.0),
            ("slider_arrow".into(), 64.0, 55.0),
        ]
        .into_iter()
        .enumerate()
        {
            images.push(assets::load(
                api,
                root,
                i,
                &format!("icons_mici/setup/small_slider/{name}.png"),
                w,
                h,
                true,
            )?);
        }
        Ok(Self {
            text: text.to_vec(),
            raw_text: None,
            position: 0.0,
            scroll: 0.0,
            start: 0.0,
            dragging: false,
            press: None,
            confirmed: 0.0,
            called: false,
            scale: 1.0,
            velocity: 0.0,
            tracking: false,
            pressed: false,
            images,
            count: 0,
        })
    }
    pub fn set_value(&mut self, value: openpilot_log_format::JsonValue) -> Result<()> {
        self.raw_text = Some(value);
        Ok(())
    }
    fn text_points(&self) -> Result<Vec<u32>> {
        use openpilot_log_format::JsonView;
        Ok(
            match self
                .raw_text
                .as_ref()
                .map(openpilot_log_format::JsonValue::view)
            {
                None => self.text.clone(),
                Some(JsonView::Null) => Vec::new(),
                Some(JsonView::Text(points)) => points.to_vec(),
                Some(_) => {
                    let value = self.raw_text.as_ref().unwrap();
                    // The pinned source's later str(value) raises RecursionError
                    // above 9994 containers. Both list/dict edge captures retain
                    // the last success, first failure, and a deeper zero-frame
                    // success. This property is consumed only during layout.
                    let mut pending = vec![(value.clone(), 0usize)];
                    while let Some((value, depth)) = pending.pop() {
                        let children = match value.view() {
                            JsonView::Array(values) => values,
                            JsonView::Object(values) => {
                                values.into_iter().map(|(_, v)| v).collect()
                            }
                            _ => continue,
                        };
                        let depth = depth + 1;
                        if depth > 9994 {
                            return Err("original UnifiedLabel Python repr recursion limit".into());
                        }
                        pending.extend(children.into_iter().map(|value| (value, depth)));
                    }
                    crate::ui_forms_check::repr(value)
                        .chars()
                        .map(u32::from)
                        .collect()
                }
            },
        )
    }
    fn percentage(&self) -> f64 {
        (-self.position / (self.images[0].0 - self.images[1].0).abs()).clamp(0.0, 1.0)
    }
    fn event(&mut self, e: &serde_json::Value, r: Rect, now: f64) {
        let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
        let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
        if e["pressed"] == true {
            let left = r.x + (r.width - self.images[1].0) + self.position - 16.0;
            if x >= left && x <= left + self.images[1].0 + 16.0 && y >= r.y && y <= r.y + r.height {
                self.start = x;
                self.dragging = true;
                self.press = Some(now);
            }
        } else if e["released"] == true {
            if self.position < (-536.0f64 / 2.0).floor() {
                self.confirmed = now;
            }
            self.dragging = false;
        }
        if self.dragging {
            self.scroll = x - self.start;
        }
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        scale: f64,
        now: f64,
        events: &[serde_json::Value],
    ) -> Result<()> {
        let activated = (-self.images[0].0 + self.images[1].0).trunc();
        self.scroll = self.scroll.clamp(activated, 0.0);
        if self.confirmed > 0.0 {
            self.position = 0.5 * self.position + 0.5 * activated;
            if self.position < activated + 1.0 && !self.called && now - self.confirmed >= 0.2 {
                self.called = true;
                self.count += 1;
            }
        } else if !self.dragging {
            self.position *= 0.5;
        } else {
            self.position = self.scroll;
        }
        let bgx = r.x + (r.width - self.images[0].0) / 2.0;
        let bgy = r.y + (r.height - self.images[0].1) / 2.0;
        api.call(&format!("texture 0 {bgx} {bgy} 1 4294967295"))?;
        let btnx = bgx + self.images[0].0 - self.images[1].0 + self.position;
        let btny = r.y + (r.height - self.images[1].1) / 2.0;
        let alpha = (255.0 * (1.0 - self.percentage())).trunc() as u8;
        if alpha > 0 {
            let label = Rect {
                x: r.x + 20.0,
                y: r.y,
                width: r.width - self.images[1].0 - 50.0,
                height: r.height,
            }
            .cast();
            self.label(api, label, scale, now, alpha)?;
        }
        let pressed =
            self.dragging || self.confirmed > 0.0 || self.press.is_some_and(|t| now - t < 0.075);
        let target = if pressed { 1.07 } else { 1.0 };
        let a = 0.05 / (0.1 + 0.05);
        self.scale = (1.0 - a) * self.scale + a * target;
        self.velocity += (target - self.scale) * 2.0 * 3.0 * 0.05;
        self.velocity *= 1.0 - 0.05 / (0.15 + 0.05);
        if self.velocity.abs() < 1e-3 {
            self.velocity = 0.0;
        }
        self.scale += self.velocity;
        let x = btnx + self.images[1].0 * (1.0 - self.scale) / 2.0;
        let y = btny + self.images[1].1 * (1.0 - self.scale) / 2.0;
        api.call(&format!(
            "texture {} {x} {y} {} 4294967295",
            if pressed { 2 } else { 1 },
            self.scale
        ))?;
        api.call(&format!(
            "texture 3 {} {} 1 4294967295",
            btnx + (self.images[1].0 - self.images[3].0) / 2.0,
            y + (self.images[1].1 - self.images[3].1) / 2.0
        ))?;
        for e in events {
            if !crate::ui_forms_input::single_touch_event(e, r)? {
                continue;
            }
            let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
            let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
            let inside = x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height;
            if e["pressed"] == true {
                if inside {
                    crate::ui_forms_input::single_touch_index(e)?;
                    self.pressed = true;
                    self.tracking = true;
                    self.event(e, r, now);
                }
            } else if e["released"] == true {
                self.event(e, r, now);
                crate::ui_forms_input::single_touch_index(e)?;
                self.pressed = false;
                self.tracking = false;
            } else if inside {
                crate::ui_forms_input::single_touch_index(e)?;
                if self.tracking {
                    self.pressed = true;
                    self.event(e, r, now);
                }
            } else {
                crate::ui_forms_input::single_touch_index(e)?;
                self.pressed = false;
                self.event(e, r, now);
            }
        }
        Ok(())
    }
    fn label(
        &self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        now: f64,
        alpha: u8,
    ) -> Result<()> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        c::font(api, 3)?;
        let width = rect.width.trunc().max(1.0);
        let mut lines = Html::wrap_codepoints(api, &self.text_points()?, 36.0, width, scale)?;
        let mut sizes = Vec::new();
        for line in &mut lines {
            *line = elide_codepoints(api, line, width, scale, false)?;
            sizes.push(crate::ui_forms_input::text_extent_codepoints(
                api, line, 36.0, scale,
            )?);
        }
        // UnifiedLabel.text_width uses the cached sizes of every wrapped line,
        // before height clipping or forced ellipsis changes the visible lines.
        let shimmer_width = sizes.iter().map(|v| v.0).fold(0.0f64, f64::max);
        let mut height = 0.0;
        let mut visible = 0;
        for (_, h) in &sizes {
            if height + h * 0.81 > rect.height && visible != 0 {
                break;
            }
            height += h * 0.81;
            visible += 1;
        }
        if visible < lines.len() && visible != 0 {
            lines[visible - 1] = elide_codepoints(api, &lines[visible - 1], width, scale, true)?;
            sizes[visible - 1] = crate::ui_forms_input::text_extent_codepoints(
                api,
                &lines[visible - 1],
                36.0,
                scale,
            )?;
        }
        lines.truncate(visible);
        sizes.truncate(visible);
        let mut total = 0.0;
        for (i, (_, h)) in sizes.iter().enumerate() {
            total += h * if i == 0 { 1.0 } else { 0.81 };
        }
        let mut y = rect.y + (rect.height - total) / 2.0;
        for (line, (w, h)) in lines.iter().zip(sizes) {
            let mut x = rect.x + rect.width - w;
            let left = rect.x + rect.width - shimmer_width;
            let sigma = shimmer_width * 0.12;
            let t = ((now % 2.5) / 2.5 / 0.9).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            let center = left + shimmer_width + shimmer_width * 0.3
                - t * (shimmer_width + shimmer_width * 0.6);
            for point in line {
                let ch = char::from_u32(*point).ok_or("slider draw cannot encode surrogate")?;
                let width = crate::ui_forms_input::text_width(api, &ch.to_string(), 36.0, scale)?;
                let d = x + width / 2.0 - center;
                let shimmer = if sigma <= 0.0 {
                    0.65
                } else {
                    0.65 + 0.35 * (-0.5 * d * d / (sigma * sigma)).exp()
                };
                let a = (255.0 * shimmer * (f64::from(alpha) / 255.0)).trunc() as u8;
                api.call(&format!(
                    "text {} {x} {y} {} {}",
                    hex(&ch.to_string()),
                    36.0 * scale,
                    c::color(255, 255, 255, a)
                ))?;
                x += width;
            }
            y += h * 0.81;
        }
        c::debug_rect(api, rect)
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"confirmed":self.confirmed>0.0,"percentage":self.percentage(),"position":self.position,"scale":self.scale,"dragging":self.dragging})
    }
}

fn elide_codepoints(
    api: &mut dyn Canvas,
    line: &[u32],
    width: f64,
    scale: f64,
    force: bool,
) -> Result<Vec<u32>> {
    use crate::ui_forms_input::text_width_codepoints as text_width;
    let current = text_width(api, line, 36.0, scale)?;
    if current <= width {
        if !force {
            return Ok(line.to_vec());
        }
        if current + text_width(api, &[46, 46, 46], 36.0, scale)? <= width {
            return Ok([line, &[46, 46, 46]].concat());
        }
    }
    let chars = line.to_vec();
    let (mut left, mut right) = (0, chars.len());
    while left < right {
        let mid = (left + right) / 2;
        let candidate = [&chars[..mid], &[46, 46, 46]].concat();
        if text_width(api, &candidate, 36.0, scale)? <= width {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    Ok(if left > 0 {
        [&chars[..left - 1], &[46, 46, 46]].concat()
    } else {
        vec![46, 46, 46]
    })
}
