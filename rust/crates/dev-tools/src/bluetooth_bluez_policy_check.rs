//! Source-independent controller for the unchanged BlueZ prompt policy matrix.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json as receipt, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::{FileExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const DATA: &str = include_str!("../data/unicode-15.0.0/DerivedNumericType.txt");
const DATA_SHA: &str = "710abf2d581ac9c57f244c0834f9d9969d9781e0396adccd330eaae658ac7d6b";
const SOURCES: [(&str, &str); 4] = [
    (
        "rust/tools/check_bluetooth_bluez_policy.py",
        "fa1e764393c064f3aa95386a221eaf1690cf14d5cb932b027304124ceaf1c3af",
    ),
    (
        "openpilot/selfdrive/carrot/bluetooth/bluez.py",
        "e4998d01bd8d614700e9a3ce027d7fe56e0dd870ea8c697da05e5228a51b5237",
    ),
    (
        "rust/crates/bluetooth/examples/bluez_policy.rs",
        "27715522f0441527a203ed4a2e9a2a1e67ebadd7f4f44699100bacf2c1e33531",
    ),
    (
        "rust/crates/bluetooth/src/bluez/policy.rs",
        "9e0d228437ed9fac9e80a9300776ac1a262f390f9d61535fa56c502572661ef2",
    ),
];
const KINDS: [&str; 7] = [
    "RequestConfirmation",
    "RequestAuthorization",
    "AuthorizeService",
    "RequestPinCode",
    "RequestPasskey",
    "DisplayPinCode",
    "DisplayPasskey",
];
const FIXED: [&str; 33] = [
    "null", "false", "true", "-1", "0", "1", "999999", "1000000",
    "10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "1.0", "-0.0", "Infinity", "NaN", "[]", "{}", "[\"1\"]", "\"\"", "\"0\"", "\"000001\"", "\"123456\"", "\"1234567\"", "\" 1 \"", "\"+1\"", "\"1_2\"", "\"1.0\"", "\"-1\"", "\"\\u0000\"", "\"\\ud800\"", "\"\\ud55c\\uae00\"", "\"\"", "\"\"", "\"\"", "\"\"",
];
pub type DigitRow = (u32, Option<u32>);

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
}
impl Bound {
    fn open(path: &Path) -> Result<Self> {
        Self::open_with_flags(path, rustix::fs::OFlags::empty())
    }
    fn open_nofollow(path: &Path) -> Result<Self> {
        Self::open_with_flags(path, rustix::fs::OFlags::NOFOLLOW)
    }
    fn open_with_flags(path: &Path, extra: rustix::fs::OFlags) -> Result<Self> {
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | extra,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("BlueZ policy input is not a regular file".into());
        }
        let mut hash = Sha256::new();
        let mut chunk = [0; 65536];
        loop {
            let n = file.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            hash.update(&chunk[..n]);
        }
        let bound = Self {
            path: path.into(),
            file,
            metadata,
            hash: format!("{:x}", hash.finalize()),
        };
        bound.verify()?;
        Ok(bound)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::metadata(&self.path)?)
        {
            return Err(format!("BlueZ policy input changed: {}", self.path.display()).into());
        }
        Ok(())
    }
    fn verify_content(&self) -> Result<()> {
        self.verify()?;
        let mut hash = Sha256::new();
        let mut chunk = [0; 65536];
        let mut offset = 0;
        loop {
            let n = self.file.read_at(&mut chunk, offset)?;
            if n == 0 {
                break;
            }
            hash.update(&chunk[..n]);
            offset += n as u64;
            if offset > self.metadata.len() {
                return Err(format!("BlueZ policy input changed: {}", self.path.display()).into());
            }
        }
        if format!("{:x}", hash.finalize()) != self.hash {
            return Err(format!("BlueZ policy input changed: {}", self.path.display()).into());
        }
        self.verify()
    }
}
struct CheckerParent {
    path: PathBuf,
    file: File,
    metadata: Metadata,
}
impl CheckerParent {
    fn open(path: &Path) -> Result<Self> {
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_dir() || !fs::symlink_metadata(path)?.is_dir() {
            return Err("BlueZ policy checker parent is not a directory".into());
        }
        Ok(Self {
            path: path.into(),
            file,
            metadata,
        })
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&fs::symlink_metadata(&self.path)?)
        {
            return Err("BlueZ policy checker parent changed".into());
        }
        Ok(())
    }
}
struct Inputs {
    live: Vec<Bound>,
    checker: Option<Bound>,
    checker_path: PathBuf,
    checker_parent: CheckerParent,
}
fn check(inputs: &Inputs, cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        return Err("BlueZ policy comparison interrupted".into());
    }
    let present = match fs::symlink_metadata(&inputs.checker_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    match (&inputs.checker, present) {
        (Some(checker), Some(metadata)) if metadata.is_file() => checker.verify_content()?,
        (None, None) => {}
        _ => return Err("BlueZ policy historical checker presence changed".into()),
    }
    inputs.checker_parent.verify()?;
    for input in &inputs.live {
        input.verify()?;
    }
    Ok(())
}
fn sources(root: &Path) -> Result<Inputs> {
    let checker_path = root.join(SOURCES[0].0);
    let checker_parent =
        CheckerParent::open(checker_path.parent().ok_or("missing checker parent")?)?;
    let checker = match fs::symlink_metadata(&checker_path) {
        Ok(metadata) if metadata.is_file() => {
            let bound = Bound::open_nofollow(&checker_path)?;
            if bound.hash != SOURCES[0].1 {
                return Err("BlueZ policy historical checker content changed".into());
            }
            Some(bound)
        }
        Ok(_) => return Err("BlueZ policy historical checker is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut inputs = Inputs {
        live: vec![],
        checker,
        checker_path,
        checker_parent,
    };
    for (name, expected) in SOURCES.iter().skip(1) {
        let bound = Bound::open(&root.join(name))?;
        if bound.hash != *expected {
            return Err(format!("BlueZ policy source changed: {name}").into());
        }
        inputs.live.push(bound);
    }
    check(&inputs, &AtomicBool::new(false))?;
    Ok(inputs)
}

/// Unicode 15 Digit|Decimal entries, with decimal values derived only from
/// Decimal ten-runs. This never reads the candidate's generated digit tables.
pub fn digits() -> Result<Vec<DigitRow>> {
    if sha(DATA.as_bytes()) != DATA_SHA {
        return Err("Unicode 15 numeric data changed".into());
    }
    let mut digits = std::collections::BTreeMap::new();
    for line in DATA.lines() {
        let fields = line.split('#').next().unwrap().trim();
        if fields.is_empty() {
            continue;
        }
        let (range, property) = fields.split_once(';').ok_or("missing Unicode property")?;
        let property = property.trim();
        if !matches!(property, "Digit" | "Decimal") {
            continue;
        }
        let (low, high) = range
            .trim()
            .split_once("..")
            .unwrap_or((range.trim(), range.trim()));
        let low = u32::from_str_radix(low, 16)?;
        let high = u32::from_str_radix(high, 16)?;
        if low > high || high > 0x10ffff {
            return Err("invalid Unicode digit range".into());
        }
        if property == "Decimal" && (high - low + 1) % 10 != 0 {
            return Err("incomplete Unicode decimal run".into());
        }
        for point in low..=high {
            let decimal = if property == "Decimal" {
                Some((point - low) % 10)
            } else {
                None
            };
            if digits.insert(point, decimal).is_some() {
                return Err("duplicate Unicode digit".into());
            }
        }
    }
    Ok(digits.into_iter().collect())
}
fn fixed_values() -> Result<Vec<JsonValue>> {
    let mut values = FIXED
        .iter()
        .map(|raw| Ok(JsonValue::parse(raw)?))
        .collect::<Result<Vec<_>>>()?;
    values[29] = JsonValue::text(&"가".repeat(16));
    values[30] = JsonValue::text(&"가".repeat(17));
    values[31] = JsonValue::text(&"🙂".repeat(16));
    values[32] = JsonValue::text(&"🙂".repeat(17));
    Ok(values)
}
pub fn values_and_digits() -> Result<(Vec<JsonValue>, Vec<DigitRow>)> {
    let table = digits()?;
    let mut values = fixed_values()?;
    for (point, _) in &table {
        values.push(JsonValue::codepoints(vec![*point]).ok_or("invalid digit point")?);
    }
    for (point, decimal) in &table {
        if *decimal == Some(0) {
            values.push(
                JsonValue::codepoints((*point..*point + 6).collect())
                    .ok_or("invalid decimal sequence")?,
            );
        }
    }
    Ok((values, table))
}
fn expected(
    kind: &str,
    value: &JsonValue,
    digits: &std::collections::BTreeSet<u32>,
) -> Result<JsonValue> {
    let answer = if matches!(value.view(), JsonView::Bool(false)) {
        None
    } else if matches!(
        kind,
        "RequestConfirmation" | "RequestAuthorization" | "AuthorizeService"
    ) {
        (!matches!(value.view(), JsonView::Bool(true))).then_some("confirmation must be boolean")
    } else if kind == "RequestPinCode" {
        match value.view() {
            JsonView::Text(points) if (1..=16).contains(&points.len()) => None,
            _ => Some("PIN must contain 1 to 16 characters"),
        }
    } else if kind == "RequestPasskey" {
        let points = openpilot_runtime_version::python_str(value)?;
        (!(1..=6).contains(&points.len()) || !points.iter().all(|p| digits.contains(p)))
            .then_some("passkey must contain 1 to 6 digits")
    } else {
        None
    };
    Ok(JsonValue::parse(&match answer {
        Some(error) => format!("{{\"error\": {}}}", JsonValue::text(error).to_json()?),
        None => "{\"accepted\": true}".into(),
    })?)
}

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
struct Pending {
    path: PathBuf,
    file: tempfile::NamedTempFile,
    inputs: Inputs,
}
impl Pending {
    fn commit(self, cancel: &AtomicBool) -> Result<()> {
        check(&self.inputs, cancel)?;
        let held = self
            .file
            .persist_noclobber(&self.path)
            .map_err(|e| e.error)?;
        let result = check(&self.inputs, cancel);
        if result.is_err()
            && held.metadata().is_ok_and(|m| {
                fs::symlink_metadata(&self.path)
                    .is_ok_and(|n| (m.dev(), m.ino()) == (n.dev(), n.ino()))
            })
        {
            let _ = fs::remove_file(&self.path);
        }
        result
    }
}
fn lines(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            parts.push(&text[start..index]);
            start = index + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                start = chars.next().unwrap().0 + 1;
            }
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}
fn python_text(bytes: Vec<u8>) -> Result<String> {
    // subprocess.run(text=True) uses universal newline translation before the
    // historical checker writes its stdout and stderr evidence files.
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn run_inner(
    options: &Options,
    runner: &mut dyn Runner,
    cancel: &AtomicBool,
) -> Result<(Value, Pending)> {
    let mut inputs = sources(&options.root)?;
    check(&inputs, cancel)?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let (values, table) = values_and_digits()?;
    if values.len() != 909
        || table.len() != 808
        || table.iter().filter(|(_, d)| *d == Some(0)).count() != 68
    {
        return Err("BlueZ policy corpus cardinality drift".into());
    }
    let digit_set = table
        .iter()
        .map(|(p, _)| *p)
        .collect::<std::collections::BTreeSet<_>>();
    let mut cases = String::new();
    let mut source = Vec::with_capacity(KINDS.len() * values.len());
    let mut accepted = 0usize;
    for kind in KINDS {
        for value in &values {
            cases.push_str(&format!(
                "{{\"kind\": \"{kind}\", \"value\": {}}}\n",
                value.to_json()?
            ));
            let answer = expected(kind, value, &digit_set)?;
            if answer.get("accepted").is_some() {
                accepted += 1;
            }
            source.push(answer);
        }
    }
    let source_text = format!(
        "[{}]",
        source
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    );
    fs::write(options.output.join("cases.jsonl"), &cases)?;
    fs::write(options.output.join("source.json"), &source_text)?;
    let binary = resolve_non_strict(&options.binary)?;
    let alias = if options.binary.is_absolute() {
        options.binary.clone()
    } else {
        std::env::current_dir()?.join(&options.binary)
    };
    let artifact = Bound::open(&binary)?;
    let alias = Bound::open(&alias)?;
    let binary_hash = artifact.hash.clone();
    inputs.live.push(artifact);
    inputs.live.push(alias);
    let mut command =
        CommandSpec::new_os(vec![binary.as_os_str().to_owned()], Duration::from_secs(30));
    command.input = Some(cases.as_bytes().to_vec());
    // The original subprocess captures complete diagnostics without a byte cap.
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    check(&inputs, cancel)?;
    let stdout_path = options.output.join("native.jsonl");
    let stderr_path = options.output.join("native.stderr");
    let code = runner
        .run_to_files(&command, &stdout_path, &stderr_path)?
        .code;
    let stdout = python_text(fs::read(&stdout_path)?)?;
    let stderr = python_text(fs::read(&stderr_path)?)?;
    fs::write(&stdout_path, &stdout)?;
    fs::write(&stderr_path, stderr)?;
    if code != 0 {
        return Err(format!("BlueZ policy fixture exited {code}").into());
    }
    let actual = lines(&stdout);
    if actual.len() != source.len() {
        return Err(format!(
            "BlueZ policy response cardinality differs: {} != {}",
            actual.len(),
            source.len()
        )
        .into());
    }
    for (index, (raw, expected)) in actual.into_iter().zip(&source).enumerate() {
        let got = json::parse_text(raw)?;
        if !json::equal(expected, &got) {
            return Err(format!("BlueZ policy response differs at case {index}").into());
        }
    }
    check(&inputs, cancel)?;
    let mut digits_command = CommandSpec::new_os(
        vec![binary.as_os_str().to_owned(), "--digits".into()],
        Duration::from_secs(30),
    );
    digits_command.stdout_limit = usize::MAX;
    digits_command.stderr_limit = usize::MAX;
    let digits_stdout = options.output.join("digits.stdout");
    let digits_stderr = options.output.join("digits.stderr");
    let code = runner
        .run_to_files(&digits_command, &digits_stdout, &digits_stderr)?
        .code;
    let stdout_bytes = fs::read(&digits_stdout)?;
    let stdout = python_text(stdout_bytes.clone())?;
    // These are direct fixture captures. The separate original --digits
    // invocation is also captured as raw bytes, including diagnostic newlines.
    let _stderr = fs::read(&digits_stderr)?;
    if code != 0 {
        return Err(format!("BlueZ policy digit probe exited {code}").into());
    }
    let actual = json::parse(stdout.as_bytes())?;
    let expected_digits = JsonValue::parse(&serde_json::to_string(&table)?)?;
    if !json::equal(&expected_digits, &actual) {
        return Err("BlueZ policy full digit table differs".into());
    }
    check(&inputs, cancel)?;
    let report = receipt!({"cases":source.len(),"accepted":accepted,"unicode_points_checked":0x110000,"digit_points":table.len(),"unicode_version":"15.0.0","binary_sha256":binary_hash});
    let provenance = receipt!({"checker_source_sha256":SOURCES[0].1,"source_policy_sha256":SOURCES[1].1,"fixture_source_sha256":SOURCES[2].1,"candidate_policy_sha256":SOURCES[3].1,"unicode_data_sha256":DATA_SHA,"cases_sha256":sha(cases.as_bytes()),"source_json_sha256":sha(source_text.as_bytes()),"digits_stdout_sha256":sha(&stdout_bytes)});
    fs::write(
        options.output.join("provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    let path = options.output.join("result.json");
    let mut file = tempfile::NamedTempFile::new_in(&options.output)?;
    file.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
    check(&inputs, cancel)?;
    Ok((report, Pending { path, file, inputs }))
}
fn finish(
    report: Value,
    pending: Pending,
    printed: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Value> {
    check(&pending.inputs, cancel)?;
    writeln!(printed, "{}", serde_json::to_string(&report)?)?;
    printed.flush()?;
    pending.commit(cancel)?;
    Ok(report)
}
pub fn run(
    options: &Options,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Value> {
    let (report, pending) = run_inner(options, runner, cancel)?;
    finish(report, pending, printed, cancel)
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
        println!("usage: check-bluetooth-bluez-policy --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancel.clone())?);
    }
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: args.values["--binary"].clone().into(),
        output: args.values["--output"].clone().into(),
    };
    let result = run_inner(&options, &mut SignalRunner(cancel.clone()), &cancel);
    drop(signals);
    if cancel.load(Ordering::SeqCst) {
        return Err("BlueZ policy comparison interrupted".into());
    }
    let (report, pending) = result?;
    finish(report, pending, &mut std::io::stdout(), &cancel)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_splitlines_treats_crlf_as_one_boundary() {
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("first\r\nsecond\n"), ["first", "second"]);
        assert_eq!(lines("\r\n\r\n"), ["", ""]);
        assert_eq!(lines("one\u{2028}two\u{85}three"), ["one", "two", "three"]);
    }
    #[test]
    fn subprocess_text_normalizes_crlf_and_cr() {
        assert_eq!(
            python_text(b"first\r\nsecond\rthird\n".to_vec()).unwrap(),
            "first\nsecond\nthird\n"
        );
        assert!(python_text(vec![0xff]).is_err());
    }
    #[test]
    fn independent_prompt_predicates_keep_false_and_digit_distinctions() {
        let table = digits().unwrap();
        let points = table
            .iter()
            .map(|(p, _)| *p)
            .collect::<std::collections::BTreeSet<_>>();
        let false_value = JsonValue::parse("false").unwrap();
        let true_value = JsonValue::parse("true").unwrap();
        let numeric_one = JsonValue::parse("1").unwrap();
        for kind in KINDS {
            assert!(expected(kind, &false_value, &points)
                .unwrap()
                .get("accepted")
                .is_some());
        }
        assert!(expected("RequestConfirmation", &true_value, &points)
            .unwrap()
            .get("accepted")
            .is_some());
        assert!(expected("RequestConfirmation", &numeric_one, &points)
            .unwrap()
            .get("error")
            .is_some());
        assert!(expected("RequestPasskey", &numeric_one, &points)
            .unwrap()
            .get("accepted")
            .is_some());
        assert!(expected("RequestPinCode", &numeric_one, &points)
            .unwrap()
            .get("error")
            .is_some());
        let nondecimal = table
            .iter()
            .find(|(_, decimal)| decimal.is_none())
            .unwrap()
            .0;
        let digit = JsonValue::codepoints(vec![nondecimal]).unwrap();
        assert!(expected("RequestPasskey", &digit, &points)
            .unwrap()
            .get("accepted")
            .is_some());
        assert!(expected("RequestPinCode", &digit, &points)
            .unwrap()
            .get("accepted")
            .is_some());
        let six = JsonValue::text("000000");
        let seven = JsonValue::text("0000000");
        assert!(expected("RequestPasskey", &six, &points)
            .unwrap()
            .get("accepted")
            .is_some());
        assert!(expected("RequestPasskey", &seven, &points)
            .unwrap()
            .get("error")
            .is_some());
    }
}
