//! Stateful policy from the unchanged source Worker and openpilot.system.sentry.
use super::{params::Params, value as v, Error, Result};
use openpilot_log_format::{JsonValue as J, JsonView};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
mod files;
mod loops;
mod reader;

pub(super) struct Worker {
    cwd: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    version: crate::version_source::Session,
    base: J,
    params_root: J,
    pc: J,
    device: J,
    failure: J,
    root: J,
    stamp: J,
    apport: J,
    enabled: J,
    active: Option<loops::Loop>,
    pub(super) control: Option<(Instant, Arc<AtomicBool>)>,
    stderr: Option<fs::File>,
    pub(super) calls: Vec<J>,
    pub(super) logs: Vec<J>,
}
fn scalar(source: &str) -> J {
    J::parse(source).expect("source scalar")
}
fn now() -> J {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64();
    scalar(&format!("{seconds:?}"))
}
// pathlib.Path construction is text-only for JSON inputs and does not perform
// filesystem encoding/stat. Normalize separators/dot components before storing
// state, retaining lone surrogates until the eventual OS/Params boundary.
fn path(value: &J) -> Result<J> {
    let points = v::points(value, "TypeError")?;
    let leading = points.iter().take_while(|&&point| point == 47).count();
    let mut out = if leading == 2 {
        vec![47, 47]
    } else if leading > 0 {
        vec![47]
    } else {
        Vec::new()
    };
    for part in points
        .split(|point| *point == 47)
        .filter(|part| !part.is_empty() && *part != [46])
    {
        if !out.is_empty() && out.last() != Some(&47) {
            out.push(47);
        }
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(46);
    }
    Ok(v::text(out))
}
fn os_path(value: &J) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = Vec::new();
    for point in v::points(value, "TypeError")? {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let character = char::from_u32(point)
                .ok_or_else(|| Error::new("UnicodeEncodeError", "surrogates not allowed"))?;
            bytes.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Ok(OsString::from_vec(bytes).into())
}
fn text_file(path: &Path) -> Result<J> {
    let bytes = fs::read(path).map_err(Error::io)?;
    let text = String::from_utf8(bytes)
        .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))?;
    Ok(J::text(&text.replace("\r\n", "\n").replace('\r', "\n")))
}
fn property(metadata: &J, key: &str) -> Result<J> {
    let capture = v::field(&v::field(metadata, "properties")?, key)?;
    if let Some(error) = capture.get("error") {
        return Err(Error::new(
            error.to_utf8().unwrap_or_else(|| "Exception".into()),
            "source metadata property",
        ));
    }
    v::field(&capture, "value")
}
impl Worker {
    pub(super) fn new(cwd: &Path, environment: &BTreeMap<OsString, OsString>) -> Result<Self> {
        let mut environment = environment.clone();
        // The original Worker changes this in its private source process.
        // Preserve what later Git/shell children inherit without mutating the
        // host process shared by native source sessions and controller peers.
        environment.insert("OPENPILOT_PREFIX".into(), "fixture".into());
        let version = crate::version_source::Session::new(cwd, &environment)
            .map_err(|error| Error::new(error.kind(), error.to_string()))?;
        Ok(Self {
            cwd: cwd.to_path_buf(),
            environment,
            version,
            base: scalar("null"),
            params_root: scalar("null"),
            pc: scalar("false"),
            device: J::text("tici"),
            failure: scalar("null"),
            root: scalar("null"),
            stamp: J::text("2020-01-02--03-04-05"),
            apport: J::text("/var/crash/"),
            enabled: scalar("null"),
            active: None,
            control: None,
            stderr: None,
            calls: Vec::new(),
            logs: Vec::new(),
        })
    }
    pub(super) fn clear_records(&mut self) {
        self.calls.clear();
        self.logs.clear();
    }
    pub(super) fn check_control(&self) -> Result<()> {
        if let Some((deadline, cancelled)) = &self.control {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Error::new("InterruptedError", "source request interrupted"));
            }
            if Instant::now() >= *deadline {
                return Err(Error::new("TimeoutError", "source request timed out"));
            }
        }
        Ok(())
    }
    pub(super) fn set_stderr(&mut self, stderr: fs::File) -> Result<()> {
        self.version
            .set_stderr(stderr.try_clone().map_err(Error::io)?);
        self.stderr = Some(stderr);
        Ok(())
    }
    fn log(&mut self, level: u32, message: J, exception: bool) {
        self.logs.push(v::object(&[
            ("levelnum", scalar(&level.to_string())),
            ("msg", message),
            (
                "exception",
                scalar(if exception { "true" } else { "false" }),
            ),
            ("created", now()),
        ]));
    }
    fn params(&self) -> Result<Params> {
        Params::new(&self.cwd, &self.params_root)
    }
    fn sdk(&mut self, operation: &str, fields: J) -> Result<()> {
        // Worker.call reads this live path before recording/failing the call.
        // A pre-configure None fails Path division rather than using "None".
        let root = os_path(&self.params_root)?;
        let current = self.cwd.join(root).join("fixture/CarrotException");
        let exception = if current.is_file() {
            text_file(&current)?
        } else {
            scalar("null")
        };
        self.calls.push(v::object(&[
            ("op", J::text(operation)),
            ("fields", fields),
            ("carrot_exception", exception),
            ("created", now()),
        ]));
        if self.failure.text_eq(operation) {
            return Err(Error::new("RuntimeError", "fixture failure"));
        }
        Ok(())
    }
    fn sdk_value(&mut self, operation: &str, key: J, value: J) -> Result<()> {
        self.sdk(
            operation,
            v::object(&[
                ("key", key),
                (
                    "value_json",
                    J::text(&value.to_json().expect("Python JSON value")),
                ),
            ]),
        )
    }
    fn version(&mut self, operation: &str, path: J) -> Result<J> {
        let raw = v::object(&[("op", J::text(operation)), ("path", path)])
            .to_json()
            .expect("source request");
        let response = match &self.control {
            Some((deadline, cancelled)) => {
                self.version
                    .request_controlled(raw.as_bytes(), *deadline, Arc::clone(cancelled))
            }
            None => self.version.request(raw.as_bytes()),
        }
        .map_err(|error| Error::new(error.kind(), error.to_string()))?;
        if let Some(logs) = response.get("logs") {
            if let JsonView::Array(logs) = logs.view() {
                for log in logs {
                    let level = log
                        .get("levelnum")
                        .and_then(|value| value.to_json().ok())
                        .and_then(|text| text.parse().ok())
                        .unwrap_or(40);
                    self.log(level, v::field(&log, "msg")?, log.get("exc_info").is_some());
                }
            }
        }
        if let Some(error) = response.get("error") {
            return Err(Error::new(
                error.to_utf8().unwrap_or_else(|| "Exception".into()),
                "source Version operation",
            ));
        }
        v::field(&response, "value")
    }
    fn dongle(&mut self) -> Result<J> {
        let bytes = self.params()?.get("DongleId");
        if bytes.is_empty() {
            return Ok(scalar("null"));
        }
        match String::from_utf8(bytes) {
            Ok(value) => Ok(J::text(&value)),
            Err(error) => {
                // Cython catches UnicodeDecodeError as ValueError and logs the
                // actual raw bytes before falling back to its None default.
                let bytes = error.into_bytes();
                let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
                    '"'
                } else {
                    '\''
                };
                let mut repr = format!("b{quote}");
                for byte in bytes {
                    match byte {
                        b'\\' => repr.push_str("\\\\"),
                        b'\t' => repr.push_str("\\t"),
                        b'\n' => repr.push_str("\\n"),
                        b'\r' => repr.push_str("\\r"),
                        byte if byte == quote as u8 => {
                            repr.push('\\');
                            repr.push(quote);
                        }
                        32..=126 => repr.push(char::from(byte)),
                        _ => repr.push_str(&format!("\\x{byte:02x}")),
                    }
                }
                repr.push(quote);
                self.log(30,J::text(&format!("Failed to cast param DongleId with value={repr} from type t=<ParamKeyType.STRING: 0>")),false);
                Ok(scalar("null"))
            }
        }
    }
    fn init(&mut self, project: &J) -> Result<J> {
        let project = v::points(project, "AttributeError")?
            .into_iter()
            .flat_map(|point| {
                char::from_u32(point)
                    .map(|ch| ch.to_uppercase().map(u32::from).collect::<Vec<_>>())
                    .unwrap_or_else(|| vec![point])
            })
            .collect::<Vec<_>>();
        let project = v::text(project);
        let native = if project.text_eq("SELFDRIVE_NATIVE") {
            true
        } else if project.text_eq("SELFDRIVE") {
            false
        } else {
            return Err(Error::new("KeyError", "unknown SentryProject"));
        };
        let base = v::text(v::string(&self.base));
        let metadata = self.version("metadata", base.clone())?;
        let openpilot = v::field(&metadata, "openpilot")?;
        let comma = property(&metadata, "comma_remote")?;
        let origin = v::field(&openpilot, "git_origin")?;
        if !v::truth(&comma)
            || !origin
                .to_utf8()
                .is_some_and(|origin| origin.contains("commaai"))
        {
            return Ok(scalar("false"));
        }
        let dongle = self.dongle()?;
        if matches!(dongle.view(), JsonView::Null)
            || dongle.text_eq("UnregisteredDevice")
            || v::truth(&self.pc)
        {
            return Ok(scalar("false"));
        }
        let release_environment = if v::truth(&property(&metadata, "tested_channel")?) {
            "release"
        } else {
            "master"
        };
        let dongle = self.dongle()?;
        let release = self.version("version", base)?;
        self.sdk(
            "init",
            v::object(&[
                (
                    "project",
                    J::text(if native {
                        "selfdrive_native"
                    } else {
                        "selfdrive"
                    }),
                ),
                ("release", release),
                ("environment", J::text(release_environment)),
                ("default_integrations", scalar("false")),
                (
                    "threading_requested",
                    scalar(if native { "false" } else { "true" }),
                ),
                ("traces_sample_rate", scalar("1.0")),
                ("max_value_length", scalar("8192")),
            ]),
        )?;
        self.sdk("set_user", v::object(&[("id", dongle)]))?;
        for (key, value) in [
            ("dirty", v::field(&openpilot, "is_dirty")?),
            ("origin", origin),
            ("branch", v::field(&metadata, "channel")?),
            ("commit", v::field(&openpilot, "git_commit")?),
            ("device", self.device.clone()),
        ] {
            self.sdk_value("set_tag", J::text(key), value)?;
        }
        Ok(scalar("true"))
    }
    pub(super) fn call(&mut self, request: &J, op: &J) -> Result<J> {
        if op.text_eq("configure") {
            // Worker.configure stops the old queue-controlled source thread
            // even when evaluating a later field fails.
            self.active = None;
            let base = path(&v::field(request, "base")?)?;
            let params = path(&v::field(request, "params")?)?;
            self.base = base;
            self.params_root = params;
            let pc = v::field(request, "pc")?;
            let device = v::field(request, "device")?;
            self.pc = pc;
            self.device = device;
            self.failure = scalar("null");
            return Ok(scalar("true"));
        }
        if op.text_eq("init") {
            let enabled = self.init(&v::field(request, "project")?)?;
            self.enabled = enabled.clone();
            return Ok(enabled);
        }
        if op.text_eq("fail") {
            self.failure = v::field(request, "operation")?;
            return Ok(scalar("null"));
        }
        if op.text_eq("tag") {
            let key = v::field(request, "key")?;
            let value = v::parse(&v::points(&v::field(request, "value")?, "TypeError")?)?;
            self.sdk_value("set_tag", key, value)?;
            return Ok(scalar("null"));
        }
        if op.text_eq("capture") {
            let exception = v::field(request, "exception")?;
            let log_exception = request
                .get("log_exception")
                .unwrap_or_else(|| scalar("true"));
            self.log(40, J::text("crash"), v::truth(&log_exception));
            let params = self.params()?;
            if params.get("CarrotExceptionSent") != b"1" {
                params.put("CarrotException", b"exception");
            }
            if self
                .sdk("capture_exception", exception)
                .and_then(|()| self.sdk("flush", scalar("null")))
                .is_err()
            {
                self.log(40, J::text("sentry exception"), true);
            }
            return Ok(scalar("null"));
        }
        if op.text_eq("tombstone") {
            let filename = v::field(request, "filename")?;
            let message = v::field(request, "message")?;
            let contents = v::field(request, "contents")?;
            self.log(40, v::object(&[("tombstone", message.clone())]), false);
            self.sdk_value("set_extra", J::text("tombstone_fn"), filename)?;
            self.sdk_value("set_extra", J::text("tombstone"), contents)?;
            self.sdk("capture_message", v::object(&[("message", message)]))?;
            self.sdk("flush", scalar("null"))?;
            return Ok(scalar("null"));
        }
        if ["scan", "clear", "retrace", "report"]
            .iter()
            .any(|name| op.text_eq(name))
        {
            return self.files(request, op);
        }
        if ["start", "cycle"].iter().any(|name| op.text_eq(name)) {
            return self.step(request, op);
        }
        Err(Error::new("AssertionError", "unknown source operation"))
    }
}
