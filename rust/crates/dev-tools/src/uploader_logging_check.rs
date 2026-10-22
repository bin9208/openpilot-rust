//! Actual uploader/collector IPC checked against sealed original source observations.
//! Both collector lanes are native processes; the reference lane uses the existing
//! independent original-policy adapter, never the candidate daemon's formatter.
mod ipc;
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::Duration,
};
const LIMIT: u64 = 32 << 20;
const IDS: [&str; 18] = [
    "missing-id",
    "status-500",
    "invalid-json",
    "success",
    "ignored",
    "fake",
    "clear-locks-error",
    "unauthorized-accepted",
    "forbidden-accepted",
    "created",
    "put-ignored",
    "zero-size",
    "oversized",
    "mark-error",
    "metadata-error",
    "header-repr",
    "getsize-error",
    "listdir-error",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    version: u32,
    source_pins: BTreeMap<String, String>,
    cases: Vec<Case>,
    collector_rows: Vec<Value>,
    source_assertions_completed: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    index: usize,
    settings: Value,
    direct: Value,
    source_records_hex: Vec<String>,
    normalized: Value,
    disk_without_id: Value,
    source_directory: String,
    id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Worker {
    root: PathBuf,
    binary: PathBuf,
    trace: PathBuf,
    collector: PathBuf,
    worker: PathBuf,
    work: PathBuf,
    output: PathBuf,
    prefix: String,
    source_collector: bool,
    case: Case,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub trace: PathBuf,
    pub collector: PathBuf,
    pub worker: PathBuf,
    pub fixture: PathBuf,
    pub fixture_sha256: String,
    pub output: PathBuf,
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= limit,
        "uploader logging evidence exceeds bound",
    )?;
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    require(
        text.len().is_multiple_of(2) && text.len() <= 4 << 20,
        "logging hex size invalid",
    )?;
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}
fn canonical_url(text: &str, host: Option<&str>) -> Result<String> {
    let Some(rest) = text.strip_prefix("upload_url v1.4 ") else {
        return Ok(text.into());
    };
    let (url, headers) = rest
        .split_once(' ')
        .ok_or("upload_url log missing header repr")?;
    let parsed = url::Url::parse(url)?;
    require(
        parsed.scheme() == "http"
            && parsed.host_str() == Some("127.0.0.1")
            && parsed.port().is_some()
            && parsed.path() == "/put"
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "upload_url log escaped owned loopback endpoint",
    )?;
    if let Some(host) = host {
        require(
            url == format!("{host}/put"),
            "upload_url log differs from current owned endpoint",
        )?;
    }
    Ok(format!("upload_url v1.4 <owned-loopback>/put {headers}"))
}
pub fn normalized(records: &[Value], directory: &Path, host: Option<&str>) -> Result<Value> {
    let mut output = Vec::new();
    for record in records {
        require(
            record["name"] == "swaglog" && record["levelnum"].is_u64(),
            "uploader record identity missing",
        )?;
        let mut msg = record["msg"].clone();
        if let Some(fields) = msg.as_object_mut() {
            if let Some(path) = fields.get("fn") {
                let relative =
                    Path::new(path.as_str().ok_or("fn is not a path")?).strip_prefix(directory)?;
                fields.insert("fn".into(), json!(relative));
            }
            if let Some(speed) = fields.get("speed") {
                require(
                    speed.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0),
                    "negative/nonfinite measured speed",
                )?;
                fields.insert("speed".into(), json!("<measured>"));
            }
            if let Some(exc) = fields.get("exc").filter(|v| !v.is_null()) {
                require(
                    exc.as_array().is_some_and(|a| {
                        a.len() == 2 && a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                    }),
                    "invalid uploader exception pair",
                )?;
                fields.insert(
                    "exc".into(),
                    json!([
                        "<native or Python error representation>",
                        "<native or Python trace>"
                    ]),
                );
            }
        } else if let Some(text) = msg.as_str() {
            msg = json!(canonical_url(text, host)?);
        }
        let mut projected =
            json!({"level":record["level"],"levelnum":record["levelnum"],"msg":msg});
        if let Some(exc) = record.get("exc_info") {
            require(
                exc.as_str().is_some_and(|s| !s.is_empty()),
                "empty error traceback",
            )?;
            projected["exc_info"] = json!("<native or Python trace>");
        }
        output.push(projected);
    }
    Ok(json!(output))
}
fn fixed(key: &str, value: &Value) -> (String, Value) {
    let suffix = match value {
        Value::String(_) => "$s",
        Value::Bool(_) => "$b",
        Value::Number(n) if n.is_f64() => "$f",
        Value::Number(_) => "$i",
        Value::Array(_) => "$a",
        _ => "",
    };
    let value = if let Some(fields) = value.as_object() {
        Value::Object(fields.iter().map(|(k, v)| fixed(k, v)).collect())
    } else {
        value.clone()
    };
    (format!("{key}{suffix}"), value)
}
/// Independent SwagLogFileFormatter projection. It transforms only msg; UUID
/// is checked separately. No LogFiles/native collector formatter is invoked.
pub fn disk_projection(record: &Value) -> Result<Value> {
    let mut fields = record
        .as_object()
        .ok_or("log record is not an object")?
        .clone();
    let msg = fields.remove("msg").ok_or("log message absent")?;
    let (key, value) = fixed("msg", &msg);
    fields.insert(key, value);
    fields.remove("id");
    Ok(Value::Object(fields))
}
fn source_records(case: &Case) -> Result<Vec<Value>> {
    case.source_records_hex
        .iter()
        .map(|raw| {
            let bytes = unhex(raw)?;
            let (&level, body) = bytes.split_first().ok_or("empty source log capture")?;
            let value: Value = serde_json::from_slice(body)?;
            require(
                value["levelnum"] == u64::from(level),
                "source logging level byte differs",
            )?;
            Ok(value)
        })
        .collect()
}
fn load(options: &Options) -> Result<Corpus> {
    let bytes = bounded(&options.fixture, LIMIT)?;
    require(
        hash(&bytes) == options.fixture_sha256,
        "uploader logging original SHA mismatch",
    )?;
    let corpus: Corpus = serde_json::from_slice(&bytes)?;
    require(
        corpus.version == 1
            && corpus.source_assertions_completed
            && corpus.cases.len() == 18
            && corpus.collector_rows.len() == 36,
        "uploader logging source roster differs",
    )?;
    for (index, case) in corpus.cases.iter().enumerate() {
        require(
            case.index == index && case.id == IDS[index],
            "uploader logging cases reordered/missing",
        )?;
        let source = source_records(case)?;
        let expected = normalized(&source, Path::new(&case.source_directory), None)?;
        let mut saved = case.normalized.clone();
        for value in saved.as_array_mut().ok_or("normalized source missing")? {
            if let Some(text) = value["msg"].as_str() {
                value["msg"] = json!(canonical_url(text, None)?);
            }
        }
        require(
            expected == saved,
            "captured normalized source disagrees with actual original records",
        )?;
        let disks = source
            .iter()
            .filter(|r| r["levelnum"].as_u64().unwrap_or(0) >= 20)
            .map(disk_projection)
            .collect::<Result<Vec<_>>>()?;
        require(
            json!(disks) == case.disk_without_id,
            "original formatter observations disagree with independent projection",
        )?;
    }
    for (name, pin) in &corpus.source_pins {
        require(
            Path::new(name)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "unsafe pinned source path",
        )?;
        require(
            hash(&bounded(&options.root.join(name), 8 << 20)?) == *pin,
            "uploader logging retained source changed",
        )?;
    }
    Ok(corpus)
}
fn fixture(root: &Path, settings: &Value) -> Result<()> {
    fs::create_dir_all(root)?;
    if settings["broken"] == true {
        fs::create_dir(root.join("route--0"))?;
        symlink("absent", root.join("route--0/qlog"))?;
    } else if settings["empty_root"] != true {
        let dir = root.join("route--0");
        fs::create_dir(&dir)?;
        let size = settings["size"].as_u64().unwrap_or(128);
        require(size <= 25_000_001, "logging fixture file too large")?;
        let mut file = File::create(dir.join("qlog"))?;
        if size > 100_000 {
            file.set_len(size)?;
        } else {
            file.write_all(
                &(0..size)
                    .map(|i| ((i * 23 + 17) % 256) as u8)
                    .collect::<Vec<_>>(),
            )?;
        }
        if settings["readonly"] == true {
            fs::set_permissions(dir.join("qlog"), fs::Permissions::from_mode(0o444))?;
        }
    }
    if settings["clear_error"] == true {
        fs::write(root.join("plain"), b"fixture")?;
    }
    if settings["no_listing"] == true {
        fs::set_permissions(root, fs::Permissions::from_mode(0o111))?;
    }
    Ok(())
}
struct Permission(PathBuf);
impl Drop for Permission {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}
fn native_origin(
    root: &Path,
    records: &[Value],
    case: &str,
    directory: &Path,
) -> Result<Vec<Value>> {
    let mut sites = Vec::new();
    let host = rustix::system::uname()
        .nodename()
        .to_string_lossy()
        .into_owned();
    for record in records {
        require(
            record["host"] == host
                && record["process"].as_u64().is_some_and(|p| p > 0)
                && record["thread"].as_u64().is_some_and(|p| p > 0)
                && record["created"].as_f64().is_some_and(|t| t > 0.0)
                && record["threadName"].as_str().is_some_and(|s| !s.is_empty()),
            "native logging process/host/time provenance missing",
        )?;
        require(
            record["ctx"]["runtime_language"] == "rust"
                && record["ctx"]["source_commit"]
                    .as_str()
                    .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())),
            "native source context missing",
        )?;
        let path = Path::new(
            record["pathname"]
                .as_str()
                .ok_or("logging source path absent")?,
        );
        require(
            !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_))),
            "log source escaped checkout",
        )?;
        let content = String::from_utf8(bounded(&root.join("rust").join(path), 4 << 20)?)?;
        let line = record["lineno"]
            .as_u64()
            .and_then(|v| v.checked_sub(1))
            .ok_or("log source line invalid")?;
        require(
            content
                .lines()
                .nth(usize::try_from(line)?)
                .is_some_and(|s| s.contains("log_site!")),
            "native log_site reporting location does not exist",
        )?;
        let msg = &record["msg"];
        let expected = if msg.is_object() {
            if msg["event"] == "uploader_getxattr_failed" {
                ("scan.rs", "list_upload_files")
            } else {
                ("lib.rs", "upload")
            }
        } else {
            let text = msg.as_str().ok_or("unexpected log message type")?;
            if text.starts_with("upload_url v1.4 ") {
                ("http.rs", "do_upload")
            } else if text == "clear_locks failed" {
                ("scan.rs", "clear_locks")
            } else if text == "listdir_by_creation failed" {
                ("scan.rs", "list_upload_files")
            } else if text == "upload: getsize failed" {
                ("lib.rs", "upload")
            } else {
                ("runtime.rs", "run")
            }
        };
        require(
            record["filename"] == expected.0
                && record["funcName"]
                    .as_str()
                    .is_some_and(|s| s.ends_with(&format!("::{}", expected.1))),
            "native uploader reporting function changed",
        )?;
        let trace = |value: &Value| -> Result<()> {
            let text = value.as_str().ok_or("native traceback is not text")?;
            require(
                text.starts_with("Rust error:")
                    && text.contains("Rust backtrace captured at the reporting site:")
                    && !text.contains("Traceback (most recent call last)"),
                "native traceback reporting semantics changed",
            )
        };
        if let Some(exc) = msg.get("exc").filter(|v| !v.is_null()) {
            require(
                exc.as_array().is_some_and(|a| a.len() == 2),
                "native exception pair invalid",
            )?;
            trace(&exc[1])?;
            if case == "invalid-json" {
                require(
                    exc[0].as_str().is_some_and(|s| s.starts_with("Json(")),
                    "invalid-JSON error category changed",
                )?;
            }
        }
        if let Some(exc) = record.get("exc_info") {
            require(record["level"] == "ERROR", "traceback logged below ERROR")?;
            trace(exc)?;
            let category = match case {
                "getsize-error" => Some("NotFound"),
                "listdir-error" => Some("PermissionDenied"),
                "clear-locks-error" => Some("NotADirectory"),
                _ => None,
            };
            if let Some(category) = category {
                let text = exc.as_str().ok_or("trace absent")?;
                require(
                    text.contains(category)
                        && text.contains(directory.to_str().ok_or("nonUTF8 owned directory")?),
                    "native error meaning/path changed",
                )?;
            }
        }
        sites.push(json!({"pathname":record["pathname"],"lineno":record["lineno"],"funcName":record["funcName"],"level":record["level"]}));
    }
    Ok(sites)
}
fn worker(request: &Path) -> Result<Value> {
    let request: Worker = serde_json::from_slice(&bounded(request, LIMIT)?)?;
    fixture(&request.work, &request.case.settings)?;
    let _permission = Permission(request.work.clone());
    let peer_settings = Value::Object(
        request
            .case
            .settings
            .as_object()
            .ok_or("settings absent")?
            .iter()
            .filter(|(k, _)| {
                ["api_status", "put_status", "bad_json", "headers", "fake"].contains(&k.as_str())
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    let server = crate::uploader_reference_check::http::Server::start(&peer_settings)?;
    let home = request
        .work
        .parent()
        .ok_or("private work parent absent")?
        .join("home");
    let persist = home
        .join(format!(".comma{}", request.prefix))
        .join("persist");
    crate::uploader_reference_check::http::write_ecdsa(&persist)?;
    let params = request.output.join("params").join(&request.prefix);
    fs::create_dir_all(&params)?;
    if request.case.settings["missing_id"] != true {
        fs::write(params.join("DongleId"), b"0000000000000000")?;
    }
    fs::write(params.join("IsOffroad"), b"0")?;
    let mut peer = ipc::Peer::start(
        &request.collector,
        &request.worker,
        &request.output,
        request.source_collector,
        &request.prefix,
    )?;
    peer.synchronize()?;
    let mut command = CommandSpec::new_os(
        if request.case.direct.is_null() {
            vec![
                request.binary.as_os_str().to_owned(),
                "--cycles".into(),
                "1".into(),
            ]
        } else {
            vec![request.trace.as_os_str().to_owned()]
        },
        Duration::from_secs(20),
    );
    command.cwd = Some(request.root.clone());
    for (key, value) in [
        ("HOME", home.as_os_str()),
        (
            "PARAMS_ROOT",
            params.parent().ok_or("Params parent absent")?.as_os_str(),
        ),
        ("LOG_ROOT", request.work.as_os_str()),
        ("OPENPILOT_PREFIX", std::ffi::OsStr::new(&request.prefix)),
        ("API_HOST", std::ffi::OsStr::new(&server.url)),
        ("UPLOADER_SLEEP", std::ffi::OsStr::new("0")),
        ("FORCEWIFI", std::ffi::OsStr::new("")),
    ] {
        command
            .env
            .insert(key.into(), value.to_string_lossy().into_owned());
    }
    command.env_remove = [
        "FAKEUPLOAD",
        "LOGPRINT",
        "OPENPILOT_LOG_CONTEXT",
        "MANAGER_DAEMON",
    ]
    .map(Into::into)
    .to_vec();
    if request.case.settings["fake"] == true {
        command.env.insert("FAKEUPLOAD".into(), String::new());
        command.env_remove.retain(|v| v != "FAKEUPLOAD");
    }
    if !request.case.direct.is_null() {
        let mut input = request.case.direct.clone();
        input["root"] = json!(request.work);
        input["log_endpoint"] = json!(peer.endpoint);
        let mut bytes = serde_json::to_vec(&input)?;
        bytes.push(b'\n');
        command.input = Some(bytes);
    }
    let output = NativeRunner.run_in_current_group(&command)?;
    fs::write(request.output.join("uploader.stdout"), &output.stdout)?;
    fs::write(request.output.join("uploader.stderr"), &output.stderr)?;
    require(
        output.code
            == if request.case.settings["missing_id"] == true {
                1
            } else {
                0
            },
        "uploader logging case exit code differs",
    )?;
    let (records, errors, disks, code) = peer.finish()?;
    let server_url = server.url.clone();
    let requests = server.finish()?;
    fs::write(
        request.output.join("http.json"),
        serde_json::to_vec_pretty(&requests)?,
    )?;
    let actual = normalized(&records, &request.work, Some(&server_url))?;
    let mut expected = request.case.normalized.clone();
    for row in expected.as_array_mut().ok_or("source normalized absent")? {
        if let Some(text) = row["msg"].as_str() {
            row["msg"] = json!(canonical_url(text, None)?);
        }
    }
    require(
        actual == expected,
        "native uploader log records differ from immutable original observations",
    )?;
    require(
        errors
            == records
                .iter()
                .filter(|r| r["levelnum"].as_u64().unwrap_or(0) >= 40)
                .cloned()
                .collect::<Vec<_>>(),
        "errorLogMessage routing changed",
    )?;
    require(
        code == if request.source_collector { -2 } else { 0 },
        "collector shutdown status differs",
    )?;
    let sites = native_origin(&request.root, &records, &request.case.id, &request.work)?;
    let stderr = String::from_utf8(output.stderr)?;
    require(
        !stderr.contains("uploader: ready"),
        "invented uploader readiness diagnostic",
    )?;
    if output.code == 0
        && !records
            .iter()
            .any(|r| r["levelnum"].as_u64().unwrap_or(0) >= 40)
    {
        require(stderr.is_empty(), "unexpected below-warning console output")?;
    }
    fs::write(
        request.output.join("records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    Ok(
        json!({"case":request.case.id,"collector":if request.source_collector{"source-policy-native-collector"}else{"rust-collector"},"exit_code":output.code,"disk_records":disks,"error_records":errors.len(),"callsites":sites,"actual_cereal_packets":true,"original_records_equal":true}),
    )
}
pub fn run(options: &Options) -> Result<Value> {
    let corpus = load(options)?;
    crate::managed_entry_collector_source::sources(&options.root)?;
    fs::create_dir_all(&options.output)?;
    let mut rows = Vec::new();
    for (index, case) in corpus.cases.into_iter().enumerate() {
        for source_collector in [true, false] {
            let lane = if source_collector {
                "source-policy-native-collector"
            } else {
                "rust-collector"
            };
            let output = options.output.join(&case.id).join(lane);
            fs::create_dir_all(&output)?;
            let temp = tempfile::tempdir()?;
            let prefix = format!(
                "rust-probe-uploader-{}-{index}-{}",
                std::process::id(),
                u8::from(source_collector)
            );
            let work = temp.path().join("logs");
            let request = Worker {
                root: options.root.clone(),
                binary: options.binary.clone(),
                trace: options.trace.clone(),
                collector: options.collector.clone(),
                worker: options.worker.clone(),
                work,
                output: output.clone(),
                prefix: prefix.clone(),
                source_collector,
                case: case.clone(),
            };
            let path = output.join("request.json");
            fs::write(&path, serde_json::to_vec(&request)?)?;
            let mut command = CommandSpec::new_os(
                vec![
                    options.worker.as_os_str().to_owned(),
                    "--uploader-logging-worker".into(),
                    path.into_os_string(),
                ],
                Duration::from_secs(90),
            );
            command.env.insert("OPENPILOT_PREFIX".into(), prefix);
            command.cwd = Some(options.root.clone());
            let captured = NativeRunner.run(&command)?;
            fs::write(output.join("worker.stdout"), &captured.stdout)?;
            fs::write(output.join("worker.stderr"), &captured.stderr)?;
            require(
                captured.code == 0,
                "uploader logging native worker failed; inspect worker.stderr",
            )?;
            rows.push(serde_json::from_slice::<Value>(&captured.stdout)?);
        }
    }
    let result = json!({"passed":true,"scenarios":rows,"cases":18,"collector_rows":36,"expected_origin":"immutable actual original source records","source_collector":"existing independent native original-policy adapter; actual live uploader requests/packets","retired_shared_helpers":false});
    fs::write(
        options.output.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|v| v == "--uploader-source-collector")
    {
        require(args.len() == 3, "source collector worker arguments invalid")?;
        return crate::managed_entry_collector_source::run(&args[1], Path::new(&args[2]));
    }
    if args
        .first()
        .is_some_and(|v| v == "--uploader-logging-worker")
    {
        require(args.len() == 2, "logging worker arguments invalid")?;
        let result = worker(Path::new(&args[1]))?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    use crate::argparse_scan::{scan, OptionSpec as S};
    let parsed = scan(
        &args,
        &[
            S::value("--root", false),
            S::value("--binary", true),
            S::value("--trace", true),
            S::value("--collector", true),
            S::value("--output", true),
            S::value("--fixture", true),
            S::value("--fixture-sha256", true),
        ],
    )?;
    if parsed.help {
        println!("Usage: check-uploader-logging --binary PATH --trace PATH --collector PATH --output DIR --fixture JSON --fixture-sha256 SHA256 [--root ROOT]");
        return Ok(());
    }
    let get = |key: &str| -> Result<String> {
        parsed
            .values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("{key} required").into())
    };
    let root = parsed
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .expect("workspace layout")
                .to_path_buf()
        });
    let result = run(&Options {
        root,
        binary: get("--binary")?.into(),
        trace: get("--trace")?.into(),
        collector: get("--collector")?.into(),
        worker: std::env::current_exe()?,
        fixture: get("--fixture")?.into(),
        fixture_sha256: get("--fixture-sha256")?,
        output: get("--output")?.into(),
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
