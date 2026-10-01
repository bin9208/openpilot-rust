use crate::at::AtPort;
use std::path::PathBuf;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub at: AtPort,
    pub diagnostic: PathBuf,
    pub nmea: PathBuf,
    pub root: PathBuf,
    pub assistance: PathBuf,
    pub assistance_url: String,
    pub alternate: Option<PathBuf>,
    pub mmcli: PathBuf,
    pub systemd: PathBuf,
    pub cold_start: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            at: AtPort {
                path: "/dev/modem_at0".into(),
                lock: "/dev/shm/modem.lock".into(),
            },
            diagnostic: "/dev/ttyUSB0".into(),
            nmea: "/dev/ttyUSB1".into(),
            root: "/".into(),
            assistance: "/tmp/xtra3grc.bin".into(),
            assistance_url: "http://xtrapath3.izatcloud.net/xtra3grc.bin".into(),
            alternate: std::env::var_os("QCOM_ALT_ASSISTANCE_PATH").map(Into::into),
            mmcli: "mmcli".into(),
            systemd: "/lib/systemd/systemd".into(),
            cold_start: std::env::var_os("GPS_COLD_START").is_some(),
        }
    }
}
