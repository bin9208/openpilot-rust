use crate::{
    logging,
    state::{self, Shared, Stop},
    Error,
};
use openpilot_logging::producer::Logger;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Default)]
pub struct Attributes(HashMap<PathBuf, Option<Vec<u8>>>);
impl Attributes {
    fn get(&mut self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        if let Some(value) = self.0.get(path) {
            return Ok(value.clone());
        }
        let mut buffer = vec![0; 65536];
        let value = match rustix::fs::getxattr(path, "user.upload", &mut buffer[..]) {
            Ok(size) => {
                buffer.truncate(size);
                Some(buffer)
            }
            Err(rustix::io::Errno::NODATA) => None,
            Err(error) => return Err(error.into()),
        };
        self.0.insert(path.into(), value.clone());
        Ok(value)
    }
    fn put(&mut self, path: &Path, value: u32) -> io::Result<()> {
        self.0.remove(path);
        rustix::fs::setxattr(
            path,
            "user.upload",
            &value.to_ne_bytes(),
            rustix::fs::XattrFlags::empty(),
        )
        .map_err(io::Error::from)
    }
}
pub fn logs(root: &Path, now: i64, attributes: &mut Attributes) -> Result<Vec<String>, Error> {
    let mut logs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let bytes = attributes.get(&entry.path())?.unwrap_or_default();
        let time = if cfg!(target_endian = "little") {
            bytes.iter().rev().fold(0_u64, |acc, &value| {
                acc.saturating_mul(256).saturating_add(u64::from(value))
            })
        } else {
            bytes.iter().fold(0_u64, |acc, &value| {
                acc.saturating_mul(256).saturating_add(u64::from(value))
            })
        };
        if time == 0 || i128::from(now) - i128::from(time) > 3600 {
            logs.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    logs.sort();
    logs.pop();
    Ok(logs)
}
pub fn stat_once(shared: &Shared) -> Result<(), Error> {
    let first = fs::read_dir(&shared.config.stats_root)?
        .find_map(|entry| match entry {
            Ok(entry) if !entry.file_name().as_encoded_bytes().starts_with(b"tmp") => {
                Some(Ok(entry))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .transpose()?;
    if let Some(entry) = first {
        let text = read_text(&entry.path())?;
        shared.low_priority.put(json!({"method":"storeStats","params":{"stats":text},"jsonrpc":"2.0","id":entry.file_name().to_string_lossy()}).to_string())?;
        fs::remove_file(entry.path())?;
    }
    Ok(())
}
pub fn stats(shared: Arc<Shared>, stop: Stop) {
    let mut logger = shared.factory.logger();
    let mut scanned = 0_i64;
    while !stop.requested() {
        let now = match state::mono_ns() {
            Ok(now) => now,
            Err(error) => {
                logging::failure(&mut logger, "athena.stat_handler.exception", &error);
                return;
            }
        };
        if now.saturating_sub(scanned) > 10_000_000_000 {
            match stat_once(&shared) {
                Ok(()) => scanned = now,
                Err(error) => {
                    logging::failure(&mut logger, "athena.stat_handler.exception", &error)
                }
            }
        }
        stop.wait(Duration::from_millis(100));
    }
}
pub fn log_worker(shared: Arc<Shared>, stop: Stop) {
    if shared.config.pc {
        return;
    }
    let mut logger = shared.factory.logger();
    let mut attributes = match shared.attributes.lock() {
        Ok(attributes) => attributes,
        Err(error) => {
            logging::failure(&mut logger, "athena.log_handler.exception", &error);
            return;
        }
    };
    let mut scanned = 0_i64;
    let mut files = Vec::new();
    while !stop.requested() {
        let result = log_cycle(
            &shared,
            &stop,
            &mut logger,
            &mut attributes,
            &mut scanned,
            &mut files,
        );
        if let Err(error) = result {
            logging::failure(&mut logger, "athena.log_handler.exception", &error);
        }
    }
}
fn log_cycle(
    shared: &Shared,
    stop: &Stop,
    logger: &mut Logger,
    attributes: &mut Attributes,
    scanned: &mut i64,
    files: &mut Vec<String>,
) -> Result<(), Error> {
    let now = state::mono_ns()?;
    if now.saturating_sub(*scanned) > 10_000_000_000 {
        *files = logs(
            &shared.config.swaglog_root,
            state::now_ms()? / 1000,
            attributes,
        )?;
        *scanned = now;
    }
    let mut current = None;
    if let Some(file) = files.pop() {
        let path = shared.config.swaglog_root.join(&file);
        let time = u32::try_from(state::now_ms()? / 1000)
            .map_err(|_| Error::Contract("log xattr timestamp"))?;
        match attributes.put(&path, time).and_then(|()| read_text(&path)) {
            Ok(text) => {
                shared.low_priority.put(json!({"method":"forwardLogs","params":{"logs":text},"jsonrpc":"2.0","id":file}).to_string())?;
                current = Some(file);
            }
            Err(error) => logging::failure(logger, "athena.log_handler.rotated", &error),
        }
    }
    for _ in 0..100 {
        if stop.requested() {
            break;
        }
        if let Some(response) = shared.log_responses.get(Duration::from_secs(1))? {
            let response: Value = serde_json::from_str(&response)?;
            let file = response.get("id").and_then(Value::as_str);
            if let Some(file) = file.filter(|_| {
                response
                    .get("result")
                    .and_then(|value| value.get("success"))
                    .is_some_and(truthy)
            }) {
                if let Err(error) =
                    attributes.put(&shared.config.swaglog_root.join(file), 2147483647)
                {
                    logging::failure(logger, "athena.log_handler.rotated", &error);
                }
            }
            if current.as_deref() == file {
                break;
            }
        } else if current.is_none() {
            break;
        }
    }
    Ok(())
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
fn read_text(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
