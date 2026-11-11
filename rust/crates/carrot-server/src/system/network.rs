use super::wifi;
use crate::{params::Backend, Error, Value};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};

pub struct Network {
    pub program: PathBuf,
    cache: Mutex<Option<Value>>,
}

pub struct Probe {
    wifi: Value,
    address: String,
}

impl Default for Network {
    fn default() -> Self {
        Self::new("nmcli".into())
    }
}

impl Network {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            cache: Mutex::new(None),
        }
    }

    fn output(&self, args: &[&str], seconds: u64) -> String {
        let captured = (|| {
            let mut child = Command::new(&self.program)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            openpilot_process_supervision::capture_output(&mut child, Duration::from_secs(seconds))
        })();
        match captured {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
            Ok(_) | Err(_) => String::new(),
        }
    }

    pub fn snapshot(&self, params: &Backend) -> Result<Value, Error> {
        let mut value = self
            .cache
            .lock()
            .map_err(|_| Error::Source("network cache lock poisoned".into()))?
            .clone()
            .unwrap_or_else(|| {
                Value::object([
                    ("wifi", Value::Array(Vec::new())),
                    ("ip_address", Value::text("")),
                ])
            });
        Self::parameters(&mut value, params);
        Ok(value)
    }

    pub fn refresh(&self, params: &Backend) -> Result<Value, Error> {
        self.publish(self.probe(), params)
    }

    pub fn probe(&self) -> Probe {
        // A read-only background viewer, not a request to scan: while the
        // other interface serves a Jetson, read only the station's cache.
        let mut arguments = vec![
            "-t",
            "-f",
            "ACTIVE,SSID,SECURITY,SIGNAL",
            "dev",
            "wifi",
            "list",
        ];
        if direct_ap_active() {
            arguments.extend(["ifname", "wlan0", "--rescan", "no"]);
        } else {
            arguments.extend(["--rescan", "auto"]);
        }
        let wifi = self.output(&arguments, 3);
        let address = self.output(&["-t", "-f", "IP4.ADDRESS", "dev", "show", "wlan0"], 2);
        Probe {
            wifi: wifi::networks(&wifi),
            address: wifi::address(&address),
        }
    }

    pub fn publish(&self, probe: Probe, params: &Backend) -> Result<Value, Error> {
        let mut value = Value::object([
            ("wifi", probe.wifi),
            ("ip_address", Value::text(&probe.address)),
        ]);
        Self::parameters(&mut value, params);
        *self
            .cache
            .lock()
            .map_err(|_| Error::Source("network cache lock poisoned".into()))? =
            Some(value.clone());
        Ok(value)
    }

    fn parameters(value: &mut Value, params: &Backend) {
        let Value::Object(fields) = value else {
            return;
        };
        for (name, key, default) in [
            ("tethering_enabled", "HotspotOnBoot", Value::Bool(false)),
            ("roaming_enabled", "GsmRoaming", Value::Bool(false)),
            ("gsm_metered", "GsmMetered", Value::Bool(false)),
            ("apn", "GsmApn", Value::text("")),
        ] {
            let points: Vec<_> = name.chars().map(u32::from).collect();
            if let Some((_, previous)) = fields.iter_mut().find(|(key, _)| *key == points) {
                *previous = params.get(key, &default);
            } else {
                fields.push((points, params.get(key, &default)));
            }
        }
    }
}

/// carrot's `common/wifi_coexistence.py`: jetlinkd's Wi-Fi direct helper is
/// serving a Jetson HUD from a private access point right now.
fn direct_ap_active() -> bool {
    let Some(value) = std::fs::read_to_string("/dev/shm/carrot-jetlink-direct-wifi.json")
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
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
