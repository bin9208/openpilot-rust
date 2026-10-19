//! Source-derived registration QA, real signed HTTP/Params/collector comparison.
//! No runtime registration policy or expected fixture participates in the oracle.
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
mod cases;
mod cli;
mod encoding;
mod gate;
mod http;
mod identity;
mod keys;
#[cfg(feature = "navd-config-tools-native")]
pub(crate) use keys::Keys;
mod process;
pub mod reference;
mod server;
mod source;
mod values;
mod worker;
use values as v;

#[derive(Clone, Copy, Debug)]
pub enum Suite {
    All,
    Hardware,
    Timeouts,
}
#[derive(Clone)]
pub struct Case {
    pub name: String,
    pub input: JsonValue,
}
#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
    pub case: Option<String>,
    pub suite: Suite,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug, Clone)]
pub struct ReferenceOptions {
    pub root: PathBuf,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
pub use cli::parse;
pub fn cases(suite: Suite) -> Result<Vec<Case>> {
    cases::cases(suite)
}
pub fn validate_source(root: &Path) -> Result<JsonValue> {
    identity::sources(root)
}

fn spec(
    worker: &Path,
    mode: &str,
    request: JsonValue,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<CommandSpec> {
    let mut spec = CommandSpec::new_os(
        vec![worker.as_os_str().to_owned(), mode.into()],
        Duration::from_secs(200),
    );
    spec.input = Some((request.to_json()? + "\n").into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.set_os_env(environment.clone());
    spec.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| !environment.contains_key(key))
        .collect();
    Ok(spec)
}
fn worker_reply(captured: crate::qa_process::Captured) -> Result<JsonValue> {
    if captured.code != 0 {
        return Err(format!(
            "registration side worker exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    let stdout = std::str::from_utf8(&captured.stdout)?;
    let mut lines = stdout.lines();
    let value = JsonValue::parse(
        lines
            .next()
            .ok_or("registration side worker produced no result")?,
    )?;
    if lines.next().is_some() {
        return Err("registration side worker produced extra output".into());
    }
    Ok(value)
}
fn request(
    options: &ReferenceOptions,
    case: &Case,
    binary: Option<(&Path, &[OsString])>,
) -> Result<JsonValue> {
    let mut fields = vec![
        ("root".into(), v::path_value(&options.root)),
        ("output".into(), v::path_value(&options.output)),
        ("name".into(), v::text(&case.name)),
        ("case".into(), case.input.clone()),
    ];
    if let Some((binary, runner)) = binary {
        fields.push(("binary".into(), v::path_value(binary)));
        fields.push((
            "runner".into(),
            v::array(runner.iter().map(|v| crate::python_fs::json_value(v)))?,
        ));
    }
    v::object(fields)
}
fn interrupted(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into())
    } else {
        Ok(())
    }
}
/// Compare actual original-derived and candidate processes. The worker path is
/// explicit so library users never accidentally execute their integration-test ELF.
pub fn run(options: &Options, worker_elf: &Path) -> Result<JsonValue> {
    run_inner(options, worker_elf, &AtomicBool::new(false))
}
fn run_inner(options: &Options, worker_elf: &Path, cancel: &AtomicBool) -> Result<JsonValue> {
    let source = validate_source(&options.root)?;
    interrupted(cancel)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?
    }
    fs::create_dir(&options.output)?;
    let worker = identity::Artifact::open(worker_elf)?;
    // Failed/nonregular candidate fingerprints are deferred until AFTER its real
    // execution phase, matching source-before-native constructor/spawn failures.
    let binary = identity::Artifact::open(&options.binary);
    let mut rows = Vec::new();
    for case in cases(options.suite)? {
        if options.case.as_ref().is_some_and(|name| name != &case.name) {
            continue;
        }
        interrupted(cancel)?;
        let output = options.output.join(&case.name);
        fs::create_dir(&output)?;
        let reference = ReferenceOptions {
            root: options.root.clone(),
            output: output.clone(),
            environment: options.environment.clone(),
        };
        // These private IPC roots outlive the worker and are cleaned even when
        // the outer process-group owner must kill an interrupted side process.
        let source_ipc = worker::Namespace::new()?;
        let native_ipc = worker::Namespace::new()?;
        let mut call = v::fields(&request(
            &reference,
            &case,
            Some((&options.binary, &options.runner)),
        )?)?;
        call.insert(
            "namespaces".into(),
            v::object([
                ("source".into(), source_ipc.descriptor()?),
                ("native".into(), native_ipc.descriptor()?),
            ])?,
        );
        let captured = NativeRunner.run_controlled(
            &spec(
                worker_elf,
                "--registration-pair",
                v::object(call)?,
                &options.environment,
            )?,
            std::time::Instant::now() + Duration::from_secs(200),
            cancel,
        )?;
        let row = worker_reply(captured)?;
        v::write(&output.join("comparison.json"), &row)?;
        rows.push(row);
        println!("{} PASS", case.name);
    }
    if rows.is_empty() {
        return Err("registration selection produced no cases".into());
    }
    interrupted(cancel)?;
    let binary = binary?;
    binary.verify(&options.binary)?;
    worker.verify(worker_elf)?;
    if !v::equal(&validate_source(&options.root)?, &source) {
        return Err("registration sources changed during execution".into());
    }
    let report = v::object([
        ("result".into(), v::text("PASS")),
        ("binary_sha256".into(), v::text(&binary.sha256)),
        ("cases".into(), v::array(rows)?),
    ])?;
    let provenance = v::object([
        ("source_sha256".into(), source.clone()),
        (
            "historical_source_sha256".into(),
            identity::historical_sources()?,
        ),
        ("checker_sha256".into(), v::text(&worker.sha256)),
    ])?;
    v::write(&options.output.join("provenance.json"), &provenance)?;
    v::write(&options.output.join("result.json"), &report)?;
    if let Err(error) = interrupted(cancel)
        .and_then(|()| binary.verify(&options.binary))
        .and_then(|()| worker.verify(worker_elf))
        .and_then(|()| {
            validate_source(&options.root).and_then(|current| {
                if v::equal(&current, &source) {
                    Ok(())
                } else {
                    Err("registration sources changed during publication".into())
                }
            })
        })
    {
        let _ = fs::remove_file(options.output.join("result.json"));
        return Err(error);
    }
    Ok(report)
}
/// Public command plus private isolated worker dispatch, never an interpreter.
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(mode) = args.first().and_then(|v| v.to_str()) {
        if mode.starts_with("--registration-") {
            return worker::dispatch(mode, &args[1..]);
        }
    }
    let command = parse(
        &args,
        &std::env::current_dir()?,
        std::env::vars_os().collect(),
    )?;
    match command {
        Command::Help => print!("{}", cli::HELP),
        Command::Run(options) => {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let interrupt = signal_hook::flag::register(
                signal_hook::consts::SIGINT,
                std::sync::Arc::clone(&cancel),
            )?;
            let terminate = signal_hook::flag::register(
                signal_hook::consts::SIGTERM,
                std::sync::Arc::clone(&cancel),
            )?;
            let result = run_inner(&options, &std::env::current_exe()?, &cancel);
            signal_hook::low_level::unregister(interrupt);
            signal_hook::low_level::unregister(terminate);
            if result.is_ok() && cancel.load(Ordering::Relaxed) {
                let _ = fs::remove_file(options.output.join("result.json"));
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
            }
            result?;
        }
    }
    Ok(())
}
