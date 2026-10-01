use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

/// Filesystem and executable locations are injectable for owned, offline fixtures.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub at_port: PathBuf,
    pub ppp_port: PathBuf,
    pub lock: PathBuf,
    pub state: PathBuf,
    pub params: PathBuf,
    pub statistics: PathBuf,
    pub modem_manager: PathBuf,
    pub sudo: PathBuf,
    pub ip: PathBuf,
    pub launcher: PathBuf,
    pub serial_timeout_ms: u64,
    pub state_wait_ms: u64,
    pub iccid_interval: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            at_port: "/dev/modem_at0".into(),
            ppp_port: "/dev/modem_at1".into(),
            lock: "/dev/shm/modem.lock".into(),
            state: "/dev/shm/modem".into(),
            params: "/data/params/d".into(),
            statistics: "/sys/class/net/ppp0/statistics".into(),
            modem_manager: "/lib/systemd/system/ModemManager.service".into(),
            sudo: "sudo".into(),
            ip: "ip".into(),
            launcher: "openpilot-process-child".into(),
            serial_timeout_ms: 5000,
            state_wait_ms: 1000,
            iccid_interval: 60.0,
        }
    }
}
impl Config {
    pub fn serial_timeout(&self) -> Duration {
        Duration::from_millis(self.serial_timeout_ms)
    }
    pub fn param(&self, key: &str) -> Result<String, std::io::Error> {
        match std::fs::read_to_string(self.params.join(key)) {
            Ok(value) => Ok(value.trim().to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error),
        }
    }
}
