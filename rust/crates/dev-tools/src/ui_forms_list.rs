//! Independent ListItem and admitted right-action policies from original source.
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Rect},
    ui_forms_reference::{
        controls::{self as c, Button, Touch},
        html::Html,
    },
};
pub struct List {
    title: openpilot_log_format::JsonValue,
    description: Vec<u32>,
    visible: openpilot_log_format::JsonValue,
    rect: Rect,
    html: Html,
    action: String,
    button: Button,
    touch: Touch,
    action_touch: Touch,
    toggle: bool,
    progress: f64,
    selected: usize,
}
impl List {
    pub fn new_codepoints(
        title: &[u32],
        description: &[u32],
        p: &serde_json::Value,
        rect: Rect,
    ) -> Result<Self> {
        let mut button = Button::new("Edit", false);
        button.size = 35.0;
        button.padding = 0.0;
        button.radius = 50.0;
        button.style = 5;
        Ok(Self {
            title: openpilot_log_format::JsonValue::codepoints(title.to_vec())
                .ok_or("List invalid text point")?,
            html: Html::new_codepoints(description, 40.0, false)?,
            description: description.to_vec(),
            visible: openpilot_log_format::JsonValue::parse(if p["description_visible"] == true {
                "true"
            } else {
                "false"
            })?,
            rect,
            action: p["action"].as_str().unwrap_or("toggle").into(),
            button,
            touch: Touch::default(),
            action_touch: Touch::default(),
            toggle: true,
            progress: 1.0,
            selected: 0,
        })
    }
    pub fn set_value(&mut self, value: openpilot_log_format::JsonValue) {
        self.title = value;
    }
    pub fn set_description_visible(&mut self, value: openpilot_log_format::JsonValue) {
        self.visible = value;
    }
    pub fn snapshot_json(
        &self,
        api: &mut dyn Canvas,
        scale: f64,
    ) -> Result<openpilot_log_format::JsonValue> {
        crate::ui_forms_input::snapshot_object(&[
            ("description_visible", self.visible.clone()),
            (
                "height",
                openpilot_log_format::JsonValue::parse(&serde_json::to_string(&self.rect.height)?)?,
            ),
            (
                "right",
                openpilot_log_format::JsonValue::parse(&serde_json::to_string(
                    &self.right(api, scale)?,
                )?)?,
            ),
        ])
    }
    fn title_points(&self) -> Result<Vec<u32>> {
        match self.title.view() {
            openpilot_log_format::JsonView::Null => Ok(Vec::new()),
            openpilot_log_format::JsonView::Text(points) => Ok(points.to_vec()),
            _ => Err("original List title cannot be measured as text".into()),
        }
    }
    fn right(&self, api: &mut dyn Canvas, scale: f64) -> Result<Rect> {
        c::font(api, 0)?;
        let hint = match self.action.as_str() {
            "toggle" => 160.0,
            "button" => crate::ui_forms_input::text_width(api, "Current", 50.0, scale)? + 270.0,
            "text" => crate::ui_forms_input::text_width(api, "Connected", 50.0, scale)? + 20.0,
            "multiple" => 420.0,
            _ => return Err("original list action missing".into()),
        };
        let title =
            crate::ui_forms_input::text_width_codepoints(api, &self.title_points()?, 50.0, scale)?;
        let width = (self.rect.width - 40.0 - title).min(hint);
        Ok(Rect {
            x: self.rect.x + self.rect.width - width,
            y: self.rect.y,
            width,
            height: 170.0,
        }
        .cast())
    }
    pub fn render(
        &mut self,
        api: &mut dyn Canvas,
        scale: f64,
        events: &[serde_json::Value],
    ) -> Result<()> {
        let r = self.rect;
        c::font(api, 0)?;
        if crate::ui_forms_input::python_truth(&self.title) {
            let h = api
                .measure_vector(
                    &crate::ui_forms_input::Input::utf8(&self.title_points()?)?,
                    50.0 * scale,
                )?
                .1;
            api.call(&format!(
                "text {} {} {} {} 4294967295",
                crate::ui_forms_input::hex_codepoints(&self.title_points()?)?,
                r.x + 20.0,
                r.y + ((170.0 - h) / 2.0).floor(),
                50.0 * scale
            ))?;
        }
        if crate::ui_forms_input::python_truth(&self.visible) {
            let width = (r.width - 40.0).trunc();
            let h = self.html.height(api, width, scale)?;
            let description = Rect {
                x: r.x + 20.0,
                y: r.y + 140.0,
                width,
                height: h,
            }
            .cast();
            self.html
                .render_color(api, description, scale, c::color(128, 128, 128, 255))?;
        }
        let right = self.right(api, scale)?;
        match self.action.as_str() {
            "toggle" => {
                let target = if self.toggle { 1.0 } else { 0.0 };
                if (self.progress - target).abs() > 0.01 {
                    self.progress += (if self.progress < target { 1.0 } else { -1.0 }) * 0.4;
                    self.progress = self.progress.clamp(0.0, 1.0);
                }
                let x = right.x;
                let y = right.y + 45.0;
                let rgb = [
                    57.0 + (51.0 - 57.0) * self.progress,
                    57.0 + (171.0 - 57.0) * self.progress,
                    57.0 + (76.0 - 57.0) * self.progress,
                ];
                api.call(&format!(
                    "rounded {} {} 150 60 1 10 {}",
                    x + 5.0,
                    y + 10.0,
                    c::color(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, 255)
                ))?;
                api.call(&format!(
                    "circle {} {} 40 4294967295",
                    (x + 40.0 + 80.0 * self.progress).trunc(),
                    (y + 40.0).trunc()
                ))?;
                c::debug_rect(
                    api,
                    Rect {
                        x,
                        y,
                        width: 160.0,
                        height: 80.0,
                    },
                )?;
                if self.action_touch.update(
                    Rect {
                        x,
                        y,
                        width: 160.0,
                        height: 80.0,
                    },
                    events,
                )? {
                    self.toggle = !self.toggle;
                }
            }
            "button" => {
                self.button.render(
                    api,
                    Rect {
                        x: right.x + right.width - 250.0,
                        y: right.y + 35.0,
                        width: 250.0,
                        height: 100.0,
                    }
                    .cast(),
                    scale,
                    events,
                )?;
                c::font(api, 0)?;
                c::gui_label(
                    api,
                    "Current",
                    Rect {
                        width: right.width - 270.0,
                        ..right
                    }
                    .cast(),
                    50.0,
                    scale,
                    0,
                    c::color(170, 170, 170, 255),
                )?;
            }
            "text" => {
                c::font(api, 0)?;
                c::gui_label(
                    api,
                    "Connected",
                    right,
                    50.0,
                    scale,
                    2,
                    c::color(170, 170, 170, 255),
                )?;
            }
            "multiple" => {
                c::font(api, 2)?;
                for (i, text) in ["One", "Two"].iter().enumerate() {
                    let button = Rect {
                        x: right.x + i as f64 * 220.0,
                        y: right.y + 35.0,
                        width: 200.0,
                        height: 100.0,
                    }
                    .cast();
                    let bg = if i == self.selected {
                        c::color(51, 171, 76, 255)
                    } else if self.action_touch.pressed() {
                        c::color(74, 74, 74, 255)
                    } else {
                        c::color(57, 57, 57, 255)
                    };
                    api.call(&format!(
                        "rounded {} {} 200 100 1 20 {bg}",
                        button.x, button.y
                    ))?;
                    let (w, h) = api.measure_vector(text, 40.0 * scale)?;
                    api.call(&format!(
                        "text {} {} {} {} {}",
                        crate::ui_forms_input::hex(text),
                        button.x + (200.0 - w) / 2.0,
                        button.y + (100.0 - h) / 2.0,
                        40.0 * scale,
                        c::color(228, 228, 228, 255)
                    ))?;
                }
                if self.action_touch.update(right, events)? {
                    for e in events {
                        if e["released"] == true {
                            let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
                            let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
                            for i in 0..2 {
                                let left = right.x + i as f64 * 220.0;
                                if x >= left
                                    && x <= left + 200.0
                                    && y >= right.y + 35.0
                                    && y <= right.y + 135.0
                                {
                                    self.selected = i;
                                }
                            }
                        }
                    }
                }
            }
            _ => return Err("original list action missing".into()),
        }
        c::debug_rect(api, right)?;
        if self.touch.update(self.rect, events)? && !self.description.is_empty() {
            let click = events.iter().rev().find(|e| e["released"] == true);
            let outside = click.is_none_or(|e| {
                let x = e["pos"]["x"].as_f64().unwrap_or(0.0);
                let y = e["pos"]["y"].as_f64().unwrap_or(0.0);
                !(x >= right.x
                    && x <= right.x + right.width
                    && y >= right.y
                    && y <= right.y + right.height)
            });
            if outside {
                self.visible = openpilot_log_format::JsonValue::parse(
                    if crate::ui_forms_input::python_truth(&self.visible) {
                        "false"
                    } else {
                        "true"
                    },
                )?;
                let h = if crate::ui_forms_input::python_truth(&self.visible) {
                    170.0
                        + self
                            .html
                            .height(api, (self.rect.width - 40.0).trunc(), scale)?
                        - 30.0
                        + 20.0
                } else {
                    170.0
                };
                self.rect.height = f64::from(h as f32);
            }
        }
        Ok(())
    }
}
