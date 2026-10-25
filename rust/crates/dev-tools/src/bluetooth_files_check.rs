//! Native fixed atomic-JSON comparison. The reference encoder is independent
//! of Bluetooth's serializer; CI seals it against independently observed Python.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json as python_json,
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
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
mod corpus;
mod identity;
pub use corpus::{cases, payload, Case, CASES, SCALARS};
use identity::{digest, Bound};
const HISTORICAL_CHECKER: &str = "rust/tools/check_bluetooth_files.py";
const HISTORICAL_SHA: &str = "e30b4cdca8c658cfbd713b6603ef23c64d21854ff41ffa1f611bdd43d2371682";
const MODEL_SOURCE: &str = "openpilot/selfdrive/carrot/bluetooth/model.py";
const MODEL_SHA: &str = "74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633";
const FIXTURE_SOURCE: &str = "rust/crates/bluetooth/examples/file_fixture.rs";
const FIXTURE_SHA: &str = "fac81b03cc32d0a26515b3ee7213e80d8c6fe392bdb0670434c09337f1d20d93";
struct Sources {
    live: Vec<Bound>,
    checker: Option<Bound>,
    checker_path: PathBuf,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
fn check_cancel(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::SeqCst) {
        Err("Bluetooth files comparison interrupted".into())
    } else {
        Ok(())
    }
}
fn source_inputs(root: &Path) -> Result<Sources> {
    let pins: BTreeMap<String, String> = serde_json::from_str(include_str!(
        "../tests/fixtures/bluetooth-files/source-pins.json"
    ))?;
    if pins.len() != 3
        || pins.get(HISTORICAL_CHECKER).map(String::as_str) != Some(HISTORICAL_SHA)
        || pins.get(MODEL_SOURCE).map(String::as_str) != Some(MODEL_SHA)
        || pins.get(FIXTURE_SOURCE).map(String::as_str) != Some(FIXTURE_SHA)
    {
        return Err("Bluetooth files source pin roster differs".into());
    }
    let checker_path = root.join(HISTORICAL_CHECKER);
    let mut sources = Sources {
        live: vec![],
        checker: None,
        checker_path,
    };
    for (name, expected) in pins {
        if name == HISTORICAL_CHECKER {
            // Only this retired helper may be absent. A dangling symlink is
            // present to symlink_metadata and must fail the regular-file guard.
            match fs::symlink_metadata(&sources.checker_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
                Ok(metadata) if metadata.file_type().is_file() => {}
                Ok(_) => return Err("Bluetooth historical checker is not a regular file".into()),
            }
            let file = Bound::open(&sources.checker_path)?;
            if file.hash != expected {
                return Err(format!("Bluetooth files source changed: {name}").into());
            }
            sources.checker = Some(file);
        } else {
            let file = Bound::open(&root.join(&name))?;
            if file.hash != expected {
                return Err(format!("Bluetooth files source changed: {name}").into());
            }
            sources.live.push(file);
        }
    }
    Ok(sources)
}
fn verify(files: &Sources, stop: &AtomicBool) -> Result<()> {
    check_cancel(stop)?;
    for file in &files.live {
        file.verify()?;
    }
    if let Some(checker) = &files.checker {
        checker.verify()?;
    } else {
        match fs::symlink_metadata(&files.checker_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => return Err("Bluetooth historical checker appeared during comparison".into()),
        }
    }
    Ok(())
}
fn newlines(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn files_count(directory: &Path) -> Result<usize> {
    Ok(fs::read_dir(directory)?
        .collect::<std::io::Result<Vec<_>>>()?
        .len())
}
fn same_file(path: &Path, expected: &[u8], mode: u32) -> Result<()> {
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(fd);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.mode() & 0o777 != mode {
        return Err(format!("Bluetooth file type/mode differs: {}", path.display()).into());
    }
    // Bound reads to expected full bytes+one, so an oversized file cannot
    // exhaust memory. No content truncation is accepted.
    let mut bytes = Vec::new();
    (&mut file)
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    let now = fs::metadata(path)?;
    if bytes != expected
        || (
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ) != (
            now.dev(),
            now.ino(),
            now.len(),
            now.mtime(),
            now.mtime_nsec(),
            now.ctime(),
            now.ctime_nsec(),
        )
    {
        return Err(format!("Bluetooth file bytes/identity differs: {}", path.display()).into());
    }
    Ok(())
}
fn publish(path: &Path, report: &Value, files: &Sources, stop: &AtomicBool) -> Result<()> {
    verify(files, stop)?;
    let mut pending =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("report parent missing")?)?;
    pending.write_all(serde_json::to_string_pretty(report)?.as_bytes())?;
    verify(files, stop)?;
    let held = pending
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    let result = verify(files, stop);
    if result.is_err() {
        if let (Ok(own), Ok(now)) = (held.metadata(), fs::symlink_metadata(path)) {
            if (own.dev(), own.ino()) == (now.dev(), now.ino()) {
                let _ = fs::remove_file(path);
            }
        }
    }
    result
}
/// Reuses the shared owned command runner; tests obtain their expected files
/// from the sealed original observation manifest, never from the candidate.
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    stop: &AtomicBool,
) -> Result<Value> {
    let mut files = source_inputs(&options.root)?;
    verify(&files, stop)?;
    fs::create_dir_all(&options.output)?;
    let cases = cases();
    let input = payload(&cases);
    fs::write(options.output.join("input.jsonl"), &input)?;
    let source = options.output.join("original");
    fs::create_dir(&source)?;
    for (index, case) in cases.iter().enumerate() {
        let path = source.join(format!("{index}.json"));
        fs::write(&path, &case.expected)?;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if case.succeeded { 0o600 } else { 0o644 }),
        )?;
    }
    let native_root = resolve_non_strict(&options.output)?.join("native");
    let binary = resolve_non_strict(&options.binary)?;
    let original_argument = if options.binary.is_absolute() {
        options.binary.clone()
    } else {
        std::env::current_dir()?.join(&options.binary)
    };
    let artifact = Bound::open(&binary)?;
    let alias = Bound::open(&original_argument)?;
    let mut command = CommandSpec::new_os(
        vec![binary.into_os_string(), native_root.as_os_str().into()],
        Duration::from_secs(30),
    );
    command.input = Some(input);
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    verify(&files, stop)?;
    let observed = runner.run(&command)?;
    let stdout = newlines(observed.stdout)?;
    let stderr = newlines(observed.stderr)?;
    fs::write(options.output.join("native.stdout"), &stdout)?;
    fs::write(options.output.join("native.stderr"), stderr)?;
    if observed.code != 0 {
        return Err(format!("Bluetooth file fixture exited {}", observed.code).into());
    }
    let mut lines = stdout
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    if lines.len() != CASES {
        return Err("Bluetooth file result cardinality differs".into());
    }
    let mut sizes = Vec::new();
    let mut hashes = Vec::new();
    for (index, (case, line)) in cases.iter().zip(lines).enumerate() {
        let expected = JsonValue::parse(if case.succeeded { "true" } else { "false" })?;
        let actual = python_json::parse_text(line)?;
        if !python_json::equal(&expected, &actual) {
            return Err(format!("Bluetooth file success differs: {index}").into());
        }
        let mode = if case.succeeded { 0o600 } else { 0o644 };
        same_file(&source.join(format!("{index}.json")), &case.expected, mode)?;
        same_file(
            &native_root.join(format!("{index}.json")),
            &case.expected,
            mode,
        )?;
        sizes.push(case.expected.len());
        hashes.push(json!({"index":index,"sha256":digest(&case.expected),"bytes":case.expected.len(),"mode":mode,"succeeded":case.succeeded}));
    }
    if files_count(&source)? != CASES || files_count(&native_root)? != CASES {
        return Err("Bluetooth file count or temporary cleanup differs".into());
    }
    artifact.verify()?;
    alias.verify()?;
    let artifact_hash = artifact.hash.clone();
    files.live.push(artifact);
    files.live.push(alias);
    verify(&files, stop)?;
    let pins = files
        .live
        .iter()
        .map(|file| {
            let path = file
                .path
                .to_str()
                .map(|s| json!(s))
                .unwrap_or_else(|| json!({"Unix":file.path.as_os_str().as_bytes()}));
            json!({"path":path,"sha256":file.hash})
        })
        .collect::<Vec<_>>();
    let report = json!({"cases":CASES,"all_unicode_scalar_values":SCALARS,"unicode_errors":cases.iter().filter(|c|!c.succeeded).count(),"bytes":sizes.iter().sum::<usize>(),"comparison":"exact bytes and permissions","source_sha256":MODEL_SHA,"binary_sha256":artifact_hash,"reference_kind":"independent fixed source schedule authenticated by immutable original observations","reference_observation_sha256":digest(include_bytes!("../tests/fixtures/bluetooth-files/original-observation.json")),"historical_source_sha256":{(HISTORICAL_CHECKER):HISTORICAL_SHA},"input_sha256":pins,"file_observations":hashes});
    // Flush the summary before publishing. Broken stdout/cancellation must not
    // leave a new success receipt; never delete a preexisting foreign receipt.
    writeln!(printed, "{}", serde_json::to_string(&report)?)?;
    printed.flush()?;
    publish(&options.output.join("result.json"), &report, &files, stop)?;
    Ok(report)
}
struct SignalRunner(Arc<AtomicBool>);
impl Runner for SignalRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(command, Instant::now() + command.timeout, &self.0)
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
        println!("usage: check-bluetooth-files --binary BINARY --output OUTPUT");
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
