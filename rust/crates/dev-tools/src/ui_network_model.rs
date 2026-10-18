//! Independent network.py state and helper-owned transport policy.
//! Drawing and navigation choose when callbacks are drained; modal screens pause it.
use crate::{control_tools_policy_check::json, seal::Result, ui_forms_input::python_truth};
use openpilot_log_format::{JsonValue, JsonView};
use std::collections::VecDeque;

pub fn field(value: &JsonValue, name: &str) -> Result<JsonValue> {
    value
        .get(name)
        .ok_or_else(|| format!("network field {name} missing").into())
}
pub fn text(value: &JsonValue) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| "network expected encodable text".into())
}
fn member(values: &JsonValue, wanted: &JsonValue) -> Result<bool> {
    if let JsonView::Text(haystack) = values.view() {
        let JsonView::Text(needle) = wanted.view() else {
            return Err("network string membership requires text".into());
        };
        return Ok(needle.is_empty() || haystack.windows(needle.len()).any(|part| part == needle));
    }
    Ok(json::sequence(values)?
        .iter()
        .any(|v| json::equal(v, wanted)))
}

pub struct Network {
    pub original: JsonValue,
    pub ssid: JsonValue,
    pub security: String,
}
impl Clone for Network {
    fn clone(&self) -> Self {
        Self {
            original: self.original.clone(),
            ssid: self.ssid.clone(),
            security: self.security.clone(),
        }
    }
}
impl Network {
    pub fn parse(value: JsonValue) -> Result<Self> {
        if !matches!(value.view(), JsonView::Object(_)) {
            return Err("network SimpleNamespace requires mapping".into());
        }
        let security = text(&field(&value, "security_type")?)?.to_uppercase();
        if !matches!(
            security.as_str(),
            "OPEN" | "WPA" | "WPA2" | "WPA3" | "UNSUPPORTED"
        ) {
            return Err("network security enum key missing".into());
        }
        Ok(Self {
            ssid: field(&value, "ssid")?,
            security,
            original: value,
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Connecting,
    NeedsAuth,
    ShowForgetConfirm,
    Forgetting,
}
impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connecting => "Connecting",
            Self::NeedsAuth => "NeedsAuth",
            Self::ShowForgetConfirm => "ShowForgetConfirm",
            Self::Forgetting => "Forgetting",
        }
    }
}
pub struct Model {
    pub snapshot: JsonValue,
    pub networks: Vec<Network>,
    pub phase: Phase,
    pub network: Option<Network>,
    pub retry: bool,
    pub ip: JsonValue,
    pub keyboard: JsonValue,
    pub events: VecDeque<JsonValue>,
    pub commands: Vec<JsonValue>,
    pub tether_enabled: bool,
    pub tether_password_enabled: bool,
    pub tether_value: bool,
    pub wifi_metered_enabled: bool,
    pub wifi_metered_selected: usize,
}
impl Model {
    pub fn new(snapshot: JsonValue) -> Result<Self> {
        Self::validate_snapshot(&snapshot)?;
        Ok(Self {
            ip: field(&snapshot, "ipv4_address")?,
            snapshot,
            networks: Vec::new(),
            phase: Phase::Idle,
            network: None,
            retry: false,
            keyboard: JsonValue::text(""),
            events: VecDeque::new(),
            commands: Vec::new(),
            tether_enabled: true,
            tether_password_enabled: true,
            tether_value: false,
            wifi_metered_enabled: true,
            wifi_metered_selected: 0,
        })
    }
    fn validate_snapshot(snapshot: &JsonValue) -> Result<()> {
        for name in [
            "ipv4_address",
            "wifi_state",
            "current_network_metered",
            "connected_ssid",
            "tethering_password",
        ] {
            field(snapshot, name)?;
        }
        if !matches!(field(snapshot, "wifi_state")?.view(), JsonView::Object(_)) {
            return Err("network wifi_state must mapping".into());
        }
        if !matches!(
            text(&field(snapshot, "current_network_metered")?)?
                .to_uppercase()
                .as_str(),
            "UNKNOWN" | "YES" | "NO"
        ) {
            return Err("network metered enum key missing".into());
        }
        Ok(())
    }
    pub fn set_snapshot(&mut self, value: JsonValue) -> Result<()> {
        Self::validate_snapshot(&value)?;
        self.snapshot = value;
        Ok(())
    }
    pub fn command(&mut self, name: &str, value: JsonValue) -> Result<()> {
        self.commands
            .push(crate::ui_forms_input::snapshot_object(&[(name, value)])?);
        Ok(())
    }
    pub fn set_active(&mut self, active: bool) -> Result<()> {
        self.command(
            "SetActive",
            JsonValue::parse(if active { "true" } else { "false" })?,
        )
    }
    pub fn saved(&self, ssid: &JsonValue) -> Result<bool> {
        member(&field(&self.snapshot, "saved_ssids")?, ssid)
    }
    pub fn selected(&self, ssid: &JsonValue) -> Result<Network> {
        self.networks
            .iter()
            .find(|n| json::equal(&n.ssid, ssid))
            .cloned()
            .ok_or_else(|| "network selection StopIteration".into())
    }
    pub fn choose(&mut self, network: Network) -> Result<()> {
        if !self.saved(&network.ssid)? && network.security != "OPEN" {
            self.phase = Phase::NeedsAuth;
            self.network = Some(network);
            self.retry = false;
        } else if !json::equal(
            &field(&field(&self.snapshot, "wifi_state")?, "ssid")?,
            &network.ssid,
        ) {
            self.connect(network, JsonValue::text(""))?;
        }
        Ok(())
    }
    pub fn request_forget(&mut self, network: Network) {
        self.phase = Phase::ShowForgetConfirm;
        self.network = Some(network);
    }
    pub fn connect(&mut self, network: Network, password: JsonValue) -> Result<()> {
        self.phase = Phase::Connecting;
        self.network = Some(network.clone());
        if self.saved(&network.ssid)? && !python_truth(&password) {
            self.command("Activate", network.ssid)
        } else {
            self.command(
                "Connect",
                crate::ui_forms_input::snapshot_object(&[
                    ("ssid", network.ssid),
                    ("password", password),
                    ("hidden", JsonValue::parse("false")?),
                ])?,
            )
        }
    }
    pub fn password_confirm(&mut self, password: JsonValue) -> Result<()> {
        // Keyboard text clears before len()/network access in original callback.
        self.keyboard = JsonValue::text("");
        if json::sequence(&password)?.len() >= 8 {
            let network = self.network.clone().ok_or("network None has no ssid")?;
            self.connect(network, password)?;
        }
        Ok(())
    }
    pub fn forget_confirm(&mut self) -> Result<()> {
        self.phase = Phase::Forgetting;
        let network = self.network.as_ref().ok_or("network None has no ssid")?;
        self.command("Forget", network.ssid.clone())
    }
    pub fn enqueue(&mut self, events: &JsonValue) -> Result<()> {
        self.events.extend(json::sequence(events)?);
        Ok(())
    }
    pub fn callbacks(&mut self, wifi_listener: bool, advanced: bool) -> Result<bool> {
        let mut networks_changed = false;
        while let Some(event) = self.events.pop_front() {
            let (name, value) = match event.view() {
                JsonView::Text(_) => (text(&event)?, JsonValue::parse("null")?),
                JsonView::Object(fields) => {
                    let Some((key, value)) = fields.first() else {
                        return Err("network callback StopIteration".into());
                    };
                    (
                        key.iter()
                            .map(|&point| char::from_u32(point))
                            .collect::<Option<String>>()
                            .ok_or("network callback key encoding")?,
                        value.clone(),
                    )
                }
                _ => return Err("network callback has no items".into()),
            };
            match name.as_str() {
                "NetworksUpdated" => {
                    let networks = json::sequence(&value)?
                        .into_iter()
                        .map(Network::parse)
                        .collect::<Result<Vec<_>>>()?;
                    if advanced {
                        self.tether_enabled = true;
                        self.tether_password_enabled = true;
                        self.tether_value =
                            python_truth(&field(&self.snapshot, "tethering_active")?);
                        self.wifi_metered_enabled = !self.tether_value
                            && python_truth(&field(&self.snapshot, "ipv4_address")?);
                        self.wifi_metered_selected = if self.wifi_metered_enabled {
                            match text(&field(&self.snapshot, "current_network_metered")?)?
                                .to_uppercase()
                                .as_str()
                            {
                                "YES" => 1,
                                "NO" => 2,
                                _ => 0,
                            }
                        } else {
                            0
                        };
                    }
                    self.networks = networks;
                    networks_changed = true;
                }
                "NeedAuth" if wifi_listener => {
                    if let Some(network) = self
                        .networks
                        .iter()
                        .find(|n| json::equal(&n.ssid, &value))
                        .cloned()
                    {
                        self.phase = Phase::NeedsAuth;
                        self.network = Some(network);
                        self.retry = true;
                    }
                }
                "Activated" | "Disconnected" if wifi_listener => {
                    if self.phase == Phase::Connecting {
                        self.phase = Phase::Idle;
                    }
                }
                "Forgotten" if wifi_listener => {
                    if self.phase == Phase::Forgetting {
                        self.phase = Phase::Idle;
                    }
                }
                "NeedAuth" | "Activated" | "Disconnected" | "Forgotten" => {}
                _ => return Err("network callback name missing".into()),
            }
        }
        self.ip = field(&self.snapshot, "ipv4_address")?;
        Ok(networks_changed)
    }
    pub fn wifi_state(&self) -> Result<JsonValue> {
        crate::ui_forms_input::snapshot_object(&[
            ("phase", JsonValue::text(self.phase.name())),
            (
                "network",
                self.network
                    .as_ref()
                    .map(|n| n.ssid.clone())
                    .unwrap_or(JsonValue::parse("null")?),
            ),
            (
                "retry",
                JsonValue::parse(if self.retry { "true" } else { "false" })?,
            ),
            ("ip", self.ip.clone()),
            ("keyboard", self.keyboard.clone()),
        ])
    }
}
