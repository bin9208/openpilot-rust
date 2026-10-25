//! Independent navd configuration source policy and real Params/JWT QA controller.
//! Candidate navd policy and golden fixtures never participate in expected output.
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    registration_check::Keys,
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod cases;
mod cli;
mod identity;
mod jwt;
mod publication;
mod source;
mod values;
pub use cli::parse;
use values as v;
/// An opt-in, exclusive and bounded trace of controller startup phases.
/// The caller owns a fresh directory; ordinary invocations do no trace I/O.
pub fn startup_phase(name: &'static str) -> Result<()> {
    let Some(directory) = std::env::var_os("NAVD_CONFIG_STARTUP_TRACE") else {
        return Ok(());
    };
    static START: OnceLock<Instant> = OnceLock::new();
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("NAVD_CONFIG_STARTUP_TRACE must be absolute".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?;
    writeln!(
        file,
        "{}",
        START.get_or_init(Instant::now).elapsed().as_nanos()
    )?;
    Ok(())
}
#[derive(Clone)]
pub struct Case {
    pub name: String,
    pub input: JsonValue,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
pub struct Observation {
    pub normalized: JsonValue,
    pub raw: JsonValue,
    pub returncode: i32,
    pub public_keys: BTreeMap<String, String>,
}
pub fn cases() -> Result<Vec<Case>> {
    cases::cases()
}
pub fn validate_source(root: &Path) -> Result<JsonValue> {
    identity::sources(root)
}
fn seconds() -> Result<f64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}
pub(super) fn no_core() -> Result<()> {
    Ok(rustix::process::setrlimit(
        rustix::process::Resource::Core,
        rustix::process::Rlimit {
            current: Some(0),
            maximum: Some(0),
        },
    )?)
}
fn setup(
    case: &Case,
    output: &Path,
    keys: &Keys,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<(BTreeMap<OsString, OsString>, PathBuf)> {
    let prefix = format!("navdc_{}", std::process::id());
    let params = output.join("params").join(&prefix);
    let comma = output.join("persist/comma");
    fs::create_dir_all(&params)?;
    fs::create_dir_all(&comma)?;
    for (key, value) in v::fields(&v::field(&case.input, "params")?)? {
        if key.contains('/') || key == "." || key == ".." || key.contains('\0') {
            return Err("invalid Params fixture filename".into());
        }
        fs::write(params.join(key), v::bytes(&value)?)?;
    }
    let public = keys.public();
    if let Some(value) = case.input.get("keys") {
        let JsonView::Array(names) = value.view() else {
            return Err("expected key roster array".into());
        };
        for name in names {
            let kind = v::text(&name)?;
            let filename = match kind.as_str() {
                "rsa" => "id_rsa",
                "ec" => "id_ecdsa",
                _ => return Err("unknown QA key kind".into()),
            };
            let bad = v::flag(&case.input, "bad_private")
                || case.input.get("bad_key").is_some_and(|v| v.text_eq(&kind));
            let private = if bad {
                "bad key"
            } else {
                keys.private(&kind).ok_or("missing QA private key")?
            };
            fs::write(comma.join(filename), private)?;
            if !v::flag(&case.input, "no_public")
                && !case
                    .input
                    .get("remove_public")
                    .is_some_and(|v| v.text_eq(&kind))
            {
                fs::write(
                    comma.join(format!("{filename}.pub")),
                    public.get(&kind).ok_or("missing QA public key")?,
                )?;
            }
        }
    }
    let mut env = environment.clone();
    env.remove(OsStr::new("NAVD_CONFIG_STARTUP_TRACE"));
    env.insert("PARAMS_ROOT".into(), output.join("params").into_os_string());
    env.insert("OPENPILOT_PREFIX".into(), prefix.into());
    env.remove(OsStr::new("MAPBOX_TOKEN"));
    if let Some(token) = case.input.get("environment") {
        env.insert("MAPBOX_TOKEN".into(), v::path(&token)?.into_os_string());
    }
    Ok((env, comma.parent().ok_or("persist parent")?.to_owned()))
}
fn side(
    case: &Case,
    output: &Path,
    binary: &Path,
    source: bool,
    keys: &Keys,
    env: &BTreeMap<OsString, OsString>,
    cancel: &AtomicBool,
) -> Result<Observation> {
    let (env, persist) = setup(case, output, keys, env)?;
    let mut argv = vec![binary.as_os_str().to_owned()];
    if source {
        argv.push("--navd-config-source".into());
    }
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(15));
    spec.input =
        Some(format!("{}\n", crate::python_fs::json_string(persist.as_os_str())?).into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.set_os_env(env.clone());
    spec.env_remove = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| !env.contains_key(k))
        .collect();
    let before = seconds()?;
    let process =
        NativeRunner.run_controlled(&spec, Instant::now() + Duration::from_secs(15), cancel)?;
    let after = seconds()?;
    fs::write(output.join("stdout"), &process.stdout)?;
    fs::write(output.join("stderr"), &process.stderr)?;
    let raw = if process.code == 0 {
        JsonValue::parse(std::str::from_utf8(&process.stdout)?.trim())?
    } else {
        v::value(serde_json::json!({"ok":false,"returncode":process.code}))?
    };
    v::write(&output.join("record.json"), &raw)?;
    let public_keys = keys.public();
    let normalized = jwt::normalize(&raw, &public_keys, before, after)?;
    v::write(&output.join("normalized.json"), &normalized)?;
    v::write(&output.join("public-keys.json"), &v::value(&public_keys)?)?;
    Ok(Observation {
        normalized,
        raw,
        returncode: process.code,
        public_keys,
    })
}
/// Exercise arbitrary source input in its own process and real Params directory.
pub fn observe(case: &Case, root: &Path, output: &Path, worker: &Path) -> Result<Observation> {
    let sources = validate_source(root)?;
    let worker_id = identity::Artifact::open(worker)?;
    let keys = Keys::new()?;
    let observed = side(
        case,
        output,
        worker,
        true,
        &keys,
        &std::env::vars_os().collect(),
        &AtomicBool::new(false),
    )?;
    worker_id.verify(worker)?;
    if !v::equal(&sources, &validate_source(root)?) {
        return Err("navd source changed during observation".into());
    }
    Ok(observed)
}
pub fn run(options: &Options, worker: &Path) -> Result<JsonValue> {
    let (report, receipt) = run_inner(options, worker, &AtomicBool::new(false))?;
    receipt.commit()?;
    Ok(report)
}
fn interrupted(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::from(io::ErrorKind::Interrupted).into())
    } else {
        Ok(())
    }
}
fn candidate_sources(root: &Path) -> Result<JsonValue> {
    let mut pending = vec![root.join("rust/crates/navd")];
    let mut rows = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.file_type()?;
            if meta.is_dir() {
                pending.push(path)
            } else if (meta.is_file() || meta.is_symlink())
                && path.extension() == Some(OsStr::new("rs"))
            {
                let artifact = identity::Artifact::open(&path)?;
                rows.push((
                    crate::python_fs::json_value(path.strip_prefix(root)?.as_os_str()),
                    JsonValue::text(&artifact.sha256),
                ));
            }
        }
    }
    rows.sort_by_key(|row| row.0.to_json().unwrap_or_default());
    v::raw_object(rows)
}
fn run_inner(
    options: &Options,
    worker: &Path,
    cancel: &AtomicBool,
) -> Result<(JsonValue, publication::Receipt)> {
    startup_phase("05-run-inner-entry")?;
    // Admission owns no preexisting directory/receipt, including on invalid source.
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    startup_phase("06-output-created")?;
    no_core()?;
    startup_phase("07-no-core-done")?;
    interrupted(cancel)?;
    startup_phase("08-source-hash-begin")?;
    let sources = validate_source(&options.root)?;
    startup_phase("09-source-hash-end")?;
    startup_phase("10-worker-hash-begin")?;
    let worker_id = identity::Artifact::open(worker)?;
    startup_phase("11-worker-hash-end")?;
    startup_phase("12-binary-hash-begin")?;
    let binary_id = identity::Artifact::open(&options.binary);
    startup_phase("13-binary-hash-returned")?;
    startup_phase("14-candidate-hash-begin")?;
    let candidate = candidate_sources(&options.root)?;
    startup_phase("15-candidate-hash-end")?;
    startup_phase("16-rsa-generation-begin")?;
    let keys = Keys::new()?;
    startup_phase("17-rsa-generation-end")?;
    let mut rows = Vec::new();
    let cases = cases()?;
    startup_phase("18-first-case-begin")?;
    for case in cases {
        interrupted(cancel)?;
        let output = options.output.join(&case.name);
        let source = side(
            &case,
            &output.join("source"),
            worker,
            true,
            &keys,
            &options.environment,
            cancel,
        )?;
        let native = side(
            &case,
            &output.join("native"),
            &options.binary,
            false,
            &keys,
            &options.environment,
            cancel,
        )?;
        rows.push(v::value(serde_json::json!({"name":case.name,"passed":v::equal(&source.normalized,&native.normalized)}))?);
    }
    let binary_id = binary_id?;
    binary_id.verify(&options.binary)?;
    worker_id.verify(worker)?;
    if !v::equal(&sources, &validate_source(&options.root)?)
        || !v::equal(&candidate, &candidate_sources(&options.root)?)
    {
        return Err("navd sources changed during comparison".into());
    }
    interrupted(cancel)?;
    let passed = rows.iter().all(|r| v::flag(r, "passed"));
    let cases = JsonValue::parse(&format!(
        "[{}]",
        rows.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?;
    let mut files = v::raw_fields(&sources)?;
    files.extend(v::raw_fields(&candidate)?);
    files.push((
        JsonValue::text("executing_checker_sha256"),
        JsonValue::text(&worker_id.sha256),
    ));
    files.push((
        crate::python_fs::json_value(options.binary.as_os_str()),
        JsonValue::text(&binary_id.sha256),
    ));
    let report = v::object([
        (
            "status".into(),
            JsonValue::text(if passed { "PASS" } else { "FAIL" }),
        ),
        ("cases".into(), cases),
        ("files".into(), v::raw_object(files)?),
        (
            "historical_source_sha256".into(),
            identity::historical_sources()?,
        ),
    ])?;
    let failures = rows
        .iter()
        .filter(|r| !v::flag(r, "passed"))
        .filter_map(|r| r.get("name").and_then(|v| v.to_utf8()))
        .collect::<Vec<_>>();
    let summary = format!(
        "{}\n",
        v::object([
            (
                "status".into(),
                JsonValue::text(if passed { "PASS" } else { "FAIL" })
            ),
            ("cases".into(), v::value(rows.len())?),
            ("failures".into(), v::value(failures)?)
        ])?
        .to_json()?
    );
    {
        let mut stdout = io::stdout().lock();
        stdout.write_all(summary.as_bytes())?;
        stdout.flush()?;
    }
    interrupted(cancel)?;
    let bytes = format!(
        "{}\n",
        crate::control_tools_policy_check::json::pretty(&report, 0)?
    );
    let receipt =
        publication::Receipt::publish(&options.output.join("report.json"), bytes.as_bytes())?;
    interrupted(cancel)
        .and_then(|()| binary_id.verify(&options.binary))
        .and_then(|()| worker_id.verify(worker))
        .and_then(|()| {
            if v::equal(&sources, &validate_source(&options.root)?)
                && v::equal(&candidate, &candidate_sources(&options.root)?)
            {
                Ok(())
            } else {
                Err("navd sources changed during publication".into())
            }
        })?;
    receipt.verify()?;
    Ok((report, receipt))
}
pub fn cli() -> Result<()> {
    startup_phase("01-cli-entry")?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "--navd-config-source") {
        if args.len() != 1 {
            return Err("unexpected private source arguments".into());
        }
        return source::run();
    }
    startup_phase("02-parse-start")?;
    let command = parse(
        &args,
        &std::env::current_dir()?,
        std::env::vars_os().collect(),
    )?;
    startup_phase("03-parse-done")?;
    match command {
        Command::Help => {
            io::stdout().write_all(cli::HELP.as_bytes())?;
            io::stdout().flush()?;
        }
        Command::Run(options) => {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let signal = std::sync::Arc::new(AtomicUsize::new(0));
            let mut handlers = Vec::new();
            for value in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                handlers.push(signal_hook::flag::register(
                    value,
                    std::sync::Arc::clone(&cancel),
                )?);
                handlers.push(signal_hook::flag::register_usize(
                    value,
                    std::sync::Arc::clone(&signal),
                    value as usize,
                )?);
            }
            startup_phase("04-handlers-done")?;
            let result = run_inner(&options, &std::env::current_exe()?, &cancel);
            for handler in handlers {
                signal_hook::low_level::unregister(handler);
            }
            let observed_signal = signal.load(Ordering::Relaxed);
            if observed_signal != 0 {
                // Drop only our uncommitted receipt before restoring signal semantics.
                drop(result);
                signal_hook::low_level::emulate_default_handler(observed_signal as i32)?;
                return Err(io::Error::from(io::ErrorKind::Interrupted).into());
            }
            interrupted(&cancel)?;
            let (report, receipt) = result?;
            receipt.commit()?;
            if !report.get("status").is_some_and(|s| s.text_eq("PASS")) {
                return Err("navd configuration comparison failed".into());
            }
        }
    }
    Ok(())
}
