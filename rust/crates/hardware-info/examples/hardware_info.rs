use num_traits::ToPrimitive;
use openpilot_hardware_info::{
    get_cmdline, get_default_route_iface, paths::Paths, wpa_supplicant_cmd, Error, HardwareInfo,
    HardwarePaths, JsonValue, JsonView, NetworkType, Pc, ThermalConfig, ThermalZone, Tici,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    wpa_endpoint: PathBuf,
    #[serde(default = "tici")]
    hardware: String,
    pc: Option<bool>,
    #[serde(default)]
    darwin: bool,
    steps: Vec<Step>,
}
fn tici() -> String {
    "tici".into()
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Step {
    Call(Call),
    Write {
        path: String,
        bytes: Vec<u8>,
    },
    Directory {
        path: String,
    },
    Remove {
        path: String,
    },
    Chmod {
        path: String,
        mode: u32,
    },
    Symlink {
        path: String,
        target: String,
    },
    Env {
        values: BTreeMap<String, Option<Vec<u8>>>,
    },
    NewInstance,
    Zone {
        name: String,
        zone: String,
        #[serde(default = "default_scale")]
        scale_json: String,
    },
}
fn default_scale() -> String {
    "1000.0".into()
}
#[derive(Deserialize)]
struct Call {
    method: String,
    #[serde(default)]
    slot: usize,
    #[serde(default)]
    network: u16,
    #[serde(default)]
    value_json: String,
    #[serde(default)]
    name: String,
    #[serde(default = "status")]
    command: String,
    #[serde(default = "timeout")]
    timeout_ms: u64,
    #[serde(default = "uptime")]
    uptime: f64,
}
fn status() -> String {
    "STATUS".into()
}
fn timeout() -> u64 {
    200
}
fn uptime() -> f64 {
    130.0
}
struct Base;
impl HardwareInfo for Base {
    fn get_device_type(&self) -> Result<String, Error> {
        Ok("base".into())
    }
}
fn boolean(value: bool) -> Result<JsonValue, Error> {
    Ok(JsonValue::parse(if value { "true" } else { "false" })?)
}
fn optional(value: Option<JsonValue>) -> Result<JsonValue, Error> {
    match value {
        Some(value) => Ok(value),
        None => Ok(JsonValue::parse("null")?),
    }
}
fn value_json(value: Value) -> Result<JsonValue, Error> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn hardware(paths: &HardwarePaths, root: &Path) -> Tici {
    let mut hardware = Tici::with_paths(paths.clone());
    hardware.params_root = Some(root.join("params"));
    hardware
}
fn zone_metadata(zone: &ThermalZone) -> Value {
    json!({"name": zone.name, "scale": zone.scale})
}
fn config_metadata(config: &ThermalConfig) -> Value {
    let zones = |value: &Option<Vec<ThermalZone>>| {
        value
            .as_ref()
            .map(|v| v.iter().map(zone_metadata).collect::<Vec<_>>())
    };
    json!({"cpu": zones(&config.cpu), "gpu": zones(&config.gpu), "dsp": config.dsp.as_ref().map(zone_metadata), "pmic": zones(&config.pmic),
        "memory": config.memory.as_ref().map(zone_metadata), "intake": config.intake.as_ref().map(zone_metadata), "exhaust": config.exhaust.as_ref().map(zone_metadata),
        "gnss": config.gnss.as_ref().map(zone_metadata), "bottomSoc": config.bottom_soc.as_ref().map(zone_metadata)})
}
fn call(
    request: &Request,
    call: &Call,
    tici: &Tici,
    zones: &mut BTreeMap<String, ThermalZone>,
    config: &mut Option<ThermalConfig>,
) -> Result<JsonValue, Error> {
    let info: &dyn HardwareInfo = match request.hardware.as_str() {
        "tici" => tici,
        "pc" => &Pc,
        "base" => &Base,
        _ => return Err(Error::Value("unknown fixture kind")),
    };
    match call.method.as_str() {
        "device_type" => Ok(JsonValue::text(&info.get_device_type()?)),
        "os_version" => optional(info.get_os_version()?.as_deref().map(JsonValue::text)),
        "serial" => Ok(JsonValue::text(&info.get_serial()?)),
        "imei" => info.get_imei(call.slot),
        "modem_state" if request.hardware == "tici" => tici.get_modem_state(),
        "modem_state" | "parse_strength" if request.hardware != "tici" => {
            Err(Error::Attribute("unavailable source method"))
        }
        "network_type" => Ok(JsonValue::parse(&info.get_network_type()?.0.to_string())?),
        "sim_info" => info.get_sim_info(),
        "network_info" => optional(info.get_network_info()?),
        "strength" => Ok(JsonValue::parse(
            &info
                .get_network_strength(NetworkType(call.network))?
                .ordinal()
                .to_string(),
        )?),
        "parse_strength" => Ok(JsonValue::parse(
            &tici
                .parse_strength(&JsonValue::parse(&call.value_json)?)?
                .ordinal()
                .to_string(),
        )?),
        "metered" => boolean(info.get_network_metered(NetworkType(call.network))?),
        "modem_version" => info.get_modem_version(),
        "modem_temperatures" => info.get_modem_temperatures(),
        "current_power" => info.get_current_power_draw()?.to_json(),
        "som_power" => info.get_som_power_draw()?.to_json(),
        "brightness" => info.get_screen_brightness()?.to_json(),
        "gpu_usage" => info.get_gpu_usage_percent()?.to_json(),
        "voltage" => info.get_voltage()?.to_json(),
        "current" => info.get_current()?.to_json(),
        "internal_panda" => boolean(info.has_internal_panda()),
        "booted" if request.hardware == "tici" => boolean(tici.booted_at(call.uptime)),
        "booted" => boolean(info.booted()?),
        "networks" => optional(
            info.get_networks()?
                .map(|networks| networks.to_json())
                .transpose()?,
        ),
        "modem_usage" => {
            let (tx, rx) = info.get_modem_data_usage()?;
            Ok(JsonValue::parse(&format!(
                "[{},{}]",
                tx.to_json()?,
                rx.to_json()?
            ))?)
        }
        "cmdline" => value_json(json!(get_cmdline(&tici.paths.cmdline)?)),
        "route" => optional(
            get_default_route_iface(&tici.paths.route)?
                .as_deref()
                .map(JsonValue::text),
        ),
        "wpa" => value_json(json!(wpa_supplicant_cmd(
            &tici.paths.wpa_control,
            &call.command,
            Duration::from_millis(call.timeout_ms)
        )?)),
        "thermal_config" => {
            let created = info.get_thermal_config()?;
            let value = value_json(config_metadata(&created))?;
            *config = Some(created);
            Ok(value)
        }
        "thermal_read" => config
            .as_mut()
            .ok_or(Error::Attribute("no thermal config"))?
            .get_msg(),
        "zone_read" => {
            let zone = zones
                .get_mut(&call.name)
                .ok_or_else(|| Error::Key(call.name.clone()))?;
            let reading = zone.read()?.to_json()?;
            Ok(JsonValue::parse(&format!(
                "{{\"reading\":{},\"zone_number\":{}}}",
                reading.to_json()?,
                zone.zone_number
            ))?)
        }
        "paths" => {
            let paths = Paths {
                pc: request.pc.unwrap_or(request.hardware != "tici"),
                darwin: request.darwin,
            };
            value_json(
                json!({"comma_home": paths.comma_home()?.as_bytes(), "log_root": paths.log_root()?.as_bytes(), "swaglog_root": paths.swaglog_root()?.as_bytes(),
                "swaglog_ipc": paths.swaglog_ipc().as_bytes(), "download_cache_root": paths.download_cache_root().as_bytes(), "persist_root": paths.persist_root()?.as_bytes(),
                "stats_root": paths.stats_root()?.as_bytes(), "config_root": paths.config_root()?.as_bytes(), "shm_path": paths.shm_path().as_bytes()}),
            )
        }
        _ => Err(Error::Value("unknown fixture method")),
    }
}
fn fixture_path(root: &Path, path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let relative = Path::new(path.trim_start_matches('/'));
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("unsafe fixture path".into());
    }
    Ok(root.join(relative))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let request: Request = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    std::fs::create_dir_all(&request.root)?;
    let mut paths = HardwarePaths::under(&request.root);
    paths.wpa_control = request.wpa_endpoint.clone();
    let mut tici = hardware(&paths, &request.root);
    let mut zones = BTreeMap::new();
    let mut config = None;
    let mut results = Vec::new();
    for step in &request.steps {
        match step {
            Step::Call(operation) => {
                match call(&request, operation, &tici, &mut zones, &mut config)
                    .and_then(|value| value.to_json().map_err(Error::Format))
                {
                    Ok(value) => {
                        results.push(json!({"method": operation.method, "value_json": value}))
                    }
                    Err(error) => {
                        results.push(json!({"method": operation.method, "error": error.category()}))
                    }
                }
            }
            Step::Write { path, bytes } => {
                let path = fixture_path(&request.root, path)?;
                std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
                std::fs::write(path, bytes)?;
            }
            Step::Directory { path } => {
                std::fs::create_dir_all(fixture_path(&request.root, path)?)?
            }
            Step::Remove { path } => {
                let path = fixture_path(&request.root, path)?;
                if path.is_dir() && !path.is_symlink() {
                    std::fs::remove_dir_all(path)?;
                } else {
                    match std::fs::remove_file(path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            Step::Chmod { path, mode } => std::fs::set_permissions(
                fixture_path(&request.root, path)?,
                std::fs::Permissions::from_mode(*mode),
            )?,
            Step::Symlink { path, target } => {
                let path = fixture_path(&request.root, path)?;
                std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
                symlink(target, path)?;
            }
            Step::Env { values } => {
                for (key, value) in values {
                    match value {
                        Some(value) => std::env::set_var(key, OsString::from_vec(value.clone())),
                        None => std::env::remove_var(key),
                    }
                }
            }
            Step::NewInstance => tici = hardware(&paths, &request.root),
            Step::Zone {
                name,
                zone,
                scale_json,
            } => {
                let scale = match JsonValue::parse(scale_json)?.view() {
                    JsonView::Float(value) => value,
                    JsonView::Integer(value) => value.parse::<f64>()?,
                    JsonView::Bool(value) => i32::from(value).to_f64().ok_or("bad scale")?,
                    _ => return Err("nonnumeric scale".into()),
                };
                zones.insert(
                    name.clone(),
                    ThermalZone::with_root(zone, scale, &paths.thermal),
                );
            }
        }
    }
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
