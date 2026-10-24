//! Independent original getter/file policy, deliberately separate from Rust Params.
use super::ops as o;
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use std::{
    ffi::OsStr,
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
pub struct Params {
    pub trace: Vec<Value>,
    pub trace_path: PathBuf,
    namespace: PathBuf,
    root: PathBuf,
    exception: JsonValue,
    pub gps: JsonValue,
}
impl Params {
    pub fn new(output: &Path, index: usize, case: &JsonValue) -> Result<Self> {
        let root = output.join(format!("params-{index}"));
        // Cython encodes the constructor path as strict UTF-8.
        if root.to_str().is_none() {
            return Err(o::error("UnicodeEncodeError", "Params path is not UTF-8"));
        }
        fs::create_dir_all(&root).map_err(|e| o::error("RuntimeError", e.to_string()))?;
        let mut prefix = root.as_os_str().to_owned();
        prefix.push("/");
        prefix.push(std::env::var_os("OPENPILOT_PREFIX").unwrap_or_else(|| "d".into()));
        let namespace = PathBuf::from(prefix);
        if !namespace.try_exists().map_err(o::classify_io)? {
            let temp = tempfile::Builder::new()
                .prefix(".tmp_")
                .tempdir_in(&root)
                .map_err(|e| o::error("RuntimeError", e.to_string()))?
                .keep();
            let mut link = temp.as_os_str().to_owned();
            link.push(".link");
            let link = PathBuf::from(link);
            symlink(&temp, &link).map_err(|e| o::error("RuntimeError", e.to_string()))?;
            fs::rename(&link, &namespace).map_err(|e| o::error("RuntimeError", e.to_string()))?;
        }
        let values = o::default(case, "values", o::object(&[])?)?;
        let JsonView::Object(fields) = values.view() else {
            return Err(o::error("AttributeError", "object has no attribute items"));
        };
        for (key, value) in fields {
            let key = JsonValue::codepoints(key.to_vec()).ok_or("invalid key")?;
            let path = Self::key_path(&namespace, Self::key_argument(&key)?.as_os_str());
            fs::write(path, o::bytes(&value)?).map_err(o::classify_io)?;
        }
        for key in o::sequence(&o::default(case, "directories", o::array(&[])?)?)? {
            let path = Self::key_path(&namespace, Self::key_argument(&key)?.as_os_str());
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(o::classify_io(e)),
            };
            fs::create_dir(path).map_err(o::classify_io)?;
        }
        Ok(Self {
            trace: vec![],
            trace_path: output.join(format!("trace-{index}.json")),
            namespace,
            root,
            exception: o::optional(case, "exception_key")?,
            gps: o::default(case, "gps_paths", o::array(&[])?)?,
        })
    }
    fn key_argument(value: &JsonValue) -> Result<std::ffi::OsString> {
        let key = o::os_text(value)?;
        if key.as_encoded_bytes().contains(&0) {
            return Err(o::error("ValueError", "embedded null byte"));
        }
        Ok(key)
    }
    fn key_path(base: &Path, key: &OsStr) -> PathBuf {
        let mut path = base.as_os_str().to_owned();
        if !key.is_empty() {
            path.push("/");
            path.push(key)
        }
        PathBuf::from(path)
    }
    fn record(&mut self, value: Value) -> Result<()> {
        self.trace.push(value);
        fs::write(
            &self.trace_path,
            format!(
                "{}\n",
                JsonValue::parse(&serde_json::to_string(&self.trace)?)?.to_json()?
            ),
        )
        .map_err(o::classify_io)?;
        Ok(())
    }
    fn access(&mut self, kind: &str, key: &str) -> Result<()> {
        self.record(json!([kind, key]))?;
        if self.exception.text_eq(key) {
            return Err(o::error("parameter", "fixture parameter exception"));
        }
        Ok(())
    }
    fn raw(&self, key: &str) -> Vec<u8> {
        fs::read(Self::key_path(&self.namespace, OsStr::new(key))).unwrap_or_default()
    }
    pub fn persisted(&self) -> bool {
        self.raw("UbloxAvailable") == b"1"
    }
    pub fn boolean(&mut self, key: &str) -> Result<bool> {
        self.access("bool", key)?;
        Ok(self.raw(key) == b"1")
    }
    pub fn default_bool(&mut self, key: &str) -> Result<bool> {
        self.access("default", key)?;
        let raw = self.raw(key);
        Ok(if raw.is_empty() {
            key == "UseWideCamera"
        } else {
            raw == b"1"
        })
    }
    pub fn integer(&mut self, key: &str) -> Result<i32> {
        self.access("int", key)?;
        integer(&self.raw(key))
    }
    pub fn put_bool(&mut self, key: &str, value: bool) -> Result<()> {
        self.record(json!(["put", key, value]))?;
        // The original Cython ignores putBool's I/O return code. Atomic replacement
        // preserves existing-directory/symlink behavior instead of writing through it.
        let write = || -> std::io::Result<()> {
            let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
            file.write_all(if value { b"1" } else { b"0" })?;
            file.as_file().sync_all()?;
            file.persist(Self::key_path(&self.namespace, OsStr::new(key)))
                .map_err(|e| e.error)?;
            fs::File::open(&self.namespace)?.sync_all()?;
            Ok(())
        };
        let _ = write();
        Ok(())
    }
    pub fn exists(&mut self, path: &str) -> Result<bool> {
        self.record(json!(["exists", path]))?;
        Ok(match self.gps.view() {
            JsonView::Text(points) => {
                let needle = path.chars().map(u32::from).collect::<Vec<_>>();
                points.windows(needle.len()).any(|v| v == needle)
            }
            _ => o::sequence(&self.gps)?.iter().any(|v| v.text_eq(path)),
        })
    }
}
fn integer(raw: &[u8]) -> Result<i32> {
    if raw.is_empty() {
        return Ok(0);
    }
    let mut i = 0;
    while i < raw.len() && matches!(raw[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let negative = raw.get(i) == Some(&b'-');
    if matches!(raw.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let start = i;
    let limit = if negative { 2147483648u64 } else { 2147483647 };
    let mut value = 0u64;
    while let Some(c @ b'0'..=b'9') = raw.get(i) {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(c - b'0')))
            .filter(|v| *v <= limit)
            .ok_or_else(|| o::error("fatal_integer", "integer range"))?;
        i += 1;
    }
    if i == start {
        return Err(o::error("fatal_integer", "integer syntax"));
    }
    Ok(if negative {
        (-(value as i64)) as i32
    } else {
        value as i32
    })
}
