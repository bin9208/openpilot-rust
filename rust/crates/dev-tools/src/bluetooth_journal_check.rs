//! Fixed native journal QA with independent source policy and unchanged-source
//! CI validation. No candidate CommandWriter is used to form expected results.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod compare;
mod identity;
mod model;
pub use compare::{compare, normalize_trace, Normalized};
use identity::Bound;
pub use model::{operations, payload, source_trace, Operation, Time, OPERATIONS};
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
fn check_cancel(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::SeqCst) {
        Err("Bluetooth journal comparison interrupted".into())
    } else {
        Ok(())
    }
}
fn verify(files: &[Bound], historical_absent: &Path, stop: &AtomicBool) -> Result<()> {
    check_cancel(stop)?;
    if !fs::symlink_metadata(historical_absent.parent().ok_or("checker parent missing")?)?.is_dir()
    {
        return Err("Bluetooth journal checker parent is not a directory".into());
    }
    match fs::symlink_metadata(historical_absent) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err("Bluetooth journal historical checker reappeared".into()),
        Err(error) => return Err(error.into()),
    }
    for file in files {
        file.verify()?;
    }
    Ok(())
}
fn sources(root: &Path) -> Result<(Vec<Bound>, PathBuf)> {
    let pins: BTreeMap<String, String> = serde_json::from_str(include_str!(
        "../tests/fixtures/bluetooth-journal/source-pins.json"
    ))?;
    let mut live = Vec::new();
    let mut historical_absent = None;
    for (name, expected) in pins {
        let path = root.join(&name);
        if name == "rust/tools/check_bluetooth_journal.py" {
            // The retired checker must be absent from admission onward.
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if !fs::symlink_metadata(path.parent().ok_or("checker parent missing")?)?
                        .is_dir()
                    {
                        return Err("Bluetooth journal checker parent is not a directory".into());
                    }
                    historical_absent = Some(path);
                    continue;
                }
                Ok(_) => return Err("Bluetooth journal historical checker reappeared".into()),
                Err(error) => return Err(error.into()),
            }
        }
        let file = Bound::open(&path)?;
        if file.hash != expected {
            return Err(format!("Bluetooth journal source changed: {name}").into());
        }
        live.push(file);
    }
    Ok((
        live,
        historical_absent.ok_or("Bluetooth journal historical source pin missing")?,
    ))
}
pub fn parse_trace(text: &str) -> Result<Vec<JsonValue>> {
    // Python str.splitlines treats CRLF as one boundary. Public callers may
    // supply raw text, even though subprocess text decoding already folds it.
    let text = text.replace("\r\n", "\n");
    let mut lines = text
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
            '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .into_iter()
        .map(crate::control_tools_policy_check::json::parse_text)
        .collect()
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn names(path: &Path) -> Result<()> {
    let mut names = fs::read_dir(path)?
        .map(|r| r.map(|r| r.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    names.sort();
    if names
        != [
            std::ffi::OsString::from("cruise.json"),
            std::ffi::OsString::from("lane.json"),
        ]
    {
        return Err("journal final directory names/temp cleanup differs".into());
    }
    Ok(())
}
fn publish(
    path: &Path,
    report: &Value,
    inputs: &[Bound],
    historical_absent: &Path,
    stop: &AtomicBool,
) -> Result<()> {
    verify(inputs, historical_absent, stop)?;
    let mut pending = tempfile::NamedTempFile::new_in(path.parent().ok_or("report parent")?)?;
    pending.write_all(serde_json::to_string_pretty(report)?.as_bytes())?;
    verify(inputs, historical_absent, stop)?;
    let held = pending.persist_noclobber(path).map_err(|e| e.error)?;
    let result = verify(inputs, historical_absent, stop);
    if result.is_err() {
        if let (Ok(own), Ok(now)) = (held.metadata(), fs::symlink_metadata(path)) {
            if (own.dev(), own.ino()) == (now.dev(), now.ino()) {
                let _ = fs::remove_file(path);
            }
        }
    }
    result
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    stop: &AtomicBool,
) -> Result<Value> {
    let (mut inputs, historical_absent) = sources(&options.root)?;
    verify(&inputs, &historical_absent, stop)?;
    fs::create_dir_all(&options.output)?;
    let operations = operations();
    if operations.len() != OPERATIONS {
        return Err("source operation schedule differs".into());
    }
    let payload = payload(&operations)?;
    fs::write(options.output.join("input.jsonl"), &payload)?;
    let source = source_trace(&operations)?;
    fs::write(
        options.output.join("original.jsonl"),
        source
            .iter()
            .map(|r| r.to_json().map(|s| s + "\n"))
            .collect::<std::result::Result<String, _>>()?,
    )?;
    let original = options.output.join("original");
    fs::create_dir_all(&original)?;
    let JsonView::Array(files) = source.last().ok_or("empty source trace")?.view() else {
        return Err("source snapshot not array".into());
    };
    for file in files {
        let channel = file
            .get("channel")
            .and_then(|v| v.to_utf8())
            .ok_or("source channel")?;
        let path = original.join(format!("{channel}.json"));
        fs::write(
            &path,
            file.get("bytes")
                .and_then(|v| v.to_utf8())
                .ok_or("source bytes")?,
        )?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let native = resolve_non_strict(&options.output)?.join("native");
    let binary = resolve_non_strict(&options.binary)?;
    let argument = if options.binary.is_absolute() {
        options.binary.clone()
    } else {
        std::env::current_dir()?.join(&options.binary)
    };
    let artifact = Bound::open(&binary)?;
    let alias = Bound::open(&argument)?;
    let mut command = CommandSpec::new_os(
        vec![binary.into_os_string(), native.as_os_str().into()],
        Duration::from_secs(30),
    );
    command.input = Some(payload);
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    verify(&inputs, &historical_absent, stop)?;
    let observed = runner.run(&command)?;
    let stdout = text(observed.stdout)?;
    let stderr = text(observed.stderr)?;
    fs::write(options.output.join("native.jsonl"), &stdout)?;
    fs::write(options.output.join("native.stderr"), stderr)?;
    if observed.code != 0 {
        return Err(format!("Bluetooth journal fixture exited{}", observed.code).into());
    }
    let actual = parse_trace(&stdout)?;
    compare(&source, &actual)?;
    names(&original)?;
    names(&native)?;
    artifact.verify()?;
    alias.verify()?;
    let hash = artifact.hash.clone();
    inputs.push(artifact);
    inputs.push(alias);
    verify(&inputs, &historical_absent, stop)?;
    let pins = inputs
        .iter()
        .map(|f| {
            let path = f
                .path
                .to_str()
                .map(|s| json!(s))
                .unwrap_or_else(|| json!({"Unix":f.path.as_os_str().as_bytes()}));
            json!({"path":path,"sha256":f.hash})
        })
        .collect::<Vec<_>>();
    let report = json!({"operations":OPERATIONS,"max_events_per_channel":64,"mode":"0600","values_and_rewrite_decisions":"exact after UUID session normalization","source_sha256":"74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633","binary_sha256":hash,"reference_kind":"independent fixed CommandWriter source policy; UUID/inode normalization; unchanged-original CI required","input_sha256":pins});
    writeln!(printed, "{}", serde_json::to_string(&report)?)?;
    printed.flush()?;
    publish(
        &options.output.join("result.json"),
        &report,
        &inputs,
        &historical_absent,
        stop,
    )?;
    Ok(report)
}
struct SignalRunner(Arc<AtomicBool>);
impl Runner for SignalRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, &self.0)
    }
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-bluetooth-journal --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: args.values["--binary"].clone().into(),
        output: args.values["--output"].clone().into(),
    };
    check(
        &options,
        &mut SignalRunner(stop.clone()),
        &mut std::io::stdout(),
        &stop,
    )?;
    Ok(())
}
