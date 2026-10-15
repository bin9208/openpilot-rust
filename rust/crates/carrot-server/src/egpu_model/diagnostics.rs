use crate::{Error, Value};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub struct Telemetry {
    pub root: PathBuf,
    pub boot_id: PathBuf,
    pub now: Option<f64>,
}
impl Default for Telemetry {
    fn default() -> Self {
        Self {
            root: "/dev/shm".into(),
            boot_id: "/proc/sys/kernel/random/boot_id".into(),
            now: None,
        }
    }
}
impl Telemetry {
    fn now(&self) -> Result<f64, Error> {
        self.now
            .map(Ok)
            .unwrap_or_else(crate::web_sound::clock::now)
    }
    pub fn recovery(&self) -> Result<Value, Error> {
        let value = fs::read_to_string(self.root.join("carrot-egpu-recovery.json"))
            .ok()
            .and_then(|text| Value::parse(&text).ok());
        let Some(value) = value else {
            return Ok(Value::Object(Vec::new()));
        };
        let updated = value.get("updated");
        let fresh = matches!(
            updated,
            Value::Integer(_) | Value::Float(_) | Value::Bool(_)
        ) && updated.float().ok().is_some_and(|updated| {
            self.now()
                .is_ok_and(|now| (0. ..3.).contains(&(now - updated)))
        });
        let known = ["waiting", "recovering", "ready", "stopped"]
            .iter()
            .any(|state| value.get("state").text_eq(state));
        Ok(if fresh && known {
            value
        } else {
            Value::Object(Vec::new())
        })
    }
    pub fn jetlink(&self) -> Result<Value, Error> {
        use openpilot_jetlink::status;
        let now = self.now()?;
        let path = self.root.join("carrot-jetlink.json");
        let value = status::diagnostics(
            &status::read_json(&path),
            &status::read_fresh(&path, now),
            &status::read_fresh(&self.root.join("carrot-jetlink-model.json"), now),
            now,
        );
        Ok(Value::parse(
            &serde_json::to_string(&value).map_err(|error| Error::Source(error.to_string()))?,
        )?)
    }
}
fn read(path: &Path) -> Option<Value> {
    // Source reads at most32,768 Unicode characters; bound UTF-8 bytes as well.
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(32768 * 4)
        .read_to_string(&mut text)
        .ok()?;
    let value = Value::parse(&text.chars().take(32768).collect::<String>()).ok()?;
    matches!(value, Value::Object(_)).then_some(value)
}
fn matching(cache: &Path, sha: &str) -> Option<Value> {
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let root = cache.join("precompiled").join(sha);
    let value = read(&root.join("last_failure.json"))?;
    let installed_path = root.join("installed.json");
    let installed = match fs::metadata(&installed_path) {
        Ok(_) => read(&installed_path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Rust packages bind the same pickle identity in a verified native
            // provider record. A broken existing legacy identity never falls back.
            read(&root.join("native-provider.json"))?;
            let identity =
                openpilot_usbgpu::model_delivery::packaged::identity(&root.join("model.pkl"))
                    .ok()?;
            Value::parse(&identity.to_string()).ok()?
        }
        Err(_) => return None,
    };
    if !installed.get("pickle").has("sha256")
        || value.get("pickle_sha256") != installed.get("pickle").get("sha256")
    {
        return None;
    }
    Some(value)
}
pub fn saved(cache: &Path, sha: &str) -> Value {
    let Some(value) = matching(cache, sha).filter(|v| matches!(v.get("error"), Value::Text(_)))
    else {
        return Value::Object(Vec::new());
    };
    Value::Object(
        [
            "time",
            "phase",
            "rejected",
            "pickle_sha256",
            "error",
            "boot_id",
            "worker",
        ]
        .into_iter()
        .filter(|key| value.has(key))
        .map(|key| (key.chars().map(u32::from).collect(), value.get(key).clone()))
        .collect(),
    )
}
pub fn current(cache: &Path, sha: &str, boot_path: &Path) -> Value {
    let Some(value) = matching(cache, sha) else {
        return Value::Object(Vec::new());
    };
    let boot = fs::read_to_string(boot_path).unwrap_or_default();
    if boot.trim().is_empty() || !value.get("boot_id").text_eq(boot.trim()) {
        return Value::Object(Vec::new());
    }
    let error = value
        .get("error")
        .string()
        .unwrap_or_default()
        .to_lowercase();
    let code = if error.contains("pcie link not up") {
        "pcie"
    } else if error.contains("precompiled egpu worker timed out") {
        "timeout"
    } else if ["libusb", "bulk out", "bulk in", "input/output error"]
        .iter()
        .any(|marker| error.contains(marker))
    {
        "usb"
    } else {
        "runtime"
    };
    Value::object([
        ("error_code", Value::text(code)),
        (
            "worker",
            if matches!(value.get("worker"), Value::Object(_)) {
                value.get("worker").clone()
            } else {
                Value::Object(Vec::new())
            },
        ),
        ("phase", value.get("phase").clone()),
    ])
}
