//! Persistent original-source oracle driver. All paths and SDK transports are supplied by fixtures.
use openpilot_crash_reporting::{
    sdk::NativeSdk, Configuration, Error as ReportError, NativeException, ParamsSource, Project,
    Reporter, RuntimeInputs, Sdk,
};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_logmessaged::JsonValue;
use openpilot_tombstoned::{
    apport::Retrace,
    daemon::{crash_filename, report_tombstone_apport, Clock, Daemon},
    discovery::{clear_apport_folder, get_tombstones},
    safe_fn, Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Configure {
        base: PathBuf,
        params: PathBuf,
        pc: bool,
        device: String,
        #[serde(default)]
        dsn: Option<String>,
    },
    Init {
        project: Project,
    },
    Fail {
        operation: Option<String>,
    },
    Capture {
        exception: NativeException,
        #[serde(default = "yes")]
        log_exception: bool,
    },
    CaptureIo {
        path: PathBuf,
    },
    Tag {
        key: String,
        value: String,
    },
    Tombstone {
        filename: String,
        message: String,
        contents: String,
    },
    Safe {
        text: String,
    },
    Filename {
        stamp: String,
        commit: String,
        path: String,
    },
    Scan {
        path: PathBuf,
    },
    Clear {
        path: PathBuf,
    },
    Retrace {
        path: PathBuf,
        #[serde(default = "timeout")]
        timeout_ms: u64,
        #[serde(default = "shell")]
        shell: PathBuf,
    },
    Report {
        path: PathBuf,
        root: PathBuf,
        stamp: String,
        #[serde(default = "timeout")]
        timeout_ms: u64,
    },
    Start {
        apport: PathBuf,
        root: PathBuf,
        stamp: String,
    },
    Cycle,
}
fn yes() -> bool {
    true
}
fn timeout() -> u64 {
    30_000
}
fn shell() -> PathBuf {
    "/bin/bash".into()
}
struct FixedClock(String);
impl Clock for FixedClock {
    fn local_stamp(&mut self) -> String {
        self.0.clone()
    }
}
struct TraceSdk {
    calls: Vec<Value>,
    failure: Option<String>,
    native: Option<NativeSdk>,
    params: PathBuf,
}
impl TraceSdk {
    fn call(&mut self, operation: &'static str, fields: Value) -> Result<(), ReportError> {
        let parameter = match std::fs::read(self.params.join("fixture/CarrotException")) {
            Ok(value) => Some(String::from_utf8_lossy(&value).into_owned()),
            Err(_) => None,
        };
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| ReportError::Sdk {
                operation: "clock",
                detail: error.to_string(),
            })?
            .as_secs_f64();
        self.calls.push(
            json!({"op":operation,"fields":fields,"carrot_exception":parameter,"created":created}),
        );
        if self.failure.as_deref() == Some(operation) {
            return Err(ReportError::Sdk {
                operation,
                detail: "fixture failure".into(),
            });
        }
        Ok(())
    }
}
impl Sdk for TraceSdk {
    fn init(&mut self, configuration: Configuration) -> Result<(), ReportError> {
        self.call("init", serde_json::to_value(&configuration)?)?;
        if let Some(native) = &mut self.native {
            native.init(configuration)?;
        }
        Ok(())
    }
    fn set_user(&mut self, id: Option<String>) -> Result<(), ReportError> {
        self.call("set_user", json!({"id":id}))?;
        if let Some(native) = &mut self.native {
            native.set_user(id)?;
        }
        Ok(())
    }
    fn set_tag(&mut self, key: &str, value: &JsonValue) -> Result<(), ReportError> {
        self.call("set_tag", json!({"key":key,"value_json":value.to_json()?}))?;
        if let Some(native) = &mut self.native {
            native.set_tag(key, value)?;
        }
        Ok(())
    }
    fn set_extra(&mut self, key: &str, value: &JsonValue) -> Result<(), ReportError> {
        self.call(
            "set_extra",
            json!({"key":key,"value_json":value.to_json()?}),
        )?;
        if let Some(native) = &mut self.native {
            native.set_extra(key, value)?;
        }
        Ok(())
    }
    fn capture_message(&mut self, message: &str) -> Result<(), ReportError> {
        self.call("capture_message", json!({"message":message}))?;
        if let Some(native) = &mut self.native {
            native.capture_message(message)?;
        }
        Ok(())
    }
    fn capture_exception(&mut self, exception: &NativeException) -> Result<(), ReportError> {
        self.call("capture_exception", serde_json::to_value(exception)?)?;
        if let Some(native) = &mut self.native {
            native.capture_exception(exception)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), ReportError> {
        self.call("flush", Value::Null)?;
        if let Some(native) = &mut self.native {
            native.flush()?;
        }
        Ok(())
    }
}
type Report = Reporter<TraceSdk, RuntimeInputs>;
type State = Daemon<TraceSdk, RuntimeInputs, FixedClock>;
fn report_error(error: ReportError) -> String {
    match error {
        ReportError::Io(error) => io_error(&error),
        ReportError::Utf8(_) => "UnicodeDecodeError".into(),
        ReportError::Metadata(openpilot_runtime_version::Error::Attribute(_)) => {
            "AttributeError".into()
        }
        ReportError::Metadata(openpilot_runtime_version::Error::Io(error)) => io_error(&error),
        ReportError::Metadata(openpilot_runtime_version::Error::VersionHeader) => {
            "IndexError".into()
        }
        ReportError::Metadata(openpilot_runtime_version::Error::Utf8(_)) => {
            "UnicodeDecodeError".into()
        }
        ReportError::Metadata(openpilot_runtime_version::Error::Json(_)) => {
            "JSONDecodeError".into()
        }
        ReportError::Metadata(_) => "Exception".into(),
        ReportError::Json(_) | ReportError::Encode(_) => "JSONDecodeError".into(),
        ReportError::Format(_) | ReportError::Logging(_) => "LoggingError".into(),
        ReportError::Params(openpilot_params::Error::Io(error)) => io_error(&error),
        ReportError::Params(_) | ReportError::ParamsString(_) => "ParamsError".into(),
        ReportError::Unicode(_) => "UnicodeEncodeError".into(),
        ReportError::Sdk { .. } => "RuntimeError".into(),
    }
}
fn io_error(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "FileNotFoundError",
        io::ErrorKind::PermissionDenied => "PermissionError",
        io::ErrorKind::IsADirectory => "IsADirectoryError",
        io::ErrorKind::NotADirectory => "NotADirectoryError",
        _ => "OSError",
    }
    .into()
}
fn failure(error: Error) -> Value {
    let kind = match error {
        Error::Cancelled => "InterruptedError".into(),
        Error::Io(error) => io_error(&error),
        Error::Reporting(error) => report_error(error),
        Error::Metadata(error) => report_error(ReportError::Metadata(error)),
        Error::Json(_) => "JSONDecodeError".into(),
        Error::Format(_) | Error::Logging(_) => "LoggingError".into(),
        Error::Decode(_) | Error::Utf8(_) => "UnicodeDecodeError".into(),
        Error::Posix(_) => "OSError".into(),
        Error::SameFile => "SameFileError".into(),
        Error::CommitType => "TypeError".into(),
        Error::CommitKey => "KeyError".into(),
        Error::Contract(_) => "ContractError".into(),
    };
    json!({"error":kind})
}
fn run(
    request: Request,
    report: &mut Option<Report>,
    state: &mut Option<State>,
) -> Result<Value, Error> {
    if let Request::Configure {
        base,
        params,
        pc,
        device,
        dsn,
    } = request
    {
        let native = dsn.as_deref().map(NativeSdk::local_capture).transpose()?;
        let inputs = RuntimeInputs {
            base,
            params: ParamsSource::Isolated {
                root: params.clone(),
                prefix: "fixture".into(),
            },
            pc,
            device_override: Some(device),
        };
        *report = Some(Reporter {
            sdk: TraceSdk {
                calls: Vec::new(),
                failure: None,
                native,
                params,
            },
            inputs,
            logger: Factory::for_runtime()?.logger(),
        });
        *state = None;
        return Ok(json!({"value":true}));
    }
    match request {
        Request::Safe { text } => return Ok(json!({"value":safe_fn(&text)})),
        Request::Filename {
            stamp,
            commit,
            path,
        } => return Ok(json!({"value":crash_filename(&stamp,&JsonValue::parse(&commit)?,&path)?})),
        Request::Scan { path } => {
            let mut files = get_tombstones(&path)?
                .into_iter()
                .map(|entry| json!([entry.path, entry.ctime]))
                .collect::<Vec<_>>();
            files.sort_by_key(Value::to_string);
            return Ok(json!({"value":files}));
        }
        Request::Clear { path } => {
            clear_apport_folder(&path);
            return Ok(json!({"value":null}));
        }
        Request::Retrace {
            path,
            timeout_ms,
            shell,
        } => {
            return Ok(
                json!({"value":Retrace{shell,timeout:Duration::from_millis(timeout_ms),..Retrace::default()}.stacktrace(&path)?}),
            )
        }
        Request::Start {
            apport,
            root,
            stamp,
        } => {
            let reporter = report
                .take()
                .ok_or(Error::Contract("configure before start"))?;
            *state = Some(Daemon::start(
                reporter,
                apport,
                root,
                Retrace::default(),
                FixedClock(stamp),
            )?);
            return Ok(
                json!({"value":state.as_ref().ok_or(Error::Contract("missing state"))?.should_report()}),
            );
        }
        Request::Cycle => {
            state
                .as_mut()
                .ok_or(Error::Contract("start before cycle"))?
                .cycle()?;
            return Ok(json!({"value":null}));
        }
        Request::Configure { .. }
        | Request::Init { .. }
        | Request::Fail { .. }
        | Request::Capture { .. }
        | Request::CaptureIo { .. }
        | Request::Tag { .. }
        | Request::Tombstone { .. }
        | Request::Report { .. } => {}
    }
    let reporter = match state {
        Some(state) => &mut state.reporter,
        None => report.as_mut().ok_or(Error::Contract("configure first"))?,
    };
    match request {
        Request::Init { project } => Ok(json!({"value":reporter.init(project)?})),
        Request::Fail { operation } => {
            reporter.sdk.failure = operation;
            Ok(json!({"value":null}))
        }
        Request::Capture {
            exception,
            log_exception,
        } => {
            reporter.capture_exception(&exception, log_exception)?;
            Ok(json!({"value":null}))
        }
        Request::CaptureIo { path } => match std::fs::read(path) {
            Ok(_) => Err(Error::Contract("I/O fixture unexpectedly exists")),
            Err(error) => {
                reporter.capture_exception(&NativeException::from_error(&error), true)?;
                Ok(json!({"value":null}))
            }
        },
        Request::Tag { key, value } => {
            reporter.set_tag(&key, &JsonValue::parse(&value)?)?;
            Ok(json!({"value":null}))
        }
        Request::Tombstone {
            filename,
            message,
            contents,
        } => {
            reporter.report_tombstone(&filename, &message, &contents)?;
            Ok(json!({"value":null}))
        }
        Request::Report {
            path,
            root,
            stamp,
            timeout_ms,
        } => {
            report_tombstone_apport(
                &path,
                &root,
                &Retrace {
                    timeout: Duration::from_millis(timeout_ms),
                    ..Retrace::default()
                },
                reporter,
                &mut FixedClock(stamp),
            )?;
            Ok(json!({"value":null}))
        }
        Request::Configure { .. }
        | Request::Safe { .. }
        | Request::Filename { .. }
        | Request::Scan { .. }
        | Request::Clear { .. }
        | Request::Retrace { .. }
        | Request::Start { .. }
        | Request::Cycle => Err(Error::Contract("handled operation reached dispatch")),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reporter = None;
    let mut state = None;
    let mut barrier = Factory::for_runtime()?.logger();
    for (index, line) in io::stdin().lock().lines().enumerate() {
        let request: Request = serde_json::from_str(&line?)?;
        let result = run(request, &mut reporter, &mut state).unwrap_or_else(failure);
        let calls = match &mut state {
            Some(state) => std::mem::take(&mut state.reporter.sdk.calls),
            None => reporter
                .as_mut()
                .map_or_else(Vec::new, |reporter| std::mem::take(&mut reporter.sdk.calls)),
        };
        barrier.emit(
            log_site!(),
            Record::text(Level::Debug, format!("barrier-{index}")),
        )?;
        writeln!(io::stdout(), "{}", json!({"result":result,"calls":calls}))?;
        io::stdout().flush()?;
    }
    Ok(())
}
