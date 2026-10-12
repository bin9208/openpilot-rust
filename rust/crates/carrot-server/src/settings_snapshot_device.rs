use crate::{json_fields::set, param_restore::read_setting_value, params::Backend, Error, Value};

enum Default {
    Text(&'static str),
    Integer(i64),
    Bool(bool),
}
impl Default {
    fn value(&self) -> Value {
        match self {
            Self::Text(value) => Value::text(value),
            Self::Integer(value) => Value::integer(*value),
            Self::Bool(value) => Value::Bool(*value),
        }
    }
}
use Default::{Bool, Integer, Text};

const GROUPS: &[(&str, &[(&str, Default)])] = &[
    (
        "Device",
        &[
            ("DeviceType", Text("unknown")),
            ("DongleId", Text("")),
            ("HardwareSerial", Text("")),
            ("LanguageSetting", Text("main_en")),
            ("SoftwareMenu", Integer(1)),
        ],
    ),
    (
        "Software",
        &[
            ("UpdaterCurrentDescription", Text("")),
            ("UpdaterState", Text("")),
            ("UpdateAvailable", Bool(false)),
            ("UpdaterFetchAvailable", Bool(false)),
            ("UpdateFailedCount", Integer(0)),
            ("UpdaterTargetBranch", Text("")),
            ("GitBranch", Text("")),
            ("UpdaterAvailableBranches", Text("")),
            ("LastUpdateTime", Text("")),
            ("UpdaterNewDescription", Text("")),
        ],
    ),
    (
        "Toggles",
        &[
            ("OpenpilotEnabledToggle", Bool(false)),
            ("ExperimentalMode", Bool(false)),
            ("ExperimentalModeConfirmed", Bool(false)),
            ("DisengageOnAccelerator", Bool(false)),
            ("IsLdwEnabled", Bool(false)),
            ("AlwaysOnDM", Bool(false)),
            ("RecordFront", Bool(false)),
            ("RecordAudio", Bool(false)),
            ("IsMetric", Bool(false)),
            ("LongitudinalPersonality", Integer(1)),
        ],
    ),
    (
        "Developer",
        &[
            ("AdbEnabled", Bool(false)),
            ("SshEnabled", Bool(false)),
            ("JoystickDebugMode", Bool(false)),
            ("LongitudinalManeuverMode", Bool(false)),
            ("AlphaLongitudinalEnabled", Bool(false)),
            ("GithubUsername", Text("")),
            ("GithubSshKeys", Text("")),
        ],
    ),
];

pub(crate) fn groups() -> Value {
    Value::Object(
        GROUPS
            .iter()
            .map(|(group, entries)| {
                (
                    group.chars().map(u32::from).collect(),
                    Value::Array(entries.iter().map(|(name, _)| Value::text(name)).collect()),
                )
            })
            .collect(),
    )
}

pub(crate) fn values(params: &Backend, ssh: &Value) -> Result<Value, Error> {
    let mut values = Value::Object(Vec::new());
    for (name, default) in GROUPS.iter().flat_map(|(_, entries)| *entries) {
        if !matches!(*name, "DeviceType" | "GithubUsername" | "GithubSshKeys") {
            set(
                &mut values,
                name,
                read_setting_value(params, name, &default.value()),
            )?;
        }
    }
    set(
        &mut values,
        "DeviceType",
        Value::text(&crate::params_http::device_type()),
    )?;
    set(&mut values, "GithubUsername", ssh.get("username").clone())?;
    set(
        &mut values,
        "GithubSshKeys",
        Value::text(if ssh.get("has_keys").truth() { "1" } else { "" }),
    )?;
    Ok(values)
}
