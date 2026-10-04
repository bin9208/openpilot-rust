//! Source: selfdrive/ui/widgets/offroad_alerts.py (MIT).
use crate::{
    context::{actions::UpdaterAction, Action, Context},
    paint::{self, Text},
    params::Read,
};
use openpilot_ui_framework::{
    draw::{Draw, BLACK, WHITE},
    geometry::{Point, Rect},
    html::HtmlRenderer,
    scroll::ScrollPanel,
    text::Font,
    text_layout::{self, float},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};

struct ActionButton {
    state: WidgetState,
    context: Context,
    text: &'static str,
    dark: bool,
    minimum: f32,
}
impl ActionButton {
    fn new(context: Context, text: &'static str, dark: bool, minimum: f32) -> Self {
        Self {
            state: WidgetState::default(),
            context,
            text,
            dark,
            minimum,
        }
    }
}
impl Widget for ActionButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let text = self.context.tr(self.text);
        let size = text_layout::measure(draw, Font::Medium, &text, 48.0, 0.0);
        self.state.rect.width = (size.x + 120.0).max(self.minimum);
        self.state.rect.height = 125.0;
        let rect = self.state.rect;
        let bg = match (self.dark, self.state.is_pressed()) {
            (false, false) => WHITE,
            (false, true) => paint::color(200, 200, 200, 255),
            (true, false) => paint::color(79, 79, 79, 255),
            (true, true) => paint::color(100, 100, 100, 255),
        };
        draw.rounded_segments(rect, 60.0 / rect.height, 10, bg, false)?;
        paint::text(
            draw,
            Point {
                x: float(
                    (f64::from(rect.x)
                        + ((f64::from(rect.width) - f64::from(size.x)) / 2.0).floor())
                    .trunc(),
                ),
                y: float(
                    (f64::from(rect.y)
                        + ((f64::from(rect.height) - f64::from(size.y)) / 2.0).floor())
                    .trunc(),
                ),
            },
            Text {
                value: &text,
                font: Font::Medium,
                size: 48.0,
                spacing: 0.0,
                color: if self.dark { WHITE } else { BLACK },
            },
        )?;
        Ok(RenderResult::None)
    }
}
#[derive(serde::Deserialize)]
struct AlertConfig {
    #[serde(default)]
    severity: i32,
}
struct Catalog(Vec<(String, AlertConfig)>);
impl<'de> serde::Deserialize<'de> for Catalog {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Catalog;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ordered offroad alert catalog")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Catalog, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Catalog(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
#[derive(serde::Deserialize, Default)]
struct ActiveAlert {
    #[serde(default)]
    text: String,
    #[serde(default)]
    extra: String,
}
#[derive(serde::Serialize)]
pub struct AlertData {
    pub key: String,
    pub text: String,
    pub severity: i32,
    pub visible: bool,
}
enum Content {
    Offroad(Vec<AlertData>),
    Update {
        notes: String,
        html: Box<HtmlRenderer>,
        cached_height: f64,
    },
}
pub struct Alert {
    pub state: WidgetState,
    context: Context,
    content: Content,
    pub dismiss: Rc<Cell<bool>>,
    close: ActionButton,
    snooze: ActionButton,
    acknowledge: ActionButton,
    reboot: ActionButton,
    pub scroll: ScrollPanel,
}
impl Alert {
    fn new(context: Context, content: Content) -> Self {
        let dismiss = Rc::new(Cell::new(false));
        let mut close = ActionButton::new(context.clone(), "Close", false, 400.0);
        let done = dismiss.clone();
        close.state.click = Some(Box::new(move || done.set(true)));
        let mut snooze = ActionButton::new(context.clone(), "Snooze Update", true, 400.0);
        let c = context.clone();
        let done = dismiss.clone();
        snooze.state.click = Some(Box::new(move || {
            match c.params.put_bool("SnoozeUpdate", true) {
                Ok(()) => done.set(true),
                Err(e) => c.actions.push(Action::Failure(e)),
            }
        }));
        let mut acknowledge = ActionButton::new(
            context.clone(),
            "Acknowledge Excessive Actuation",
            true,
            800.0,
        );
        let c = context.clone();
        let done = dismiss.clone();
        acknowledge.state.click = Some(Box::new(move || {
            match c.params.remove("Offroad_ExcessiveActuation") {
                Ok(()) => done.set(true),
                Err(e) => c.actions.push(Action::Failure(e)),
            }
        }));
        let mut reboot = ActionButton::new(context.clone(), "Reboot and Update", false, 600.0);
        let actions = context.actions.clone();
        reboot.state.click = Some(Box::new(move || {
            actions.push(Action::Updater(UpdaterAction::Reboot))
        }));
        Self {
            state: WidgetState::default(),
            context,
            content,
            dismiss,
            close,
            snooze,
            acknowledge,
            reboot,
            scroll: ScrollPanel::new(false, true, true),
        }
    }
    pub fn offroad(context: Context) -> Self {
        Self::new(context, Content::Offroad(Vec::new()))
    }
    pub fn update(context: Context) -> Result<Self, Error> {
        Ok(Self::new(
            context,
            Content::Update {
                notes: String::new(),
                html: Box::new(HtmlRenderer::new("", 48.0)?),
                cached_height: 0.0,
            },
        ))
    }
    pub fn refresh(&mut self) -> Result<usize, crate::Error> {
        match &mut self.content {
            Content::Update {
                notes,
                html,
                cached_height,
            } => {
                let available = self.context.params.boolean("UpdateAvailable")?;
                let fallback = format!(
                    "<h2>{}</h2>",
                    self.context.tr("No release notes available.")
                );
                if available {
                    let decoded = String::from_utf8(
                        self.context
                            .params
                            .bytes("UpdaterNewReleaseNotes")?
                            .unwrap_or_default(),
                    )
                    .map_err(|_| crate::Error::Parameter("UpdaterNewReleaseNotes".into()))?;
                    *notes = openpilot_ui_framework::text::trim(&decoded).into();
                    html.parse(if notes.is_empty() { &fallback } else { notes })?;
                    *cached_height = 0.0;
                } else {
                    html.parse(&fallback)?;
                }
                Ok(usize::from(available))
            }
            Content::Offroad(alerts) => {
                if alerts.is_empty() {
                    let path = self
                        .context
                        .source_root
                        .join("openpilot/selfdrive/selfdrived/alerts_offroad.json");
                    let mut catalog: Catalog = serde_json::from_slice(&std::fs::read(path)?)
                        .map_err(|e| crate::Error::Io(std::io::Error::other(e)))?;
                    catalog.0.sort_by(|a, b| b.1.severity.cmp(&a.1.severity));
                    *alerts = catalog
                        .0
                        .into_iter()
                        .map(|(key, value)| AlertData {
                            key,
                            text: String::new(),
                            severity: value.severity,
                            visible: false,
                        })
                        .collect();
                }
                let mut count = 0;
                let mut connectivity = false;
                let mut excessive = false;
                for alert in alerts {
                    let value = self
                        .context
                        .params
                        .bytes(&alert.key)?
                        .filter(|v| !v.is_empty());
                    let active = active_alert(value.as_deref())?;
                    alert.text = active.text.replace("%1", &active.extra);
                    alert.visible = !alert.text.is_empty();
                    count += usize::from(alert.visible);
                    connectivity |= alert.key == "Offroad_ConnectivityNeeded" && alert.visible;
                    excessive |= alert.key == "Offroad_ExcessiveActuation" && alert.visible;
                }
                self.acknowledge.state.visible = excessive.into();
                self.snooze.state.visible = (connectivity && !excessive).into();
                Ok(count)
            }
        }
    }
    fn height(&mut self, draw: &dyn Draw, width: f32) -> f64 {
        match &mut self.content {
            Content::Update {
                notes,
                cached_height,
                ..
            } => {
                if notes.is_empty() {
                    return 100.0;
                }
                if *cached_height == 0.0 {
                    *cached_height =
                        (f64::from(text_layout::measure(draw, Font::Normal, notes, 48.0, 0.0).y)
                            + 60.0)
                            .max(100.0);
                }
                *cached_height
            }
            Content::Offroad(alerts) => {
                if alerts.is_empty() {
                    return 0.0;
                }
                let mut height = 20.0;
                for alert in alerts.iter().filter(|a| a.visible) {
                    let lines = text_layout::wrap(
                        draw,
                        Font::Normal,
                        &alert.text,
                        48.0,
                        0.0,
                        f64::from(width - 120.0).trunc(),
                    );
                    let count = num_traits::ToPrimitive::to_f64(&lines.len()).unwrap_or(0.0);
                    height += ((count * 48.0 * draw.font_scale() + 120.0).max(120.0) + 10.0)
                        .round_ties_even();
                }
                if height > 20.0 {
                    height += 10.0;
                }
                height
            }
        }
    }
}
impl Widget for Alert {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        self.scroll.set_offset(0.0);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.rounded_segments(
            rect,
            60.0 / rect.height,
            10,
            paint::color(57, 57, 57, 255),
            false,
        )?;
        let viewport = Rect {
            x: rect.x + 50.0,
            y: rect.y + 50.0,
            width: rect.width - 100.0,
            height: rect.height - 255.0,
        };
        let total = self.height(draw, viewport.width);
        let offset = self.scroll.update(viewport, total, frame.events, frame.dt);
        draw.scissor(Some(Rect {
            x: viewport.x.trunc(),
            y: viewport.y.trunc(),
            width: viewport.width.trunc(),
            height: viewport.height.trunc(),
        }))?;
        let content = Rect {
            y: float(f64::from(viewport.y) + offset),
            height: float(total),
            ..viewport
        };
        let result = (|| -> Result<(), Error> {
            match &mut self.content {
                Content::Update { html, .. } => {
                    html.set_rect(Rect {
                        x: content.x + 30.0,
                        y: content.y + 30.0,
                        width: content.width - 60.0,
                        height: content.height - 60.0,
                    });
                    html.render(frame, draw)?;
                }
                Content::Offroad(alerts) => {
                    let mut y = 10.0;
                    for alert in alerts.iter().filter(|a| a.visible) {
                        let lines = text_layout::wrap(
                            draw,
                            Font::Normal,
                            &alert.text,
                            48.0,
                            0.0,
                            f64::from(content.width - 120.0).trunc(),
                        );
                        let count = num_traits::ToPrimitive::to_f64(&lines.len()).unwrap_or(0.0);
                        let height = (count * 48.0 * draw.font_scale() + 120.0).max(120.0);
                        let item = Rect {
                            x: content.x + 10.0,
                            y: float(f64::from(content.y) + y),
                            width: content.width - 30.0,
                            height: float(height),
                        };
                        draw.rounded_segments(
                            item,
                            60.0 / item.height.min(item.width),
                            10,
                            if alert.severity > 0 {
                                paint::color(226, 44, 44, 255)
                            } else {
                                paint::color(41, 41, 41, 255)
                            },
                            false,
                        )?;
                        let mut text_y = f64::from(item.y + 60.0);
                        for line in lines {
                            paint::text(
                                draw,
                                Point {
                                    x: item.x + 60.0,
                                    y: float(text_y),
                                },
                                Text {
                                    value: &line,
                                    font: Font::Normal,
                                    size: 48.0,
                                    spacing: 0.0,
                                    color: WHITE,
                                },
                            )?;
                            text_y += 48.0 * draw.font_scale();
                        }
                        y += (height + 10.0).round_ties_even();
                    }
                }
            }
            Ok(())
        })();
        draw.scissor(None)?;
        result?;
        let y = rect.y + rect.height - 175.0;
        self.close.set_position(rect.x + 50.0, y);
        self.close.render(frame, draw)?;
        let right = match &self.content {
            Content::Update { .. } => Some(&mut self.reboot),
            Content::Offroad(_) => {
                if self.acknowledge.state.visible.get() {
                    Some(&mut self.acknowledge)
                } else if self.snooze.state.visible.get() {
                    Some(&mut self.snooze)
                } else {
                    None
                }
            }
        };
        if let Some(button) = right {
            button.set_position(rect.x + rect.width - 50.0 - button.state.rect.width, y);
            button.render(frame, draw)?;
        }
        Ok(RenderResult::None)
    }
}

fn active_alert(bytes: Option<&[u8]>) -> Result<ActiveAlert, crate::Error> {
    let Some(bytes) = bytes else {
        return Ok(ActiveAlert::default());
    };
    let value = match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(value) => value,
        Err(error) => {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Warning,
                format!("Failed to cast offroad alert JSON: {error}"),
            );
            return Ok(ActiveAlert::default());
        }
    };
    let empty = match &value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(value) => !*value,
        serde_json::Value::Number(value) => value.as_f64() == Some(0.0),
        serde_json::Value::String(value) => value.is_empty(),
        serde_json::Value::Array(value) => value.is_empty(),
        serde_json::Value::Object(value) => value.is_empty(),
    };
    if empty {
        return Ok(ActiveAlert::default());
    }
    serde_json::from_value(value).map_err(|error| crate::Error::Io(std::io::Error::other(error)))
}
