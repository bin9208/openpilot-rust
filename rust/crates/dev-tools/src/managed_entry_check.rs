//! Original managed-entry QA schedule and comparison contract.
//! The independent source policy and candidate must both execute their scenario;
//! recorded answers are only regression fixtures, never a candidate oracle.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    path_resolve::resolve_non_strict,
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};
mod runtime;
mod worker;
pub use runtime::{run, run_readiness};

pub fn run_worker(request: &Path) -> Result<Value> {
    worker::run(request)
}

#[derive(Clone, Debug)]
pub struct Options {
    pub binary: PathBuf,
    pub collector: PathBuf,
    pub params_binding: PathBuf,
    pub output: PathBuf,
    pub case: Option<String>,
    pub runner: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}

#[derive(Debug)]
pub struct InvalidCase {
    pub output: PathBuf,
    name: String,
}
impl std::fmt::Display for InvalidCase {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "unknown managed-entry case: {}", self.name)
    }
}
impl std::error::Error for InvalidCase {}

pub fn cases() -> Vec<(&'static str, Value)> {
    vec![
        ("normal", json!({"body":"return"})),
        ("interrupt", json!({"body":"interrupt"})),
        ("sigint", json!({"body":"signal"})),
        ("error", json!({"body":"error","real_sdk":true})),
        ("chain", json!({"body":"chain","real_sdk":true})),
        (
            "reporting-disabled",
            json!({"body":"error","real_sdk":true,"reporting":false}),
        ),
        (
            "prepare-error",
            json!({"body":"return","prepare":"error","real_sdk":true}),
        ),
        (
            "prepare-interrupt",
            json!({"body":"return","prepare":"interrupt"}),
        ),
        ("reset-error", json!({"body":"return","reset":"error"})),
        (
            "reset-interrupt",
            json!({"body":"return","reset":"interrupt"}),
        ),
        (
            "name-error",
            json!({"body":"return","process":"invalid\0name"}),
        ),
        (
            "tag-error",
            json!({"body":"return","sdk_failure":"daemon_tag"}),
        ),
        (
            "capture-error",
            json!({"body":"error","sdk_failure":"capture_exception"}),
        ),
        ("flush-error", json!({"body":"error","sdk_failure":"flush"})),
        ("logger-error", json!({"body":"error","close_logger":true})),
        (
            "logger-interrupt",
            json!({"body":"interrupt","close_logger":true}),
        ),
        (
            "params-open-error",
            json!({"body":"error","params_fault":"open"}),
        ),
        (
            "params-put-error",
            json!({"body":"error","params_fault":"put"}),
        ),
        (
            "already-sent",
            json!({"body":"error","sent":"1","prior":"previous","real_sdk":true}),
        ),
        (
            "false-sent",
            json!({"body":"error","sent":"true","prior":"previous"}),
        ),
        (
            "global-context-precedence",
            json!({"body":"error","global_daemon":"global-owner","daemon":"local-child","real_sdk":true}),
        ),
        (
            "unicode-daemon",
            json!({"body":"interrupt","daemon":"데몬😀"}),
        ),
    ]
}
/// The readiness checker has no case selector or runner remainder. Its
/// argparse grammar is only four filesystem paths, help and literal `--`.
pub fn parse_readiness(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    let parsed = scan_os_with_positionals(args, &[], 4)?;
    if parsed.help {
        return Ok(Command::Help);
    }
    Ok(Command::Run(Options {
        binary: resolve_non_strict(&cwd.join(&parsed.positionals[0]))?,
        collector: resolve_non_strict(&cwd.join(&parsed.positionals[1]))?,
        params_binding: resolve_non_strict(&cwd.join(&parsed.positionals[2]))?,
        output: resolve_non_strict(&cwd.join(&parsed.positionals[3]))?,
        case: None,
        runner: Vec::new(),
        environment: environment.clone(),
    }))
}

pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Command> {
    // A bare --runner consumes every following token, including --help. An
    // inline --runner=value consumes only that value, just as argparse does.
    let mut literal = false;
    let mut split = None;
    for (index, value) in args.iter().enumerate() {
        if value == "--" {
            literal = true;
        }
        if !literal
            && value
                .to_str()
                .is_some_and(|s| s.starts_with("--") && s.len() > 2 && "--runner".starts_with(s))
        {
            split = Some(index);
            break;
        }
    }
    let head = split.map_or(args, |index| &args[..index]);
    let parsed = scan_os_with_positionals(
        head,
        &[
            OptionSpec::value("--case", false),
            OptionSpec::value("--runner", false),
        ],
        4,
    )?;
    if parsed.help {
        return Ok(Command::Help);
    }
    let case = parsed
        .values
        .get("--case")
        .map(|v| {
            v.to_str()
                .ok_or("managed-entry case must be UTF-8")
                .map(str::to_owned)
        })
        .transpose()?;
    if case
        .as_ref()
        .is_some_and(|name| !cases().iter().any(|(known, _)| known == name))
    {
        return Err(InvalidCase {
            output: resolve_non_strict(&cwd.join(&parsed.positionals[3]))?,
            name: case.unwrap_or_default(),
        }
        .into());
    }
    let runner = split.map_or_else(
        || parsed.values.get("--runner").cloned().into_iter().collect(),
        |index| args[index + 1..].to_vec(),
    );
    Ok(Command::Run(Options {
        binary: resolve_non_strict(&cwd.join(&parsed.positionals[0]))?,
        collector: resolve_non_strict(&cwd.join(&parsed.positionals[1]))?,
        params_binding: resolve_non_strict(&cwd.join(&parsed.positionals[2]))?,
        output: resolve_non_strict(&cwd.join(&parsed.positionals[3]))?,
        case,
        runner,
        environment: environment.clone(),
    }))
}

/// Exact recursive exclusions in the original checker's normalized() function.
pub fn normalize(value: &Value) -> Result<Value> {
    match value {
        Value::Array(values) => Ok(Value::Array(
            values.iter().map(normalize).collect::<Result<_>>()?,
        )),
        Value::Object(fields) => {
            let capture = fields.get("op").is_some_and(|v| v == "capture_exception");
            let mut result = serde_json::Map::new();
            for (key, value) in fields {
                if [
                    "exception",
                    "traceback",
                    "message",
                    "kind",
                    "cmdline",
                    "comm",
                    "secondary",
                    "exception_type",
                ]
                .contains(&key.as_str())
                {
                    continue;
                }
                let value = if capture && key == "fields" {
                    json!({"captured_exception":true})
                } else if key == "value_json" {
                    serde_json::from_str(value.as_str().ok_or("value_json is not text")?)?
                } else {
                    normalize(value)?
                };
                result.insert(key.clone(), value);
            }
            Ok(Value::Object(result))
        }
        _ => Ok(value.clone()),
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| format!("missing managed-entry field {name}").into())
}
fn array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    field(value, name)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("managed-entry {name} is not an array").into())
}

/// Compare completed live executions, including the original diagnostic and
/// platform differences. Collector/disk/IPC ownership is checked during each run.
pub fn compare(source: &Value, native: &Value, scenario: &Value) -> Result<Value> {
    require(
        field(source, "exit_code")? == field(native, "exit_code")?,
        "managed-entry exit code differs",
    )?;
    require(
        field(source, "params")? == field(native, "params")?,
        "managed-entry Params differs",
    )?;
    require(
        field(source, "ready")? == field(native, "ready")?,
        "managed-entry readiness differs",
    )?;
    let left = field(source, "response")?;
    let right = field(native, "response")?;
    let outcome = field(right, "outcome")?;
    require(
        field(field(left, "outcome")?, "kind")? == field(outcome, "kind")?,
        "managed-entry outcome differs",
    )?;
    if outcome["kind"] != "interrupt_log_failed" {
        require(
            left["outcome"].get("stage") == outcome.get("stage"),
            "managed-entry failure stage differs",
        )?;
    }
    for name in ["steps", "sdk_calls"] {
        require(
            normalize(field(left, name)?)? == normalize(field(right, name)?)?,
            &format!("managed-entry {name} order or values differ"),
        )?;
    }
    let source_records = field(source, "records")?;
    let native_records = field(native, "records")?;
    for name in ["logMessage", "errorLogMessage"] {
        let expected = array(source_records, name)?;
        let actual = array(native_records, name)?;
        require(
            expected.len() == actual.len(),
            "managed-entry collector message count differs",
        )?;
        for (expected, actual) in expected.iter().zip(actual) {
            let mut context = field(actual, "ctx")?
                .as_object()
                .ok_or("managed-entry context is not an object")?
                .clone();
            for key in ["runtime_language", "source_commit", "source_tree"] {
                context.remove(key);
            }
            require(
                field(expected, "ctx")? == &Value::Object(context),
                "managed-entry log context differs",
            )?;
            for key in ["msg", "levelnum"] {
                require(
                    field(expected, key)? == field(actual, key)?,
                    "managed-entry log message or level differs",
                )?;
            }
            require(
                expected.get("exc_info").is_some() == actual.get("exc_info").is_some(),
                "managed-entry exception presence differs",
            )?;
            require(
                actual["ctx"]["runtime_language"] == "rust",
                "managed-entry native language tag missing",
            )?;
            if actual["msg"] == "crash" {
                let exception = outcome
                    .get("exception")
                    .or_else(|| outcome.get("original"))
                    .ok_or("native crash exception missing")?;
                let kind = field(exception, "kind")?
                    .as_str()
                    .ok_or("native exception kind is not text")?;
                let message = field(exception, "message")?
                    .as_str()
                    .ok_or("native exception message is not text")?;
                let info = field(actual, "exc_info")?
                    .as_str()
                    .ok_or("native exception information is not text")?;
                require(
                    info.starts_with(&format!("{kind}: {message}\n")),
                    "managed-entry crash does not describe the raised exception",
                )?;
                require(
                    exception
                        .get("backtrace")
                        .and_then(Value::as_str)
                        .is_some_and(|value| {
                            !["", "disabled backtrace", "unsupported backtrace"].contains(&value)
                        }),
                    "managed-entry native backtrace absent",
                )?;
            }
        }
    }
    let source_captures = array(source, "captures")?;
    let native_captures = array(native, "captures")?;
    require(
        source_captures.len() == native_captures.len(),
        "managed-entry SDK event count differs",
    )?;
    for (expected, actual) in source_captures.iter().zip(native_captures) {
        let expected = field(expected, "event")?;
        let actual = field(actual, "event")?;
        for key in ["tags", "user", "release", "environment"] {
            require(
                expected.get(key) == actual.get(key),
                &format!("managed-entry SDK {key} differs"),
            )?;
        }
        require(
            expected["platform"] == "python" && actual["platform"] == "native",
            "managed-entry SDK platform identity differs",
        )?;
        // The retained Python SDK is an explicitly captured external boundary
        // in the source port. Its selected fields are compared, but no source
        // HTTP request or exception payload is invented. Native HTTP remains
        // independently observed by the worker's local receiver.
        let source_projection = source.get("sdk_boundary").is_some();
        if source_projection {
            require(
                source["sdk_boundary"] == "captured-original-sdk-2.55.0-selected-fields",
                "unknown managed-entry source SDK boundary",
            )?;
        }
        for value in std::iter::once(actual).chain((!source_projection).then_some(expected)) {
            require(
                !array(field(value, "exception")?, "values")?.is_empty(),
                "managed-entry SDK exception absent",
            )?;
        }
        require(
            actual["extra"]
                .get("rust_backtrace")
                .and_then(Value::as_str)
                .is_some_and(|value| !["", "disabled backtrace"].contains(&value)),
            "managed-entry SDK Rust backtrace absent",
        )?;
    }
    let process = scenario
        .get("process")
        .and_then(Value::as_str)
        .unwrap_or("openpilot.fixture_managed_entry");
    if scenario.get("prepare").is_none() && !process.contains('\0') {
        let comm = process.chars().take(15).collect::<String>();
        require(
            field(left, "comm")? == &Value::String(comm.clone())
                && field(right, "comm")? == &Value::String(comm),
            "managed-entry process comm differs",
        )?;
        require(
            field(left, "cmdline")?
                .as_str()
                .is_some_and(|text| text.trim() == process),
            "managed-entry source process title differs",
        )?;
        require(
            field(right, "cmdline")?.as_str().is_some_and(|text| {
                text.contains("managed_child")
                    && !text.starts_with("openpilot.fixture_managed_entry")
            }),
            "managed-entry native command line lost executable identity",
        )?;
    }
    Ok(
        json!({"result":"PASS","exit_code":native["exit_code"],"params":native["params"],"log_messages":array(native_records,"logMessage")?.len(),"error_messages":array(native_records,"errorLogMessage")?.len(),"sdk_events":native_captures.len()}),
    )
}
