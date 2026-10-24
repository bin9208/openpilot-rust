//! Independent native input_event schedule and CPython numeric comparison policy.
//! The candidate Bluetooth decoder never supplies expected results.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    journald_check::Random,
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const SOURCE: &str = "openpilot/selfdrive/carrot/bluetooth/daemon.py";
const SOURCE_SHA: &str = "d032d6069416bae35aea07e25f28f9930795f29cf0a9518c122bfecb6cebe4d3";
const CHECKER: &str = "rust/tools/check_bluetooth_input_decode.py";
const CHECKER_SHA: &str = "2396e77d952213074555f47413934d090500d7d1ec8a338f06c38adefda6cc5a";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn packed(sec: i64, usec: i64, kind: u16, code: u16, value: i32) -> Vec<u8> {
    [
        sec.to_le_bytes().as_slice(),
        usec.to_le_bytes().as_slice(),
        kind.to_le_bytes().as_slice(),
        code.to_le_bytes().as_slice(),
        value.to_le_bytes().as_slice(),
    ]
    .concat()
}
fn signed64(random: &mut Random) -> i64 {
    // CPython randrange(2**64) draws65bits, rejecting the high bit. Preserve
    // all three MT words, including rejected draws and little-endian word order.
    loop {
        let low = random.getrandbits(64);
        let high = random.getrandbits(1);
        if high == 0 {
            return (i128::from(low) - (1_i128 << 63)) as i64;
        }
    }
}
pub fn cases() -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(10_049);
    for sec in [i64::MIN, -1, 0, 1, i64::MAX] {
        for usec in [i64::MIN, -1, 0, 999999, i64::MAX] {
            out.push(packed(sec, usec, 65535, 65535, i32::MIN));
        }
    }
    let mut rng = Random::new(15564);
    for _ in 0..10000 {
        let sec = signed64(&mut rng);
        let usec = signed64(&mut rng);
        let kind = rng.below(65536) as u16;
        let code = rng.below(65536) as u16;
        let value = (rng.below(1_u64 << 32) as i64 - (1_i64 << 31)) as i32;
        out.push(packed(sec, usec, kind, code, value));
    }
    out.extend((0..24).map(|n| vec![0; n]));
    out
}
pub fn expected(raw: &[u8]) -> Result<Value> {
    if raw.is_empty() || !raw.len().is_multiple_of(24) {
        return Ok(value!({"error":"HID device disconnected or incomplete event"}));
    }
    let mut events = vec![];
    for bytes in raw.chunks_exact(24) {
        let sec = i64::from_le_bytes(bytes[..8].try_into()?);
        let usec = i64::from_le_bytes(bytes[8..16].try_into()?);
        events.push(value!({"kind":u16::from_le_bytes(bytes[16..18].try_into()?),"code":u16::from_le_bytes(bytes[18..20].try_into()?),"value":i32::from_le_bytes(bytes[20..24].try_into()?),"at":sec as f64+usec as f64/1e6}));
    }
    Ok(value!({"events":events}))
}
fn expected_json(raw: &[u8]) -> Result<JsonValue> {
    let row = expected(raw)?;
    let Some(events) = row["events"].as_array() else {
        return Ok(JsonValue::parse(&row.to_string())?);
    };
    let text = events
        .iter()
        .map(|event| {
            format!(
                "{{\"kind\":{},\"code\":{},\"value\":{},\"at\":{}}}",
                event["kind"], event["code"], event["value"], event["at"]
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(JsonValue::parse(&format!("{{\"events\":[{text}]}}"))?)
}
// CPython struct.pack('>d', value) accepts JSON booleans as numbers.
// Coercion is only for the bit comparison; structural Python equality follows.
fn timestamp(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Bool(value) => Ok(f64::from(u8::from(value))),
        JsonView::Integer(value) => Ok(value.parse()?),
        JsonView::Float(value) => Ok(value),
        _ => Err("candidate timestamp is not numeric".into()),
    }
}
fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
struct BoundFile {
    file: File,
    metadata: Metadata,
    hash: String,
}
impl BoundFile {
    fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("checker input is not a regular file".into());
        }
        let mut bytes = vec![];
        file.read_to_end(&mut bytes)?;
        if identity(&metadata) != identity(&file.metadata()?) {
            return Err("checker input changed while reading".into());
        }
        Ok(Self {
            file,
            metadata,
            hash: hash(&bytes),
        })
    }
    fn verify(&self, path: &Path) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(path)?)
        {
            return Err("checker input changed during comparison".into());
        }
        Ok(())
    }
}
fn sources(root: &Path) -> Result<Vec<(PathBuf, BoundFile)>> {
    let mut files = vec![];
    for (name, pin, retired) in [(SOURCE, SOURCE_SHA, false), (CHECKER, CHECKER_SHA, true)] {
        let path = root.join(name);
        if retired {
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
                Ok(_) => {}
            }
        }
        let bound = BoundFile::open(&path)?;
        if bound.hash != pin {
            return Err(format!("Bluetooth source changed: {name}").into());
        }
        files.push((path, bound));
    }
    Ok(files)
}
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::SeqCst) {
        Err("Bluetooth input check interrupted".into())
    } else {
        Ok(())
    }
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    run(
        root,
        binary,
        output,
        runner,
        printed,
        &AtomicBool::new(false),
    )
}
fn run(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    stop: &AtomicBool,
) -> Result<()> {
    // The unchanged source uses native struct '@llHHi'. Its sealed corpus is
    // Linux little-endian LP64 (24-byte input_event), not a portable wire ABI.
    if !cfg!(all(
        target_os = "linux",
        target_pointer_width = "64",
        target_endian = "little"
    )) {
        return Err("Bluetooth source input_event requires little-endian Linux LP64".into());
    }
    let pins = sources(root)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let result = (|| {
        let inputs = cases();
        let mut payload = String::new();
        for row in &inputs {
            payload.push_str(&JsonValue::parse(&serde_json::to_string(row)?)?.to_json()?);
            payload.push('\n');
        }
        fs::write(output.join("input.jsonl"), &payload)?;
        let argument = if binary.is_absolute() {
            binary.to_owned()
        } else {
            std::env::current_dir()?.join(binary)
        };
        let binary = resolve_non_strict(binary)?;
        // Retain executable identity now, but defer read errors until the original
        // post-comparison hash phase. Exec failures still precede report hashing.
        let artifact = BoundFile::open(&binary);
        let mut command = CommandSpec::new_os(
            vec![binary.as_os_str().to_owned(), "decode".into()],
            Duration::from_secs(30),
        );
        command.input = Some(payload.into_bytes());
        command.stdout_limit = usize::MAX;
        command.stderr_limit = usize::MAX;
        cancelled(stop)?;
        let actual = runner.run(&command)?;
        let stdout = String::from_utf8(actual.stdout)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let stderr = String::from_utf8(actual.stderr)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        fs::write(output.join("native.jsonl"), &stdout)?;
        fs::write(output.join("native.stderr"), stderr)?;
        if actual.code != 0 {
            return Err(format!("Bluetooth candidate exited {}", actual.code).into());
        }
        let mut lines = stdout
            .split([
                '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
                '\u{2029}',
            ])
            .collect::<Vec<_>>();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let observed = lines
            .into_iter()
            .map(json::parse_text)
            .collect::<Result<Vec<_>>>()?;
        if observed.len() != inputs.len() {
            return Err("Bluetooth candidate cardinality differs".into());
        }
        let mut original = vec![];
        for (input, actual) in inputs.iter().zip(&observed) {
            let reference = expected_json(input)?;
            if let Some(events) = reference.get("events") {
                let JsonView::Array(events) = events.view() else {
                    unreachable!()
                };
                let JsonView::Array(other) = actual
                    .get("events")
                    .ok_or("candidate events missing")?
                    .view()
                else {
                    return Err("candidate events not array".into());
                };
                if events.len() != other.len() {
                    return Err("candidate event count differs".into());
                }
                for (a, b) in events.iter().zip(other) {
                    let a = timestamp(&a.get("at").ok_or("source timestamp missing")?)?;
                    let b = timestamp(&b.get("at").ok_or("candidate timestamp missing")?)?;
                    if a.to_bits() != b.to_bits() {
                        return Err("candidate timestamp binary64 bits differ".into());
                    }
                }
            }
            if !json::equal(&reference, actual) {
                return Err("Bluetooth source/native values differ".into());
            }
            original.push(reference.to_json()?);
        }
        fs::write(
            output.join("source.json"),
            format!("[{}]", original.join(", ")),
        )?;
        let artifact = artifact?;
        artifact.verify(&binary)?;
        artifact.verify(&argument)?;
        for (path, file) in &pins {
            file.verify(path)?;
        }
        // Detect an originally absent helper appearing during the comparison too.
        if sources(root)?.len() != pins.len() {
            return Err("Bluetooth source roster changed".into());
        }
        let report=JsonValue::parse(&format!("{{\"cases\":10049,\"exact\":\"signed fields and timestamp binary64 bits\",\"binary_sha256\":\"{}\",\"source_sha256\":\"{SOURCE_SHA}\"}}",artifact.hash))?;
        cancelled(stop)?;
        fs::write(output.join("result.json"), json::pretty(&report, 0)?)?;
        cancelled(stop)?;
        writeln!(printed, "{}", report.to_json()?)?;
        Ok(())
    })();
    let interrupted = stop.load(Ordering::SeqCst);
    if result.is_err() || interrupted {
        match fs::remove_file(output.join("result.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if interrupted {
        Err("Bluetooth input check interrupted".into())
    } else {
        result
    }
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
        println!("usage: check-bluetooth-input-decode --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    run(
        &root,
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--output"]),
        &mut SignalRunner(stop.clone()),
        &mut std::io::stdout(),
        &stop,
    )
}
