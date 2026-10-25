//! Live C++ Params and native alert-parameter adapter differential. No Python execution.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    params_reference_check::{build_native, ParamsRunner},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
#[path = "selfdrive_params_process.rs"]
mod process;
#[path = "selfdrive_params_zmq.rs"]
mod zmq;
use openpilot_log_format::JsonValue;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const CHECKER: &str = "rust/tools/check_selfdrive_params.py";
const CHECKER_SHA: &str = "0e358cfdb18e3696f506f1f00ded39268a13c3d82b2b7687c5ea9a74a8d4d774";
const HELPER: &str =
    "rust/crates/dev-tools/tests/fixtures/selfdrive-params/selfdrive_params_reference.cc";
const MANIFEST_SHA: &str = "2c13f5a26bec20550c0c537b9b8e39d63a0d1f4721e04af842aa05c8eef934d0";
#[derive(Deserialize)]
struct Case {
    key: String,
    operation: String,
    bytes: Option<String>,
    special: Option<String>,
}
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
    expected: Vec<Expected>,
}
#[derive(Deserialize)]
struct Expected {
    value: serde_json::Value,
    messages: Vec<String>,
    fatal: Option<serde_json::Value>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest() -> Result<serde_json::Value> {
    decode_manifest(include_str!(
        "../tests/fixtures/selfdrive-params/manifest.json"
    ))
}
fn decode_manifest(bytes: &str) -> Result<serde_json::Value> {
    if sha(bytes.as_bytes()) != MANIFEST_SHA {
        return Err("selfdrive Params independent capture manifest changed".into());
    }
    Ok(serde_json::from_str(bytes)?)
}
fn sources(root: &Path) -> Result<serde_json::Value> {
    let m = manifest()?;
    if m["sources"][CHECKER] != CHECKER_SHA
        || m["cases"] != 47
        || m["implementation"] != "CPython"
        || !m["python"]
            .as_str()
            .is_some_and(|version| version.starts_with("3.12."))
    {
        return Err("selfdrive Params original source provenance changed".into());
    }
    for (name, pin) in m["sources"]
        .as_object()
        .ok_or("missing Params source pins")?
    {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(format!("{name} changed; independently recapture selfdrive Params").into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let dependencies = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing original Params dependency lock")?
        .iter()
        .filter(|record| {
            record
                .get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| matches!(name, "numpy" | "pyzmq" | "cython"))
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(dependencies)? != m["dependency_lock"] {
        return Err("original Params source dependency lock changed".into());
    }
    Ok(m)
}
fn cases(root: &Path) -> Result<Corpus> {
    let m = sources(root)?;
    let packed = include_bytes!("../tests/fixtures/selfdrive-params/original.json.gz");
    if m["gzip_sha256"] != sha(packed) {
        return Err("Params corpus compressed bytes changed".into());
    }
    let size = m["bytes"].as_u64().ok_or("missing corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || m["sha256"] != sha(&plain) {
        return Err("Params corpus bytes changed".into());
    }
    let c: Corpus = serde_json::from_slice(&plain)?;
    if c.cases.len() != 47 || c.expected.len() != 47 {
        return Err("Params corpus count differs".into());
    }
    Ok(c)
}
fn receipt(reference: &Path) -> PathBuf {
    reference.with_file_name("selfdrive-params-reference.provenance.json")
}
fn validate_reference(root: &Path, reference: &Path) -> Result<()> {
    let r: serde_json::Value = serde_json::from_slice(&fs::read(receipt(reference))?)?;
    if r["kind"] != "selfdrive-params-original-cpp"
        || r["binary_sha256"] != sha(&fs::read(reference)?)
    {
        return Err("selfdrive Params reference receipt differs".into());
    }
    let pins = r["sources"]
        .as_object()
        .ok_or("missing reference sources")?;
    let original = manifest()?;
    let expected = original["sources"]
        .as_object()
        .ok_or("missing original sources")?;
    if pins.len() != expected.len() + 1
        || expected
            .iter()
            .any(|(name, pin)| pins.get(name) != Some(pin))
    {
        return Err("Params reference source pin set changed".into());
    }
    for (name, pin) in pins {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(format!("reference source changed: {name}").into());
        }
    }
    if r["sources"][HELPER] != sha(&fs::read(root.join(HELPER))?) {
        return Err("Params reference helper is not pinned".into());
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd Params hex byte count".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn configure(root: &Path, case: &Case) -> Result<()> {
    let directory = root.join("fixture");
    let path = directory.join(&case.key);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir(&path)?
        } else {
            fs::remove_file(&path)?
        }
    }
    if case.special.as_deref() == Some("directory") {
        fs::create_dir(path)?
    } else if case.bytes.is_some() || case.special.as_deref() == Some("permission") {
        fs::write(
            &path,
            match &case.bytes {
                Some(bytes) => unhex(bytes)?,
                None => b"text".to_vec(),
            },
        )?;
        if case.special.as_deref() == Some("permission") {
            fs::set_permissions(path, fs::Permissions::from_mode(0o0))?;
        }
    }
    Ok(())
}
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut text = String::from("b");
    text.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => text.push_str("\\t"),
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\\' => text.push_str("\\\\"),
            byte if byte == quote => {
                text.push('\\');
                text.push(char::from(byte));
            }
            32..=126 => text.push(char::from(byte)),
            _ => text.push_str(&format!("\\x{byte:02x}")),
        }
    }
    text.push(char::from(quote));
    text
}
fn source_value(operation: &str, response: &str, key: &str) -> Result<(JsonValue, Vec<String>)> {
    if response == "unknown" {
        return Ok((json::parse_text(r#"{"error":"unknown_key"}"#)?, vec![]));
    }
    if operation != "text" {
        return Ok((
            json::parse_text(&format!("{{\"value\":{response}}}"))?,
            vec![],
        ));
    }
    let bytes = unhex(response)?;
    if bytes.is_empty() {
        return Ok((json::parse_text(r#"{"value":null}"#)?, vec![]));
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok((
            json::parse_text(&format!(
                "{{\"value\":{}}}",
                JsonValue::text(&text).to_json()?
            ))?,
            vec![],
        )),
        Err(error) => Ok((
            json::parse_text(r#"{"value":null}"#)?,
            vec![format!(
                "Failed to cast param {key} with value={} from type t=<ParamKeyType.STRING: 0>",
                bytes_repr(error.as_bytes())
            )],
        )),
    }
}
fn record(message: &str, warning: bool) -> Result<Vec<u8>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let level = if warning { 30 } else { 10 };
    let value = serde_json::json!({"msg":message,"ctx":{},"level":if warning{"WARNING"}else{"DEBUG"},"levelnum":level,"name":"source-reference","filename":"params_pyx.pyx","lineno":0,"pathname":"openpilot/common/params_pyx.pyx","module":"params_pyx","funcName":"source-reference","host":"source-reference","process":std::process::id(),"thread":0,"threadName":"MainThread","created":now});
    let mut bytes = vec![level];
    bytes.extend(serde_json::to_vec(&value)?);
    Ok(bytes)
}
fn decode_record(packet: &[u8]) -> Result<JsonValue> {
    let Some((&level, data)) = packet.split_first() else {
        return Err("empty log packet".into());
    };
    // The original json.loads receives bytes here (including encoding
    // autodetection/surrogatepass), while child stdout is already text.
    let record = json::parse(data)?;
    let number = record.get("levelnum").ok_or("missing log levelnum")?;
    if !json::equal(&number, &json::parse_text(&level.to_string())?) {
        return Err("log packet severity byte differs".into());
    }
    Ok(record)
}
fn receive(socket: &zmq::Socket, marker: &str) -> Result<Vec<JsonValue>> {
    let mut records = vec![];
    loop {
        let packet = socket.receive()?;
        let record = decode_record(&packet)?;
        if record.get("msg").ok_or("missing log msg")?.text_eq(marker) {
            return Ok(records);
        }
        if packet[0] != 30
            || !record
                .get("level")
                .is_some_and(|level| level.text_eq("WARNING"))
        {
            return Err("unexpected non-warning log before barrier".into());
        }
        records.push(json::parse_text(&format!(
            "{{\"packet_hex\":\"{}\",\"record\":{}}}",
            hex(&packet),
            record.to_json()?
        ))?);
    }
}
fn messages(records: &[JsonValue]) -> Result<JsonValue> {
    let values = records
        .iter()
        .map(|record| {
            Ok(record
                .get("record")
                .and_then(|record| record.get("msg"))
                .ok_or("missing log message")?
                .to_json()?)
        })
        .collect::<Result<Vec<_>>>()?;
    json::parse_text(&format!("[{}]", values.join(",")))
}
fn array(values: &[JsonValue]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))
}
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let corpus = cases(root)?;
    let cases = &corpus.cases;
    validate_reference(root, reference)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    rustix::process::setrlimit(
        rustix::process::Resource::Core,
        rustix::process::Rlimit {
            current: Some(0),
            maximum: Some(0),
        },
    )?;
    let temporary = tempfile::Builder::new()
        .prefix("selfdrive-params-")
        .tempdir()?;
    let store = temporary.path();
    let source = store.join("source");
    let native = store.join("native");
    let context = zmq::Zmq::new()?;
    let source_endpoint = format!("ipc://{}", store.join("source-log").display());
    let native_endpoint = format!("ipc://{}", store.join("native-log").display());
    let source_socket = context.socket(&source_endpoint, true)?;
    let native_socket = context.socket(&native_endpoint, true)?;
    let source_sender = context.socket(&source_endpoint, false)?;
    let mut original = process::Process::start(
        &resolve_non_strict(reference)?,
        &[source.as_os_str(), OsStr::new("serve")],
        fs::File::create(store.join("source.stderr"))?,
    )?;
    let native_stderr = fs::File::create(output.join("native.stderr"))?;
    let mut candidate = process::Process::start(
        &resolve_non_strict(binary)?,
        &[
            native.as_os_str(),
            OsStr::new("fixture"),
            OsStr::new(&native_endpoint),
        ],
        native_stderr,
    )?;
    let mut rows = vec![];
    for (index, case) in cases.iter().enumerate() {
        configure(&source, case)?;
        configure(&native, case)?;
        let mut failure = None;
        let (expected, warnings) = if case.special.as_deref() == Some("fatal") {
            let mut spec = CommandSpec::new_os(
                vec![
                    resolve_non_strict(reference)?.into_os_string(),
                    source.as_os_str().to_owned(),
                    "fatal".into(),
                    case.key.clone().into(),
                ],
                Duration::from_secs(10),
            );
            spec.stdout_limit = usize::MAX;
            spec.stderr_limit = usize::MAX;
            let result = runner.run(&spec)?;
            fs::write(
                output.join(format!("source-fatal-{index}.stderr")),
                &result.stderr,
            )?;
            if result.code != -6 {
                return Err("original Params fatal reader did not SIGABRT".into());
            }
            failure = Some(
                serde_json::json!({"returncode":result.code,"stderr":String::from_utf8_lossy(&result.stderr)}),
            );
            (json::parse_text(r#"{"error":"fatal_integer"}"#)?, vec![])
        } else {
            let response = original.request(&format!("{}\t{}", case.operation, case.key))?;
            source_value(&case.operation, &response, &case.key)?
        };
        let marker = format!("barrier-{index}");
        let pinned = &corpus.expected[index];
        if !json::equal(
            &expected,
            &json::parse_text(&serde_json::to_string(&pinned.value)?)?,
        ) || warnings != pinned.messages
            || failure != pinned.fatal
        {
            return Err(format!(
                "live source reference differs from original Cython capture at {index}"
            )
            .into());
        }
        for warning in warnings {
            eprintln!("{warning}");
            source_sender.send(&record(&warning, true)?)?;
        }
        source_sender.send(&record(&marker, false)?)?;
        let source_logs = receive(&source_socket, &marker)?;
        let request =
            serde_json::json!({"operation":case.operation,"key":case.key,"marker":marker});
        let response = candidate.request(&serde_json::to_string(&request)?)?;
        let actual = json::parse_text(&response)?;
        let native_logs = receive(&native_socket, &marker)?;
        if !json::equal(&expected, &actual) {
            return Err(format!("Params source/native value differs at {index}").into());
        }
        if !json::equal(&messages(&source_logs)?, &messages(&native_logs)?) {
            return Err(format!("Params source/native log messages differ at {index}").into());
        }
        rows.push(format!(
            "{{\"key\":{},\"operation\":{},\"bytes\":{},\"special\":{},\"source\":{},\"native\":{},\"source_failure\":{},\"source_logs\":{},\"native_logs\":{}}}",
            JsonValue::text(&case.key).to_json()?,
            JsonValue::text(&case.operation).to_json()?,
            serde_json::to_string(&case.bytes)?,
            serde_json::to_string(&case.special)?,
            expected.to_json()?,
            actual.to_json()?,
            serde_json::to_string(&failure)?,
            array(&source_logs)?,
            array(&native_logs)?
        ));
    }
    candidate.finish()?;
    original.finish()?;
    drop(candidate);
    drop(original);
    drop(source_sender);
    drop(source_socket);
    drop(native_socket);
    drop(context);
    // The original leaves the TemporaryDirectory context before hashing or
    // opening report.json. A cleanup failure must not publish success.
    temporary.close()?;
    let provenance = JsonValue::text("Original C++ Params get/getBool/getInt on live files. STRING UTF-8 decoding and warning/marker ZMQ delivery are native ports of the pinned Cython/Swaglog boundary; no Cython or Python is executed.");
    let value = json::parse_text(&format!(
        "{{\"passed\":true,\"cases\":[{}],\"native_sha256\":\"{}\",\"reference_sha256\":\"{}\",\"source_reference\":{}}}",
        rows.join(","), sha(&fs::read(binary)?), sha(&fs::read(reference)?), provenance.to_json()?
    ))?;
    fs::write(
        output.join("report.json"),
        json::pretty_with_limit(&value, 0, 995)?,
    )?;
    writeln!(printed, "{{\"passed\": true, \"cases\": 47}}")?;
    Ok(())
}

pub fn build_reference(
    root: &Path,
    output: &Path,
    prefix: Option<&Path>,
    runner: &mut dyn ParamsRunner,
) -> Result<PathBuf> {
    sources(root)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    openpilot_disk_space::inspect(output, 512 * 1024 * 1024)?.require()?;
    build_native(root, output, prefix, runner)?;
    let binary = output.join("selfdrive-params-reference");
    let mut argv = vec![
        "g++".into(),
        "-std=c++17".into(),
        "-pthread".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wl,--gc-sections".into(),
    ];
    for directory in [
        root.join("openpilot"),
        output.to_path_buf(),
        output.join("cereal/gen/cpp"),
    ] {
        argv.push("-I".into());
        argv.push(directory.into_os_string());
    }
    if let Some(prefix) = prefix {
        argv.push("-I".into());
        argv.push(prefix.join("include").into_os_string());
    }
    for name in [
        "openpilot/common/params.cc",
        "openpilot/common/util.cc",
        HELPER,
    ] {
        argv.push(root.join(name).into_os_string());
    }
    argv.extend(["-o".into(), binary.as_os_str().to_owned()]);
    let mut spec = CommandSpec::new_os(argv, Duration::MAX);
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    if runner.run(&spec)?.code != 0 {
        return Err("selfdrive Params reference source build failed".into());
    }
    let m = manifest()?;
    let mut pins: BTreeMap<String, String> = serde_json::from_value(m["sources"].clone())?;
    pins.insert(HELPER.into(), sha(&fs::read(root.join(HELPER))?));
    let record = serde_json::json!({"kind":"selfdrive-params-original-cpp","binary_sha256":sha(&fs::read(&binary)?),"sources":pins,"source_changes":"None. Narrow native entrypoint invokes unchanged original C++ Params methods. Fatal integer mode leaves std::stoi exceptions uncaught."});
    fs::write(
        receipt(&binary),
        serde_json::to_string_pretty(&record)? + "\n",
    )?;
    Ok(binary)
}
struct BuildRunner;
impl Runner for BuildRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run(spec)
    }
}
impl ParamsRunner for BuildRunner {
    fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
        Err("unused Params build locking seam".into())
    }
}
fn root() -> PathBuf {
    std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--native", true),
            OptionSpec::value("--reference", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-selfdrive-params --native CANDIDATE --reference ORIGINAL_CPP_HELPER --output DIRECTORY");
        return Ok(());
    }
    check(
        &root(),
        Path::new(&args.values["--native"]),
        Path::new(&args.values["--reference"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
pub fn build_cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--capnp-prefix", false),
        ],
    )?;
    if args.help {
        println!(
            "usage: build-selfdrive-params-reference --output DIRECTORY [--capnp-prefix PREFIX]"
        );
        return Ok(());
    }
    let binary = build_reference(
        &root(),
        Path::new(&args.values["--output"]),
        args.values.get("--capnp-prefix").map(Path::new),
        &mut BuildRunner,
    )?;
    println!("{}", binary.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuring_inputs_does_not_create_a_missing_candidate_store() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("candidate-store");
        let case = Case {
            key: "NNFFModelName".into(),
            operation: "text".into(),
            bytes: Some(String::new()),
            special: None,
        };
        assert!(configure(&missing, &case).is_err());
        assert!(!missing.exists());
    }
    #[test]
    fn every_original_source_pin_is_bound_to_the_independent_capture() {
        let original = manifest().unwrap();
        for name in original["sources"].as_object().unwrap().keys() {
            let mut removed = original.clone();
            removed["sources"].as_object_mut().unwrap().remove(name);
            assert!(
                decode_manifest(&serde_json::to_string(&removed).unwrap()).is_err(),
                "omitted {name}"
            );
            let mut changed = original.clone();
            changed["sources"][name] = serde_json::Value::String("0".repeat(64));
            assert!(
                decode_manifest(&serde_json::to_string(&changed).unwrap()).is_err(),
                "changed {name}"
            );
        }
    }
    #[test]
    fn wire_json_keeps_python_numeric_equality_and_surrogate_metadata() {
        let captured = include_bytes!("../tests/fixtures/selfdrive-params/boundaries.json");
        assert_eq!(
            sha(captured),
            "f312677f29ec67355e177c07cd57e53e28db02b3b7cc93a74580d9031ea54c91"
        );
        let original: serde_json::Value = serde_json::from_slice(captured).unwrap();
        assert_eq!(original["checker_sha256"], CHECKER_SHA);
        assert_eq!(original["cases"].as_array().unwrap().len(), 15);
        for packet in original["accepted_packets"].as_array().unwrap() {
            let actual =
                decode_record(&unhex(packet["packet_hex"].as_str().unwrap()).unwrap()).unwrap();
            let expected = json::parse_text(packet["record_json"].as_str().unwrap()).unwrap();
            assert!(json::equal(&actual, &expected), "{}", packet["mode"]);
        }
        for payload in [
            br#"{"msg":"warning","levelnum":30.0,"level":"WARNING","metadata":"\ud800"}"#
                .as_slice(),
            br#"{"msg":"marker","levelnum":true}"#.as_slice(),
        ] {
            let mut packet = vec![if payload.windows(4).any(|part| part == b"true") {
                1
            } else {
                30
            }];
            packet.extend_from_slice(payload);
            let value = decode_record(&packet).unwrap();
            assert!(value.get("msg").is_some());
            if let Some(metadata) = value.get("metadata") {
                assert!(metadata.to_utf8().is_none());
                assert_eq!(metadata.to_json().unwrap(), "\"\\ud800\"");
            }
        }
        let mut packet = vec![10];
        packet.extend_from_slice(b"\xff\xfe");
        for unit in r#"{"msg":"marker","levelnum":10}"#.encode_utf16() {
            packet.extend_from_slice(&unit.to_le_bytes());
        }
        assert!(decode_record(&packet)
            .unwrap()
            .get("msg")
            .unwrap()
            .text_eq("marker"));
        assert!(decode_record(b"\x1e{\"levelnum\":31}").is_err());
    }
    #[test]
    fn captured_cython_text_boolean_integer_values_and_warning_messages() {
        let corpus = cases(&root()).unwrap();
        for (case, pinned) in corpus.cases.iter().zip(&corpus.expected) {
            if case.special.as_deref() == Some("fatal") {
                continue;
            }
            let response = if case.key == "NotARegisteredKey" {
                "unknown".to_owned()
            } else if case.operation == "text" {
                if matches!(case.special.as_deref(), Some("directory" | "permission")) {
                    String::new()
                } else {
                    case.bytes.clone().unwrap_or_default()
                }
            } else if case.operation == "boolean" {
                if case.bytes.as_deref() == Some("31") {
                    String::from("true")
                } else {
                    String::from("false")
                }
            } else {
                match case.bytes.as_deref() {
                    Some("202b3132737566666978") => "12",
                    Some("2d32313437343833363438") => "-2147483648",
                    Some("32313437343833363437") => "2147483647",
                    Some("313233007461696c") => "123",
                    Some("31") => "1",
                    _ => "0",
                }
                .to_owned()
            };
            let (value, messages) = source_value(&case.operation, &response, &case.key).unwrap();
            assert!(
                json::equal(
                    &value,
                    &json::parse_text(&serde_json::to_string(&pinned.value).unwrap()).unwrap()
                ),
                "{} {:?}",
                case.key,
                case.bytes
            );
            assert_eq!(messages, pinned.messages);
        }
    }
    #[test]
    fn raw_log_packet_severity_and_marker_are_not_canned_candidate_output() {
        let warning = record("diagnostic", true).unwrap();
        assert_eq!(warning[0], 30);
        let value: serde_json::Value = serde_json::from_slice(&warning[1..]).unwrap();
        assert_eq!(value["msg"], "diagnostic");
        assert_eq!(value["level"], "WARNING");
        assert_eq!(value["process"], std::process::id());
        let marker = record("barrier-0", false).unwrap();
        assert_eq!(marker[0], 10);
    }
}
