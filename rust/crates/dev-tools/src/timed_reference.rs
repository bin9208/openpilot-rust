//! Source-derived timed oracle. No candidate timed crate, compiled policy, or
//! expected-result corpus is used. Inputs come from callers; effects are real.
use crate::{
    control_tools_policy_check::json as python_json,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    timed_fixtures::{self, Fixture},
};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Utc};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt, fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

// Only the explicitly retired tool closure may be absent. Every present file
// remains pinned, and shared runtime/logging/schema inputs remain mandatory.
pub(crate) fn retired_source(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_timed_reference.py"
            | "rust/tools/check_timed_dates.py"
            | "rust/tools/check_timed_http.py"
            | "rust/tools/check_timed_logging.py"
            | "rust/tools/check_timed_daemon.py"
            | "rust/tools/check_timed_date_daemon.py"
            | "rust/tools/check_timed_lifecycle.py"
            | "rust/tools/check_timed_shutdown.py"
            | "rust/tools/timed_fixtures.py"
            | "rust/tools/timed_reference.py"
            | "rust/tools/timed_source_live.py"
            | "rust/tools/timed_shutdown_phase.py"
            | "rust/tools/tests/test_timed_shutdown_phase.py"
    )
}

pub fn sources(root: &Path) -> Result<()> {
    let bytes = include_bytes!("../tests/fixtures/timed-tools/manifest.json");
    if format!("{:x}", Sha256::digest(bytes))
        != "0e3589f9570980f005590fb8f4588feffed0c882b001717266b15176c7715db5"
    {
        return Err("timed original provenance manifest changed".into());
    }
    let manifest: Value = serde_json::from_slice(bytes)?;
    let pins = manifest["sources"]
        .as_object()
        .ok_or("missing timed source pins")?;
    if pins.len() != 20
        || manifest["implementation"] != "CPython"
        || manifest["python"] != "3.12.15"
    {
        return Err("timed original provenance changed".into());
    }
    for (name, sha) in pins {
        let original = match fs::read(root.join(name)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound && retired_source(name) => {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        if format!("{:x}", Sha256::digest(original))
            != sha.as_str().ok_or("invalid source digest")?
        {
            return Err(format!("timed source changed: {name}").into());
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Failure {
    pub kind: String,
    pub message: String,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Failure {}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self {
            kind: match e.kind() {
                std::io::ErrorKind::NotFound => "FileNotFoundError",
                std::io::ErrorKind::PermissionDenied => "PermissionError",
                _ => "OSError",
            }
            .into(),
            message: e.to_string(),
        }
    }
}
type PolicyResult<T> = std::result::Result<T, Failure>;
fn fail(kind: &str, message: impl Into<String>) -> Failure {
    Failure {
        kind: kind.into(),
        message: message.into(),
    }
}
fn field(value: &JsonValue, name: &str) -> PolicyResult<JsonValue> {
    value
        .get(name)
        .ok_or_else(|| fail("KeyError", format!("'{name}'")))
}
fn text(value: &JsonValue) -> PolicyResult<String> {
    value
        .to_utf8()
        .ok_or_else(|| fail("TypeError", "expected string"))
}
fn number(value: &JsonValue) -> PolicyResult<f64> {
    match value.view() {
        JsonView::Bool(v) => Ok(if v { 1. } else { 0. }),
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(v) => {
            let value: f64 = v
                .parse()
                .map_err(|_| fail("OverflowError", "int too large to convert to float"))?;
            if !value.is_finite() {
                return Err(fail("OverflowError", "int too large to convert to float"));
            }
            Ok(value)
        }
        _ => Err(fail("TypeError", "numeric operation requires a number")),
    }
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v != "0",
        JsonView::Float(v) => v != 0.,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn fixed(year: i32, month: u32, day: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}

// Reproduce CPython's seconds/microsecond split and naive-local fold probe,
// including the year checked by each localtime construction. TZ is child-only.
fn local_from_timestamp(epoch: f64) -> PolicyResult<NaiveDateTime> {
    if epoch.is_nan() {
        return Err(fail("ValueError", "Invalid value NaN (not a number)"));
    }
    if !epoch.is_finite() {
        return Err(fail(
            "OverflowError",
            "timestamp out of range for platform time_t",
        ));
    }
    let seconds: i64 = format!("{:.0}", epoch.floor()).parse().map_err(|_| {
        fail(
            "OverflowError",
            "timestamp out of range for platform time_t",
        )
    })?;
    let micros = ((epoch - epoch.floor()) * 1e6).round_ties_even() as i64;
    let utc = DateTime::<Utc>::from_timestamp(seconds, 0)
        .and_then(|v| v.checked_add_signed(chrono::Duration::microseconds(micros)))
        .ok_or_else(|| fail("ValueError", "timestamp out of range"))?;
    let local = |utc: DateTime<Utc>| -> PolicyResult<NaiveDateTime> {
        let local = utc.with_timezone(&Local).naive_local();
        if !(1..=9999).contains(&local.year()) {
            return Err(fail(
                "ValueError",
                format!("year {} is out of range", local.year()),
            ));
        }
        Ok(local)
    };
    let result = local(utc)?;
    local(
        utc.checked_sub_signed(chrono::Duration::days(1))
            .ok_or_else(|| {
                fail(
                    "OverflowError",
                    "timestamp out of range for platform time_t",
                )
            })?,
    )?;
    Ok(result)
}
fn priority(source: &str) -> u8 {
    match source {
        "gps" => 1,
        "wifi" => 2,
        "app" => 3,
        _ => 0,
    }
}
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut result = format!("b{}", char::from(quote));
    for &b in bytes {
        match b {
            b if b == quote || b == b'\\' => {
                result.push('\\');
                result.push(char::from(b));
            }
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            32..=126 => result.push(char::from(b)),
            _ => result.push_str(&format!("\\x{b:02x}")),
        }
    }
    result.push(char::from(quote));
    result
}

pub struct Source {
    pub config: JsonValue,
    pub params: PathBuf,
    pub records: Vec<Value>,
    pub sleeps: Vec<u64>,
    pub publications: Vec<Value>,
    pub monotonic: f64,
    pub last_attempt: f64,
    socket: Option<zmq::Socket>,
    host: String,
    closed: bool,
    _context: Option<zmq::Context>,
}
impl Source {
    pub fn new(config: JsonValue, params: PathBuf) -> PolicyResult<Self> {
        let monotonic = config
            .get("monotonic")
            .filter(|v| !matches!(v.view(), JsonView::Null))
            .map(|v| number(&v))
            .transpose()?
            .unwrap_or(0.);
        Ok(Self {
            config,
            params,
            records: vec![],
            sleeps: vec![],
            publications: vec![],
            monotonic,
            last_attempt: 0.,
            socket: None,
            host: String::new(),
            closed: false,
            _context: None,
        })
    }
    pub fn connect_logger(&mut self, endpoint: &str) -> PolicyResult<()> {
        // Original SwagFormatter freezes socket.gethostname(), not HOSTNAME.
        self.host = rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned();
        let context = zmq::Context::new();
        let socket = context
            .socket(zmq::PUSH)
            .map_err(|e| fail("ZMQError", e.to_string()))?;
        socket
            .set_linger(10)
            .map_err(|e| fail("ZMQError", e.to_string()))?;
        socket
            .connect(endpoint)
            .map_err(|e| fail("ZMQError", e.to_string()))?;
        self.socket = Some(socket);
        self._context = Some(context);
        Ok(())
    }
    fn emit(&mut self, level: u8, msg: String, exception: bool, site: &str) -> PolicyResult<()> {
        self.records
            .push(json!({"level":level,"msg":msg,"exception":exception}));
        if self.closed {
            return Err(fail("ZMQError", "Socket operation on non-socket"));
        }
        if let Some(socket) = &self.socket {
            let mut record = json!({"msg":msg,"ctx":{},"level":match level {10=>"DEBUG",20=>"INFO",30=>"WARNING",_=>"ERROR"},
                "levelnum":level,"name":"swaglog","filename":"timed_reference.rs","lineno":1,"pathname":file!(),"module":"timed_reference", "funcName":site,
                "host":self.host,"process":std::process::id(),"thread":std::process::id(),"threadName":"MainThread",
                "created":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_|fail("OSError","wall clock before epoch"))?.as_secs_f64()});
            if exception {
                record["exc_info"] = Value::String("CalledProcessError".into());
            }
            let mut raw = vec![level];
            raw.extend(serde_json::to_vec(&record).map_err(|e| fail("ValueError", e.to_string()))?);
            match socket.send(raw, zmq::DONTWAIT) {
                Ok(()) | Err(zmq::Error::EAGAIN) => {}
                Err(error) => return Err(fail("ZMQError", error.to_string())),
            }
        }
        Ok(())
    }
    fn get_param(&mut self, key: &str) -> PolicyResult<Option<String>> {
        let data = match fs::read(self.params.join(key)) {
            Ok(v) if !v.is_empty() => v,
            _ => return Ok(None),
        };
        match String::from_utf8(data.clone()) {
            Ok(v) => Ok(Some(v)),
            Err(_) => {
                self.emit(30,format!("Failed to cast param {key} with value={} from type t=<ParamKeyType.STRING: 0>",bytes_repr(&data)),false,"get")?;
                Ok(None)
            }
        }
    }
    pub fn current_source(&mut self) -> PolicyResult<String> {
        Ok(self.get_param("TimezoneSource")?.unwrap_or_default())
    }
    fn path(&self, key: &str) -> PolicyResult<PathBuf> {
        Ok(text(&field(&field(&self.config, "paths")?, key)?)?.into())
    }
    fn wall_seconds(&self) -> PolicyResult<f64> {
        Ok(number(&field(&self.config, "wall")?)? / 1e9)
    }
    fn bounds(&self) -> PolicyResult<(NaiveDateTime, NaiveDateTime)> {
        let mut minimum = fixed(2025, 2, 21);
        let systemd = self.path("systemd")?;
        if systemd.exists() {
            let metadata = fs::metadata(systemd)?;
            let epoch = metadata.mtime() as f64 + metadata.mtime_nsec() as f64 / 1e9;
            minimum = minimum.max(
                local_from_timestamp(epoch)?
                    .checked_add_signed(chrono::Duration::days(1))
                    .ok_or_else(|| fail("OverflowError", "date value out of range"))?,
            );
        }
        Ok((minimum, fixed(2035, 1, 1)))
    }
    pub fn valid(&self) -> PolicyResult<bool> {
        let (low, high) = self.bounds()?;
        let now = local_from_timestamp(self.wall_seconds()?)?;
        Ok(low < now && now < high)
    }
    fn run_command(&mut self, args: &[String], error_message: &str) -> PolicyResult<bool> {
        let status = Command::new("sudo").args(args).status().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                fail(
                    "FileNotFoundError",
                    "[Errno 2] No such file or directory: 'sudo'",
                )
            } else {
                error.into()
            }
        })?;
        if !status.success() {
            self.emit(40, error_message.into(), true, "command")?;
            return Ok(false);
        }
        Ok(true)
    }
    pub fn apply(&mut self, zone: &str, source: &str) -> PolicyResult<bool> {
        let target = self.path("zoneinfo")?.join(zone);
        if zone.is_empty() || !target.is_file() {
            self.emit(
                40,
                format!("timezone: invalid zone '{zone}' (source={source})"),
                false,
                "apply_timezone",
            )?;
            return Ok(false);
        }
        if priority(source) < priority(&self.current_source()?) {
            return Ok(false);
        }
        let link = self.path("localtime")?;
        let already = link.is_symlink()
            && crate::path_resolve::resolve_non_strict(&link).ok()
                == crate::path_resolve::resolve_non_strict(&target).ok();
        if !already {
            fs::create_dir_all(
                link.parent()
                    .ok_or_else(|| fail("OSError", "localtime parent"))?,
            )?;
            if !self.run_command(
                &[
                    "rm".into(),
                    "-f".into(),
                    link.to_string_lossy().into_owned(),
                ],
                "timezone: failed to set /data/etc/localtime",
            )? {
                return Ok(false);
            }
            if !self.run_command(
                &[
                    "ln".into(),
                    "-s".into(),
                    target.to_string_lossy().into_owned(),
                    link.to_string_lossy().into_owned(),
                ],
                "timezone: failed to set /data/etc/localtime",
            )? {
                return Ok(false);
            }
        }
        fs::write(self.params.join("TimezoneName"), zone)?;
        fs::write(self.params.join("TimezoneSource"), source)?;
        self.emit(
            20,
            format!("timezone: set to {zone} (source={source})"),
            false,
            "apply_timezone",
        )?;
        Ok(true)
    }
    pub fn gps(longitude: f64) -> PolicyResult<String> {
        let value = (longitude / 15.).round_ties_even();
        if value.is_nan() {
            return Err(fail("ValueError", "cannot convert float NaN to integer"));
        }
        if !value.is_finite() {
            return Err(fail(
                "OverflowError",
                "cannot convert float infinity to integer",
            ));
        }
        let value = value.clamp(-12., 14.);
        Ok(if value == 0. {
            "Etc/GMT".into()
        } else {
            format!(
                "Etc/GMT{}{:.0}",
                if value > 0. { "-" } else { "+" },
                value.abs()
            )
        })
    }
    pub fn internet(&self) -> Option<String> {
        let endpoint = text(&field(&self.config, "endpoint").ok()?).ok()?;
        let body = http_get(&endpoint).ok()?;
        let value = python_json::parse_text(std::str::from_utf8(&body).ok()?).ok()?;
        if !value.get("status")?.text_eq("success") {
            return None;
        }
        let zone = value.get("timezone")?.to_utf8()?;
        (!zone.is_empty() && self.path("zoneinfo").ok()?.join(&zone).is_file()).then_some(zone)
    }
    pub fn set_time(&mut self, epoch: f64) -> PolicyResult<()> {
        self.set_time_inner(epoch, None)
    }
    fn set_time_inner(&mut self, epoch: f64, integer: Option<&str>) -> PolicyResult<()> {
        let diff = (self.wall_seconds()? - epoch).abs();
        let formatted = if diff.is_nan() {
            "nan".into()
        } else {
            format!("{diff:.1}")
        };
        if diff < 10. {
            self.emit(
                10,
                format!("Time diff too small: {formatted}s"),
                false,
                "set_time",
            )?;
            return Ok(());
        }
        self.emit(
            20,
            format!("Setting system time from GPS (diff {formatted}s)"),
            false,
            "set_time",
        )?;
        if epoch.is_nan() {
            return Err(fail("ValueError", "cannot convert float NaN to integer"));
        }
        if !epoch.is_finite() {
            return Err(fail(
                "OverflowError",
                "cannot convert float infinity to integer",
            ));
        }
        // int(original_integer) is exact even when subtraction above rounded
        // it to a float. int(-0.5) and int(-0.0) both produce canonical zero.
        let decimal = integer.map(str::to_owned).unwrap_or_else(|| {
            if epoch.trunc() == 0. {
                "0".into()
            } else {
                format!("{:.0}", epoch.trunc())
            }
        });
        self.run_command(
            &["date".into(), "-s".into(), format!("@{decimal}")],
            "timed.failed_setting_time",
        )?;
        Ok(())
    }
    pub fn action(&mut self, action: &JsonValue) -> PolicyResult<Value> {
        let kind = text(&field(action, "kind")?)?;
        match kind.as_str() {
            "apply" => Ok(json!(self.apply(
                &text(&field(action, "zone")?)?,
                &text(&field(action, "source")?)?
            )?)),
            "gps" => Ok(json!(Self::gps(number(&field(action, "longitude")?)?)?)),
            "internet" => Ok(json!(self.internet())),
            "valid" => Ok(json!(self.valid()?)),
            "bounds" => {
                let (a, b) = self.bounds()?;
                Ok(json!([a.to_string(), b.to_string()]))
            }
            "set_time" => {
                let value = field(action, "epoch")?;
                let number = number(&value)?;
                let integer = match value.view() {
                    JsonView::Integer("-0") => Some("0"),
                    JsonView::Integer(v) => Some(v),
                    _ => None,
                };
                self.set_time_inner(number, integer)?;
                Ok(Value::Null)
            }
            "close_logger" => {
                self.socket.take();
                self.closed = true;
                Ok(Value::Null)
            }
            _ => Err(fail("ValueError", kind)),
        }
    }
    pub fn service(&mut self) -> PolicyResult<&'static str> {
        Ok(
            if self.get_param("UbloxAvailable")?.as_deref() == Some("1") {
                "gpsLocationExternal"
            } else {
                "gpsLocation"
            },
        )
    }
    pub fn step(&mut self, row: &JsonValue) -> PolicyResult<()> {
        self.step_publish(row, |_| Ok(()))
    }
    pub fn step_publish(
        &mut self,
        row: &JsonValue,
        publish: impl FnOnce(&Value) -> PolicyResult<()>,
    ) -> PolicyResult<()> {
        self.monotonic = number(&field(row, "monotonic")?)?;
        let clock = json!({"valid":self.valid()?,"wall":serde_json::from_str::<Value>(&field(&self.config,"wall")?.to_json().map_err(|e|fail("ValueError",e.to_string()))?).map_err(|e|fail("ValueError",e.to_string()))?});
        self.publications.push(clock.clone());
        publish(&clock)?;
        let gps = field(row, "gps")?;
        let gps_ok = truth(&field(&gps, "updated")?)
            && (self.monotonic / 1e9 - number(&field(&gps, "log_mono_time")?)? / 1e9) <= 2.
            && truth(&field(&gps, "has_fix")?);
        let current = priority(&self.current_source()?);
        if current < priority("wifi") {
            let interval = if current == 0 { 30. } else { 300. };
            if self.monotonic / 1e9 - self.last_attempt > interval {
                self.last_attempt = self.monotonic / 1e9;
                if let Some(zone) = self.internet() {
                    self.apply(&zone, "wifi")?;
                } else if gps_ok {
                    self.apply(&Self::gps(number(&field(&gps, "longitude")?)?)?, "gps")?;
                }
            }
        }
        if !gps_ok {
            return Ok(());
        }
        let epoch = number(&field(&gps, "unix_timestamp_millis")?)? / 1000.;
        let local = local_from_timestamp(epoch)?;
        let (low, high) = self.bounds()?;
        if local < low || local > high {
            return Ok(());
        }
        self.set_time(epoch)?;
        self.sleeps.push(10);
        Ok(())
    }
    pub fn state(&self) -> PolicyResult<Value> {
        let mut params = serde_json::Map::new();
        for name in ["TimezoneName", "TimezoneSource"] {
            params.insert(
                name.into(),
                if self.params.join(name).exists() {
                    json!(fs::read_to_string(self.params.join(name))?)
                } else {
                    Value::Null
                },
            );
        }
        let root = text(&field(&self.config, "params_root")?)?;
        let link = self.path("localtime")?;
        let target = if link.is_symlink() {
            Some(
                fs::read_link(link)?
                    .to_string_lossy()
                    .replace(&root, "<root>"),
            )
        } else {
            None
        };
        let commands = timed_fixtures::commands(Path::new(&root))
            .map_err(|e| fail("OSError", e.to_string()))?;
        Ok(
            json!({"params":params,"target":target,"commands":timed_fixtures::replace_root(&json!(commands),&root,"<root>"),"records":self.records}),
        )
    }
}

fn failure_value(error: &Failure) -> Value {
    json!({"type":error.kind,"message":error.message})
}
pub fn evaluate(config: JsonValue, loop_mode: bool) -> PolicyResult<Value> {
    let root = text(&field(&config, "params_root")?)?;
    let prefix = std::env::var("OPENPILOT_PREFIX").map_err(|e| fail("KeyError", e.to_string()))?;
    let mut source = Source::new(config.clone(), Path::new(&root).join(prefix))?;
    let mut errors = vec![];
    let actions = config
        .get("actions")
        .unwrap_or_else(|| JsonValue::parse("[]").unwrap());
    let actions = python_json::sequence(&actions).map_err(|e| fail("TypeError", e.to_string()))?;
    let values = if loop_mode {
        let service = source.service()?;
        for row in actions {
            if let Err(e) = source.step(&row) {
                errors.push(failure_value(&e));
                break;
            }
        }
        json!({"service":service,"publications":source.publications,"sleeps":source.sleeps,"last_attempt":(source.last_attempt*1e9).round_ties_even() as u64})
    } else {
        let mut values = vec![];
        for action in actions {
            match source.action(&action) {
                Ok(value) => {
                    values.push(json!({"ok":value}));
                    errors.push(Value::Null);
                }
                Err(e) => {
                    values.push(json!({"error":e.kind}));
                    errors.push(failure_value(&e));
                }
            }
        }
        json!(values)
    };
    Ok(json!({"values":values,"errors":errors,"state":source.state()?}))
}
/// Original closed-handler arm with independent source policy and actual ZMQ.
pub fn evaluate_logging(config: JsonValue, malformed: bool) -> Result<Value> {
    let root = text(&field(&config, "params_root")?)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    let endpoint = format!("/tmp/logmessage{prefix}");
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL)?;
    collector.set_linger(0)?;
    collector.bind(&format!("ipc://{endpoint}"))?;
    let result: Result<Value> = (|| {
        let mut source = Source::new(config.clone(), Path::new(&root).join(prefix))?;
        source.connect_logger(&format!("ipc://{endpoint}"))?;
        let mut results = vec![];
        for action in python_json::sequence(&field(&config, "actions")?)? {
            match source.action(&action) {
                Ok(_) => results.push("ok"),
                Err(error)
                    if error.kind == "ZMQError"
                        && error.message == "Socket operation on non-socket" =>
                {
                    results.push("error")
                }
                Err(error) => return Err(error.into()),
            }
        }
        let mut records = vec![];
        while collector.poll(zmq::POLLIN, 50)? != 0 {
            let raw = collector.recv_bytes(0)?;
            records.push(serde_json::from_str::<Value>(
                &python_json::parse(raw.get(1..).unwrap_or_default())?.to_json()?,
            )?);
        }
        let mut params = serde_json::Map::new();
        for name in ["TimezoneName", "TimezoneSource"] {
            let path = source.params.join(name);
            params.insert(
                name.into(),
                if path.exists() {
                    json!(fs::read(path)?
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>())
                } else {
                    Value::Null
                },
            );
        }
        if malformed {
            fs::remove_file(source.params.join("TimezoneSource"))?;
        }
        let mut state = source.state()?;
        state["records"] = json!([]);
        Ok(
            json!({"records":records,"result":{"results":results,"state":state,"raw_params":params}}),
        )
    })();
    drop(collector);
    drop(context);
    let _ = fs::remove_file(endpoint);
    result
}
pub fn run_case(
    root: &Path,
    request: &Value,
    directory: &Path,
    reference: &Path,
    command: &Path,
) -> Result<Value> {
    sources(root)?;
    let mut fixture = Fixture::new(
        directory,
        request["timezone"]
            .as_str()
            .ok_or("missing case timezone")?,
        command,
    )?;
    fixture.prepare(request)?;
    let mut spec = CommandSpec::new_os(
        vec![reference.as_os_str().to_owned()],
        Duration::from_secs(30),
    );
    fixture.configure(&mut spec);
    spec.input = Some(
        format!(
            "{}\n",
            json!({"config":fixture.config,"loop":request["loop"]})
        )
        .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = NativeRunner.run(&spec)?;
    if captured.code != 0 {
        return Err(format!(
            "timed source reference exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&captured.stdout)?)
}
pub fn cli() -> Result<()> {
    let line = std::io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("missing source configuration")??;
    let request = python_json::parse_text(&line)?;
    let config = field(&request, "config")?;
    let loop_mode = request.get("loop").is_some_and(|v| truth(&v));
    let result = if request.get("logging").is_some_and(|value| truth(&value)) {
        evaluate_logging(
            config,
            request.get("malformed").is_some_and(|value| truth(&value)),
        )?
    } else {
        evaluate(config, loop_mode)?
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

// Independent urllib subset used by this source fixture: real HTTP, redirect
// limits and a five-second timeout on each socket operation, not total elapsed.
fn endpoint_parts(endpoint: &str) -> Result<(String, String)> {
    let address = endpoint
        .strip_prefix("http://")
        .ok_or("source fixture expects HTTP URL")?;
    let (authority, path) = address
        .split_once('/')
        .map_or((address, "/".to_owned()), |(host, path)| {
            (host, format!("/{path}"))
        });
    Ok((authority.into(), path))
}
// The original shutdown worker routes the fixture's otherwise-unused endpoint
// through a loopback HTTP proxy. Follow urllib's environment precedence and
// suffix bypass rules; all environment belongs to this isolated source child.
fn proxy_environment() -> HashMap<String, String> {
    let environment = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(key, _)| key.to_ascii_lowercase().ends_with("_proxy"))
        .collect::<Vec<_>>();
    let mut proxies = HashMap::new();
    for (key, value) in &environment {
        if !value.is_empty() {
            proxies.insert(key[..key.len() - 6].to_ascii_lowercase(), value.clone());
        }
    }
    if std::env::var_os("REQUEST_METHOD").is_some() {
        proxies.remove("http");
    }
    for (key, value) in &environment {
        if let Some(key) = key.strip_suffix("_proxy") {
            let key = key.to_ascii_lowercase();
            if value.is_empty() {
                proxies.remove(&key);
            } else {
                proxies.insert(key, value.clone());
            }
        }
    }
    proxies
}
fn proxy_bypass(authority: &str, bypass: Option<&String>) -> bool {
    let Some(bypass) = bypass else { return false };
    if bypass == "*" {
        return true;
    }
    let authority = authority.to_ascii_lowercase();
    let host = authority
        .rsplit_once(':')
        .map_or(authority.as_str(), |(host, port)| {
            if port.bytes().all(|byte| byte.is_ascii_digit()) {
                host
            } else {
                &authority
            }
        });
    bypass.split(',').any(|name| {
        let name =
            name.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch));
        if name.is_empty() {
            return false;
        }
        let name = name.trim_start_matches('.').to_ascii_lowercase();
        host == name
            || authority == name
            || host.ends_with(&format!(".{name}"))
            || authority.ends_with(&format!(".{name}"))
    })
}
struct SocketIo(TcpStream);
impl SocketIo {
    fn operation(
        &mut self,
        flags: rustix::event::PollFlags,
        mut operation: impl FnMut(&mut TcpStream) -> io::Result<usize>,
    ) -> io::Result<usize> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match operation(&mut self.0) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "HTTP socket timeout",
                        ));
                    }
                    let timeout =
                        rustix::event::Timespec::try_from(remaining).map_err(io::Error::other)?;
                    let mut fds = [rustix::event::PollFd::new(&self.0, flags)];
                    match rustix::event::poll(&mut fds, Some(&timeout)) {
                        Ok(0) => {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "HTTP socket timeout",
                            ))
                        }
                        Ok(_) => {}
                        Err(rustix::io::Errno::INTR) => continue,
                        Err(error) => return Err(error.into()),
                    }
                }
                result => return result,
            }
        }
    }
}
impl Read for SocketIo {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.operation(rustix::event::PollFlags::IN, |socket| socket.read(buffer))
    }
}
impl Write for SocketIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.operation(rustix::event::PollFlags::OUT, |socket| socket.write(buffer))
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
fn http_get(endpoint: &str) -> Result<Vec<u8>> {
    let mut endpoint = endpoint.to_owned();
    let mut redirects = HashMap::<String, usize>::new();
    let proxies = proxy_environment();
    loop {
        let (authority, path) = endpoint_parts(&endpoint)?;
        let proxy = proxies
            .get("http")
            .filter(|_| !proxy_bypass(&authority, proxies.get("no")));
        let connection = if let Some(proxy) = proxy {
            let proxy = if proxy.contains("://") {
                proxy
                    .strip_prefix("http://")
                    .ok_or("source fixture expects HTTP proxy")?
            } else {
                proxy.as_str()
            };
            proxy
                .split('/')
                .next()
                .ok_or("missing HTTP proxy authority")?
        } else {
            &authority
        };
        let target = if connection
            .rsplit_once(':')
            .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
        {
            connection.to_owned()
        } else {
            format!("{connection}:80")
        };
        let mut socket = None;
        for addr in target.to_socket_addrs()? {
            if let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                socket = Some(stream);
                break;
            }
        }
        let socket = socket.ok_or("HTTP connect failed")?;
        socket.set_nonblocking(true)?;
        // CPython socket timeout uses an exact monotonic poll, not Linux's
        // coarser SO_RCVTIMEO scheduling. Restart only after actual progress.
        let mut socket = SocketIo(socket);
        let request_target = if proxy.is_some() { &endpoint } else { &path };
        write!(socket,"GET {request_target} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: openpilot-timed\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n")?;
        let mut stream = BufReader::new(socket);
        let mut line = String::new();
        stream.read_line(&mut line)?;
        let status = line
            .split_whitespace()
            .nth(1)
            .ok_or("missing HTTP status")?
            .parse::<u16>()?;
        let mut headers = HashMap::new();
        loop {
            line.clear();
            let n = stream.read_line(&mut line)?;
            if n == 0 {
                return Err("HTTP headers ended".into());
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = headers
                .get("location")
                .or_else(|| headers.get("uri"))
                .ok_or("redirect without location")?;
            endpoint = if location.starts_with("http://") {
                location.clone()
            } else if location.starts_with('/') {
                format!("http://{authority}{location}")
            } else {
                format!(
                    "http://{authority}{}{location}",
                    path.rsplit_once('/')
                        .map_or("/".to_owned(), |(parent, _)| format!("{parent}/"))
                )
            };
            if redirects.get(&endpoint).copied().unwrap_or(0) >= 4 || redirects.len() >= 10 {
                return Err("redirect limit".into());
            }
            *redirects.entry(endpoint.clone()).or_default() += 1;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err("HTTP error".into());
        }
        let mut body = vec![];
        if headers
            .get("transfer-encoding")
            .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
        {
            loop {
                line.clear();
                stream.read_line(&mut line)?;
                let n = usize::from_str_radix(
                    line.trim()
                        .split(';')
                        .next()
                        .ok_or("missing chunk length")?,
                    16,
                )?;
                if n == 0 {
                    break;
                }
                let start = body.len();
                stream
                    .by_ref()
                    .take(u64::try_from(n)?)
                    .read_to_end(&mut body)?;
                if body.len().checked_sub(start) != Some(n) {
                    return Err("incomplete HTTP chunk".into());
                }
                line.clear();
                stream.read_line(&mut line)?;
            }
        } else if let Some(n) = headers.get("content-length") {
            let count: u64 = n.parse()?;
            stream.by_ref().take(count).read_to_end(&mut body)?;
            if u64::try_from(body.len())? != count {
                return Err("incomplete HTTP body".into());
            }
        } else {
            stream.read_to_end(&mut body)?;
        }
        return Ok(body);
    }
}
