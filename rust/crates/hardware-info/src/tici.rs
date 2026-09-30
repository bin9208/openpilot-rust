use crate::{
    default_network_metered, get_cmdline, get_default_route_iface,
    io::{monotonic, read_integer, read_integer_default, read_text, sudo_read},
    keyfile::{ssid_bytes, Keyfile},
    networks, numeric, value, wpa_supplicant_cmd, Commands, Error, HardwareInfo, HardwarePaths,
    JsonValue, JsonView, NativeCommands, NetworkStrength, NetworkType, Networks, Number,
    ThermalConfig, ThermalZone,
};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::{
    collections::HashMap,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

static MODEL: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();

pub struct Tici<C = NativeCommands> {
    pub paths: HardwarePaths,
    pub commands: C,
    pub wpa_timeout: Duration,
    /// None uses Params::for_runtime on every access, matching Params().
    pub params_root: Option<PathBuf>,
    pub params_prefix: String,
}
impl Default for Tici {
    fn default() -> Self {
        Self::with_paths(HardwarePaths::default())
    }
}
impl Tici {
    pub fn with_paths(paths: HardwarePaths) -> Self {
        Self {
            paths,
            commands: NativeCommands,
            wpa_timeout: Duration::from_millis(200),
            params_root: None,
            params_prefix: "d".into(),
        }
    }
}
impl<C: Commands> Tici<C> {
    pub fn get_modem_state(&self) -> Result<JsonValue, Error> {
        let text = match read_text(&self.paths.modem) {
            Ok(text) => text,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(JsonValue::parse("{}")?)
            }
            Err(error) => return Err(error),
        };
        match JsonValue::parse(&text) {
            Ok(value) => Ok(value),
            Err(openpilot_logmessaged::JsonError::Syntax { .. }) => Ok(JsonValue::parse("{}")?),
            Err(error) => Err(error.into()),
        }
    }
    pub fn parse_strength(&self, percentage: &JsonValue) -> Result<NetworkStrength, Error> {
        let less = |threshold: i32| -> Result<bool, Error> {
            match percentage.view() {
                JsonView::Integer(value) => {
                    Ok(numeric::integer(value, 10)? < BigInt::from(threshold))
                }
                JsonView::Float(value) => Ok(value < f64::from(threshold)),
                JsonView::Bool(value) => Ok(i32::from(value) < threshold),
                JsonView::Null | JsonView::Text(_) | JsonView::Array(_) | JsonView::Object(_) => {
                    Err(Error::Type(
                        "signal quality cannot be ordered with an integer",
                    ))
                }
            }
        };
        Ok(if less(25)? {
            NetworkStrength::Poor
        } else if less(50)? {
            NetworkStrength::Moderate
        } else if less(75)? {
            NetworkStrength::Good
        } else {
            NetworkStrength::Great
        })
    }
    fn wpa(&self, command: &str) -> Result<std::collections::BTreeMap<String, String>, Error> {
        wpa_supplicant_cmd(&self.paths.wpa_control, command, self.wpa_timeout)
    }
    fn wifi_metered(&self) -> Result<Option<bool>, Error> {
        let status = self.wpa("STATUS")?;
        let Some(ssid) = status.get("ssid").filter(|ssid| !ssid.is_empty()) else {
            return Ok(None);
        };
        let bytes = ssid_bytes(ssid)?;
        let list = bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(";")
            + ";";
        for directory in &self.paths.nm_connections {
            let entries = match std::fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::NotADirectory
                            | std::io::ErrorKind::PermissionDenied
                    ) || error.raw_os_error() == Some(40) =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                if !entry.file_name().as_bytes().ends_with(b".nmconnection") {
                    continue;
                }
                let raw = sudo_read(&self.commands, &entry.path());
                if raw.is_empty() {
                    continue;
                }
                let parsed = (|| {
                    let keyfile = Keyfile::parse(&raw)?;
                    let keyfile_ssid = keyfile.get("wifi", "ssid", "");
                    if keyfile_ssid != ssid && keyfile_ssid != list {
                        return Ok(None);
                    }
                    Ok(Some(keyfile.metered()?))
                })();
                match parsed {
                    Ok(Some(metered)) if metered == BigInt::from(1) => return Ok(Some(true)),
                    Ok(Some(metered)) if metered == BigInt::from(2) => return Ok(Some(false)),
                    Ok(Some(_)) => return Ok(None),
                    Ok(None) | Err(Error::Value(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(None)
    }
    fn gsm_metered(&self) -> Result<bool, Error> {
        let params = match &self.params_root {
            Some(root) => openpilot_params::Params::open(root, &self.params_prefix)?,
            None => openpilot_params::Params::for_runtime()?,
        };
        match params.get_bool("GsmMetered") {
            Ok(value) => Ok(value),
            Err(openpilot_params::Error::Io(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    pub fn booted_at(&self, uptime: f64) -> bool {
        !(sudo_read(&self.commands, &self.paths.encoder_state).contains("Core state: 0")
            && uptime < 120.0)
    }
}
impl<C: Commands> HardwareInfo for Tici<C> {
    fn get_device_type(&self) -> Result<String, Error> {
        let cache = MODEL.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(value) = cache
            .lock()
            .map_err(|_| Error::CachePoisoned)?
            .get(&self.paths.model)
        {
            return Ok(value.clone());
        }
        let model = read_text(&self.paths.model)?;
        let value = model
            .trim_matches('\0')
            .rsplit("comma ")
            .next()
            .unwrap_or("")
            .to_owned();
        cache
            .lock()
            .map_err(|_| Error::CachePoisoned)?
            .insert(self.paths.model.clone(), value.clone());
        Ok(value)
    }
    fn booted(&self) -> Result<bool, Error> {
        let state = sudo_read(&self.commands, &self.paths.encoder_state);
        Ok(!state.contains("Core state: 0") || monotonic()? >= 120.0)
    }
    fn get_os_version(&self) -> Result<Option<String>, Error> {
        Ok(Some(numeric::trim(&read_text(&self.paths.version)?).into()))
    }
    fn get_serial(&self) -> Result<String, Error> {
        get_cmdline(&self.paths.cmdline)?
            .remove("androidboot.serialno")
            .ok_or_else(|| Error::Key("androidboot.serialno".into()))
    }
    fn get_voltage(&self) -> Result<Number, Error> {
        Ok(Number::Integer(read_integer(&self.paths.voltage)?))
    }
    fn get_current(&self) -> Result<Number, Error> {
        Ok(Number::Integer(read_integer(&self.paths.current)?))
    }
    fn get_network_type(&self) -> Result<NetworkType, Error> {
        if let Ok(Some(interface)) = get_default_route_iface(&self.paths.route) {
            if interface.starts_with("wlan") {
                return Ok(NetworkType::WIFI);
            }
            if interface.starts_with("eth") {
                return Ok(NetworkType::ETHERNET);
            }
        }
        let state = self.get_modem_state()?;
        if value::truthy(&value::get(&state, "connected", "null")?) {
            let network = value::get(&state, "network_type", "\"\"")?;
            if network.text_eq("nr") {
                return Ok(NetworkType::CELL_5G);
            }
            if network.text_eq("lte") {
                return Ok(NetworkType::CELL_4G);
            }
            if network.text_eq("utran") || network.text_eq("umts") {
                return Ok(NetworkType::CELL_3G);
            }
            if network.text_eq("gsm") {
                return Ok(NetworkType::CELL_2G);
            }
        }
        Ok(NetworkType::NONE)
    }
    fn get_sim_info(&self) -> Result<JsonValue, Error> {
        let state = self.get_modem_state()?;
        let sim_id = value::get(&state, "iccid", "\"\"")?;
        let mut sim_state = value::get(&state, "sim_state", "null")?;
        if !value::truthy(&sim_state) {
            sim_state = JsonValue::text(if value::truthy(&sim_id) {
                "READY"
            } else {
                "ABSENT"
            });
        }
        let mut mcc_mnc = value::get(&state, "mcc_mnc", "null")?;
        if !value::truthy(&mcc_mnc) {
            mcc_mnc = JsonValue::parse("null")?;
        }
        value::object([
            ("sim_id", sim_id),
            ("mcc_mnc", mcc_mnc),
            ("network_type", JsonValue::parse("[\"Unknown\"]")?),
            ("sim_state", value::array(&[sim_state])?),
            ("data_connected", value::get(&state, "connected", "false")?),
        ])
    }
    fn get_imei(&self, slot: usize) -> Result<JsonValue, Error> {
        if slot != 0 {
            return Ok(JsonValue::text(""));
        }
        value::get(&self.get_modem_state()?, "imei", "\"\"")
    }
    fn get_network_info(&self) -> Result<Option<JsonValue>, Error> {
        if self.get_device_type()? == "mici" {
            return Ok(None);
        }
        let state = self.get_modem_state()?;
        let network = value::get(&state, "network_type", "null")?;
        let technology = if value::truthy(&network) {
            value::upper(&network)?
        } else {
            JsonValue::text("")
        };
        Ok(Some(value::object([
            ("technology", technology),
            ("operator", value::get(&state, "operator", "\"\"")?),
            ("band", value::get(&state, "band", "\"\"")?),
            ("channel", value::get(&state, "channel", "0")?),
            ("extra", value::get(&state, "extra", "\"\"")?),
            ("state", value::get(&state, "state", "\"UNKNOWN\"")?),
        ])?))
    }
    fn get_network_strength(&self, network: NetworkType) -> Result<NetworkStrength, Error> {
        let read = || -> Result<NetworkStrength, Error> {
            match network {
                NetworkType::NONE => Ok(NetworkStrength::Unknown),
                NetworkType::ETHERNET => Ok(NetworkStrength::Great),
                NetworkType::WIFI => {
                    let status = self.wpa("SIGNAL_POLL")?;
                    if let Some(rssi) = status.get("RSSI") {
                        let dbm = numeric::integer(rssi, 10)?;
                        if dbm > BigInt::from(-100) && dbm <= BigInt::from(0) {
                            let percentage =
                                120 + dbm.to_i32().ok_or(Error::Overflow)?.clamp(-100, -20);
                            return self
                                .parse_strength(&JsonValue::parse(&percentage.to_string())?);
                        }
                    }
                    Ok(NetworkStrength::Unknown)
                }
                _ => self.parse_strength(&value::get(
                    &self.get_modem_state()?,
                    "signal_quality",
                    "0",
                )?),
            }
        };
        match read() {
            Ok(strength) => Ok(strength),
            Err(_) => Ok(NetworkStrength::Unknown),
        }
    }
    fn get_network_metered(&self, network: NetworkType) -> Result<bool, Error> {
        if matches!(
            network,
            NetworkType::CELL_2G
                | NetworkType::CELL_3G
                | NetworkType::CELL_4G
                | NetworkType::CELL_5G
        ) {
            return self.gsm_metered();
        }
        if network == NetworkType::WIFI {
            if let Ok(Some(metered)) = self.wifi_metered() {
                return Ok(metered);
            }
        }
        Ok(default_network_metered(network))
    }
    fn get_modem_version(&self) -> Result<JsonValue, Error> {
        let version = value::get(&self.get_modem_state()?, "modem_version", "null")?;
        if value::truthy(&version) {
            Ok(version)
        } else {
            Ok(JsonValue::parse("null")?)
        }
    }
    fn get_modem_temperatures(&self) -> Result<JsonValue, Error> {
        value::get(&self.get_modem_state()?, "temperatures", "[]")
    }
    fn get_current_power_draw(&self) -> Result<Number, Error> {
        Ok(Number::Float(numeric::divide(
            &read_integer_default(&self.paths.power),
            1e6,
        )?))
    }
    fn get_som_power_draw(&self) -> Result<Number, Error> {
        let voltage = read_integer_default(&self.paths.som_voltage);
        let current = read_integer_default(&self.paths.som_current);
        Ok(Number::Float(numeric::divide(&(voltage * current), 1e12)?))
    }
    fn get_thermal_config(&self) -> Result<ThermalConfig, Error> {
        let zone = |name: &str| ThermalZone::with_root(name, 1000.0, &self.paths.thermal);
        let mici = self.get_device_type()? == "mici";
        Ok(ThermalConfig {
            cpu: Some(
                (0..4)
                    .map(|i| zone(&format!("cpu{i}-silver-usr")))
                    .chain((0..4).map(|i| zone(&format!("cpu{i}-gold-usr"))))
                    .collect(),
            ),
            gpu: Some(vec![zone("gpu0-usr"), zone("gpu1-usr")]),
            dsp: Some(zone("compute-hvx-usr")),
            memory: Some(zone("ddr-usr")),
            pmic: Some(vec![zone("pm8998_tz"), zone("pm8005_tz")]),
            intake: mici.then(|| zone("intake")),
            exhaust: mici.then(|| zone("exhaust")),
            gnss: mici.then(|| zone("gnss")),
            bottom_soc: mici.then(|| zone("bottom_soc")),
        })
    }
    fn get_screen_brightness(&self) -> Result<Number, Error> {
        let read = || -> Result<Number, Error> {
            let maximum = numeric::float(numeric::trim(&read_text(&self.paths.max_brightness)?))?;
            let value = numeric::float(&read_text(&self.paths.brightness)?)?;
            let divisor = maximum / 100.0;
            if divisor == 0.0 {
                return Err(Error::ZeroDivision);
            }
            Ok(Number::Integer(numeric::float_integer(value / divisor)?))
        };
        match read() {
            Ok(value) => Ok(value),
            Err(_) => Ok(Number::zero()),
        }
    }
    fn get_gpu_usage_percent(&self) -> Result<Number, Error> {
        let read = || -> Result<Number, Error> {
            let text = read_text(&self.paths.gpu_busy)?;
            let parts: Vec<_> = text
                .split(numeric::whitespace)
                .filter(|s| !s.is_empty())
                .collect();
            let [used, total] = parts.as_slice() else {
                return Err(Error::Value("expected used and total"));
            };
            let used = 100.0 * numeric::integer_float(&numeric::integer(used, 10)?)?;
            let total = numeric::integer_float(&numeric::integer(total, 10)?)?;
            if total == 0.0 {
                return Err(Error::ZeroDivision);
            }
            Ok(Number::Float(used / total))
        };
        match read() {
            Ok(value) => Ok(value),
            Err(_) => Ok(Number::zero()),
        }
    }
    fn get_networks(&self) -> Result<Option<Networks>, Error> {
        let wlan = networks::scan(&self.commands, "wlan0");
        let lte = match self.get_network_info()? {
            Some(info) => networks::parse_lte(&value::get(&info, "extra", "\"\"")?)?,
            None => None,
        };
        Ok(Some(Networks { wlan, lte }))
    }
    fn get_modem_data_usage(&self) -> Result<(JsonValue, JsonValue), Error> {
        let state = self.get_modem_state()?;
        Ok((
            value::get(&state, "tx_bytes", "-1")?,
            value::get(&state, "rx_bytes", "-1")?,
        ))
    }
    fn has_internal_panda(&self) -> bool {
        true
    }
}
