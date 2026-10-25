//! Independent original Sentry/tombstoned source Worker.
//! Source policy and filesystem effects are independent of the runtime candidate.
//! Runtime/controller comparisons are a separate validation boundary.
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, BufRead, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
#[cfg(all(test, target_os = "linux"))]
mod controlled_tests;
mod params;
mod pure;
mod state;
mod value;
#[cfg(test)]
mod wire_tests;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub struct Error {
    kind: String,
    message: String,
}
impl Error {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    fn io(error: io::Error) -> Self {
        let kind = match error.raw_os_error() {
            Some(2) => "FileNotFoundError",
            Some(13 | 1) => "PermissionError",
            Some(20) => "NotADirectoryError",
            Some(21) => "IsADirectoryError",
            _ => "OSError",
        };
        Self::new(kind, error.to_string())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
const SOURCES: &[(&str, &str)] = &[
    (
        "openpilot/system/tombstoned.py",
        "a7a5f32535fe7c3a74593b3252366db5ac1c8b43f64837293e1c192f517527a5",
    ),
    (
        "openpilot/system/sentry.py",
        "fec900bf600e7a714460d8494472d6cc648759abf36ff72b79158bff61135fd8",
    ),
    (
        "openpilot/system/athena/registration.py",
        "461b5976d0e0ad7849c8096e5372dc3514da24fcd7c1924c97027cdcb62a0924",
    ),
    (
        "openpilot/system/version.py",
        "69b91ce6fc8fe3b164c4f6c23e9514c89a4b6759043cfb607b733c7e006144d7",
    ),
    (
        "openpilot/common/git.py",
        "68a68daab8137ba307753b6f1dbc18a4a8c736ed118383800ad6bfd8690140d9",
    ),
    (
        "openpilot/common/utils.py",
        "b1cc68b63cff1386ba397c0cc527dbb38fb50b154043ffcb066ed7acce65ec83",
    ),
    (
        "rust/tools/check_version_reference.py",
        "d0ad5fbec23e1ba4a2507ae6dbc68856141d03eda07dadebab3280d5731525d5",
    ),
    (
        "rust/tools/tombstoned_reference.py",
        "7b647dba322d93e5d2bd9c5692356b10d3f85f2a2e0f9da8b982e05206c8e781",
    ),
    (
        "rust/tools/check_tombstoned_reference.py",
        "6271ece66356f377357ffc2b37bf3d98fcdb4c8231c8c07f556ed0294c261cd4",
    ),
];
fn retired_helper(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_version_reference.py"
            | "rust/tools/tombstoned_reference.py"
            | "rust/tools/check_tombstoned_reference.py"
    )
}
/// Runtime policy stays mandatory; only actual absence of the three retired
/// helpers is accepted. Present changed, dangling or unreadable paths fail.
pub fn sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for &(name, expected) in SOURCES {
        if retired_helper(name) {
            match fs::symlink_metadata(root.join(name)) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(Error::io(error)),
            }
        }
        let bytes = fs::read(root.join(name)).map_err(Error::io)?;
        let hash = format!("{:x}", Sha256::digest(bytes));
        if hash != expected {
            return Err(Error::new(
                "ValueError",
                format!("changed tombstone source {name}"),
            ));
        }
        hashes.insert(name.into(), hash);
    }
    Ok(hashes)
}
/// Historical identities are disjoint from live hashes and require the same
/// complete admission checks, including present retired helpers.
pub fn historical_sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let live = sources(root)?;
    Ok(SOURCES
        .iter()
        .filter(|(name, _)| retired_helper(name) && !live.contains_key(*name))
        .map(|(name, hash)| ((*name).into(), (*hash).into()))
        .collect())
}
pub struct Session {
    worker: state::Worker,
}
impl Session {
    pub fn new(cwd: &Path, environment: &BTreeMap<OsString, OsString>) -> Result<Self> {
        Ok(Self {
            worker: state::Worker::new(cwd, environment)?,
        })
    }
    pub fn request(&mut self, raw_json: &[u8]) -> Result<JsonValue> {
        self.worker.check_control()?;
        self.worker.clear_records();
        let result = std::str::from_utf8(raw_json)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))
            .and_then(|raw| value::parse(&raw.chars().map(u32::from).collect::<Vec<_>>()))
            .and_then(|request| self.call(&request));
        self.response(result)
    }
    /// stdin is already decoded with Python's surrogateescape convention;
    /// the explicit byte request API above keeps its strict UTF-8 boundary.
    fn request_codepoints(&mut self, points: &[u32]) -> Result<JsonValue> {
        self.worker.check_control()?;
        self.worker.clear_records();
        let result = value::parse(points).and_then(|request| self.call(&request));
        self.response(result)
    }
    fn response(&self, result: Result<JsonValue>) -> Result<JsonValue> {
        self.worker.check_control()?;
        Ok(value::object(&[
            ("result", value::capture(result)),
            ("calls", value::array(&self.worker.calls)),
            ("logs", value::array(&self.worker.logs)),
        ]))
    }
    /// One controller deadline covers the entire source call, including Git
    /// and shell helpers. Abort is a transport failure, never source JSON.
    pub fn request_controlled(
        &mut self,
        raw_json: &[u8],
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JsonValue> {
        self.worker.control = Some((deadline, cancelled));
        let result = self.request(raw_json);
        let control = self.worker.check_control();
        self.worker.control = None;
        control.and(result)
    }
    /// Route all source Git/retrace stderr into the caller's owned evidence.
    pub fn set_stderr(&mut self, stderr: fs::File) -> Result<()> {
        self.worker.set_stderr(stderr)
    }
    fn call(&mut self, request: &JsonValue) -> Result<JsonValue> {
        let op = value::field(request, "op")?;
        if op.text_eq("safe") {
            return pure::safe(&value::field(request, "text")?);
        }
        if op.text_eq("filename") {
            return pure::filename(request);
        }
        self.worker.call(request, &op)
    }
}
/// Original tombstoned main catches ordinary exceptions inside each row and
/// continues, including unknown-operation AssertionError. EOF itself is no row.
pub fn source(
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<()> {
    let mut session = Session::new(cwd, environment)?;
    let mut line = Vec::new();
    loop {
        line.clear();
        // The actual original subprocess iterates sys.stdin on LF boundaries.
        // A CR is JSON whitespace, not a second request delimiter on this pipe.
        if input.read_until(b'\n', &mut line).map_err(Error::io)? == 0 {
            return Ok(());
        }
        let points = crate::python_fs::codepoints(OsStr::from_bytes(&line));
        let value = session.request_codepoints(&points)?;
        writeln!(
            output,
            "{}",
            value
                .to_json()
                .map_err(|error| Error::new("ValueError", error.to_string()))?
        )
        .map_err(Error::io)?;
        output.flush().map_err(Error::io)?;
    }
}
