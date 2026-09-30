use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    #[default]
    Initializing,
    Searching,
    Connecting,
    Connected,
    Disconnecting,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Snapshot {
    pub seconds_since_boot: f64,
    pub state: State,
    pub connected: bool,
    pub ip_address: String,
    pub iccid: String,
    pub mcc_mnc: String,
    pub imei: String,
    pub modem_version: String,
    pub sim_state: String,
    pub signal_strength: i64,
    pub signal_quality: i64,
    pub network_type: String,
    pub operator: String,
    pub band: String,
    pub channel: i64,
    pub registration: String,
    pub temperatures: Vec<i64>,
    pub extra: String,
    pub tx_bytes: i64,
    pub rx_bytes: i64,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            seconds_since_boot: 0.0,
            state: State::Initializing,
            connected: false,
            ip_address: String::new(),
            iccid: String::new(),
            mcc_mnc: String::new(),
            imei: String::new(),
            modem_version: String::new(),
            sim_state: "ABSENT".into(),
            signal_strength: 0,
            signal_quality: 0,
            network_type: "unknown".into(),
            operator: String::new(),
            band: String::new(),
            channel: 0,
            registration: "unknown".into(),
            temperatures: Vec::new(),
            extra: String::new(),
            tx_bytes: 0,
            rx_bytes: 0,
        }
    }
}
pub fn registration(value: &str) -> &'static str {
    match value
        .split(',')
        .nth(1)
        .and_then(|v| v.trim_matches('"').trim().parse::<i64>().ok())
    {
        Some(0) => "not_registered",
        Some(1) => "home",
        Some(2) => "searching",
        Some(3) => "denied",
        Some(5) => "roaming",
        _ => "unknown",
    }
}
pub fn network_type(value: i64) -> &'static str {
    match value {
        0 | 1 | 3 | 8 => "gsm",
        2 | 4..=6 => "utran",
        7 | 9 | 10 => "lte",
        11..=13 => "nr",
        _ => "unknown",
    }
}
