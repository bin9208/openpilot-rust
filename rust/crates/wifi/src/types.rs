use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SecurityType {
    Open,
    Wpa,
    Wpa2,
    Wpa3,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum MeteredType {
    #[default]
    Unknown,
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConnectStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    pub strength: i32,
    pub security_type: SecurityType,
    pub is_tethering: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WifiState {
    pub ssid: Option<String>,
    pub status: ConnectStatus,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub networks: Vec<Network>,
    pub wifi_state: WifiState,
    pub ipv4_address: String,
    pub current_network_metered: MeteredType,
    pub connecting_to_ssid: Option<String>,
    pub connected_ssid: Option<String>,
    pub tethering_password: String,
    pub tethering_active: bool,
    pub saved_ssids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Event {
    NeedAuth(String),
    Activated,
    Forgotten(String),
    NetworksUpdated(Vec<Network>),
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Command {
    SetActive(bool),
    Connect {
        ssid: String,
        password: String,
        hidden: bool,
    },
    Forget(String),
    Activate(String),
    SetTetheringPassword(String),
    SetTetheringActive(bool),
    SetCurrentNetworkMetered(MeteredType),
    SetIpv4Forward(bool),
    Stop,
}
