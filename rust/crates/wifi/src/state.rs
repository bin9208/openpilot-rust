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
        (self.active || self.snapshot.wifi_state.status != ConnectStatus::Connected)
            && now - self.last_scan > 5.
    }
}
