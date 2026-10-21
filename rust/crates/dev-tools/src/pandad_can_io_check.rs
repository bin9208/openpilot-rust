//! Original Panda bulk CAN transport QA controller, without physical transports.
use crate::{
    argparse_scan::{scan_os_required_first, ArgumentError, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod cases;
mod ownership;
pub use cases::{cases, packets, seeds, sent};
use ownership::{Bound, Publication};
pub const HELP:&str="Usage: check-pandad-can-io --source SOURCE --native NATIVE --output OUTPUT [--runner REMAINDER...]\n";
pub const SCOPE:&str="exact bulk/control ordering, endpoint, timeout, bytes, partial frames, health gate and checksum reset";
pub const EXCLUDED: &str =
    "healthy negative receive count with MAXOUT would exceed its fixed junk buffer";
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub source: PathBuf,
    pub native: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
}
#[derive(serde::Serialize)]
pub struct Report {
    #[serde(flatten)]
    fields: Value,
    native_command: Box<serde_json::value::RawValue>,
}
impl Report {
    pub fn json_value(&self) -> Result<openpilot_log_format::JsonValue> {
        Ok(openpilot_log_format::JsonValue::parse(
            &serde_json::to_string(self)?,
        )?)
    }
}
pub fn parse(args: &[OsString], root: &Path) -> Result<Option<Options>> {
    let mut boundary = None;
    for (index, arg) in args.iter().enumerate() {
        let bytes = arg.as_bytes();
        if bytes == b"--" {
            break;
        }
        let name = bytes.split(|b| *b == b'=').next().unwrap_or(bytes);
        // argparse consumes only the explicit value in --runner=VALUE; later
        // options still act normally. A bare runner consumes the remainder.
        if name.len() > 2 && b"--runner".starts_with(name) && !bytes.contains(&b'=') {
            boundary = Some(index);
            break;
        }
    }
    let (prefix, remainder) = if let Some(index) = boundary {
        (&args[..index], Some(args[index + 1..].to_vec()))
    } else {
        (args, None)
    };
    let parsed = scan_os_required_first(
        prefix,
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--runner", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let runner = if let Some(remainder) = remainder {
        // argparse's optional REMAINDER excludes the standalone option
        // separator. Preserve earlier help actions; an attached value of "--"
        // is a literal runner value and does not enter this branch.
        if remainder.iter().any(|value| value == "--") {
            return Err(ArgumentError("unrecognized arguments: --".into()).into());
        }
        remainder
    } else {
        parsed.values.get("--runner").cloned().into_iter().collect()
    };
    Ok(Some(Options {
        root: root.into(),
        source: PathBuf::from(&parsed.values["--source"]),
        native: PathBuf::from(&parsed.values["--native"]),
        output: PathBuf::from(&parsed.values["--output"]),
        runner,
    }))
}
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted).into());
    }
    Ok(())
}
fn executable(path: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    if path.as_os_str().as_bytes().contains(&b'/') {
        return path.into();
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(path))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .unwrap_or_else(|| path.into())
}
fn payload(rows: &[Value]) -> String {
    rows.iter().map(|row| format!("{row}\n")).collect()
}
fn run(
    runner: &mut dyn Runner,
    argv: &[OsString],
    rows: &[Value],
    maxout: bool,
    output: &Path,
    label: &str,
    flag: &AtomicBool,
) -> Result<Vec<Value>> {
    cancelled(flag)?;
    let input = payload(rows);
    fs::write(output.join(format!("{label}.input.jsonl")), &input)?;
    let mut spec = CommandSpec::new_os(argv.to_vec(), Duration::from_secs(90));
    spec.input = Some(input.into_bytes());
    spec.stdout_limit = 128 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    if maxout {
        spec.os_env = Some([("PANDAD_MAXOUT".into(), "1".into())].into_iter().collect());
    } else {
        spec.env_remove.push("PANDAD_MAXOUT".into());
    }
    let child = runner.run(&spec)?;
    let text = |bytes| -> Result<String> {
        Ok(String::from_utf8(bytes)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"))
    };
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join(format!("{label}.jsonl")), &stdout)?;
    fs::write(output.join(format!("{label}.stderr")), &stderr)?;
    if child.code != 0 {
        return Err(format!("Panda CAN-I/O {label} exited {}", child.code).into());
    }
    let values = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    if values.len() != rows.len() {
        return Err("Panda CAN-I/O incomplete trace".into());
    }
    if values
        .iter()
        .any(|row| row.get("unused_replies") != Some(&json!(0)))
    {
        return Err("Panda CAN-I/O unconsumed reply".into());
    }
    cancelled(flag)?;
    Ok(values)
}
struct Pending {
    report: Report,
    proof: Publication,
    result: Publication,
    pins: ownership::Sources,
    bounds: Vec<Bound>,
}
impl Pending {
    fn commit(mut self, flag: &AtomicBool) -> Result<Report> {
        self.pins.verify()?;
        for bound in &self.bounds {
            bound.verify()?;
        }
        cancelled(flag)?;
        self.proof.commit()?;
        self.result.commit()?;
        Ok(self.report)
    }
}
fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    flag: &AtomicBool,
) -> Result<Pending> {
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let pins = ownership::sources(&options.root)?;
    let source_command = vec![resolve_non_strict(&options.source)?.into_os_string()];
    let mut native_command = options.runner.clone();
    native_command.push(resolve_non_strict(&options.native)?.into_os_string());
    let source = Bound::open(&options.source);
    let source_exec = Bound::open(Path::new(&source_command[0]));
    let native = Bound::open(&options.native);
    let native_exec = Bound::open(&resolve_non_strict(&options.native)?);
    let executor = Bound::open(&executable(Path::new(&native_command[0])));
    let current = Bound::open(&std::env::current_exe()?)?;
    let encoded = run(
        runner,
        &source_command,
        &seeds(),
        false,
        &options.output,
        "seeds",
        flag,
    )?;
    let packets = packets(&encoded)?;
    let mut summaries = Vec::new();
    for maxout in [false, true] {
        let output = options
            .output
            .join(if maxout { "maxout" } else { "normal" });
        fs::create_dir(&output)?;
        let corpus = cases(&packets, maxout)?;
        let expected = run(
            runner,
            &source_command,
            &corpus,
            maxout,
            &output,
            "source",
            flag,
        )?;
        let actual = run(
            runner,
            &native_command,
            &corpus,
            maxout,
            &output,
            "native",
            flag,
        )?;
        if let Some(index) = expected
            .iter()
            .zip(&actual)
            .position(|(want, got)| want != got)
        {
            fs::write(
                output.join("mismatch.json"),
                serde_json::to_vec_pretty(
                    &json!({"index":index,"input":corpus[index],"source":expected[index],"native":actual[index]}),
                )?,
            )?;
            return Err(format!("CAN I/O differs at {index}, maxout={maxout}").into());
        }
        summaries.push(json!({"maxout":maxout,"scenarios":corpus.len(),"operations":corpus.iter().map(|row|row["operations"].as_array().map_or(0,Vec::len)).sum::<usize>()}));
    }
    let (source, source_exec, native, native_exec, executor) =
        (source?, source_exec?, native?, native_exec?, executor?);
    if source.hash != source_exec.hash || native.hash != native_exec.hash {
        return Err("CAN-I/O supplied probe differs from executed path".into());
    }
    let verify = || -> Result<()> {
        pins.verify()?;
        for bound in [
            &source,
            &source_exec,
            &native,
            &native_exec,
            &executor,
            &current,
        ] {
            bound.verify()?;
        }
        cancelled(flag)
    };
    verify()?;
    let fields = json!({"result":"PASS","runs":summaries,"source_sha256":source.hash,"native_sha256":native.hash,"scope":SCOPE,"excluded_source_invalid_contract":EXCLUDED});
    let report = Report {
        fields,
        native_command: crate::pandad_protocol_check::command_json(&native_command)?,
    };
    let proof = json!({"source_sha256":pins.hashes(),"historical_checker_absent":pins.absent(),"historical_corpus_absent":pins.corpus_absent(),"executing_checker_sha256":current.hash,"source_executable_sha256":source_exec.hash,"native_runner_executable_sha256":executor.hash,"timeout_seconds":90});
    let proof = Publication::write(&options.output.join("provenance.json"), &proof)?;
    let result = Publication::write(&options.output.join("report.json"), &report)?;
    verify()?;
    serde_json::to_writer(&mut *stdout, &report)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    verify()?;
    Ok(Pending {
        report,
        proof,
        result,
        pins,
        bounds: vec![source, source_exec, native, native_exec, executor, current],
    })
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    flag: &AtomicBool,
) -> Result<Report> {
    execute(options, runner, stdout, flag)?.commit(flag)
}
struct Controlled<'a>(&'a AtomicBool);
impl Runner for Controlled<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, self.0)
    }
}
pub fn cli() -> Result<()> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    let Some(options) = parse(&std::env::args_os().skip(1).collect::<Vec<_>>(), &root)? else {
        io::stdout().write_all(HELP.as_bytes())?;
        return Ok(());
    };
    let flag = Arc::new(AtomicBool::new(false));
    let mut handlers = Vec::new();
    let registration = (|| -> Result<()> {
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            handlers.push(signal_hook::flag::register(signal, Arc::clone(&flag))?);
        }
        Ok(())
    })();
    let pending = registration
        .and_then(|()| execute(&options, &mut Controlled(&flag), &mut io::stdout(), &flag));
    for handler in handlers {
        signal_hook::low_level::unregister(handler);
    }
    cancelled(&flag)?;
    pending?.commit(&flag).map(|_| ())
}
