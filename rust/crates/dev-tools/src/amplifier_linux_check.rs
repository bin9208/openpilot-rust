//! Independent Python SMBus source policy over a minimal raw syscall boundary.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Duration,
};
struct Raw {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    closed: bool,
}
impl Raw {
    fn new(helper: &Path, device: &Path) -> Result<Self> {
        // Only the raw syscall child loads the fixture. The Rust owner has no
        // fixture destructor and never mutates its process-global environment.
        if std::env::var_os("LD_PRELOAD").is_some_and(|v| !v.is_empty()) {
            return Err("launch the Rust source owner without LD_PRELOAD; use AMP_SYSCALL_PRELOAD for its syscall child".into());
        }
        let mut command = Command::new(helper);
        command
            .arg(device)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(preload) = std::env::var_os("AMP_SYSCALL_PRELOAD") {
            command.env("LD_PRELOAD", preload);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().ok_or("missing syscall stdin")?;
        let output = BufReader::new(child.stdout.take().ok_or("missing syscall stdout")?);
        Ok(Self {
            child,
            input,
            output,
            closed: false,
        })
    }
    fn call(&mut self, request: &str) -> Result<(i32, u8)> {
        writeln!(self.input, "{request}")?;
        self.input.flush()?;
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            return Err("syscall boundary terminated without response".into());
        }
        let mut fields = line.split_whitespace();
        let error = fields.next().ok_or("missing syscall errno")?.parse()?;
        let value = fields.next().ok_or("missing syscall byte")?.parse()?;
        if fields.next().is_some() {
            return Err("invalid syscall response".into());
        }
        Ok((error, value))
    }
    fn finish(&mut self) -> Result<()> {
        writeln!(self.input, "Q")?;
        self.input.flush()?;
        let status = self.child.wait()?;
        self.closed = true;
        if !status.success() {
            return Err(format!("syscall boundary exited {status}").into());
        }
        Ok(())
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing source input field {key}").into())
}
fn path(value: &JsonValue) -> Result<PathBuf> {
    let JsonView::Text(points) = value.view() else {
        return Err("source device requires a path string".into());
    };
    let mut bytes = vec![];
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let c = char::from_u32(point).ok_or("source path cannot encode surrogate")?;
            let mut encoded = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut encoded).as_bytes());
        }
    }
    let mut normalized = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !normalized.is_empty() && !normalized.ends_with(b"/") {
            normalized.push(b'/');
        }
        normalized.extend_from_slice(part);
    }
    if normalized.is_empty() {
        normalized.push(b'.');
    }
    Ok(OsString::from_vec(normalized).into())
}
fn byte(value: &JsonValue) -> Result<u8> {
    match value.view() {
        JsonView::Bool(v) => Ok(u8::from(v)),
        JsonView::Integer(text) => {
            let mut value = 0u8;
            for digit in text.bytes().filter(|b| *b != b'-') {
                value = value.wrapping_mul(10).wrapping_add(digit - b'0');
            }
            Ok(if text.starts_with('-') {
                value.wrapping_neg()
            } else {
                value
            })
        }
        _ => Err("source byte requires an integer".into()),
    }
}
fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    json::sequence(value)
}
fn text_parse(bytes: Vec<u8>) -> Result<JsonValue> {
    let text = String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    json::parse_text(&text)
}
/// Run the actual open/ioctl/close boundary for arbitrary supported action input.
/// This policy does not import the native candidate's amplifier/I2C implementation.
pub fn reference(input: &Path, helper: &Path, printed: &mut dyn Write) -> Result<()> {
    reference_input(Some(input), helper, printed)
}
fn reference_input(input: Option<&Path>, helper: &Path, printed: &mut dyn Write) -> Result<()> {
    // The original preloaded process persists initial fixture registers even
    // when input/path admission fails before open. Start the independent raw
    // owner first, but do not issue open until the same admission has passed.
    let device = std::env::var_os("AMP_DEVICE_PATH");
    let mut raw = Raw::new(
        helper,
        Path::new(device.as_deref().unwrap_or(std::ffi::OsStr::new("."))),
    )?;
    let admitted = (|| -> Result<JsonValue> {
        let input = input.ok_or("amplifier source reference needs INPUT")?;
        let config = text_parse(fs::read(input)?)?;
        let configured = path(&field(&config, "device")?)?;
        if Some(configured.as_os_str()) != device.as_deref() {
            return Err("source device must equal AMP_DEVICE_PATH".into());
        }
        Ok(config)
    })();
    let config = match admitted {
        Ok(config) => config,
        Err(error) => {
            raw.finish()?;
            return Err(error);
        }
    };
    let (error, _) = raw.call("O")?;
    let mut values = vec![];
    let mut failure = error;
    if failure == 0 {
        let operation = (|| -> Result<i32> {
            for action in sequence(&field(&config, "actions")?)? {
                let read = field(&action, "kind")?.text_eq("read");
                let register = field(&action, "register")?;
                let value = if read {
                    None
                } else {
                    Some(field(&action, "value")?)
                };
                let (error, _) = raw.call("A 16")?;
                if error != 0 {
                    return Ok(error);
                }
                let register = byte(&register)?;
                let value = value.as_ref().map(byte).transpose()?.unwrap_or(0);
                let (error, value) =
                    raw.call(&format!("B {} {register} {value}", u8::from(read)))?;
                if error != 0 {
                    return Ok(error);
                }
                if read {
                    values.push(value);
                }
            }
            Ok(0)
        })();
        let (close, _) = raw.call("C")?;
        if close != 0 {
            failure = close;
        } else {
            match operation {
                Ok(error) => failure = error,
                Err(error) => {
                    // Context-manager cleanup completes even for a non-OSError
                    // action failure, before propagating its native diagnostic.
                    raw.finish()?;
                    return Err(error);
                }
            }
        }
    }
    raw.finish()?;
    let values = values
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        printed,
        "{{\"values\": [{values}], \"errno\": {}}}",
        if failure == 0 {
            "null".into()
        } else {
            failure.to_string()
        }
    )?;
    Ok(())
}
pub fn reference_cli() -> Result<()> {
    let input = std::env::args_os().nth(1);
    let helper = std::env::var_os("AMP_SYSCALL_HELPER")
        .ok_or("amplifier source reference needs AMP_SYSCALL_HELPER")?;
    reference_input(
        input.as_deref().map(Path::new),
        Path::new(&helper),
        &mut std::io::stdout().lock(),
    )
}

const ACTIONS: &str = r#"[{"kind": "read", "register": 81}, {"kind": "write", "register": 81, "value": 133}, {"kind": "read", "register": 81}, {"kind": "write", "register": 16, "value": 23}, {"kind": "read", "register": 16}]"#;
const ORIGINAL_SHA256: &str = "bce5cf130c58f7f25d72f182de8830e3dfc4ac471e92d1110c471095f4d25e4d";
const RETIRED: [(&str, &str); 2] = [
    (
        "rust/tools/check_amplifier_linux.py",
        "661361825dc730544f4a6bbb22d6cf38057bae109e44187dbf8c7d4b701c5d3f",
    ),
    (
        "rust/tools/amplifier_linux_source.py",
        "4d6db40d80a2f21a82e0620f40391925edf017272907aed3950cc136089aa95d",
    ),
];
fn pins(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("../tests/fixtures/amplifier-linux/original27.json");
    if format!("{:x}", Sha256::digest(bytes)) != ORIGINAL_SHA256 {
        return Err(
            "Linux amplifier original provenance changed; independently recapture and review"
                .into(),
        );
    }
    let manifest: serde_json::Value = serde_json::from_slice(bytes)?;
    for (name, expected) in manifest["sources"]
        .as_object()
        .ok_or("missing Linux amplifier source pins")?
    {
        if let Some((_, identity)) = RETIRED.iter().find(|(path, _)| *path == name) {
            if expected.as_str() != Some(*identity) {
                return Err("Linux amplifier retired source provenance changed".into());
            }
            match fs::read(root.join(name)) {
                Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != *identity => {
                    return Err(format!("{name} changed after reintroduction").into());
                }
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into())
                }
                _ => {}
            }
            continue;
        }
        let observed = format!("{:x}", Sha256::digest(fs::read(root.join(name))?));
        if expected.as_str() != Some(observed.as_str()) {
            return Err(format!(
                "{name} changed; independently recapture Linux amplifier source behavior"
            )
            .into());
        }
    }
    Ok(())
}
// Path.read_text has already normalized CR/CRLF. Python str.splitlines also
// recognizes these Unicode and ASCII separators, retaining internal blanks.
fn source_lines(text: &str) -> Vec<&str> {
    let mut lines = text
        .split([
            '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}
#[allow(clippy::too_many_arguments)]
fn run_side(
    binary: &Path,
    fixture: &Path,
    output: &Path,
    runners: &[OsString],
    name: &str,
    faults: &[(usize, i32)],
    missing: bool,
    reference: Option<(&Path, &Path)>,
    runner: &mut dyn Runner,
) -> Result<JsonValue> {
    let lane = if reference.is_some() {
        "source"
    } else {
        "native"
    };
    let output = output.join(name).join(lane);
    fs::create_dir_all(output.parent().ok_or("source output has no parent")?)?;
    fs::create_dir(&output)?;
    let device = output.join("device");
    if !missing {
        fs::write(&device, b"only a regular fixture file")?;
    }
    let config = format!(
        "{{\"device\": {}, \"actions\": {ACTIONS}}}",
        python_fs::json_string(device.as_os_str())?
    );
    let input = output.join("input.json");
    fs::write(&input, config)?;
    let mut env = BTreeMap::new();
    env.insert("AMP_DEVICE_PATH".into(), device.clone().into_os_string());
    env.insert(
        "AMP_TRACE_PATH".into(),
        output.join("syscalls.jsonl").into_os_string(),
    );
    env.insert(
        "AMP_STATE_PATH".into(),
        output.join("registers.bin").into_os_string(),
    );
    env.insert("AMP_SEED".into(), "47".into());
    env.insert(
        "AMP_FAULTS".into(),
        faults
            .iter()
            .map(|(index, error)| format!("{index}:{error}"))
            .collect::<Vec<_>>()
            .join(",")
            .into(),
    );
    env.insert("LD_PRELOAD".into(), fixture.as_os_str().to_owned());
    let mut argv = if let Some((reference, helper)) = reference {
        env.insert("AMP_SYSCALL_HELPER".into(), helper.as_os_str().to_owned());
        vec![reference.as_os_str().to_owned()]
    } else {
        let mut command = runners.to_vec();
        command.push(binary.as_os_str().to_owned());
        command
    };
    argv.push(input.into_os_string());
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::MAX);
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let mut process_env = env.clone();
    if reference.is_some() {
        process_env.insert("LD_PRELOAD".into(), OsString::new());
        process_env.insert("AMP_SYSCALL_PRELOAD".into(), fixture.as_os_str().to_owned());
    }
    spec.set_os_env(process_env.clone());
    let captured = runner.run(&spec)?;
    let stdout = String::from_utf8(captured.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let stderr = String::from_utf8(captured.stderr)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    fs::write(
        output.join("process-stdout.log"),
        if stdout.is_empty() {
            "<empty>\n"
        } else {
            &stdout
        },
    )?;
    fs::write(
        output.join("process-stderr.log"),
        if stderr.is_empty() {
            "<empty>\n"
        } else {
            &stderr
        },
    )?;
    if captured.code != 0 {
        return Err(format!("Linux amplifier {name}/{lane} exited {}", captured.code).into());
    }
    let outcome = text_parse(stdout.into_bytes())?;
    let trace = output.join("syscalls.jsonl");
    let mut events = vec![];
    if trace.try_exists()? {
        let trace = String::from_utf8(fs::read(trace)?)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        for line in source_lines(&trace) {
            let value = text_parse(line.as_bytes().to_vec())?;
            let kind = field(&value, "kind")?;
            if kind.text_eq("close") {
                if !matches!(field(&value, "closed")?.view(), JsonView::Bool(true)) {
                    return Err("source fixture close did not release descriptor".into());
                }
            } else if kind.text_eq("smbus") {
                for (key, expected) in [("struct_size", "16"), ("union_size", "34")] {
                    if !json::equal(&field(&value, key)?, &JsonValue::parse(expected)?) {
                        return Err("source SMBus fixture ABI size changed".into());
                    }
                }
                if !matches!(field(&value, "aligned")?.view(), JsonView::Bool(true)) {
                    return Err("source SMBus fixture pointer misaligned".into());
                }
            }
            events.push(value.to_json()?);
        }
    }
    let registers = fs::read(output.join("registers.bin"))?;
    let result = JsonValue::parse(&format!(
        "{{\"outcome\": {}, \"events\": [{}], \"registers\": [{}]}}",
        outcome.to_json()?,
        events.join(", "),
        registers
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ))?;
    fs::write(
        output.join("result.json"),
        json::pretty_with_limit(&result, 0, 995)?,
    )?;
    fs::write(
        output.join("stderr.log"),
        if stderr.is_empty() {
            "<empty>\n"
        } else {
            &stderr
        },
    )?;
    use sha2::{Digest, Sha256};
    let argv = argv
        .iter()
        .map(|v| python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let mut environment = vec![];
    for key in [
        "AMP_DEVICE_PATH",
        "AMP_TRACE_PATH",
        "AMP_STATE_PATH",
        "AMP_SEED",
        "AMP_FAULTS",
        "LD_PRELOAD",
        "AMP_SYSCALL_HELPER",
        "AMP_SYSCALL_PRELOAD",
    ] {
        let Some(value) = process_env.get(std::ffi::OsStr::new(key)) else {
            continue;
        };
        environment.push(format!(
            "{}: {}",
            JsonValue::text(key).to_json()?,
            python_fs::json_string(value)?
        ));
    }
    let digest = if lane == "native" {
        format!("\"{:x}\"", Sha256::digest(fs::read(binary)?))
    } else {
        "null".into()
    };
    let invocation=JsonValue::parse(&format!("{{\"argv\": [{argv}], \"environment\": {{{}}}, \"exit\": {}, \"binary_sha256\": {digest}}}",environment.join(", "),captured.code))?;
    fs::write(
        output.join("invocation.json"),
        json::pretty_with_limit(&invocation, 0, 995)?,
    )?;
    Ok(result)
}
/// Run all original live source/candidate ioctl and explicit-close comparisons.
#[allow(clippy::too_many_arguments)]
pub fn check(
    root: &Path,
    binary: &Path,
    fixture: &Path,
    output: &Path,
    runners: &[OsString],
    selected: Option<&str>,
    reference: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let selected = selected.filter(|name| !name.is_empty());
    let binary = resolve_non_strict(binary)?;
    let fixture = resolve_non_strict(fixture)?;
    let output = resolve_non_strict(output)?;
    pins(root)?;
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/amplifier-linux/original27.json"
    ))?;
    let cases = manifest["cases"]
        .as_array()
        .ok_or("missing source Linux amplifier cases")?;
    let mut results = vec![];
    // No build or dependency install occurs without the shared disk guard.
    let mut disk_path = output.as_path();
    while !disk_path.try_exists()? {
        disk_path = disk_path
            .parent()
            .ok_or("Linux amplifier output has no existing ancestor")?;
    }
    let guard = openpilot_disk_space::inspect(disk_path, 16 * 1024 * 1024)?;
    guard.require()?;
    let build = tempfile::tempdir()?;
    let source = build.path().join("syscalls.c");
    let helper = build.path().join("syscalls");
    fs::write(
        &source,
        include_bytes!("../tests/fixtures/amplifier-linux/syscalls.c"),
    )?;
    let compile = CommandSpec::new_os(
        vec![
            "cc".into(),
            "-std=c11".into(),
            "-Wall".into(),
            "-Wextra".into(),
            "-Werror".into(),
            source.into_os_string(),
            "-o".into(),
            helper.clone().into_os_string(),
        ],
        Duration::MAX,
    );
    let status = runner.run(&compile)?;
    if status.code != 0 {
        return Err("Linux amplifier raw syscall boundary build failed".into());
    }
    for case in cases {
        let name = case["name"].as_str().ok_or("missing source case name")?;
        if selected.is_some_and(|selected| selected != name) {
            continue;
        }
        let faults = case["faults"]
            .as_object()
            .ok_or("missing source faults")?
            .iter()
            .map(|(index, error)| {
                Ok((
                    index.parse()?,
                    error.as_i64().ok_or("invalid source errno")? as i32,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let missing = case["missing"]
            .as_bool()
            .ok_or("missing source device admission")?;
        let expected = run_side(
            &binary,
            &fixture,
            &output,
            runners,
            name,
            &faults,
            missing,
            Some((reference, &helper)),
            runner,
        )?;
        let actual = run_side(
            &binary, &fixture, &output, runners, name, &faults, missing, None, runner,
        )?;
        if !json::equal(&expected, &actual) {
            return Err(format!("Linux amplifier source/native mismatch {name}").into());
        }
        let count = sequence(&field(&actual, "events")?)?.len();
        results.push(format!(
            "{{\"scenario\": {}, \"result\": \"PASS\", \"syscalls\": {count}}}",
            JsonValue::text(name).to_json()?
        ));
        writeln!(printed, "{name} PASS")?;
        printed.flush()?;
    }
    let result = JsonValue::parse(&format!(
        "{{\"result\": \"PASS\", \"cases\": [{}]}}",
        results.join(", ")
    ))?;
    fs::write(
        output.join("result.json"),
        json::pretty_with_limit(&result, 0, 995)?,
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os_with_positionals(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::append("--runner"),
            OptionSpec::value("--case", false),
        ],
        3,
    )?;
    if args.help {
        println!("usage: check-amplifier-linux BINARY FIXTURE OUTPUT [--runner ARG] [--case CASE]");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let reference = std::env::current_exe()?
        .parent()
        .ok_or("Linux amplifier checker has no parent")?
        .join("amplifier-linux-reference");
    let selected = args
        .values
        .get("--case")
        .map(|value| value.to_str().ok_or("source case requires Unicode"))
        .transpose()?;
    check(
        &root,
        Path::new(&args.positionals[0]),
        Path::new(&args.positionals[1]),
        Path::new(&args.positionals[2]),
        args.append_values
            .get("--runner")
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        selected,
        &reference,
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_splitlines_matches_captured_original_unicode_and_blank_admission() {
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/amplifier-linux/splitlines-original.json"
        ))
        .unwrap();
        for row in original.as_array().unwrap() {
            let text = row["trace"]
                .as_str()
                .unwrap()
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let parsed = source_lines(&text)
                .into_iter()
                .map(json::parse_text)
                .collect::<Result<Vec<_>>>();
            assert_eq!(parsed.is_err(), !row["error"].is_null(), "{}", row["name"]);
            if let Ok(events) = parsed {
                let observed = events
                    .iter()
                    .map(|event| {
                        serde_json::from_str::<serde_json::Value>(&event.to_json().unwrap())
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    serde_json::json!(observed),
                    row["events"],
                    "{}",
                    row["name"]
                );
            }
        }
    }
    #[test]
    fn invocation_receipt_records_exact_process_environment_overlays() {
        struct Peer {
            overlay: BTreeMap<OsString, OsString>,
        }
        impl Runner for Peer {
            fn run(&mut self, spec: &CommandSpec) -> Result<crate::qa_process::Captured> {
                self.overlay = spec.effective_env();
                fs::write(
                    Path::new(&self.overlay[std::ffi::OsStr::new("AMP_STATE_PATH")]),
                    [1, 2, 3],
                )?;
                Ok(crate::qa_process::Captured {
                    code: 0,
                    stdout: b"{\"values\":[],\"errno\":null}\n".to_vec(),
                    stderr: vec![],
                })
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let mut peer = Peer {
            overlay: BTreeMap::new(),
        };
        run_side(
            Path::new("candidate"),
            Path::new("fixture"),
            temp.path(),
            &[],
            "case",
            &[],
            false,
            Some((Path::new("reference"), Path::new("syscalls"))),
            &mut peer,
        )
        .unwrap();
        let record: serde_json::Value = serde_json::from_slice(
            &fs::read(temp.path().join("case/source/invocation.json")).unwrap(),
        )
        .unwrap();
        let expected = peer
            .overlay
            .iter()
            .map(|(k, v)| {
                (
                    k.to_str().unwrap().to_owned(),
                    serde_json::Value::String(v.to_str().unwrap().to_owned()),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(record["environment"], serde_json::Value::Object(expected));
        assert_eq!(record["environment"]["LD_PRELOAD"], "");
        assert_eq!(record["environment"]["AMP_SYSCALL_PRELOAD"], "fixture");
    }
}
