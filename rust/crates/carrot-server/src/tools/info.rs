//! Local-only support metadata from device_info.get_tools_device_info, using the existing HardwareInfo provider.
use super::config::Config;
use crate::{json_fields::set, params::Backend, Value};
use openpilot_hardware_info::{HardwareInfo, JsonValue};

fn native(value: JsonValue) -> Value {
    value
        .to_json()
        .ok()
        .and_then(|text| Value::parse(&text).ok())
        .unwrap_or(Value::Null)
}
fn stripped(value: &Value) -> Value {
    crate::param_changes::text::stripped(value, true).unwrap_or_else(|_| Value::text(""))
}
pub(super) fn snapshot(config: &Config) -> Value {
    let params = crate::system::fresh::reopen(config.params.as_ref())
        .ok()
        .flatten();
    let state = config
        .paths
        .history
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let backend = params.map_or_else(
        || Backend::memory(state.into()),
        |params| Backend::native(params, state.into()),
    );
    gather(
        &backend,
        &*openpilot_hardware_info::for_runtime(),
        std::path::Path::new("/proc/stat"),
    )
}
pub fn gather(params: &Backend, hardware: &dyn HardwareInfo, stat: &std::path::Path) -> Value {
    let mut values = Value::object([]);
    for name in [
        "DongleId",
        "HardwareSerial",
        "DevicePosition",
        "GitBranch",
        "GitCommit",
        "GitCommitDate",
        "GitPullTime",
    ] {
        if let Err(error) = set(&mut values, name, params.get(name, &Value::text(""))) {
            eprintln!("Tools device value: {error}");
        }
    }
    let device = hardware
        .get_device_type()
        .unwrap_or_else(|_| "unknown".into());
    let serial = hardware.get_serial().unwrap_or_default();
    let imei = hardware
        .get_imei(0)
        .map(native)
        .unwrap_or_else(|_| Value::text(""));
    let imei = stripped(&imei);
    let modem = hardware
        .get_modem_version()
        .map(native)
        .unwrap_or_else(|_| Value::text(""));
    let network = hardware
        .get_network_info()
        .ok()
        .flatten()
        .map(native)
        .unwrap_or(Value::Null);
    let sim = hardware.get_sim_info().map(native).unwrap_or(Value::Null);
    let sim_state = match sim.get("sim_state") {
        Value::Array(values) => values
            .iter()
            .map(stripped)
            .find(Value::truth)
            .unwrap_or_else(|| Value::text("")),
        value => stripped(value),
    };
    let serial_param = stripped(values.get("HardwareSerial"));
    let serial = if serial_param.truth() {
        serial_param
    } else {
        Value::text(serial.trim())
    };
    let boot = std::fs::read_to_string(stat)
        .ok()
        .and_then(|body| {
            body.lines().find_map(|line| {
                line.strip_prefix("btime ")
                    .and_then(|value| Value::text(value.trim()).float().ok())
            })
        })
        .and_then(|seconds| {
            use num_traits::ToPrimitive;
            chrono::DateTime::from_timestamp(seconds.to_i64()?, 0)
                .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .unwrap_or_default();
    Value::object([
        (
            "identity",
            Value::object([
                (
                    "device_type",
                    Value::text(if device.trim().is_empty() {
                        "unknown"
                    } else {
                        device.trim()
                    }),
                ),
                ("imei_available", Value::Bool(imei.truth())),
                ("imei", imei),
                ("dongle_id", stripped(values.get("DongleId"))),
                ("hardware_serial", serial),
                ("position", stripped(values.get("DevicePosition"))),
            ]),
        ),
        (
            "software",
            Value::object([
                ("branch", stripped(values.get("GitBranch"))),
                ("commit", stripped(values.get("GitCommit"))),
                ("commit_date", stripped(values.get("GitCommitDate"))),
                ("last_update", stripped(values.get("GitPullTime"))),
            ]),
        ),
        (
            "connectivity",
            Value::object([
                ("carrier", stripped(network.get("operator"))),
                ("technology", stripped(network.get("technology"))),
                ("state", stripped(network.get("state"))),
                ("sim_state", sim_state),
                ("modem_version", stripped(&modem)),
            ]),
        ),
        (
            "runtime",
            Value::object([("boot_time", Value::text(&boot))]),
        ),
    ])
}
