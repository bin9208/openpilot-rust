use crate::{ConnectStatus, Event, Network, Snapshot, WifiState};

pub struct State {
    pub snapshot: Snapshot,
    pub connections: Vec<(String, String)>,
    pub events: Vec<Event>,
    pub tethering_ssid: String,
    pub device: Option<String>,
    pub active: bool,
    pub ipv4_forward: bool,
    pub last_scan: f64,
    pub epoch: u64,
}

impl State {
    pub fn new(dongle: Option<&str>) -> Self {
        let mut tethering_ssid = "weedle".to_owned();
        if let Some(id) = dongle.filter(|id| !id.is_empty()) {
            tethering_ssid.push('-');
            tethering_ssid.extend(id.chars().take(4));
        }
        Self {
            snapshot: Snapshot::default(),
            connections: Vec::new(),
            events: Vec::new(),
            tethering_ssid,
            device: None,
            active: true,
            ipv4_forward: false,
            last_scan: 0.,
            epoch: 0,
        }
    }
    pub fn set_connecting(&mut self, ssid: Option<String>) {
        self.epoch = self.epoch.wrapping_add(1);
        self.snapshot.wifi_state = WifiState {
            status: if ssid.is_some() {
                ConnectStatus::Connecting
            } else {
                ConnectStatus::Disconnected
            },
            ssid,
        };
    }
    pub fn connection(&self, ssid: &str) -> Option<&str> {
        self.connections
            .iter()
            .find(|(name, _)| name == ssid)
            .map(|(_, path)| path.as_str())
    }
    pub fn ssid(&self, path: &str) -> Option<String> {
        self.connections
            .iter()
            .find(|(_, known)| known == path)
            .map(|(ssid, _)| ssid.clone())
    }
    pub fn new_connection(&mut self, ssid: String, path: String) {
        if ssid.is_empty() {
            return;
        }
        if let Some((_, old)) = self.connections.iter_mut().find(|(name, _)| name == &ssid) {
            *old = path;
        } else {
            self.connections.push((ssid, path));
        }
    }
    pub fn remove_connection(&mut self, path: &str) {
        self.connections.retain(|(_, known)| known != path);
    }
    pub fn networks(&self) -> Vec<Network> {
        let mut networks = self.snapshot.networks.clone();
        networks.sort_by(|left, right| {
            let active = self.snapshot.wifi_state.ssid.as_deref();
            (active != Some(left.ssid.as_str()))
                .cmp(&(active != Some(right.ssid.as_str())))
                .then_with(|| {
                    self.connection(&left.ssid)
                        .is_none()
                        .cmp(&self.connection(&right.ssid).is_none())
                })
                .then_with(|| right.strength.cmp(&left.strength))
                .then_with(|| left.ssid.to_lowercase().cmp(&right.ssid.to_lowercase()))
        });
        networks
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut result = self.snapshot.clone();
        result.networks = self.networks();
        result.saved_ssids = self
            .connections
            .iter()
            .map(|(ssid, _)| ssid.clone())
            .collect();
        result.tethering_active =
            result.wifi_state.ssid.as_deref() == Some(self.tethering_ssid.as_str());
        result.connecting_to_ssid = if result.wifi_state.status == ConnectStatus::Connecting {
            result.wifi_state.ssid.clone()
        } else {
            None
        };
        result.connected_ssid = if result.wifi_state.status == ConnectStatus::Connected {
            result.wifi_state.ssid.clone()
        } else {
            None
        };
        result
    }
    pub fn scan_due(&self, now: f64) -> bool {
        // Explicit network-page scans stay available. A disconnected optional
        // uplink must not scan the band every five seconds on the radio that
        // is serving the Jetson HUD access point.
        (self.active
            || (self.snapshot.wifi_state.status != ConnectStatus::Connected && !direct_ap_active()))
            && now - self.last_scan > 5.
    }
}

/// Secret-free hint written by jetlinkd while it serves a Jetson HUD from a
/// private access point (carrot's `common/wifi_coexistence.py`).
const DIRECT_AP_STATUS: &str = "/dev/shm/carrot-jetlink-direct-wifi.json";

/// True while that access point is fresh: background discovery must then
/// stay off the radio the HUD uses.
pub fn direct_ap_active() -> bool {
    let Ok(text) = std::fs::read_to_string(DIRECT_AP_STATUS) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let now = now.tv_sec as f64 + now.tv_nsec as f64 * 1e-9;
    value["state"] == "active"
        && value["interface"] == "wljetson0"
        && value["updated"]
            .as_f64()
            .is_some_and(|updated| (0. ..8.).contains(&(now - updated)))
}
