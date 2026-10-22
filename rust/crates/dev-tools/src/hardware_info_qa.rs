//! Native runner for the captured HardwareInfo source-oracle corpus.
//!
//! Integration API: [`run`] accepts a native binary, immutable plans.json and
//! source-evidence directory, an output directory, optional case regex,
//! and an injectable bounded [`qa_process::Runner`]. The plan describes inputs;
//! expected values always come from the independently captured original tree.
use crate::{qa_process, seal::Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{ErrorKind, Read, Seek, SeekFrom, Write},
    os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStringExt,
            fs::{MetadataExt, PermissionsExt},
            net::UnixDatagram,
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const SHIM_SUDO: &str = "#!/bin/sh\nprintf '%s\\0' sudo \"$@\" >> \"$HW_INFO_COMMAND_LOG\"\nprintf '\\0' >> \"$HW_INFO_COMMAND_LOG\"\n[ \"$1\" = cat ] || exit 91\nshift\nfor item do\n  case \"$item\" in \"$HW_INFO_FIXTURE_ROOT\"/*) ;; *) exit 92 ;; esac\ndone\nexec /bin/cat \"$@\"\n";
const SHIM_IWLIST: &str = "#!/bin/sh\nprintf '%s\\0' iwlist \"$@\" >> \"$HW_INFO_COMMAND_LOG\"\nprintf '\\0' >> \"$HW_INFO_COMMAND_LOG\"\n[ \"$1\" = wlan0 ] && [ \"$2\" = scan ] || exit 93\nif [ -f \"$HW_INFO_FIXTURE_ROOT/commands/iwlist.stdout\" ]; then /bin/cat \"$HW_INFO_FIXTURE_ROOT/commands/iwlist.stdout\"; fi\ncode=0\nif [ -f \"$HW_INFO_FIXTURE_ROOT/commands/iwlist.status\" ]; then IFS= read -r code < \"$HW_INFO_FIXTURE_ROOT/commands/iwlist.status\"; fi\nexit \"$code\"\n";

#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub require_retired: bool,
    pub binary: PathBuf,
    pub runner: Vec<OsString>,
    pub plans: PathBuf,
    pub source_evidence: PathBuf,
    pub output: PathBuf,
    pub cases: Option<String>,
}
#[derive(Deserialize)]
struct Plans {
    source_commit: String,
    environment: BTreeMap<String, Option<Vec<u8>>>,
    cases: Vec<PlanCase>,
}
#[derive(Clone, Deserialize, Serialize)]
struct PlanCase {
    name: String,
    hardware: String,
    pc: Option<bool>,
    darwin: bool,
    endpoint_file: bool,
    steps: Vec<Value>,
    wire: Vec<WirePlan>,
}
#[derive(Clone, Deserialize, Serialize)]
struct WirePlan {
    request_hex: String,
    replies_hex: Vec<String>,
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn decode_hex(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err("odd-length hexadecimal packet".into());
    }
    if !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err("packet contains non-ASCII hexadecimal bytes".into());
    }
    bytes
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect()
}
pub fn valid_case_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.bytes().any(|byte| byte == b'/' || byte == 0)
}
const MAX_EVIDENCE_BYTES: u64 = 128 * 1024 * 1024;
fn read_json(path: &Path) -> Result<Value> {
    let bytes = read_evidence(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn read_evidence(path: &Path) -> Result<Vec<u8>> {
    let compressed;
    let actual = if path.exists() {
        path
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(".gz");
        compressed = PathBuf::from(name);
        &compressed
    };
    let file = fs::File::open(actual)?;
    let mut bytes = Vec::new();
    if actual.extension().is_some_and(|e| e == "gz") {
        flate2::read::GzDecoder::new(file)
            .take(MAX_EVIDENCE_BYTES + 1)
            .read_to_end(&mut bytes)?;
    } else {
        file.take(MAX_EVIDENCE_BYTES + 1).read_to_end(&mut bytes)?;
    }
    if bytes.len() as u64 > MAX_EVIDENCE_BYTES {
        return Err("HardwareInfo evidence exceeds 128 MiB bound".into());
    }
    Ok(bytes)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map(|mut b| {
            b.push(b'\n');
            b
        })?,
    )?;
    Ok(())
}
fn create_shims(bin: &Path) -> Result<()> {
    fs::create_dir(bin)?;
    for (name, body) in [("sudo", SHIM_SUDO), ("iwlist", SHIM_IWLIST)] {
        let path = bin.join(name);
        fs::write(&path, body)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

struct PeerRecord {
    request_hex: String,
    peer_hex: String,
    replies_hex: Vec<String>,
}
struct Peer {
    stop: Arc<AtomicBool>,
    records: Arc<Mutex<Vec<PeerRecord>>>,
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Peer {
    fn start(path: &Path, plans: Vec<WirePlan>) -> Result<Self> {
        let socket = UnixDatagram::bind(path)?;
        socket.set_read_timeout(Some(Duration::from_millis(50)))?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let records = Arc::new(Mutex::new(Vec::<PeerRecord>::new()));
        let failure = Arc::new(Mutex::new(None::<String>));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread_stop = stop.clone();
        let thread_records = records.clone();
        let thread_failure = failure.clone();
        let thread = thread::spawn(move || {
            let _ = ready_tx.send(());
            let deadline = Instant::now() + Duration::from_secs(92);
            let mut buffer = [0u8; 16384];
            while !thread_stop.load(Ordering::Acquire) && Instant::now() < deadline {
                match socket.recv_from(&mut buffer) {
                    Ok((length, address)) => {
                        let result =
                            (|| -> Result<()> {
                                let addr = address
                                    .as_abstract_name()
                                    .ok_or("WPA peer is not an abstract Unix address")?;
                                let peer_name = addr
                                    .strip_prefix(b"openpilot-wpa-")
                                    .ok_or("unexpected WPA peer name")?;
                                let mut fields = peer_name.split(|b| *b == b'-');
                                if !fields.next().is_some_and(|v| {
                                    !v.is_empty() && v.iter().all(u8::is_ascii_digit)
                                }) || !fields.next().is_some_and(|v| {
                                    !v.is_empty() && v.iter().all(u8::is_ascii_digit)
                                }) || fields.next().is_some()
                                {
                                    return Err("unexpected WPA peer name".into());
                                }
                                let mut rs = thread_records
                                    .lock()
                                    .map_err(|_| "peer record lock poisoned")?;
                                let index = rs.len();
                                let plan = plans.get(index).ok_or("unexpected WPA datagram")?;
                                let request = decode_hex(&plan.request_hex)?;
                                if buffer[..length] != request {
                                    return Err(format!(
                                        "WPA request {index} differs from captured plan"
                                    )
                                    .into());
                                }
                                let mut peer_bytes = vec![0];
                                peer_bytes.extend_from_slice(addr);
                                let mut record = PeerRecord {
                                    request_hex: encode_hex(&buffer[..length]),
                                    peer_hex: encode_hex(&peer_bytes),
                                    replies_hex: Vec::new(),
                                };
                                for reply in &plan.replies_hex {
                                    let bytes = decode_hex(reply)?;
                                    socket.send_to_addr(&bytes, &address)?;
                                    record.replies_hex.push(encode_hex(&bytes));
                                }
                                rs.push(record);
                                Ok(())
                            })();
                        if let Err(e) = result {
                            if let Ok(mut failure) = thread_failure.lock() {
                                *failure = Some(e.to_string());
                            }
                            break;
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                        ) => {}
                    Err(e) => {
                        if let Ok(mut failure) = thread_failure.lock() {
                            *failure = Some(e.to_string());
                        }
                        break;
                    }
                }
            }
        });
        let owner = Self {
            stop,
            records,
            failure,
            thread: Some(thread),
        };
        ready_rx.recv_timeout(Duration::from_secs(2))?;
        Ok(owner)
    }
    fn finish(mut self, expected: usize) -> Result<Vec<Value>> {
        let drain_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < drain_deadline {
            if self
                .failure
                .lock()
                .ok()
                .and_then(|e| e.as_ref().map(|_| true))
                .unwrap_or(false)
            {
                break;
            }
            if self
                .records
                .lock()
                .map(|r| r.len() >= expected)
                .unwrap_or(true)
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            handle.join().map_err(|_| "WPA peer thread panicked")?;
        }
        if let Some(error) = self
            .failure
            .lock()
            .map_err(|_| "peer failure lock poisoned")?
            .take()
        {
            return Err(error.into());
        }
        let records = self
            .records
            .lock()
            .map_err(|_| "peer record lock poisoned")?;
        if records.len() != expected {
            return Err(format!("WPA request count {} != {expected}", records.len()).into());
        }
        Ok(records.iter().map(|r| json!({"request_hex":r.request_hex,"peer_hex":r.peer_hex,"replies_hex":r.replies_hex})).collect())
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub fn canonical_float_hex(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    let bits = value.to_bits();
    let sign = if bits >> 63 == 1 { "-" } else { "" };
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    if exponent == 0 && fraction == 0 {
        return format!("{sign}0x0.0p+0");
    }
    if exponent == 0 {
        format!("{sign}0x0.{fraction:013x}p-1022")
    } else {
        format!("{sign}0x1.{fraction:013x}p{:+}", exponent - 1023)
    }
}

fn normalized_result(result: &Value) -> Result<Value> {
    let rows = result.as_array().ok_or("result.json is not a list")?;
    let mut values = Vec::new();
    for row in rows {
        let method = row
            .get("method")
            .and_then(Value::as_str)
            .ok_or("result row has no method")?;
        if let Some(raw) = row.get("value_json").and_then(Value::as_str) {
            let parsed = openpilot_log_format::JsonValue::parse(raw)?;
            values.push(json!({"method":point_value(&method.chars().map(u32::from).collect::<Vec<_>>()),"value":canonical_json_view(&parsed)}));
        } else {
            values.push(json!({"method":point_value(&method.chars().map(u32::from).collect::<Vec<_>>()),"error":row["error"]}));
        }
    }
    Ok(Value::Array(values))
}
fn canonical_json_view(value: &openpilot_log_format::JsonValue) -> Value {
    use openpilot_log_format::JsonView;
    match value.view() {
        JsonView::Null => json!(["null"]),
        JsonView::Bool(b) => json!(["bool", b]),
        JsonView::Integer(s) => json!(["int", s]),
        JsonView::Float(f) => json!(["float", canonical_float_hex(f)]),
        JsonView::Text(points) => json!(["str", point_value(points)]),
        JsonView::Array(items) => json!([
            "list",
            items.iter().map(canonical_json_view).collect::<Vec<_>>()
        ]),
        JsonView::Object(fields) => {
            let mut fields = fields;
            fields.sort_by(|a, b| a.0.cmp(b.0));
            json!([
                "dict",
                fields
                    .into_iter()
                    .map(|(k, v)| json!([point_value(k), canonical_json_view(&v)]))
                    .collect::<Vec<_>>()
            ])
        }
    }
}
fn point_value(points: &[u32]) -> Value {
    Value::Array(points.iter().map(|p| json!(p)).collect())
}
fn text_points(value: &openpilot_log_format::JsonValue) -> Result<Vec<u32>> {
    match value.view() {
        openpilot_log_format::JsonView::Text(points) => Ok(points.to_vec()),
        _ => Err("expected evidence string".into()),
    }
}
fn array_items(
    value: &openpilot_log_format::JsonValue,
) -> Result<Vec<openpilot_log_format::JsonValue>> {
    match value.view() {
        openpilot_log_format::JsonView::Array(items) => Ok(items),
        _ => Err("expected evidence array".into()),
    }
}
fn canonical_expected_tag(value: &openpilot_log_format::JsonValue) -> Result<Value> {
    use openpilot_log_format::JsonView;
    let JsonView::Array(fields) = value.view() else {
        return Err("expected canonical value tag".into());
    };
    if fields.is_empty() || fields.len() > 2 {
        return Err("malformed canonical value tag".into());
    }
    let tag = fields[0]
        .to_utf8()
        .ok_or("canonical value tag is not text")?;
    if tag == "null" {
        if fields.len() != 1 {
            return Err("malformed null tag".into());
        }
        return Ok(json!(["null"]));
    }
    if fields.len() != 2 {
        return Err("canonical value tag lacks payload".into());
    }
    let payload = fields[1].clone();
    match tag.as_str() {
        "str" => Ok(json!(["str", point_value(&text_points(&payload)?)])),
        "list" => Ok(json!([
            "list",
            array_items(&payload)?
                .iter()
                .map(canonical_expected_tag)
                .collect::<Result<Vec<_>>>()?
        ])),
        "dict" => {
            let mut pairs = Vec::new();
            for pair in array_items(&payload)? {
                let pair = array_items(&pair)?;
                if pair.len() != 2 {
                    return Err("bad canonical dictionary entry".into());
                }
                pairs.push(json!([
                    point_value(&text_points(&pair[0])?),
                    canonical_expected_tag(&pair[1])?
                ]));
            }
            Ok(json!(["dict", pairs]))
        }
        "bool" => match payload.view() {
            JsonView::Bool(v) => Ok(json!(["bool", v])),
            _ => Err("bad canonical bool payload".into()),
        },
        "int" | "float" => Ok(json!([
            tag,
            payload
                .to_utf8()
                .ok_or("canonical numeric payload is not text")?
        ])),
        _ => Err("unknown canonical value tag".into()),
    }
}

fn expected_normalized(bytes: &[u8]) -> Result<Value> {
    let parsed = openpilot_log_format::JsonValue::parse(std::str::from_utf8(bytes)?)?;
    let values = array_items(
        &parsed
            .get("values")
            .ok_or("source normalized data lacks values")?,
    )?;
    let mut normalized_values = Vec::new();
    for row in values {
        let method = row.get("method").ok_or("normalized row lacks method")?;
        let method = point_value(&text_points(&method)?);
        if let Some(value) = row.get("value") {
            normalized_values
                .push(json!({"method":method,"value":canonical_expected_tag(&value)?}));
        } else {
            let error = row.get("error").ok_or("normalized row lacks value/error")?;
            normalized_values.push(
                json!({"method":method,"error":error.to_utf8().ok_or("error name is not text")?}),
            );
        }
    }
    let commands = parsed
        .get("commands")
        .ok_or("source normalized data lacks commands")?
        .to_utf8()
        .ok_or("commands normalization is not text")?;
    let wire = array_items(
        &parsed
            .get("wire")
            .ok_or("source normalized data lacks wire")?,
    )?;
    let mut normalized_wire = Vec::new();
    for item in wire {
        let replies = array_items(&item.get("replies_hex").ok_or("wire item lacks replies")?)?
            .iter()
            .map(|reply| {
                reply
                    .to_utf8()
                    .ok_or_else(|| "wire reply is not text".into())
            })
            .collect::<Result<Vec<String>>>()?;
        normalized_wire.push(json!({"request_hex":item.get("request_hex").ok_or("wire item lacks request")?.to_utf8().ok_or("wire request is not text")?,"replies_hex":replies}));
    }
    Ok(json!({"values":normalized_values,"commands":commands,"wire":normalized_wire}))
}

fn raw_commands(path: &Path, root: &Path) -> Result<(Vec<u8>, String)> {
    let raw = fs::read(path).or_else(|e| {
        if e.kind() == ErrorKind::NotFound {
            Ok(Vec::new())
        } else {
            Err(e)
        }
    })?;
    let normalized = encode_hex(&normalize_fixture_bytes(&raw, root));
    Ok((raw, normalized))
}

fn normalize_fixture_bytes(raw: &[u8], root: &Path) -> Vec<u8> {
    let from = root.as_os_str().as_encoded_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(from) {
            out.extend_from_slice(b"$FIXTURE");
            i += from.len();
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    out
}

struct KeyfileEvidence<'a> {
    source_stderr: &'a [u8],
    native_stderr: &'a [u8],
    source_root: &'a Path,
    native_root: &'a Path,
}

// Path.glob and read_dir both use filesystem enumeration order. In the original
// keyfile-read-errors fixture, the two first keyfiles are unreadable (mode 0)
// and a directory, respectively; sudo_read returns empty for either failure.
// Only those failed reads may commute. The successful fallback c must remain last.
fn failed_keyfile_order(commands: &str, stderr: &[u8]) -> Result<&'static str> {
    const A: &[u8] =
        b"sudo\0cat\0$FIXTURE/run/NetworkManager/system-connections/a.nmconnection\0\0";
    const B: &[u8] =
        b"sudo\0cat\0$FIXTURE/run/NetworkManager/system-connections/b.nmconnection\0\0";
    const C: &[u8] =
        b"sudo\0cat\0$FIXTURE/data/etc/NetworkManager/system-connections/c.nmconnection\0\0";
    const A_ERROR: &[u8] = b"/bin/cat: $FIXTURE/run/NetworkManager/system-connections/a.nmconnection: Permission denied\n";
    const B_ERROR: &[u8] = b"/bin/cat: $FIXTURE/run/NetworkManager/system-connections/b.nmconnection: Is a directory\n";
    let commands = decode_hex(commands)?;
    for (label, first, second, first_error, second_error) in [
        ("a,b,c", A, B, A_ERROR, B_ERROR),
        ("b,a,c", B, A, B_ERROR, A_ERROR),
    ] {
        if commands == [first, second, C].concat() && stderr == [first_error, second_error].concat()
        {
            return Ok(label);
        }
    }
    Err("keyfile failed-read order, count, or diagnostics differ".into())
}

fn compare_case(
    name: &str,
    expected: &Value,
    native: &Value,
    keyfile: Option<KeyfileEvidence<'_>>,
) -> Result<Option<Value>> {
    if name != "keyfile-read-errors" {
        if expected != native {
            return Err("original and native HardwareInfo observations differ".into());
        }
        return Ok(None);
    }
    if expected["values"] != native["values"] || expected["wire"] != native["wire"] {
        return Err("keyfile values or WPA wire differ".into());
    }
    let evidence = keyfile.ok_or("keyfile stderr evidence missing")?;
    let source_stderr = normalize_fixture_bytes(evidence.source_stderr, evidence.source_root);
    let native_stderr = normalize_fixture_bytes(evidence.native_stderr, evidence.native_root);
    let source_commands = expected["commands"]
        .as_str()
        .ok_or("original keyfile commands missing")?;
    let native_commands = native["commands"]
        .as_str()
        .ok_or("native keyfile commands missing")?;
    let source_order = failed_keyfile_order(source_commands, &source_stderr)?;
    let native_order = failed_keyfile_order(native_commands, &native_stderr)?;
    Ok(Some(
        json!({"policy":"keyfile-read-errors failed-read commutation only",
        "source_order":source_order,"native_order":native_order,
        "source_commands_normalized_hex":source_commands,
        "native_commands_normalized_hex":native_commands,
        "source_stderr_normalized_hex":encode_hex(&source_stderr),
        "native_stderr_normalized_hex":encode_hex(&native_stderr)}),
    ))
}

fn lexical_absolute(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            std::path::Component::RootDir => out.push("/"),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn canonical_future_path(path: &Path) -> Result<PathBuf> {
    let absolute = lexical_absolute(path);
    let mut ancestor = absolute.clone();
    while !ancestor.exists() {
        if !ancestor.pop() {
            return Err("no existing ancestor for output path".into());
        }
    }
    let canonical = ancestor.canonicalize()?;
    let tail = absolute.strip_prefix(&ancestor)?;
    Ok(canonical.join(tail))
}

fn resolve_runner(arguments: &[OsString], checkout: &Path) -> Result<Vec<OsString>> {
    let Some(program) = arguments.first() else {
        return Ok(Vec::new());
    };
    let executable = |path: &Path| {
        fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    let path = if program.as_encoded_bytes().contains(&b'/') {
        let candidate = checkout.join(program);
        if !executable(&candidate) {
            return Err("HardwareInfo runner is not executable".into());
        }
        candidate
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| checkout.join(directory).join(program))
            .find(|candidate| executable(candidate))
            .ok_or("HardwareInfo runner is not on PATH")?
    };
    let mut resolved = arguments.to_vec();
    resolved[0] = path.canonicalize()?.into_os_string();
    Ok(resolved)
}

/// Run selected captured plans against the actual native HardwareInfo example.
/// The output directory may already exist, but each selected case directory must
/// be new. `root` is the source checkout; each case gets its own fixture root.
pub fn run(options: &Options, runner: &mut dyn qa_process::Runner) -> Result<Value> {
    run_with_emit(options, runner, |_| Ok(()))
}

struct PendingPublication {
    path: PathBuf,
    file: fs::File,
    committed: bool,
}
impl PendingPublication {
    fn verify_owned(&self, expected: &[u8]) -> Result<()> {
        let owned = self.file.metadata()?;
        let current = fs::symlink_metadata(&self.path)?;
        if !current.is_file()
            || (owned.dev(), owned.ino()) != (current.dev(), current.ino())
            || owned.len() != expected.len() as u64
        {
            return Err("HardwareInfo success manifest ownership changed".into());
        }
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut contents = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut contents)?;
        if contents != expected
            || !fs::symlink_metadata(&self.path)
                .is_ok_and(|m| (m.dev(), m.ino()) == (owned.dev(), owned.ino()))
        {
            return Err("HardwareInfo success manifest bytes changed".into());
        }
        Ok(())
    }
}
impl Drop for PendingPublication {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let (Ok(owned), Ok(current)) = (self.file.metadata(), fs::symlink_metadata(&self.path)) {
            if (owned.dev(), owned.ino()) == (current.dev(), current.ino()) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
fn finalize_report(
    path: &Path,
    report: &Value,
    verify: impl Fn() -> Result<()>,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<()> {
    verify()?;
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("manifest parent missing")?)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    let file = temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    let mut publication = PendingPublication {
        path: path.into(),
        file,
        committed: false,
    };
    emit(report)?;
    verify()?;
    publication.verify_owned(&bytes)?;
    publication.committed = true;
    Ok(())
}

fn run_with_emit(
    options: &Options,
    runner: &mut dyn qa_process::Runner,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<Value> {
    let checkout = options.root.canonicalize()?;
    let resolve_existing = |path: &Path| -> Result<PathBuf> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            checkout.join(path)
        };
        Ok(path.canonicalize()?)
    };
    let binary = resolve_existing(&options.binary)?;
    let runner_arguments = resolve_runner(&options.runner, &checkout)?;
    let source_evidence = resolve_existing(&options.source_evidence)?;
    let plans_path = resolve_existing(&options.plans)?;
    let output_requested = if options.output.is_absolute() {
        options.output.clone()
    } else {
        checkout.join(&options.output)
    };
    let output_requested = canonical_future_path(&output_requested)?;
    if output_requested.starts_with(&source_evidence)
        || source_evidence.starts_with(&output_requested)
    {
        return Err("HardwareInfo output cannot overlap original evidence".into());
    }
    let receipt = crate::hardware_info_capture::validate_for_runner(
        &checkout,
        &source_evidence,
        &plans_path,
        options.require_retired,
        &binary,
        &runner_arguments,
    )?;
    fs::create_dir_all(&output_requested)?;
    let output = output_requested.canonicalize()?;
    write_json(&output.join("input-receipt.before.json"), &receipt.report())?;
    let plans: Plans = serde_json::from_slice(&read_evidence(&plans_path)?)?;
    if plans.source_commit.len() != 40
        || !plans.source_commit.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid source commit in plans".into());
    }
    let selected = if let Some(pattern) = &options.cases {
        let re = regex::Regex::new(pattern)?;
        plans
            .cases
            .into_iter()
            .filter(|c| re.is_match(&c.name))
            .collect::<Vec<_>>()
    } else {
        plans.cases
    };
    if selected.is_empty() {
        return Err("HardwareInfo case selection is empty".into());
    }
    if selected.iter().any(|c| !valid_case_name(&c.name)) {
        return Err("invalid HardwareInfo case name".into());
    }
    let temp = tempfile::Builder::new().prefix("hwi-native-").tempdir()?;
    let shims = temp.path().join("bin");
    create_shims(&shims)?;
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let mut scenarios = Vec::new();
    let mut comparisons = 0usize;
    let mut packets = 0usize;
    for (index, case) in selected.iter().enumerate() {
        let source = source_evidence.join(&case.name).join("source");
        let original_request = read_json(&source.join("request.json"))?;
        let expected_request = original_request
            .get("steps")
            .ok_or("source request missing steps")?;
        if expected_request != &Value::Array(case.steps.clone())
            || original_request
                .get("hardware")
                .and_then(Value::as_str)
                .unwrap_or("tici")
                != case.hardware
            || original_request.get("pc").and_then(Value::as_bool) != case.pc
            || original_request
                .get("darwin")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                != case.darwin
        {
            return Err(format!("plan/source request differs: {}", case.name).into());
        }
        let folder = output.join(&case.name);
        fs::create_dir(&folder)?;
        let fixture_root = folder.join("root");
        fs::create_dir(&fixture_root)?;
        let endpoint = temp.path().join(format!("w{index}"));
        let peer = if case.wire.is_empty() {
            None
        } else {
            Some(Peer::start(&endpoint, case.wire.clone())?)
        };
        if case.endpoint_file {
            fs::write(&endpoint, b"not a socket")?;
        }
        let request = json!({"root":fixture_root,"hardware":case.hardware,"wpa_endpoint":endpoint,"steps":case.steps,"darwin":case.darwin,"pc":case.pc});
        let request_path = folder.join("request.json");
        write_json(&request_path, &request)?;
        let command_log = folder.join("commands.bin");
        let mut argv = runner_arguments.clone();
        argv.push(binary.as_os_str().to_owned());
        argv.push(request_path.as_os_str().to_owned());
        argv.push(folder.as_os_str().to_owned());
        let argv_hex = argv
            .iter()
            .map(|arg| encode_hex(arg.as_encoded_bytes()))
            .collect::<Vec<_>>();
        let mut spec = qa_process::CommandSpec::new_os(argv, Duration::from_secs(90));
        spec.cwd = Some(checkout.clone());
        let mut env = std::env::vars_os().collect::<BTreeMap<_, _>>();
        for key in [
            "HOME",
            "OPENPILOT_PREFIX",
            "LOG_ROOT",
            "COMMA_CACHE",
            "PYTHONPATH",
            "PYTHONHOME",
        ] {
            env.remove(std::ffi::OsStr::new(key));
        }
        let mut path_bytes = shims.as_os_str().as_encoded_bytes().to_vec();
        path_bytes.push(b':');
        path_bytes.extend_from_slice(path_env.as_encoded_bytes());
        env.insert("PATH".into(), OsString::from_vec(path_bytes));
        env.insert(
            "PARAMS_ROOT".into(),
            fixture_root.join("params").into_os_string(),
        );
        env.insert(
            "HW_INFO_FIXTURE_ROOT".into(),
            fixture_root.as_os_str().to_owned(),
        );
        env.insert(
            "HW_INFO_COMMAND_LOG".into(),
            command_log.as_os_str().to_owned(),
        );
        for (name, value) in &plans.environment {
            let key = OsString::from(name);
            if let Some(bytes) = value {
                env.insert(key, OsString::from_vec(bytes.clone()));
            } else {
                env.remove(&key);
            }
        }
        spec.set_os_env(env);
        write_json(
            &folder.join("invocation.json"),
            &json!({"argv_hex":argv_hex,"timeout_seconds":90,"source_commit":plans.source_commit}),
        )?;
        let captured = runner.run_to_files(
            &spec,
            &folder.join("stdout.log"),
            &folder.join("stderr.log"),
        )?;
        write_json(
            &folder.join("exit-status.json"),
            &json!({"exit_code":captured.code}),
        )?;
        let wire = if let Some(peer) = peer {
            peer.finish(case.wire.len())?
        } else {
            Vec::new()
        };
        fs::write(folder.join("wire.json"), serde_json::to_vec_pretty(&wire)?)?;
        if captured.code != 0 {
            return Err(format!(
                "native HardwareInfo case {} exited {}",
                case.name, captured.code
            )
            .into());
        }
        let result = read_json(&folder.join("result.json"))?;
        let values = normalized_result(&result)?;
        let (raw, commands) = raw_commands(&command_log, &fixture_root)?;
        fs::write(folder.join("commands.bin"), &raw)?;
        let normalized_wire = wire
            .iter()
            .map(|v| json!({"request_hex":v["request_hex"],"replies_hex":v["replies_hex"]}))
            .collect::<Vec<_>>();
        let native = json!({"values":values,"commands":commands,"wire":normalized_wire});
        write_json(&folder.join("native-normalized.json"), &native)?;
        let expected_bytes = read_evidence(&source.join("normalized.json"))?;
        let expected = expected_normalized(&expected_bytes)?;
        let comparison = (|| -> Result<Option<Value>> {
            if case.name == "keyfile-read-errors" {
                let source_commands = read_json(&source.join("commands.json"))?;
                let source_root = Path::new(
                    original_request["root"]
                        .as_str()
                        .ok_or("captured keyfile root missing")?,
                );
                let source_raw = decode_hex(
                    source_commands["raw_hex"]
                        .as_str()
                        .ok_or("captured keyfile raw commands missing")?,
                )?;
                if source_commands["normalized_hex"] != expected["commands"]
                    || encode_hex(&normalize_fixture_bytes(&source_raw, source_root))
                        != expected["commands"]
                {
                    return Err("captured keyfile raw and normalized commands differ".into());
                }
                let source_stderr = read_evidence(&source.join("stderr.log"))?;
                let native_stderr = fs::read(folder.join("stderr.log"))?;
                let mut admission = compare_case(
                    &case.name,
                    &expected,
                    &native,
                    Some(KeyfileEvidence {
                        source_stderr: &source_stderr,
                        native_stderr: &native_stderr,
                        source_root,
                        native_root: &fixture_root,
                    }),
                )?;
                if let Some(value) = admission.as_mut() {
                    value["source_raw_commands_hex"] = source_commands["raw_hex"].clone();
                    value["native_raw_commands_hex"] = json!(encode_hex(&raw));
                }
                Ok(admission)
            } else {
                compare_case(&case.name, &expected, &native, None)
            }
        })();
        let keyfile = match comparison {
            Ok(admission) => admission,
            Err(error) => {
                write_json(
                    &folder.join("failure.json"),
                    &json!({"result":"FAIL","case":case.name,"reason":error.to_string(),"source_canonical":expected,"native_canonical":native}),
                )?;
                return Err(format!("HardwareInfo parity failure: {}: {error}", case.name).into());
            }
        };
        if let Some(admission) = &keyfile {
            write_json(&folder.join("command-order-admission.json"), admission)?;
        }
        let count = values.as_array().map_or(0, Vec::len);
        comparisons += count;
        packets += case.wire.len();
        let mut scenario =
            json!({"scenario":case.name,"result":"PASS","calls":count,"packets":case.wire.len()});
        if let Some(admission) = keyfile {
            scenario["command_order_admission"] = admission;
        }
        scenarios.push(scenario);
    }
    receipt.verify()?;
    write_json(&output.join("input-receipt.after.json"), &receipt.report())?;
    let binary_hash = {
        use sha2::{Digest, Sha256};
        encode_hex(&Sha256::digest(fs::read(&binary)?))
    };
    let report = json!({"result":"PASS","source_commit":plans.source_commit,"scenario_count":scenarios.len(),"comparisons":comparisons,"datagram_requests_per_implementation":packets,"selection":options.cases,"scenarios":scenarios,"binary_sha256":binary_hash});
    finalize_report(
        &output.join("manifest.json"),
        &report,
        || receipt.verify(),
        emit,
    )?;
    Ok(report)
}

/// CLI interface used by the registered `check-hardware-info` binary.
/// BINARY PLANS SOURCE_EVIDENCE OUTPUT [--root PATH] [--require-retired]
/// [--cases REGEX] [--runner PROGRAM ...].
fn parse_cli(
    args: &[OsString],
) -> std::result::Result<Option<Options>, crate::argparse_scan::ArgumentError> {
    use crate::argparse_scan::{scan_os_with_positionals, ArgumentError, OptionSpec};
    let runner_index = args.iter().position(|arg| arg == "--runner");
    let (parse_args, runner) = if let Some(index) = runner_index {
        (&args[..index], args[index + 1..].to_vec())
    } else {
        (args, Vec::new())
    };
    let specs = [
        OptionSpec::value("--root", false),
        OptionSpec::value("--cases", false),
        OptionSpec::flag("--require-retired", None),
    ];
    let parsed = scan_os_with_positionals(parse_args, &specs, 4)?;
    if parsed.help {
        return Ok(None);
    }
    let value = |name: &str| parsed.values.get(name).map(PathBuf::from);
    let cases = parsed
        .values
        .get("--cases")
        .map(|v| {
            v.to_str()
                .map(str::to_owned)
                .ok_or_else(|| ArgumentError("--cases regex must be UTF-8".into()))
        })
        .transpose()?;
    let positionals = &parsed.positionals;
    Ok(Some(Options {
        root: value("--root")
            .unwrap_or(std::env::current_dir().map_err(|e| ArgumentError(e.to_string()))?),
        require_retired: parsed.values.contains_key("--require-retired"),
        binary: PathBuf::from(&positionals[0]),
        plans: PathBuf::from(&positionals[1]),
        source_evidence: PathBuf::from(&positionals[2]),
        output: PathBuf::from(&positionals[3]),
        cases,
        runner,
    }))
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let options =
        parse_cli(&args).map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
    let Some(options) = options else {
        println!("check-hardware-info BINARY PLANS SOURCE_EVIDENCE OUTPUT [--root PATH] [--require-retired] [--cases REGEX] [--runner PROGRAM ...]");
        return Ok(());
    };
    let mut native = qa_process::NativeRunner;
    let stdout = std::io::stdout();
    let mut locked = stdout.lock();
    run_with_emit(&options, &mut native, |report| {
        serde_json::to_writer(&mut locked, report)?;
        locked.write_all(b"\n")?;
        locked.flush()?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use std::io;

    const REPORT: &str = "manifest.json";

    #[test]
    fn late_source_change_removes_only_the_owned_success_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        let manifest = directory.path().join(REPORT);
        fs::write(&source, b"admitted source").unwrap();
        let result = finalize_report(
            &manifest,
            &json!({"result":"PASS"}),
            || {
                if fs::read(&source)? != b"admitted source" {
                    return Err("source changed before final publication".into());
                }
                Ok(())
            },
            |_| {
                assert!(
                    manifest.exists(),
                    "writer runs after provisional publication"
                );
                fs::write(&source, b"mutated source")?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!manifest.exists());
    }

    #[test]
    fn broken_writer_rolls_back_but_foreign_replacement_is_preserved() {
        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join(REPORT);
        let report = json!({"result":"PASS"});
        let result = finalize_report(
            &manifest,
            &report,
            || Ok(()),
            |value| {
                serde_json::to_writer(&mut BrokenWriter, value)?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!manifest.exists());

        let result = finalize_report(
            &manifest,
            &report,
            || Ok(()),
            |_| {
                fs::remove_file(&manifest)?;
                fs::write(&manifest, b"foreign replacement")?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&manifest).unwrap(), b"foreign replacement");
    }
}

#[cfg(test)]
mod keyfile_order_tests {
    use super::*;

    fn evidence<'a>(
        source_stderr: &'a [u8],
        native_stderr: &'a [u8],
        root: &'a Path,
    ) -> KeyfileEvidence<'a> {
        KeyfileEvidence {
            source_stderr,
            native_stderr,
            source_root: root,
            native_root: root,
        }
    }

    fn original() -> (Value, Value, Vec<u8>) {
        let normalized = flate2::read::GzDecoder::new(
            &include_bytes!("../tests/fixtures/hardware-info-original/keyfile-read-errors/source/normalized.json.gz")[..],
        );
        let request = flate2::read::GzDecoder::new(
            &include_bytes!("../tests/fixtures/hardware-info-original/keyfile-read-errors/source/request.json.gz")[..],
        );
        let mut normalized = normalized;
        let mut request = request;
        let mut normalized_bytes = Vec::new();
        let mut request_bytes = Vec::new();
        normalized.read_to_end(&mut normalized_bytes).unwrap();
        request.read_to_end(&mut request_bytes).unwrap();
        (
            expected_normalized(&normalized_bytes).unwrap(),
            serde_json::from_slice(&request_bytes).unwrap(),
            include_bytes!(
                "../tests/fixtures/hardware-info-original/keyfile-read-errors/source/stderr.log"
            )
            .to_vec(),
        )
    }

    fn rows(commands: &str) -> Vec<Vec<u8>> {
        let bytes = decode_hex(commands).unwrap();
        let mut tail = bytes.as_slice();
        let mut rows = Vec::new();
        while !tail.is_empty() {
            let end = tail.windows(2).position(|w| w == b"\0\0").unwrap() + 2;
            rows.push(tail[..end].to_vec());
            tail = &tail[end..];
        }
        rows
    }

    #[test]
    fn only_the_two_independently_observed_failed_reads_may_commute() {
        let (expected, request, source_stderr) = original();
        let root = Path::new(request["root"].as_str().unwrap());
        let original_rows = rows(expected["commands"].as_str().unwrap());
        assert_eq!(original_rows.len(), 3);
        let source_lines = source_stderr
            .split_inclusive(|b| *b == b'\n')
            .collect::<Vec<_>>();
        assert_eq!(source_lines.len(), 2);
        let mut native_rows = original_rows.clone();
        native_rows.swap(0, 1);
        let mut native = expected.clone();
        native["commands"] = json!(encode_hex(&native_rows.concat()));
        let native_stderr = [source_lines[1], source_lines[0]].concat();
        let admission = compare_case(
            "keyfile-read-errors",
            &expected,
            &native,
            Some(evidence(&source_stderr, &native_stderr, root)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(admission["source_order"], "b,a,c");
        assert_eq!(admission["native_order"], "a,b,c");

        for changed in [
            vec![
                original_rows[2].clone(),
                original_rows[0].clone(),
                original_rows[1].clone(),
            ],
            vec![
                original_rows[1].clone(),
                original_rows[1].clone(),
                original_rows[2].clone(),
            ],
            vec![original_rows[0].clone(), original_rows[2].clone()],
        ] {
            let mut bad = native.clone();
            bad["commands"] = json!(encode_hex(&changed.concat()));
            assert!(compare_case(
                "keyfile-read-errors",
                &expected,
                &bad,
                Some(evidence(&source_stderr, &native_stderr, root)),
            )
            .is_err());
        }
        assert!(compare_case(
            "keyfile-read-errors",
            &expected,
            &native,
            Some(evidence(
                &source_stderr,
                &native_stderr[source_lines[1].len()..],
                root,
            )),
        )
        .is_err());
        assert!(compare_case(
            "keyfile-read-errors",
            &expected,
            &native,
            Some(evidence(
                &source_stderr[..source_lines[0].len()],
                &native_stderr,
                root
            )),
        )
        .is_err());
        let wrong_error = native_stderr
            .windows(b"Permission denied".len())
            .position(|w| w == b"Permission denied")
            .unwrap();
        let mut wrong_stderr = native_stderr.clone();
        wrong_stderr.splice(
            wrong_error..wrong_error + b"Permission denied".len(),
            b"No such file".iter().copied(),
        );
        assert!(compare_case(
            "keyfile-read-errors",
            &expected,
            &native,
            Some(evidence(&source_stderr, &wrong_stderr, root)),
        )
        .is_err());
        let mut wrong_value = native.clone();
        wrong_value["values"] = json!([]);
        assert!(compare_case(
            "keyfile-read-errors",
            &expected,
            &wrong_value,
            Some(evidence(&source_stderr, &native_stderr, root)),
        )
        .is_err());
        let mut wrong_wire = native.clone();
        wrong_wire["wire"] = json!([]);
        assert!(compare_case(
            "keyfile-read-errors",
            &expected,
            &wrong_wire,
            Some(evidence(&source_stderr, &native_stderr, root)),
        )
        .is_err());
        assert!(compare_case("another-case", &expected, &native, None).is_err());
    }
}
