//! Full original Panda device transcript controller; no candidate policy imports.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
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
pub use cases::cases;
use ownership::{Bound, Publication};
pub const HELP: &str = "Usage: check-pandad-device --source SOURCE --native NATIVE --output OUTPUT [--runner REMAINDER...]\n";
pub const SCOPE: &str = "packed health fields/bits, short reads, failures, control order/parameters, firmware and serial bytes";
#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    pub source: PathBuf,
    pub native: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
}
pub fn parse(args: &[OsString], root: &Path) -> Result<Option<Options>> {
    // A recognized --runner (including its long abbreviations/inline first
    // value) consumes every subsequent token, exactly argparse.REMAINDER.
    let mut boundary = None;
    for (index, arg) in args.iter().enumerate() {
        let bytes = arg.as_bytes();
        if bytes == b"--" {
            break;
        }
        let name = bytes.split(|b| *b == b'=').next().unwrap_or(bytes);
        if name.len() > 2 && b"--runner".starts_with(name) {
            boundary = Some(index);
            break;
        }
    }
    let (prefix, runner) = if let Some(index) = boundary {
        let mut rest = Vec::new();
        if let Some(at) = args[index].as_bytes().iter().position(|b| *b == b'=') {
            rest.push(std::ffi::OsStr::from_bytes(&args[index].as_bytes()[at + 1..]).to_owned());
        }
        rest.extend_from_slice(&args[index + 1..]);
        (&args[..index], rest)
    } else {
        (args, vec![])
    };
    let parsed = scan_os(
        prefix,
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        root: root.into(),
        source: PathBuf::from(&parsed.values["--source"]),
        native: PathBuf::from(&parsed.values["--native"]),
        output: PathBuf::from(&parsed.values["--output"]),
        runner,
    }))
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted).into());
    }
    Ok(())
}
/// Remove exactly the one unspecified float field normalized by the original.
pub fn normalize(rows: &mut [Value]) -> Result<()> {
    for row in rows {
        let results = row
            .get_mut("results")
            .and_then(Value::as_array_mut)
            .ok_or("Panda device results are not an array")?;
        for result in results {
            if let Some(object) = result.as_object_mut() {
                if let Some(health) = object.get_mut("health") {
                    if health
                        .as_object_mut()
                        .ok_or("Panda health is not an object")?
                        .remove("interrupt_load")
                        .is_none()
                    {
                        return Err(
                            "Panda health interrupt_load missing before normalization".into()
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
pub fn compare(source: &[Value], native: &[Value], count: usize) -> Result<()> {
    if source.len() != count || native.len() != count {
        return Err("Panda device incomplete trace".into());
    }
    for (index, (expected, actual)) in source.iter().zip(native).enumerate() {
        if !equal(expected, actual)? {
            return Err(format!("Panda device differs at {index}").into());
        }
        if !equal(
            expected
                .get("remaining")
                .ok_or("Panda remaining count missing")?,
            &json!(0),
        )? {
            return Err(format!("Panda device unconsumed input at {index}").into());
        }
    }
    Ok(())
}
fn equal(left: &Value, right: &Value) -> Result<bool> {
    use crate::control_tools_policy_check::json;
    Ok(json::equal(
        &json::parse_text(&left.to_string())?,
        &json::parse_text(&right.to_string())?,
    ))
}
fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }
    let mut rows = text
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if rows.last() == Some(&"") {
        rows.pop();
    }
    rows
}
fn invoke(
    runner: &mut dyn Runner,
    argv: Vec<OsString>,
    payload: &[u8],
    output: &Path,
    name: &str,
    count: usize,
) -> Result<Vec<Value>> {
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(60));
    command.input = Some(payload.into());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let child = runner.run(&command)?;
    // Python text=True validates both streams before retaining either artifact.
    let text = |bytes| -> Result<String> {
        Ok(String::from_utf8(bytes)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"))
    };
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join(format!("{name}.jsonl")), &stdout)?;
    fs::write(output.join(format!("{name}.stderr")), &stderr)?;
    if child.code != 0 {
        return Err(format!("Panda device {name} exited {}", child.code).into());
    }
    let mut rows = lines(&stdout)
        .into_iter()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    if rows.len() != count {
        return Err(format!("Panda device {name} incomplete trace").into());
    }
    normalize(&mut rows)?;
    Ok(rows)
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
struct Pending {
    report: Value,
    proof: Publication,
    result: Publication,
}
impl Pending {
    fn commit(mut self) -> Result<Value> {
        self.proof.commit()?;
        self.result.commit()?;
        Ok(self.report)
    }
}
fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Pending> {
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::create_dir(&options.output)?;
    check_cancel(cancel)?;
    let pins = ownership::sources(&options.root)?;
    let source = Bound::open(&options.source);
    let native = Bound::open(&options.native);
    let source_exec = Bound::open(&executable(&options.source));
    let native_exec = Bound::open(&executable(
        options
            .runner
            .first()
            .map(Path::new)
            .unwrap_or(&options.native),
    ));
    let current = Bound::open(&std::env::current_exe()?)?;
    let corpus = cases();
    let payload = corpus
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    fs::write(options.output.join("input.jsonl"), &payload)?;
    let expected = invoke(
        runner,
        vec![options.source.clone().into_os_string()],
        payload.as_bytes(),
        &options.output,
        "source",
        corpus.len(),
    )?;
    check_cancel(cancel)?;
    let mut argv = options.runner.clone();
    argv.push(options.native.clone().into_os_string());
    let actual = invoke(
        runner,
        argv,
        payload.as_bytes(),
        &options.output,
        "native",
        corpus.len(),
    )?;
    if let Some(index) = expected
        .iter()
        .zip(&actual)
        .position(|(a, b)| !equal(a, b).unwrap_or(false))
    {
        fs::write(
            options.output.join("mismatch.json"),
            serde_json::to_vec_pretty(
                &json!({"index":index,"source":expected[index],"native":actual[index]}),
            )?,
        )?;
    }
    compare(&expected, &actual, corpus.len())?;
    // Original read_bytes() hash admission follows child success. Keep failures
    // at that phase; successful evidence additionally binds actual executables.
    let source = source?;
    let native = native?;
    let source_exec = source_exec?;
    let native_exec = native_exec?;
    if source.hash != source_exec.hash {
        return Err("Panda source PATH executable differs from supplied hash path".into());
    }
    let verify = || -> Result<()> {
        for bound in pins
            .iter()
            .chain([&source, &native, &source_exec, &native_exec, &current])
        {
            bound.verify()?;
        }
        check_cancel(cancel)
    };
    verify()?;
    let operations = corpus
        .iter()
        .map(|row| {
            row["operations"]
                .as_array()
                .expect("owned request roster")
                .len()
        })
        .sum::<usize>();
    let report = json!({"status":"PASS","scenarios":corpus.len(),"operations":operations,"scope":SCOPE,"source_sha256":source.hash,"native_sha256":native.hash});
    let source_hashes = ownership::PINS
        .iter()
        .zip(&pins)
        .map(|((name, _), pin)| ((*name).to_owned(), json!(pin.hash)))
        .collect::<serde_json::Map<_, _>>();
    let proof = json!({"source_sha256":source_hashes,"executing_checker_sha256":current.hash,"source_executable_sha256":source_exec.hash,"native_runner_executable_sha256":native_exec.hash,"timeout_seconds":60,"source_path_bytes":options.source.as_os_str().as_bytes(),"native_path_bytes":options.native.as_os_str().as_bytes(),"runner_bytes":options.runner.iter().map(|v|v.as_bytes().to_vec()).collect::<Vec<_>>()});
    serde_json::to_writer(&mut *stdout, &report)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    verify()?;
    let proof = Publication::write(&options.output.join("provenance.json"), &proof)?;
    let result = Publication::write(&options.output.join("report.json"), &report)?;
    verify()?;
    Ok(Pending {
        report,
        proof,
        result,
    })
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Value> {
    let pending = execute(options, runner, stdout, cancel)?;
    check_cancel(cancel)?;
    pending.commit()
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
    let cancel = Arc::new(AtomicBool::new(false));
    let mut handlers = Vec::new();
    let register = (|| -> Result<()> {
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            handlers.push(signal_hook::flag::register(signal, Arc::clone(&cancel))?);
        }
        Ok(())
    })();
    let result = register.and_then(|()| {
        execute(
            &options,
            &mut Controlled(&cancel),
            &mut io::stdout(),
            &cancel,
        )
    });
    for handler in handlers {
        signal_hook::low_level::unregister(handler);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted).into());
    }
    result?.commit().map(|_| ())
}
