//! Independent port of the unchanged register/API fixture definitions.
//! Params storage and logging transport are mechanical dependencies; all typed
//! conversion, call order, retry, spinner, hardware and HTTP policy is owned here.
use super::{http, keys, values as v};
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_params::Params;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
struct PolicyError {
    kind: &'static str,
    detail: String,
}
impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}
impl std::error::Error for PolicyError {}
type Policy<T> = std::result::Result<T, PolicyError>;
fn error(kind: &'static str, detail: impl ToString) -> PolicyError {
    PolicyError {
        kind,
        detail: detail.to_string(),
    }
}
fn io_error(cause: io::Error) -> PolicyError {
    error("io", cause)
}
fn decode(raw: Vec<u8>) -> Policy<String> {
    String::from_utf8(raw)
        .map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
        .map_err(|e| error("decode", e))
}
fn read_text(path: &Path) -> Policy<String> {
    decode(fs::read(path).map_err(io_error)?)
}
struct Context {
    config: JsonValue,
    trace: Vec<JsonValue>,
    mono: f64,
    sleeps: i64,
    imeis: VecDeque<JsonValue>,
    params: Params,
    persist: PathBuf,
    root: PathBuf,
    logger: Logger,
    closed: bool,
}
impl Context {
    fn push(&mut self, kind: &str, values: impl IntoIterator<Item = JsonValue>) -> Policy<()> {
        let mut row = vec![v::text(kind)];
        row.extend(values);
        self.trace
            .push(v::array(row).map_err(|e| error("other", e))?);
        Ok(())
    }
    fn log(&mut self, level: Level, message: String, exception: Option<&str>) -> Policy<()> {
        let mut record = Record::text(level, message);
        if let Some(exception) = exception {
            record = record.with_exception(exception.into())
        }
        self.logger
            .emit(log_site!(), record)
            .map(|_| ())
            .map_err(|e| error("logging", e))
    }
    fn spinner(&mut self, name: &str, text: Option<JsonValue>) -> Policy<()> {
        self.push(name, text)?;
        if self
            .config
            .get("spinner_fail")
            .is_some_and(|v| v.text_eq(name))
        {
            return Err(error("spinner", format!("{name} fixture failure")));
        }
        Ok(())
    }
    fn monotonic(&mut self) -> Policy<f64> {
        let old = self.mono;
        self.mono += self
            .config
            .get("step")
            .map_or(Ok(0.0), |n| v::number(&n))
            .map_err(|e| error("other", e))?;
        self.push("monotonic", [v::value(old).map_err(|e| error("other", e))?])?;
        Ok(old)
    }
    fn sleep(&mut self, duration: i64) -> Policy<()> {
        self.push(
            "sleep",
            [v::value(duration).map_err(|e| error("other", e))?],
        )?;
        self.sleeps += 1;
        let max = self
            .config
            .get("max_sleeps")
            .map_or(Ok(40), |n| v::integer(&n))
            .map_err(|e| error("other", e))?;
        if self.sleeps > max {
            return Err(error("stop", "fixture stop"));
        }
        self.mono += duration as f64;
        Ok(())
    }
    fn params_get(&mut self) -> Policy<JsonValue> {
        // Original C++ get returns an empty string on read/open failure; empty bytes
        // map to None before the typed converter, including a DongleId directory.
        let raw = self
            .params
            .get("DongleId")
            .ok()
            .flatten()
            .unwrap_or_default();
        if raw.is_empty() {
            return Ok(v::null());
        }
        match String::from_utf8(raw.clone()) {
            Ok(text) => Ok(v::text(&text)),
            Err(_) => {
                self.log(Level::Warning,format!("Failed to cast param DongleId with value={} from type t=<ParamKeyType.STRING: 0>",v::bytes_repr(&raw)),None)?;
                Ok(v::null())
            }
        }
    }
    fn get_keys(&self) -> Policy<Option<(String, String, String)>> {
        for (name, algorithm) in [("id_rsa", "RS256"), ("id_ecdsa", "ES256")] {
            let private = self.persist.join("comma").join(name);
            let public = self.persist.join("comma").join(format!("{name}.pub"));
            if private.is_file() && public.is_file() {
                // Both opens occur before either read/decode, as Python's two context
                // managers require; a public-open error must precede private invalid UTF-8.
                let mut private = File::open(private).map_err(io_error)?;
                let mut public = File::open(public).map_err(io_error)?;
                let mut raw = Vec::new();
                private.read_to_end(&mut raw).map_err(io_error)?;
                let private = decode(raw)?;
                let mut raw = Vec::new();
                public.read_to_end(&mut raw).map_err(io_error)?;
                let public = decode(raw)?;
                return Ok(Some((algorithm.into(), private, public)));
            }
        }
        Ok(None)
    }
    fn serial(&mut self) -> Policy<JsonValue> {
        self.push("serial", [])?;
        if let Some(hardware) = self.config.get("hardware_root") {
            let root = PathBuf::from(v::os(&hardware).map_err(|e| error("hardware", e))?);
            let text = read_text(&root.join("proc/cmdline")).map_err(|e| error("hardware", e))?;
            let mut serial = None;
            for token in text.split(' ') {
                let pair = token.split('=').collect::<Vec<_>>();
                if pair.len() == 2 && pair[0] == "androidboot.serialno" {
                    serial = Some(v::text(pair[1]));
                }
            }
            return serial.ok_or_else(|| error("hardware", "androidboot.serialno"));
        }
        let value = self
            .config
            .get("serial")
            .unwrap_or_else(|| v::text("synthetic-serial"));
        if matches!(value.view(), JsonView::Text(_)) {
            Ok(value)
        } else {
            Err(error("hardware", "serial fixture failure"))
        }
    }
    fn imei(&mut self, slot: i64) -> Policy<JsonValue> {
        self.push("imei", [v::value(slot).map_err(|e| error("other", e))?])?;
        if let Some(hardware) = self.config.get("hardware_root") {
            if slot != 0 {
                return Ok(v::text(""));
            }
            let root = PathBuf::from(v::os(&hardware).map_err(|e| error("other", e))?);
            let state = match fs::read(root.join("dev/shm/modem")) {
                Ok(raw) => match String::from_utf8(raw) {
                    Ok(text) => JsonValue::parse(&text)
                        .unwrap_or(v::value(serde_json::json!({})).map_err(|e| error("other", e))?),
                    Err(e) => return Err(error("decode", e)),
                },
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    v::value(serde_json::json!({})).map_err(|e| error("other", e))?
                }
                Err(e) => return Err(io_error(e)),
            };
            if !state.is_object() {
                return Err(error("other", "modem state has no get method"));
            }
            return Ok(state.get("imei").unwrap_or_else(|| v::text("")));
        }
        match self.imeis.pop_front() {
            Some(value) if v::is_null(&value) || matches!(value.view(), JsonView::Text(_)) => {
                Ok(value)
            }
            _ => Err(error("other", "IMEI fixture failure")),
        }
    }
    fn progress(&mut self, serial: &JsonValue, imei1: &JsonValue, imei2: &JsonValue) -> Policy<()> {
        let mut points = "registering device - serial: "
            .chars()
            .map(u32::from)
            .collect::<Vec<_>>();
        for (value, suffix) in [(serial, ", IMEI: ("), (imei1, ", "), (imei2, ")")] {
            points.extend(v::python_str(value).map_err(|e| error("other", e))?);
            points.extend(suffix.chars().map(u32::from));
        }
        self.spinner(
            "spinner_update",
            Some(
                JsonValue::codepoints(points)
                    .ok_or_else(|| error("other", "invalid spinner codepoint"))?,
            ),
        )
    }
    fn expiration(&mut self) -> Policy<i64> {
        let utc = self
            .config
            .get("utc")
            .unwrap_or(v::value(1_700_000_000i64).map_err(|e| error("other", e))?);
        self.push("now", [utc.clone()])?;
        let utc = v::number(&utc).map_err(|e| error("other", e))?;
        if !utc.is_finite()
            || !(-62_135_596_800.0..253_402_300_800.0).contains(&utc)
            || utc + 3600.0 >= 253_402_300_800.0
        {
            return Err(error("other", "datetime outside year 1..9999"));
        }
        Ok(utc.floor() as i64 + 3600)
    }
    fn register(&mut self) -> Policy<JsonValue> {
        let mut identity = self.params_get()?;
        let fallback = self.persist.join("comma/dongle_id");
        if v::is_null(&identity) {
            let present = match fs::metadata(&fallback) {
                Ok(v) => v.is_file(),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    false
                }
                Err(e) => return Err(io_error(e)),
            };
            if present {
                let text = read_text(&fallback)?;
                identity = v::text(text.trim_matches(|c: char| {
                    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
                }));
            }
        }
        let pair = self.get_keys()?;
        let public = pair.as_ref().map_or("", |(_, _, public)| public.as_str());
        if public.is_empty() {
            identity = v::text("UnregisteredDevice");
            self.log(Level::Warning, "missing public key".into(), None)?;
        } else if v::is_null(&identity) {
            let spinner = v::flag(&self.config, "spinner");
            if spinner {
                self.spinner("spinner_start", None)?;
                self.spinner("spinner_update", Some(v::text("registering device")))?;
            }
            let serial = self.serial()?;
            let started = self.monotonic()?;
            let mut imei1 = v::null();
            let mut imei2 = v::null();
            while v::is_null(&imei1) && v::is_null(&imei2) {
                let attempt = (|| {
                    let first = self.imei(0)?;
                    let second = self.imei(1)?;
                    Ok::<_, PolicyError>((first, second))
                })();
                match attempt {
                    Ok((first, second)) => {
                        imei1 = first;
                        imei2 = second
                    }
                    Err(e) => {
                        self.log(
                            Level::Error,
                            "Error getting imei, trying again...".into(),
                            Some(&e.detail),
                        )?;
                        self.sleep(1)?;
                    }
                }
                if self.monotonic()? - started > 60.0 && spinner {
                    self.progress(&serial, &imei1, &imei2)?
                }
            }
            let mut backoff = 0;
            let started = self.monotonic()?;
            let (algorithm, private, public) =
                pair.ok_or_else(|| error("other", "selected key disappeared"))?;
            loop {
                let attempt = (|| {
                    let expiration = self.expiration()?;
                    let token = keys::sign(&private, &algorithm, expiration)
                        .map_err(|e| error("other", e))?;
                    self.log(Level::Info, "getting pilotauth".into(), None)?;
                    let version = read_text(&self.root.join("openpilot/common/version.h"))?
                        .split('"')
                        .nth(1)
                        .ok_or_else(|| error("other", "version string absent"))?
                        .to_owned();
                    let host = v::required_text(&self.config, "api_host")
                        .map_err(|e| error("other", e))?;
                    let (status, body) = http::authenticate(
                        &host,
                        &[
                            ("imei".into(), imei1.clone()),
                            ("imei2".into(), imei2.clone()),
                            ("serial".into(), serial.clone()),
                            ("public_key".into(), v::text(&public)),
                            ("register_token".into(), v::text(&token)),
                        ],
                        &format!("openpilot-{version}"),
                    )
                    .map_err(|e| error("other", e))?;
                    if matches!(status, 402 | 403) {
                        self.log(
                            Level::Info,
                            format!("Unable to register device, got {status}"),
                            None,
                        )?;
                        Ok(v::text("UnregisteredDevice"))
                    } else {
                        body.get("dongle_id")
                            .ok_or_else(|| error("other", "response has no dongle_id"))
                    }
                })();
                match attempt {
                    Ok(value) => {
                        identity = value;
                        break;
                    }
                    Err(e) => {
                        self.log(
                            Level::Error,
                            "failed to authenticate".into(),
                            Some(&e.detail),
                        )?;
                        backoff = (backoff + 1).min(15);
                        self.sleep(backoff)?;
                    }
                }
                if self.monotonic()? - started > 60.0 && spinner {
                    self.progress(&serial, &imei1, &imei2)?
                }
            }
            if spinner {
                self.spinner("spinner_close", None)?;
            }
        }
        if v::truth(&identity) {
            let JsonView::Text(points) = identity.view() else {
                return Err(error("identity_type", "DongleId STRING requires str"));
            };
            let text = v::scalar_utf8(points).map_err(|e| error("identity_type", e))?;
            // Cython's Params.put ignores its C++ int return; a valid STRING encode
            // succeeds even when the filesystem write fails (directory fixture).
            let _ = self.params.put("DongleId", text.as_bytes());
        }
        Ok(identity)
    }
}
pub(super) fn run() -> Result<()> {
    let mut lines = io::stdin().lock().lines();
    let config = JsonValue::parse(&lines.next().ok_or("missing registration config")??)?;
    let params = Params::open(&v::path(&config, "params")?, "d")?;
    let endpoint = v::required_text(&config, "endpoint")?;
    let closed = v::flag(&config, "closed_log");
    let mut logger = Factory::new(if closed {
        format!("{endpoint}-closed")
    } else {
        endpoint.clone()
    })?
    .logger();
    if closed {
        logger.emit(
            log_site!(),
            Record::text(Level::Debug, "prepare closed socket".into()),
        )?;
        logger.close();
    }
    let imeis = config.get("imeis").map_or_else(
        || Ok(vec![v::text("synthetic-imei"), v::null()]),
        |v| v::items(&v),
    )?;
    let persist = v::path(&config, "persist")?;
    let root = v::path(&config, "source_root")?;
    let mut context = Context {
        config,
        trace: Vec::new(),
        mono: 0.0,
        sleeps: 0,
        imeis: imeis.into(),
        params,
        persist,
        root,
        logger,
        closed,
    };
    let outcome = if context
        .config
        .get("mode")
        .is_some_and(|v| v.text_eq("is_registered"))
    {
        context.params_get().map(|v| {
            v::value(serde_json::json!({"value":!v::is_null(&v)&&!v.text_eq("UnregisteredDevice")}))
                .expect("boolean outcome")
        })
    } else {
        context.register().and_then(|value| {
            v::object([(
                "value_json".into(),
                v::text(&value.to_json().map_err(|e| error("other", e))?),
            )])
            .map_err(|e| error("other", e))
        })
    };
    let outcome = match outcome {
        Ok(value) => value,
        Err(error) => v::value(
            serde_json::json!({"error":error.kind,"detail":error.detail,"type":"IndependentSourceError"}),
        )?,
    };
    if context.closed {
        context.logger = Factory::new(endpoint)?.logger();
    }
    context.logger.emit(
        log_site!(),
        Record::text(Level::Debug, "registration-fixture-end".into()),
    )?;
    let path = context.params.directory().join("DongleId");
    let params_hex = if path.is_file() {
        v::text(&v::hex(&fs::read(path)?))
    } else {
        v::null()
    };
    let result = v::object([
        ("outcome".into(), outcome),
        ("trace".into(), v::array(context.trace)?),
        ("params_hex".into(), params_hex),
    ])?;
    println!("{}", result.to_json()?);
    io::stdout().flush()?;
    let _ = lines.next();
    Ok(())
}
