use openpilot_carrot_server::{
    param_changes::{History, Paths},
    params::Backend,
    settings::Catalog,
    system::{
        actions::{self, Action, Failure},
        calibration, defaults,
    },
    Error, Value,
};
use openpilot_params::Params;
use serde::Deserialize;
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
pub struct Input {
    pub mode: String,
    pub root: PathBuf,
    #[serde(default = "available")]
    pub params: bool,
    #[serde(default)]
    pub engaged: bool,
    #[serde(default)]
    pub action: Option<Selected>,
    #[serde(default)]
    pub initial: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub namespace: Option<Namespace>,
}
fn available() -> bool {
    true
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selected {
    Reboot,
    Poweroff,
    Recalibrate,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Namespace {
    Removed,
    Blocked,
}

fn bytes(hex: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|part| Ok(u8::from_str_radix(std::str::from_utf8(part)?, 16)?))
        .collect()
}

pub fn run(input: Input) -> Result<Value, Box<dyn std::error::Error>> {
    let store = Params::open(&input.root.join("params"), "d")?;
    for (key, raw) in &input.initial {
        fs::write(store.directory().join(key), bytes(raw)?)?;
    }
    if let Some(namespace) = &input.namespace {
        fs::remove_file(store.directory())?;
        if matches!(namespace, Namespace::Blocked) {
            fs::set_permissions(input.root.join("params"), fs::Permissions::from_mode(0o0))?;
        }
    }
    let mut backend = if input.params {
        Backend::native(store.clone(), input.root.join("state"))
    } else {
        Backend::memory(input.root.join("state"))
    };
    let result = match input.mode.as_str() {
        "action" => {
            let action = match input.action.ok_or("missing action")? {
                Selected::Reboot => Action::Reboot,
                Selected::Poweroff => Action::Poweroff,
                Selected::Recalibrate => Action::Recalibrate,
            };
            actions::run(action, &mut backend, input.engaged)
                .map(|body| (200, body))
                .map_err(|error| match error {
                    Failure::Engaged => (409, Error::Source(error.to_string())),
                    Failure::Boundary(error) => (500, error),
                })
        }
        "calibration" => calibration::status(backend.native_params())
            .map(|body| (200, body))
            .map_err(|error| (500, error)),
        "defaults" => {
            let history = History::new(Paths {
                log: input.root.join("state/param_changes.jsonl"),
                baseline: input.root.join("state/fingerprint_baseline.json"),
            });
            let catalog = Catalog::from_data(Value::parse(&input.data.to_string())?)?
                .with_gap_limits(backend.maximum_gap_levels())?;
            defaults::reset(&mut backend, &catalog, &history)
                .map(|body| (if body.get("ok").truth() { 200 } else { 500 }, body))
                .map_err(|error| (500, error))
        }
        _ => return Err("invalid action fixture mode".into()),
    };
    let (status, body) = result.unwrap_or_else(|(status, error)| {
        (
            status,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        )
    });
    if input.namespace.is_some() {
        fs::set_permissions(input.root.join("params"), fs::Permissions::from_mode(0o700))?;
    }
    let spawned = input.root.join("spawn.json");
    if !input.params
        && !input.engaged
        && matches!(input.action, Some(Selected::Reboot))
        && status == 200
    {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(record) = fs::read_to_string(&spawned) {
                let record: serde_json::Value = serde_json::from_str(&record)?;
                if let Some(pid) = record["pid"].as_u64() {
                    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                        break;
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err("owned reboot recipient not reaped".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let mut keys: std::collections::BTreeSet<_> = input.initial.keys().cloned().collect();
    keys.extend(["DoReboot", "DoShutdown", "OnroadCycleRequested"].map(String::from));
    if let Some(rows) = input.data["params"].as_array() {
        keys.extend(
            rows.iter()
                .filter_map(|row| row["name"].as_str().map(String::from)),
        );
    }
    let mut stored = Vec::new();
    for key in keys {
        let path = store.directory().join(&key);
        if path.is_file() {
            let bytes = fs::read(path)?;
            stored.push((
                key.chars().map(u32::from).collect(),
                Value::text(
                    &bytes
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                ),
            ));
        }
    }
    let attempted = spawned.exists()
        || (!input.params
            && !input.engaged
            && matches!(input.action, Some(Selected::Reboot))
            && status == 500);
    let commands = if attempted {
        Value::Array(vec![Value::Array(vec![
            Value::text("sudo"),
            Value::text("reboot"),
        ])])
    } else {
        Value::Array(Vec::new())
    };
    Ok(Value::object([
        ("status", Value::integer(status)),
        ("body", body),
        ("commands", commands),
        ("stored", Value::Object(stored)),
    ]))
}
