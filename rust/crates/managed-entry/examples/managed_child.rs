//! Isolated real child fixture for the managed in-process API; never a production launcher.
use openpilot_crash_reporting::{
    sdk::NativeSdk, Configuration, Error as ReportError, NativeException, ParamsSource, Project,
    Reporter, RuntimeInputs, Sdk,
};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
    Fields, Value as LogValue,
};
use openpilot_logmessaged::JsonValue;
use openpilot_managed_entry::{launch, EntryError, Outcome, Sigint, StepError};
use openpilot_msgq::Publisher;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    time::Duration,
};

#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    prefix: String,
    endpoint: String,
    dsn: String,
    process: String,
    daemon: String,
    body: String,
    #[serde(default)]
    prepare: Option<String>,
    #[serde(default)]
    reset: Option<String>,
    #[serde(default)]
    sdk_failure: Option<String>,
    #[serde(default)]
    close_logger: bool,
    #[serde(default)]
    params_fault: Option<String>,
    #[serde(default)]
    global_daemon: Option<String>,
    #[serde(default)]
    real_sdk: bool,
}
struct ProbeSdk {
    inner: NativeSdk,
    calls: Vec<Value>,
    root: PathBuf,
    prefix: String,
    failure: Option<String>,
    real: bool,
}
impl ProbeSdk {
    fn call(&mut self, operation: &str, fields: Value) -> Result<(), ReportError> {
        let parameter = fs::read(
            self.root
                .join("params")
                .join(&self.prefix)
                .join("CarrotException"),
        )
        .ok()
        .map(|value| String::from_utf8_lossy(&value).into_owned());
        self.calls
            .push(json!({"op":operation,"fields":fields,"carrot_exception":parameter}));
        if self.failure.as_deref() == Some(operation) {
            return Err(ReportError::Sdk {
                operation: "fixture",
                detail: format!("{operation} fixture failure"),
            });
        }
        Ok(())
    }
}
impl Sdk for ProbeSdk {
    fn init(&mut self, configuration: Configuration) -> Result<(), ReportError> {
        self.call("init", serde_json::to_value(&configuration)?)?;
        if self.real {
            self.inner.init(configuration)?;
        }
        Ok(())
    }
    fn set_user(&mut self, id: Option<String>) -> Result<(), ReportError> {
        self.call("set_user", json!({"id":id}))?;
        if self.real {
            self.inner.set_user(id)?;
        }
        Ok(())
    }
    fn set_tag(&mut self, key: &str, value: &JsonValue) -> Result<(), ReportError> {
        self.call(
            if key == "daemon" {
                "daemon_tag"
            } else {
                "set_tag"
            },
            json!({"key":key,"value_json":value.to_json()?}),
        )?;
        if self.real {
            self.inner.set_tag(key, value)?;
        }
        Ok(())
    }
    fn set_extra(&mut self, key: &str, value: &JsonValue) -> Result<(), ReportError> {
        self.call(
            "set_extra",
            json!({"key":key,"value_json":value.to_json()?}),
        )?;
        if self.real {
            self.inner.set_extra(key, value)?;
        }
        Ok(())
    }
    fn capture_message(&mut self, message: &str) -> Result<(), ReportError> {
        self.call("capture_message", json!({"message":message}))?;
        if self.real {
            self.inner.capture_message(message)?;
        }
        Ok(())
    }
    fn capture_exception(&mut self, exception: &NativeException) -> Result<(), ReportError> {
        self.call("capture_exception", serde_json::to_value(exception)?)?;
        if self.real {
            self.inner.capture_exception(exception)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), ReportError> {
        self.call("flush", Value::Null)?;
        if self.real {
            self.inner.flush()?;
        }
        Ok(())
    }
}
#[derive(Debug, thiserror::Error)]
#[error("failed to load managed body fixture")]
struct BodyError {
    #[source]
    source: io::Error,
}
fn missing(root: &std::path::Path) -> io::Error {
    fs::read(root.join("missing-input")).expect_err("fixture input must be absent")
}
fn gate(value: &Option<String>, root: &std::path::Path) -> Result<(), StepError> {
    match value.as_deref() {
        None => Ok(()),
        Some("interrupt") => Err(StepError::Interrupted),
        Some("error") => Err(StepError::raised(missing(root))),
        Some(_) => Err(StepError::raised(io::Error::other("unknown gate mode"))),
    }
}
fn line(value: &Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}
fn acknowledge() -> io::Result<()> {
    let mut value = String::new();
    io::stdin().lock().read_line(&mut value)?;
    if value.trim() != "continue" {
        return Err(io::Error::other("missing fixture acknowledgement"));
    }
    Ok(())
}
fn run(request: Request) -> Result<bool, Box<dyn std::error::Error>> {
    let factory = Factory::new(request.endpoint.clone())?;
    let mut global: Fields = [("inherited".into(), LogValue::Text("global".into()))]
        .into_iter()
        .collect();
    if let Some(value) = &request.global_daemon {
        global.insert("daemon".into(), LogValue::Text(value.clone()));
    }
    factory.bind_global(global)?;
    let mut inputs = RuntimeInputs::new(request.root.join("base"));
    inputs.params = ParamsSource::Isolated {
        root: request.root.join("params"),
        prefix: request.prefix.clone(),
    };
    inputs.pc = false;
    inputs.device_override = Some("tici".into());
    let sdk = ProbeSdk {
        inner: NativeSdk::local_capture(&request.dsn)?,
        calls: Vec::new(),
        root: request.root.clone(),
        prefix: request.prefix.clone(),
        failure: None,
        real: request.real_sdk,
    };
    let mut reporter = Reporter {
        sdk,
        inputs,
        logger: factory.logger(),
    };
    reporter.logger.bind(
        [("local".into(), LogValue::Text("retained".into()))]
            .into_iter()
            .collect(),
    );
    let enabled = reporter.init(Project::Selfdrive)?;
    reporter.sdk.calls.clear();
    reporter.sdk.failure = request.sdk_failure.clone();
    let signal = Sigint::install()?;
    reporter.logger.emit(
        log_site!(),
        Record::text(Level::Debug, "managed-entry-ready".into()),
    )?;
    line(&json!({"ready":true,"reporting_enabled":enabled}))?;
    acknowledge()?;
    let steps = Rc::new(RefCell::new(Vec::<Value>::new()));
    let result = launch(
        &mut reporter,
        &request.process,
        &request.daemon,
        || {
            steps.borrow_mut().push(json!({"step":"prepare"}));
            gate(&request.prepare, &request.root)?;
            fs::read(request.root.join("prepared-input")).map_err(StepError::raised)
        },
        |prepared| {
            steps.borrow_mut().push(json!({"step":"reset_context","comm":fs::read_to_string("/proc/self/comm").map_err(StepError::raised)?.trim()}));
            gate(&request.reset, &request.root)?;
            let publisher = Publisher::for_runtime("managedEntryBody", 1024 * 1024)
                .map_err(StepError::raised)?;
            line(&json!({"context_ready":true})).map_err(StepError::raised)?;
            acknowledge().map_err(StepError::raised)?;
            Ok((prepared, publisher))
        },
        |(prepared, mut publisher), reporter| {
            steps
                .borrow_mut()
                .push(json!({"step":"body","prepared":String::from_utf8_lossy(&prepared)}));
            reporter
                .logger
                .emit(
                    log_site!(),
                    Record::text(Level::Info, "managed body entered".into()),
                )
                .map_err(StepError::raised)?;
            publisher
                .send(b"managed body IPC")
                .map_err(StepError::raised)?;
            if request.close_logger {
                reporter.logger.close();
            }
            match request.params_fault.as_deref() {
                Some("open") => {
                    reporter.inputs.params = ParamsSource::Isolated {
                        root: request.root.join("params-blocked"),
                        prefix: request.prefix.clone(),
                    }
                }
                Some("put") => {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        request.root.join("params"),
                        fs::Permissions::from_mode(0o500),
                    )
                    .map_err(StepError::raised)?;
                }
                None => {}
                Some(_) => return Err(StepError::raised(io::Error::other("unknown Params fault"))),
            }
            match request.body.as_str() {
                "return" => Ok(()),
                "error" => Err(StepError::raised(missing(&request.root))),
                "chain" => Err(StepError::raised(BodyError {
                    source: missing(&request.root),
                })),
                "interrupt" => Err(StepError::Interrupted),
                "signal" => {
                    line(&json!({"waiting_for_sigint":true})).map_err(StepError::raised)?;
                    while !signal.requested() {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(StepError::Interrupted)
                }
                _ => Err(StepError::raised(io::Error::other("unknown body mode"))),
            }
        },
    );
    let (success, outcome) = match result {
        Ok(outcome) => (
            true,
            json!({"kind":if outcome==Outcome::Returned {"returned"} else {"interrupted"}}),
        ),
        Err(EntryError::Raised { stage, error }) => (
            false,
            json!({"kind":"raised","stage":format!("{stage:?}"),"exception":error.exception}),
        ),
        Err(EntryError::Reporting {
            stage,
            error,
            reporting,
        }) => (
            false,
            json!({"kind":"reporting_failed","stage":format!("{stage:?}"),"original":error.exception,"secondary":reporting.to_string()}),
        ),
        Err(EntryError::InterruptLog(error)) => (
            false,
            json!({"kind":"interrupt_log_failed","secondary":error.to_string()}),
        ),
    };
    let response = json!({"outcome":outcome,"steps":*steps.borrow(),"sdk_calls":reporter.sdk.calls,"comm":fs::read_to_string("/proc/self/comm")?.trim(),"cmdline":String::from_utf8_lossy(&fs::read("/proc/self/cmdline")?).replace('\0'," ")});
    line(&response)?;
    acknowledge()?;
    Ok(success)
}
fn main() -> ExitCode {
    let result = (|| {
        let path = std::env::args_os()
            .nth(1)
            .ok_or("fixture request path required")?;
        let request = serde_json::from_slice(&fs::read(path)?)?;
        run(request)
    })();
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("managed child fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
