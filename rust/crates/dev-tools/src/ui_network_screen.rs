//! Independent source network screens and stack transitions.
use super::{
    controls::{self as n, Button, Event, Touch, SCALE},
    keyboard::Keyboard,
    model::{field, text, Model, Network, Phase},
};
use crate::{
    seal::Result,
    ui_forms_input::{python_truth, Canvas, Rect},
    ui_forms_reference::{assets, controls as c, html::Html},
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modal {
    WifiPassword,
    Forget,
    TetherPassword,
    Apn,
    HiddenSsid,
    HiddenPassword,
}
#[derive(Default)]
struct Scroll {
    offset: f64,
    velocity: f64,
    initialized: bool,
    dragging: bool,
    last_y: f64,
    start_y: f64,
    last_time: f64,
}
impl Scroll {
    fn update(&mut self, bounds: Rect, content: f64, events: &[Event], wheel: f64) -> Result<f64> {
        let max = (content - bounds.height).max(0.);
        for event in events {
            if !event.primary() {
                continue;
            }
            let (x, y) = event.position()?;
            let time = n::numeric(&event.time)?;
            if !self.dragging {
                if n::inside(bounds, x, y) {
                    if event.pressed {
                        self.start_y = y;
                        if self.velocity.abs() > 120. {
                            self.dragging = true;
                            self.initialized = false;
                        }
                    }
                    if event.down && (y - self.start_y).abs() > 12. {
                        self.dragging = true;
                        self.initialized = false;
                    }
                }
            } else if event.released {
                self.dragging = false;
            } else {
                let mut delta = y - self.last_y;
                if self.offset > 0. || self.offset < -max {
                    delta /= 3.;
                }
                self.offset += delta;
                let dt = time - self.last_time;
                if dt > 0. {
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
        self.offset += wheel * 50.;
        if !self.dragging {
            let out = self.offset > 0. || self.offset < -max;
            self.velocity = if self.velocity.abs() > 2. {
                let f = (-0.25f64).exp();
                self.velocity * if out { f * f } else { f }
            } else {
                0.
            };
            if out {
                self.offset = (1. - 0.05 / 0.15) * self.offset
                    + (0.05 / 0.15) * if self.offset > 0. { 0. } else { -max };
            }
            self.offset += self.velocity / 20.;
        } else if events.is_empty() {
            self.velocity *= 0.5;
        }
        if self.offset.abs() < 0.01 {
            self.offset = 0.;
        } else if (self.offset + max).abs() < 0.01 {
            self.offset = -max;
        }
        Ok(self.offset)
    }
    fn valid(&self) -> bool {
        !self.dragging && self.velocity.abs() < 120.
    }
}
struct Toggle {
    touch: Touch,
    value: bool,
    progress: f64,
    target: f64,
    clicked: bool,
    enabled: bool,
}
impl Toggle {
    fn new() -> Self {
        Self {
            touch: Touch::default(),
            value: false,
            progress: 0.,
            target: 0.,
            clicked: false,
            enabled: true,
        }
    }
    fn set(&mut self, value: bool) {
        self.value = value;
        self.target = if value { 1. } else { 0. };
    }
    fn render(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        events: &[Event],
        valid: bool,
    ) -> Result<bool> {
        if (self.progress - self.target).abs() > 0.01 {
            self.progress += (if self.progress < self.target { 1. } else { -1. }) * 0.4;
            self.progress = self.progress.clamp(0., 1.);
        }
        let on = if self.enabled {
            [51., 171., 76.]
        } else {
            [34., 119., 34.]
        };
        let rgb = [
            57. + (on[0] - 57.) * self.progress,
            57. + (on[1] - 57.) * self.progress,
            57. + (on[2] - 57.) * self.progress,
        ];
        api.call(&format!(
            "rounded {} {} 150 60 1 10 {}",
            r.x + 5.,
            r.y + 10.,
            c::color(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, 255)
        ))?;
        let color = if self.enabled {
            4294967295u32
        } else {
            c::color(136, 136, 136, 255)
        };
        api.call(&format!(
            "circle {} {} 40 {color}",
            (r.x + 40. + 80. * self.progress).trunc(),
            (r.y + 40.).trunc()
        ))?;
        let clicked = std::mem::take(&mut self.clicked);
        if self.enabled {
            for _ in self.touch.update(r, events, valid)? {
                self.set(!self.value);
                self.clicked = true;
            }
        } else {
            self.touch.reset();
        }
        c::debug_rect(api, r)?;
        Ok(clicked)
    }
}
struct Row {
    title: &'static str,
    description: &'static str,
    visible: bool,
    height: f64,
    touch: Touch,
    button: Button,
    toggle: Toggle,
    multiple: Touch,
    description_html: Html,
}
impl Row {
    fn new(title: &'static str, description: &'static str) -> Result<Self> {
        let mut button = Button::new("EDIT", 5);
        button.size = 35.;
        button.padding = 0.;
        button.radius = 50.;
        Ok(Self {
            title,
            description,
            visible: false,
            height: 170.,
            touch: Touch::default(),
            button,
            toggle: Toggle::new(),
            multiple: Touch::default(),
            description_html: Html::new_codepoints(
                &description.chars().map(u32::from).collect::<Vec<_>>(),
                40.,
                false,
            )?,
        })
    }
}
pub struct Screen {
    pub model: Model,
    kind: String,
    advanced: bool,
    wifi_keyboard: Keyboard,
    advanced_keyboard: Keyboard,
    stack: Vec<Modal>,
    confirm: Button,
    cancel: Button,
    confirm_text: String,
    nav: Button,
    wifi_scroll: Scroll,
    advanced_scroll: Scroll,
    buttons: BTreeMap<String, Button>,
    forget: BTreeMap<String, Button>,
    rows: Vec<Row>,
    icons: Vec<(f64, f64)>,
    root_touch: Touch,
    advanced_callback: Modal,
    hidden: JsonValue,
    pub values: BTreeMap<String, JsonValue>,
}
impl Screen {
    pub fn new(
        root: &Path,
        kind: String,
        snapshot: JsonValue,
        api: &mut dyn Canvas,
    ) -> Result<Self> {
        if !matches!(kind.as_str(), "wifi" | "advanced" | "network") {
            return Err("network kind key missing".into());
        }
        let wifi_keyboard = Keyboard::new(root, api, None)?;
        let advanced_keyboard = Keyboard::new(root, api, Some(wifi_keyboard.textures()))?;
        let mut icons = Vec::new();
        for (i, name) in [
            "wifi_strength_low.png",
            "wifi_strength_medium.png",
            "wifi_strength_high.png",
            "wifi_strength_full.png",
            "checkmark.png",
            "circled_slash.png",
            "lock_closed.png",
        ]
        .iter()
        .enumerate()
        {
            icons.push(assets::load(
                api,
                root,
                7 + i,
                &format!("icons/{name}"),
                50.,
                50.,
                true,
            )?);
        }
        let mut model = Model::new(snapshot)?;
        if kind != "advanced" {
            model.set_active(true)?;
        }
        let rows = [
            ("Enable Tethering", ""),
            ("Tethering Password", ""),
            ("IP Address", ""),
            ("Enable Roaming", ""),
            ("APN Setting", ""),
            (
                "Cellular Metered",
                "Prevent large data uploads when on a metered cellular connection",
            ),
            (
                "Wi-Fi Network Metered",
                "Prevent large data uploads when on a metered Wi-Fi connection",
            ),
            ("Hidden Network", ""),
        ]
        .into_iter()
        .map(|(t, d)| Row::new(t, d))
        .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            model,
            kind,
            advanced: false,
            wifi_keyboard,
            advanced_keyboard,
            stack: vec![],
            confirm: Button::new("Forget", 1),
            cancel: Button::new("Cancel", 0),
            confirm_text: String::new(),
            nav: Button::new("Advanced", 5),
            wifi_scroll: Scroll::default(),
            advanced_scroll: Scroll::default(),
            buttons: BTreeMap::new(),
            forget: BTreeMap::new(),
            rows,
            icons,
            root_touch: Touch::default(),
            advanced_callback: Modal::Apn,
            hidden: JsonValue::text(""),
            values: BTreeMap::new(),
        })
    }
    fn key(ssid: &JsonValue) -> Result<String> {
        if !matches!(ssid.view(), JsonView::Text(_)) {
            return Err("normalize_ssid requires string".into());
        }
        // JSON encoding is a lossless dictionary key, including lone surrogates.
        Ok(ssid.to_json()?)
    }
    fn normalize(ssid: &JsonValue) -> Result<JsonValue> {
        let JsonView::Text(points) = ssid.view() else {
            return Err("normalize_ssid requires string".into());
        };
        Ok(JsonValue::codepoints(
            points
                .iter()
                .map(|v| if *v == 0x2019 { 39 } else { *v })
                .collect(),
        )
        .ok_or("network SSID point")?)
    }
    fn rebuild(&mut self) -> Result<()> {
        for network in &self.model.networks {
            let mut button = Button::new("", 9);
            if let JsonView::Text(points) = Self::normalize(&network.ssid)?.view() {
                button.text = points.to_vec();
            }
            button.size = 55.;
            button.align = 0;
            self.buttons.insert(Self::key(&network.ssid)?, button);
            let mut button = Button::new("Forget", 8);
            button.size = 45.;
            self.forget.insert(Self::key(&network.ssid)?, button);
        }
        Ok(())
    }
    fn push(&mut self, modal: Modal) {
        if !self.stack.contains(&modal) {
            self.stack.push(modal);
        }
    }
    #[allow(clippy::too_many_arguments)] // Direct source keyboard reset/title/text/callback sequence.
    fn password_screen(
        &mut self,
        modal: Modal,
        title: String,
        subtitle: String,
        min: usize,
        initial: JsonValue,
        api: &mut dyn Canvas,
        now: f64,
    ) -> Result<()> {
        let keyboard = if modal == Modal::WifiPassword {
            &mut self.wifi_keyboard
        } else {
            &mut self.advanced_keyboard
        };
        keyboard.reset(min);
        if modal != Modal::WifiPassword {
            self.advanced_callback = modal;
        }
        keyboard.title(title, subtitle);
        keyboard.set_text(initial, api, now)?;
        self.push(modal);
        Ok(())
    }
    pub fn operation(&mut self, frame: &JsonValue, api: &mut dyn Canvas, now: f64) -> Result<()> {
        let operation = frame
            .get("operation")
            .and_then(|v| v.to_utf8())
            .unwrap_or_default();
        match operation.as_str() {
            "choose" | "forget" => {
                if self.kind != "wifi" {
                    return Err("network root has no _networks".into());
                }
                let network = self.model.selected(&field(frame, "ssid")?)?;
                if operation == "choose" {
                    self.model.choose(network)?;
                } else {
                    self.model.request_forget(network);
                }
            }
            "password" => {
                if self.kind != "wifi" {
                    return Err("network root has no keyboard".into());
                }
                self.wifi_keyboard
                    .set_text(field(frame, "text")?, api, now)?;
                self.model.password_confirm(self.wifi_keyboard.text()?)?;
                self.wifi_keyboard.clear();
            }
            "forgot" => {
                if self.kind != "wifi" {
                    return Err("network root has no callback".into());
                }
                self.model.forget_confirm()?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn frame(
        &mut self,
        api: &mut dyn Canvas,
        frame: &JsonValue,
        now: f64,
    ) -> Result<JsonValue> {
        if !matches!(frame.view(), JsonView::Object(_)) {
            return Err("network frame must support get".into());
        }
        if let Some(snapshot) = frame.get("snapshot") {
            if python_truth(&snapshot) {
                self.model.set_snapshot(snapshot)?;
            }
        }
        if let Some(events) = frame.get("events") {
            self.model.enqueue(&events)?;
        }
        let events = crate::control_tools_policy_check::json::sequence(
            &frame.get("touch").unwrap_or(JsonValue::parse("[]")?),
        )?
        .into_iter()
        .map(Event::parse)
        .collect::<Result<Vec<_>>>()?;
        self.operation(frame, api, now)?;
        let chars = crate::control_tools_policy_check::json::sequence(
            &frame.get("chars").unwrap_or(JsonValue::text("")),
        )?
        .into_iter()
        .map(|value| match value.view() {
            JsonView::Text(points) if points.len() == 1 => Ok(points[0]),
            _ => Err("network ord requires one-character text".into()),
        })
        .collect::<Result<Vec<u32>>>()?;
        let wheel = frame.get("wheel").unwrap_or(JsonValue::parse("0")?);
        api.call("begin")?;
        api.call("clear 4278190080")?;
        if let Some(modal) = self.stack.last().copied() {
            if modal == Modal::Forget {
                api.call("rect 200 200 1760 680 4279966491")?;
                n::label(
                    api,
                    &JsonValue::text(&self.confirm_text),
                    Rect {
                        x: 250.,
                        y: 210.,
                        width: 1660.,
                        height: 450.,
                    },
                    70.,
                    1,
                    1,
                    c::color(201, 201, 201, 255),
                )?;
                for confirmed in [true, false] {
                    let r = if confirmed {
                        Rect {
                            x: 1105.,
                            y: 670.,
                            width: 805.,
                            height: 160.,
                        }
                    } else {
                        Rect {
                            x: 250.,
                            y: 670.,
                            width: 805.,
                            height: 160.,
                        }
                    };
                    let button = if confirmed {
                        &mut self.confirm
                    } else {
                        &mut self.cancel
                    };
                    let count = button.paint(api, r, &events, r, true, None)?.len();
                    for _ in 0..count {
                        self.stack.pop();
                        if confirmed {
                            self.model.forget_confirm()?;
                        } else {
                            self.model.phase = Phase::Idle;
                        }
                    }
                }
            } else {
                let keyboard = if modal == Modal::WifiPassword {
                    &mut self.wifi_keyboard
                } else {
                    &mut self.advanced_keyboard
                };
                let results = keyboard.render(api, now, &chars, &events)?;
                for confirmed in results {
                    let value = if modal == Modal::WifiPassword {
                        self.wifi_keyboard.text()?
                    } else {
                        self.advanced_keyboard.text()?
                    };
                    self.stack.pop();
                    let callback = if modal == Modal::WifiPassword {
                        modal
                    } else {
                        self.advanced_callback
                    };
                    self.keyboard_result(callback, confirmed, value, api, now)?;
                }
            }
        } else {
            let advanced = self.kind == "advanced" || (self.kind == "network" && self.advanced);
            let r = if self.kind == "network" {
                Rect {
                    x: 0.,
                    y: 140.,
                    width: 2160.,
                    height: 940.,
                }
            } else {
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 2160.,
                    height: 1080.,
                }
            };
            if advanced {
                self.draw_advanced(api, r, &events, &wheel, now)?;
            } else {
                self.draw_wifi(api, r, &events, &wheel, now)?;
            }
            if self.kind == "network" {
                c::debug_rect(api, r)?;
            }
            if self.kind == "network" {
                self.nav.text = if advanced { "Back" } else { "Advanced" }
                    .chars()
                    .map(u32::from)
                    .collect();
                self.nav.radius = 30.;
                let rect = Rect {
                    x: if advanced { 0. } else { 1760. },
                    y: 20.,
                    width: 400.,
                    height: 100.,
                };
                // NavButton gui_label differs from Button's wrapped Label.
                api.call(&format!(
                    "rounded {} 20 400 100 0.6 10 {}",
                    rect.x,
                    if self.nav.touch.pressed() {
                        c::color(74, 74, 74, 255)
                    } else {
                        c::color(57, 57, 57, 255)
                    }
                ))?;
                n::gui_label(
                    api,
                    if advanced { "Back" } else { "Advanced" },
                    rect,
                    60.,
                    1,
                    c::color(255, 255, 255, 229),
                )?;
                for _ in self.nav.touch.update(rect, &events, true)? {
                    self.advanced = !self.advanced;
                }
            }
        }
        self.root_touch.update(
            Rect {
                x: 0.,
                y: 0.,
                width: 2160.,
                height: 1080.,
            },
            &events,
            true,
        )?;
        c::debug_rect(
            api,
            Rect {
                x: 0.,
                y: 0.,
                width: 2160.,
                height: 1080.,
            },
        )?;
        self.model.keyboard = self.wifi_keyboard.text()?;
        let state = if self.kind == "wifi" {
            self.model.wifi_state()?
        } else {
            JsonValue::parse("null")?
        };
        crate::ui_forms_input::snapshot_object(&[
            ("state", state),
            (
                "stack",
                JsonValue::parse(&(self.stack.len() + 1).to_string())?,
            ),
            (
                "commands",
                crate::ui_forms_input::json_array(&self.model.commands)?,
            ),
            (
                "apn",
                self.values
                    .get("GsmApn")
                    .cloned()
                    .unwrap_or(JsonValue::parse("null")?),
            ),
            (
                "roaming",
                self.values
                    .get("GsmRoaming")
                    .cloned()
                    .unwrap_or(JsonValue::parse("false")?),
            ),
            (
                "metered",
                self.values
                    .get("GsmMetered")
                    .cloned()
                    .unwrap_or(JsonValue::parse("false")?),
            ),
        ])
    }
    fn keyboard_result(
        &mut self,
        modal: Modal,
        confirmed: bool,
        value: JsonValue,
        api: &mut dyn Canvas,
        now: f64,
    ) -> Result<()> {
        if !confirmed {
            if modal == Modal::WifiPassword {
                self.model.phase = Phase::Idle;
            }
            return Ok(());
        }
        match modal {
            Modal::WifiPassword => {
                self.wifi_keyboard.clear();
                self.model.password_confirm(value)?;
            }
            Modal::TetherPassword => {
                self.model.command("SetTetheringPassword", value)?;
                self.model.tether_password_enabled = false;
            }
            Modal::Apn => {
                let value = text(&value)?;
                let value =
                    value.trim_matches(|c: char| crate::ui_forms_input::python_space(u32::from(c)));
                if value.is_empty() {
                    self.values.remove("GsmApn");
                } else {
                    self.values.insert("GsmApn".into(), JsonValue::text(value));
                }
            }
            Modal::HiddenSsid => {
                if python_truth(&value) {
                    self.hidden = value.clone();
                    self.password_screen(
                        Modal::HiddenPassword,
                        "Enter password".into(),
                        format!("for \"{}\"", text(&value)?),
                        0,
                        JsonValue::text(""),
                        api,
                        now,
                    )?;
                }
            }
            Modal::HiddenPassword => {
                self.model.command(
                    "Connect",
                    crate::ui_forms_input::snapshot_object(&[
                        ("ssid", self.hidden.clone()),
                        ("password", value),
                        ("hidden", JsonValue::parse("true")?),
                    ])?,
                )?;
            }
            Modal::Forget => {}
        }
        Ok(())
    }
    fn draw_wifi(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        events: &[Event],
        wheel: &JsonValue,
        now: f64,
    ) -> Result<()> {
        let changed = self.model.callbacks(true, self.kind == "network")?;
        if changed {
            self.rebuild()?;
        }
        if self.model.networks.is_empty() {
            return n::gui_label(
                api,
                "Scanning Wi-Fi networks...",
                r,
                72.,
                1,
                c::color(255, 255, 255, 229),
            );
        }
        if self.model.phase == Phase::NeedsAuth {
            if let Some(network) = &self.model.network {
                let title = if self.model.retry {
                    "Wrong password"
                } else {
                    "Enter password"
                };
                let subtitle = format!("for \"{}\"", text(&Self::normalize(&network.ssid)?)?);
                return self.password_screen(
                    Modal::WifiPassword,
                    title.into(),
                    subtitle,
                    8,
                    JsonValue::text(""),
                    api,
                    now,
                );
            }
        } else if self.model.phase == Phase::ShowForgetConfirm {
            if let Some(network) = &self.model.network {
                self.confirm_text = format!(
                    "Forget Wi-Fi Network \"{}\"?",
                    text(&Self::normalize(&network.ssid)?)?
                );
                self.push(Modal::Forget);
                return Ok(());
            }
        }
        let offset = self.wifi_scroll.update(
            r,
            self.model.networks.len() as f64 * 160.,
            events,
            n::numeric(wheel)?,
        )?;
        api.call(&format!(
            "clip {} {} {} {}",
            r.x.trunc(),
            r.y.trunc(),
            r.width.trunc(),
            r.height.trunc()
        ))?;
        let networks = self.model.networks.clone();
        for (i, network) in networks.iter().enumerate() {
            let rect = Rect {
                x: r.x,
                y: r.y + i as f64 * 160. + offset,
                width: r.width,
                height: 160.,
            }
            .cast();
            if rect.y + rect.height <= r.y || rect.y >= r.y + r.height {
                continue;
            }
            self.draw_network(api, rect, network, events)?;
            if i + 1 < networks.len() {
                api.call(&format!(
                    "line {} {} {} {} 4291348680",
                    rect.x.trunc(),
                    (rect.y + 159.).trunc(),
                    (rect.x + rect.width).trunc(),
                    (rect.y + 159.).trunc()
                ))?;
            }
        }
        api.call("unclip")
    }
    fn draw_network(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        network: &Network,
        events: &[Event],
    ) -> Result<()> {
        let key = Self::key(&network.ssid)?;
        let active = self.model.network.as_ref().is_some_and(|n| {
            crate::control_tools_policy_check::json::equal(&n.ssid, &network.ssid)
        });
        let status = if self.model.phase == Phase::Connecting {
            if active {
                "CONNECTING..."
            } else {
                ""
            }
        } else if self.model.phase == Phase::Forgetting {
            if active {
                "FORGETTING..."
            } else {
                ""
            }
        } else {
            ""
        };
        let button = self
            .buttons
            .get_mut(&key)
            .ok_or("network row button missing")?;
        // Source leaves other rows' enabled state unchanged in connecting/forgetting branches.
        if !matches!(self.model.phase, Phase::Connecting | Phase::Forgetting) {
            button.enabled = network.security != "UNSUPPORTED";
        } else if active {
            button.enabled = false;
        }
        let rr = Rect {
            width: r.width - 400.,
            ..r
        }
        .cast();
        let releases = button.paint(api, rr, events, rr, self.wifi_scroll.valid(), None)?;
        let target = self
            .model
            .networks
            .iter()
            .rev()
            .find(|n| crate::control_tools_policy_check::json::equal(&n.ssid, &network.ssid))
            .cloned()
            .ok_or("network callback missing")?;
        for _ in releases {
            self.model.choose(target.clone())?;
        }
        let security_x = r.x + r.width - 150.;
        let signal_x = r.x + r.width - 50.;
        if !status.is_empty() {
            n::gui_label(
                api,
                status,
                Rect {
                    x: security_x - 410.,
                    width: 410.,
                    ..r
                },
                48.,
                1,
                c::color(255, 255, 255, 229),
            )?;
        } else if self.model.saved(&network.ssid)? {
            let rr = Rect {
                x: security_x - 250.,
                y: r.y + 40.,
                width: 200.,
                height: 80.,
            }
            .cast();
            let button = self.forget.get_mut(&key).ok_or("network forget missing")?;
            for _ in button.paint(api, rr, events, rr, self.wifi_scroll.valid(), None)? {
                self.model.request_forget(target.clone());
            }
        }
        let icon = if crate::control_tools_policy_check::json::equal(
            &field(&self.model.snapshot, "connected_ssid")?,
            &network.ssid,
        ) && self.model.phase != Phase::Connecting
        {
            Some(11)
        } else if network.security == "UNSUPPORTED" {
            Some(12)
        } else if network.security != "OPEN" {
            Some(13)
        } else {
            None
        };
        if let Some(id) = icon {
            api.call(&format!(
                "texture {id} {security_x} {} 1 4294967295",
                r.y + 55. + (50. - self.icons[id - 7].1) / 2.
            ))?;
        }
        let strength = n::numeric(&field(&network.original, "strength")?)?;
        let level = (strength / 33.).round_ties_even().clamp(0., 3.) as usize;
        api.call(&format!(
            "texture {} {signal_x} {} 1 4294967295",
            7 + level,
            r.y + 55.
        ))
    }
    fn draw_advanced(
        &mut self,
        api: &mut dyn Canvas,
        r: Rect,
        events: &[Event],
        wheel: &JsonValue,
        now: f64,
    ) -> Result<()> {
        let changed = self.model.callbacks(self.kind == "network", true)?;
        if changed {
            if self.kind == "network" {
                self.rebuild()?;
            }
            self.rows[0].toggle.set(self.model.tether_value);
        }
        self.model
            .command("SetIpv4Forward", JsonValue::parse("true")?)?;
        self.rows[0].toggle.enabled = self.model.tether_enabled;
        self.rows[1].button.enabled = self.model.tether_password_enabled;
        let content = f64::from((self.rows.iter().map(|row| row.height).sum::<f64>() + 7.) as f32);
        let offset = self
            .advanced_scroll
            .update(r, content, events, n::numeric(wheel)?)?;
        api.call(&format!(
            "clip {} {} {} {}",
            r.x.trunc(),
            r.y.trunc(),
            r.width.trunc(),
            r.height.trunc()
        ))?;
        let mut y = r.y + offset;
        for i in 0..8 {
            let rect = Rect {
                x: r.x,
                y,
                width: r.width,
                height: self.rows[i].height,
            }
            .cast();
            y += rect.height;
            if rect.y + rect.height > r.y && rect.y < r.y + r.height {
                self.draw_row(api, i, rect, r, events, now)?;
            }
            if i < 7 {
                api.call(&format!(
                    "line {} {} {} {} 4286743170",
                    (r.x + 40.).trunc(),
                    y.trunc(),
                    (r.x + r.width - 40.).trunc(),
                    y.trunc()
                ))?;
                y += 1.;
            }
        }
        api.call("unclip")
    }
    fn draw_row(
        &mut self,
        api: &mut dyn Canvas,
        i: usize,
        r: Rect,
        viewport: Rect,
        events: &[Event],
        now: f64,
    ) -> Result<()> {
        let valid = self.advanced_scroll.valid();
        let row = &mut self.rows[i];
        c::font(api, 0)?;
        let (_, height) = api.measure_vector(row.title, 50. * SCALE)?;
        api.call(&format!(
            "text {} {} {} {} 4294967295",
            crate::ui_forms_input::hex(row.title),
            r.x + 20.,
            r.y + ((170. - height) / 2.).floor(),
            50. * SCALE
        ))?;
        if row.visible {
            let h = row.description_html.height(api, r.width - 40., SCALE)?;
            row.description_html.render_color(
                api,
                Rect {
                    x: r.x + 20.,
                    y: r.y + 140.,
                    width: r.width - 40.,
                    height: h,
                },
                SCALE,
                c::color(128, 128, 128, 255),
            )?;
        }
        let hint = match i {
            0 | 3 | 5 => 160.,
            6 => 805.,
            2 => {
                let t = text(&field(&self.model.snapshot, "ipv4_address")?)?;
                api.measure(&t, 50. * SCALE)? + 20.
            }
            _ => 250.,
        };
        c::font(api, 0)?;
        let title_width = api.measure(row.title, 50. * SCALE)?;
        let width = hint.min(r.width - 40. - title_width);
        let action = Rect {
            x: r.x + r.width - width,
            y: r.y,
            width,
            height: 170.,
        }
        .cast();
        let mut clicked = false;
        match i {
            0 | 3 | 5 => {
                clicked = row.toggle.render(
                    api,
                    Rect {
                        x: action.x,
                        y: action.y + 45.,
                        width: 160.,
                        height: 80.,
                    },
                    events,
                    valid,
                )?;
            }
            2 => {
                n::gui_label(
                    api,
                    &text(&field(&self.model.snapshot, "ipv4_address")?)?,
                    action,
                    50.,
                    2,
                    c::color(170, 170, 170, 255),
                )?;
            }
            6 => {
                let enabled = self.model.wifi_metered_enabled;
                for (index, title) in ["default", "metered", "unmetered"].iter().enumerate() {
                    let br = Rect {
                        x: action.x + index as f64 * 275.,
                        y: action.y + 35.,
                        width: 255.,
                        height: 100.,
                    }
                    .cast();
                    let (rr, gg, bb) = if index == self.model.wifi_metered_selected {
                        (51, 171, 76)
                    } else if row.multiple.pressed() {
                        (74, 74, 74)
                    } else {
                        (57, 57, 57)
                    };
                    api.call(&format!(
                        "rounded {} {} 255 100 1 20 {}",
                        br.x,
                        br.y,
                        c::color(rr, gg, bb, if enabled { 255 } else { 150 })
                    ))?;
                    c::font(api, 2)?;
                    let (w, h) = api.measure_vector(title, 40. * SCALE)?;
                    api.call(&format!(
                        "text {} {} {} {} {}",
                        crate::ui_forms_input::hex(title),
                        br.x + (255. - w) / 2.,
                        br.y + (100. - h) / 2.,
                        40. * SCALE,
                        if enabled {
                            c::color(228, 228, 228, 255)
                        } else {
                            c::color(150, 150, 150, 255)
                        }
                    ))?;
                }
                // Widget decides action enabled once before processing this frame.
                if enabled {
                    for at in row.multiple.update(action, events, valid)? {
                        let (x, y) = events[at].position()?;
                        for index in 0..3 {
                            if n::inside(
                                Rect {
                                    x: action.x + index as f64 * 275.,
                                    y: action.y + 35.,
                                    width: 255.,
                                    height: 100.,
                                },
                                x,
                                y,
                            ) {
                                self.model.wifi_metered_selected = index;
                                self.model.wifi_metered_enabled = false;
                                self.model.command(
                                    "SetCurrentNetworkMetered",
                                    JsonValue::text(["Unknown", "Yes", "No"][index]),
                                )?;
                            }
                        }
                    }
                } else {
                    row.multiple.reset();
                }
            }
            _ => {
                row.button.text = if i == 7 { "CONNECT" } else { "EDIT" }
                    .chars()
                    .map(u32::from)
                    .collect();
                clicked = !row
                    .button
                    .paint(
                        api,
                        Rect {
                            x: action.x + action.width - 250.,
                            y: action.y + 35.,
                            width: 250.,
                            height: 100.,
                        },
                        events,
                        Rect {
                            x: action.x + action.width - 250.,
                            y: action.y + 35.,
                            width: 250.,
                            height: 100.,
                        },
                        valid,
                        None,
                    )?
                    .is_empty();
            }
        }
        // ListItem handles descriptions after rendering its action child.
        for at in row
            .touch
            .update(c::intersection(r, viewport), events, valid)?
        {
            let (x, y) = events[at].position()?;
            if !n::inside(action, x, y) && !row.description.is_empty() {
                row.visible = !row.visible;
                row.height = if row.visible {
                    f64::from(
                        (160. + row.description_html.height(api, r.width - 40., SCALE)?) as f32,
                    )
                } else {
                    170.
                };
            }
        }
        c::debug_rect(api, action)?;
        c::debug_rect(api, r)?;
        if clicked {
            match i {
                0 => {
                    self.model.tether_value = row.toggle.value;
                    self.model.tether_enabled = false;
                    if row.toggle.value {
                        self.model.wifi_metered_enabled = false;
                    }
                    self.model.command(
                        "SetTetheringActive",
                        JsonValue::parse(if row.toggle.value { "true" } else { "false" })?,
                    )?;
                }
                3 | 5 => {
                    self.values.insert(
                        if i == 3 { "GsmRoaming" } else { "GsmMetered" }.into(),
                        JsonValue::parse(if row.toggle.value { "true" } else { "false" })?,
                    );
                }
                1 => {
                    self.password_screen(
                        Modal::TetherPassword,
                        "Enter new tethering password".into(),
                        "".into(),
                        8,
                        field(&self.model.snapshot, "tethering_password")?,
                        api,
                        now,
                    )?;
                }
                4 => {
                    let initial = self
                        .values
                        .get("GsmApn")
                        .cloned()
                        .unwrap_or(JsonValue::text(""));
                    self.password_screen(
                        Modal::Apn,
                        "Enter APN".into(),
                        "leave blank for automatic configuration".into(),
                        0,
                        initial,
                        api,
                        now,
                    )?;
                }
                7 => {
                    self.password_screen(
                        Modal::HiddenSsid,
                        "Enter SSID".into(),
                        "".into(),
                        1,
                        JsonValue::text(""),
                        api,
                        now,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
