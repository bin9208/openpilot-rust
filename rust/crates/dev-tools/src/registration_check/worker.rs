//! Per-side native workers isolate msgq/global environment and interactive ACKs.
use super::{keys::Keys, process::Interactive, reference, server::Server, values as v};
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use openpilot_msgq::Subscriber;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{self, BufRead, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const CAPACITY: usize = 10 * 1024 * 1024;

pub(super) fn dispatch(mode: &str, args: &[OsString]) -> Result<()> {
    match mode {
        "--registration-source" if args.is_empty() => super::source::run(),
        "--registration-collector" if args.len() == 2 => {
            crate::managed_entry_collector_source::run(
                args[0].to_str().ok_or("collector endpoint is not UTF-8")?,
                Path::new(&args[1]),
            )
        }
        "--registration-collector-gated" if args.len() == 3 => {
            crate::managed_entry_collector_source::run_gated(
                args[0].to_str().ok_or("collector endpoint is not UTF-8")?,
                Path::new(&args[1]),
                Path::new(&args[2]),
            )
        }
        "--registration-side" | "--registration-pair" if args.is_empty() => {
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let request = JsonValue::parse(line.trim_end())?;
            let result = if mode == "--registration-pair" {
                pair(&request)?
            } else {
                side(&request)?
            };
            println!("{}", result.to_json()?);
            io::stdout().flush()?;
            Ok(())
        }
        _ => Err("invalid private registration worker command".into()),
    }
}
pub(super) struct Namespace {
    path: PathBuf,
    identity: (u64, u64),
    endpoint_path: PathBuf,
    endpoint_identity: Option<(u64, u64)>,
    prefix: String,
    owned: bool,
}
impl Namespace {
    pub(super) fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_regqa-")
            .tempdir_in("/dev/shm")?;
        let path = directory.keep();
        let metadata = fs::symlink_metadata(&path)?;
        let prefix = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("invalid owned IPC namespace")?
            .to_owned();
        let endpoint_path = path.join("log.sock");
        if fs::symlink_metadata(&endpoint_path).is_ok() {
            fs::remove_dir(&path)?;
            return Err("owned registration endpoint collision".into());
        }
        Ok(Self {
            path,
            identity: (metadata.dev(), metadata.ino()),
            endpoint_path,
            endpoint_identity: None,
            prefix,
            owned: true,
        })
    }
    pub(super) fn descriptor(&self) -> Result<JsonValue> {
        v::object([
            ("prefix".into(), v::text(&self.prefix)),
            ("path".into(), v::path_value(&self.path)),
        ])
    }
    fn borrowed(value: &JsonValue) -> Result<Self> {
        let prefix = v::required_text(value, "prefix")?;
        let path = v::path(value, "path")?;
        if path != Path::new("/dev/shm").join(format!("msgq_{prefix}"))
            || !prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("invalid delegated registration namespace".into());
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("delegated registration namespace is not an owned real directory".into());
        }
        Ok(Self {
            endpoint_path: path.join("log.sock"),
            path,
            identity: (metadata.dev(), metadata.ino()),
            endpoint_identity: None,
            prefix,
            owned: false,
        })
    }
    fn endpoint(&self) -> String {
        format!("ipc://{}", self.endpoint_path.display())
    }
    fn observe(&mut self) -> Result<()> {
        if self.endpoint_identity.is_none() {
            match fs::symlink_metadata(&self.endpoint_path) {
                Ok(m) => self.endpoint_identity = Some((m.dev(), m.ino())),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
        if self.endpoint_identity.is_some_and(|id| {
            fs::symlink_metadata(&self.endpoint_path).is_ok_and(|m| (m.dev(), m.ino()) == id)
        }) {
            let _ = fs::remove_file(&self.endpoint_path);
        }
        if fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
struct Permissions(PathBuf);
impl Drop for Permissions {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
        if let Ok(files) = fs::read_dir(&self.0) {
            for file in files.flatten() {
                if file.path().is_file() {
                    let _ = fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
}
fn setup(output: &Path, case: &JsonValue, keys: &Keys) -> Result<Permissions> {
    let comma = output.join("persist/comma");
    fs::create_dir_all(&comma)?;
    fs::create_dir_all(output.join("params/d"))?;
    let restore = Permissions(comma.clone());
    if let Some(hardware) = case.get("hardware") {
        for (name, bytes) in v::fields(&hardware)? {
            let path = output.join("hardware").join(&name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?
            }
            fs::write(path, v::bytes(&bytes)?)?;
        }
    }
    for (name, path) in [
        ("param", output.join("params/d/DongleId")),
        ("fallback", comma.join("dongle_id")),
    ] {
        if let Some(bytes) = case.get(name) {
            fs::write(path, v::bytes(&bytes)?)?
        }
    }
    let selected = case
        .get("keys")
        .map_or_else(|| Ok(vec![v::text("rsa")]), |value| v::items(&value))?;
    for name in selected {
        let name = v::string(&name)?;
        let pair = keys
            .0
            .get(&name)
            .ok_or("fixture selects unknown signing key")?;
        let filename = match name.as_str() {
            "rsa" => "id_rsa",
            "ec" => "id_ecdsa",
            _ => return Err("fixture key name is unsupported".into()),
        };
        let mut private = case.get("private").map_or_else(
            || {
                Ok(if v::flag(case, "pkcs8") {
                    pair.pkcs8.as_bytes().to_vec()
                } else {
                    pair.private.as_bytes().to_vec()
                })
            },
            |value| v::bytes(&value),
        )?;
        let mut public = case.get("public").map_or_else(
            || Ok(pair.public.as_bytes().to_vec()),
            |value| v::bytes(&value),
        )?;
        if v::flag(case, "crlf") {
            private = crlf(&private);
            public = crlf(&public)
        }
        fs::write(comma.join(filename), private)?;
        if !v::flag(case, "no_public") {
            let path = comma.join(format!("{filename}.pub"));
            fs::write(&path, public)?;
            if v::flag(case, "public_unreadable") {
                fs::set_permissions(path, fs::Permissions::from_mode(0o0))?
            }
        }
    }
    if v::flag(case, "param_directory") {
        fs::create_dir(output.join("params/d/DongleId"))?
    }
    if v::flag(case, "fallback_parent_unreadable") {
        fs::set_permissions(&comma, fs::Permissions::from_mode(0o0))?
    }
    Ok(restore)
}
fn crlf(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    for &b in bytes {
        if b == b'\n' {
            result.push(b'\r')
        }
        result.push(b)
    }
    result
}
fn packet(raw: &[u8], error: bool) -> Result<JsonValue> {
    let message = capnp::serialize::read_message(
        &mut io::Cursor::new(raw),
        capnp::message::ReaderOptions::new(),
    )?;
    let event = message.get_root::<openpilot_cereal::log_capnp::event::Reader>()?;
    if !event.get_valid() {
        return Err("registration collector published invalid event".into());
    }
    let text = match event.which()? {
        openpilot_cereal::log_capnp::event::LogMessage(text) if !error => text?,
        openpilot_cereal::log_capnp::event::ErrorLogMessage(text) if error => text?,
        _ => return Err("registration collector published wrong topic".into()),
    };
    Ok(JsonValue::parse(text.to_str()?)?)
}
fn disk(root: &Path) -> Result<Vec<JsonValue>> {
    let mut paths = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.retain(|p| {
        p.file_name()
            .is_some_and(|s| s.as_encoded_bytes().starts_with(b"swaglog."))
    });
    paths.sort();
    let mut rows = Vec::new();
    for path in paths {
        for line in fs::read_to_string(path)?.lines() {
            let mut row = v::fields(&JsonValue::parse(line)?)?;
            let message = row
                .remove("msg$s")
                .ok_or("source disk record has no msg$s")?;
            row.insert("msg".into(), message);
            row.remove("id");
            rows.push(v::object(row)?);
        }
    }
    Ok(rows)
}
fn source_config(
    request: &JsonValue,
    case: &JsonValue,
    output: &Path,
    endpoint: &str,
    log_root: &Path,
    host: &str,
) -> Result<JsonValue> {
    let mut config = case
        .get("config")
        .map_or_else(|| Ok(BTreeMap::new()), |v| v::fields(&v))?;
    for (name, value) in [
        ("persist", v::path_value(&output.join("persist"))),
        ("params", v::path_value(&output.join("params"))),
        ("source_root", v::get(request, "root")?),
        ("api_host", v::text(host)),
        ("endpoint", v::text(endpoint)),
        ("log_root", v::path_value(log_root)),
    ] {
        config.insert(name.into(), value);
    }
    if case.get("hardware").is_some() {
        config.insert(
            "hardware_root".into(),
            v::path_value(&output.join("hardware")),
        );
    }
    v::object(config)
}
fn side(request: &JsonValue) -> Result<JsonValue> {
    let output = v::path(request, "output")?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?
    }
    fs::create_dir(&output)?;
    let case = v::get(request, "case")?;
    let keys = if let Some(keys) = request.get("keys") {
        Keys::from_request(&keys)?
    } else {
        Keys::new()?
    };
    let _permissions = setup(&output, &case, &keys)?;
    // All environment changes happen before the first HTTP/ZMQ thread exists.
    let mut namespace = request
        .get("namespace")
        .map_or_else(Namespace::new, |value| Namespace::borrowed(&value))?;
    std::env::set_var("OPENPILOT_PREFIX", &namespace.prefix);
    let server = if request.get("api_host").is_none() {
        Some(Server::new(&case)?)
    } else {
        None
    };
    let host = request.get("api_host").map_or_else(
        || {
            server
                .as_ref()
                .map(|s| s.url.clone())
                .ok_or_else(|| "HTTP fixture absent".into())
        },
        |value| v::string(&value),
    )?;
    let collector_output = output.join("collector");
    fs::create_dir(&collector_output)?;
    let log_root = collector_output.join("log");
    let worker = std::env::current_exe()?;
    let endpoint = namespace.endpoint();
    let environment = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let gate = super::gate::Gate::from_environment(&namespace.path)?;
    let mut collector_argv = vec![
        worker.clone().into_os_string(),
        "--registration-collector".into(),
        endpoint.clone().into(),
        log_root.clone().into_os_string(),
    ];
    if let Some(gate) = &gate {
        collector_argv[1] = "--registration-collector-gated".into();
        collector_argv.push(gate.path.clone().into_os_string());
    }
    let mut collector = Interactive::spawn(
        &collector_argv,
        &environment,
        &collector_output.join("stderr.log"),
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !["logMessage", "errorLogMessage"]
        .iter()
        .all(|name| namespace.path.join(name).is_file())
        || !namespace.endpoint_path.exists()
    {
        namespace.observe()?;
        if collector.exited()? {
            return Err(format!(
                "registration collector exited before readiness: {}",
                fs::read_to_string(collector_output.join("stderr.log")).unwrap_or_default()
            )
            .into());
        }
        if Instant::now() >= deadline {
            return Err("registration collector readiness exceeds original 20 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    namespace.observe()?;
    let mut schedule = gate
        .as_ref()
        .map(|gate| gate.accept(&collector, deadline))
        .transpose()?;
    let mut log = Subscriber::for_runtime("logMessage", false, CAPACITY)?;
    let mut errors = Subscriber::for_runtime("errorLogMessage", false, CAPACITY)?;
    if let Some(schedule) = &mut schedule {
        schedule.release(&namespace.path.join("errorLogMessage"))?;
    }
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PUSH)?;
    socket.set_linger(0)?;
    socket.set_sndtimeo(10_000)?;
    socket.connect(&endpoint)?;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let marker = format!(
            "{{\"msg\":\"registration-collector-ready-{}-{attempts}\"}}",
            namespace.prefix
        );
        socket.send_multipart([&[10][..], marker.as_bytes()], 0)?;
        let next = Instant::now() + Duration::from_millis(50);
        let mut ready = false;
        while Instant::now() < next {
            if let Some(raw) = log.receive(Duration::ZERO)? {
                super::process::append(&collector_output.join("readiness.bin"), &raw)?;
                let row = packet(&raw, false)?;
                if row.get("msg").is_some_and(|v| {
                    v.text_eq(&format!(
                        "registration-collector-ready-{}-{attempts}",
                        namespace.prefix
                    ))
                }) {
                    ready = true;
                    break;
                }
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if ready {
            if errors.receive(Duration::ZERO)?.is_some() {
                return Err("unexpected error log before registration".into());
            }
            if let Some(schedule) = &schedule {
                schedule.refreshed(
                    &namespace.path.join("errorLogMessage"),
                    &collector_output.join("publisher-schedule.json"),
                )?;
            }
            v::write(
                &collector_output.join("readiness.json"),
                &v::value(serde_json::json!({"acknowledged_marker":marker,"attempts":attempts}))?,
            )?;
            break;
        }
        if collector.exited()? || Instant::now() >= deadline {
            return Err("registration collector did not acknowledge readiness".into());
        }
    }
    let config = source_config(request, &case, &output, &endpoint, &log_root, &host)?;
    v::write(&output.join("config.json"), &config)?;
    let mut child_env = environment.clone();
    child_env.insert("OPENPILOT_PREFIX".into(), "d".into());
    child_env.insert("NO_PROXY".into(), "*".into());
    let argv = if let Some(binary) = request.get("binary") {
        let mut runner = request
            .get("runner")
            .map_or_else(|| Ok(vec![]), |v| v::items(&v))?
            .iter()
            .map(v::os)
            .collect::<Result<Vec<_>>>()?;
        runner.push(v::os(&binary)?);
        runner
    } else {
        vec![worker.into_os_string(), "--registration-source".into()]
    };
    let started = Instant::now();
    let mut child = Interactive::spawn(&argv, &child_env, &output.join("stderr.log"))?;
    child.send((config.to_json()? + "\n").as_bytes())?;
    let raw = child.line(Duration::from_secs(120))?;
    let result = JsonValue::parse(std::str::from_utf8(&raw)?)?;
    let records_deadline = Instant::now() + Duration::from_secs(90);
    let mut records = Vec::new();
    loop {
        let raw = log
            .receive(Duration::from_secs(10))?
            .ok_or("registration collector timeout")?;
        super::process::append(&output.join("logMessage.bin"), &raw)?;
        let record = packet(&raw, false)?;
        let end = record
            .get("msg")
            .is_some_and(|v| v.text_eq("registration-fixture-end"));
        records.push(record);
        if end {
            break;
        }
        if Instant::now() >= records_deadline {
            return Err("registration collector end marker absent after 90 seconds".into());
        }
    }
    let mut error_records = Vec::new();
    while let Some(raw) = errors.receive(Duration::ZERO)? {
        super::process::append(&output.join("errorLogMessage.bin"), &raw)?;
        error_records.push(packet(&raw, true)?)
    }
    let expected_errors = records
        .iter()
        .filter(|r| {
            r.get("levelnum")
                .and_then(|v| v::integer(&v).ok())
                .is_some_and(|n| n >= 40)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !v::equal(&v::array(error_records)?, &v::array(expected_errors)?) {
        return Err(
            "complete registration errorLogMessage stream differs from error records".into(),
        );
    }
    v::write(&output.join("records.json"), &v::array(records.clone())?)?;
    child.send(b"ack\n")?;
    let status = child.finish(Duration::from_secs(10))?;
    if !status.success() {
        return Err(format!("registration child exited after ACK: {status}").into());
    }
    let elapsed = started.elapsed();
    v::write(&output.join("result.json"), &result)?;
    let argv = v::array(argv.iter().map(|value| crate::python_fs::json_value(value)))?;
    v::write(
        &output.join("invocation.json"),
        &v::object([
            ("argv".into(), argv),
            ("elapsed_seconds".into(), v::value(elapsed.as_secs_f64())?),
            ("exit".into(), v::value(0)?),
        ])?,
    )?;
    let stopping = Instant::now();
    if !collector.signal(rustix::process::Signal::INT)? {
        return Err("registration collector exited before owned SIGINT".into());
    }
    socket.send_multipart(
        [&[10][..], &b"{\"msg\":\"shutdown-wakeup\"}"[..]],
        zmq::DONTWAIT,
    )?;
    let status = collector.finish(Duration::from_secs(10))?;
    if status.signal() != Some(signal_hook::consts::SIGINT) {
        return Err(
            format!("registration collector exit differs from original SIGINT: {status}").into(),
        );
    }
    let exit = v::value(
        serde_json::json!({"code":-2,"shutdown_seconds":stopping.elapsed().as_secs_f64(),"signal":2,"wake_after_signal":true}),
    )?;
    v::write(&collector_output.join("exit.json"), &exit)?;
    let disk = disk(&log_root)?;
    let expected_disk = records
        .iter()
        .filter(|r| {
            r.get("levelnum")
                .and_then(|v| v::integer(&v).ok())
                .is_some_and(|n| n >= 20)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !v::equal(&v::array(disk.clone())?, &v::array(expected_disk)?) {
        return Err(
            "complete registration source disk records differ from actual collected records".into(),
        );
    }
    v::write(&output.join("disk-records.json"), &v::array(disk)?)?;
    let elapsed_json = v::value(elapsed.as_secs_f64())?;
    let records = v::array(records)?;
    let raw_reply = v::object([
        ("result".into(), result),
        ("records".into(), records),
        ("elapsed_seconds".into(), elapsed_json.clone()),
    ])?;
    if let Some(server) = server {
        let requests = server.rows()?;
        v::write(&output.join("requests.json"), &requests)?;
        let normalized = normalize(
            &v::required_text(request, "name")?,
            &case,
            &raw_reply,
            &requests,
            &keys,
        )?;
        return v::object([
            ("normalized".into(), normalized),
            ("elapsed_seconds".into(), elapsed_json),
            ("public_keys".into(), v::value(keys.public())?),
        ]);
    }
    Ok(raw_reply)
}
fn normalize(
    name: &str,
    case: &JsonValue,
    raw: &JsonValue,
    requests: &JsonValue,
    keys: &Keys,
) -> Result<JsonValue> {
    let config = case.get("config");
    let utc = config
        .and_then(|v| v.get("utc"))
        .map_or(Ok(1_700_000_000i64), |v| {
            v::number(&v).map(|v| v.floor() as i64)
        })?;
    v::object([
        ("case".into(), v::text(name)),
        (
            "result".into(),
            reference::normalize_result(&v::get(raw, "result")?)?,
        ),
        (
            "records".into(),
            reference::normalize_records(&v::get(raw, "records")?)?,
        ),
        (
            "requests".into(),
            reference::normalize_requests(requests, &keys.public(), utc + 3600)?,
        ),
        (
            "record_count".into(),
            v::value(v::items(&v::get(raw, "records")?)?.len())?,
        ),
        ("disk_matches_records".into(), v::value(true)?),
        ("error_packets_match_records".into(), v::value(true)?),
        (
            "collector_exit".into(),
            v::value(serde_json::json!({"code":-2,"signal":2,"wake_after_signal":true}))?,
        ),
    ])
}
fn pair(request: &JsonValue) -> Result<JsonValue> {
    let case = v::get(request, "case")?;
    let output = v::path(request, "output")?;
    let name = v::required_text(request, "name")?;
    let keys = Keys::new()?;
    let server = Server::new(&case)?;
    let worker = std::env::current_exe()?;
    let environment = std::env::vars_os().collect();
    let mut observations = Vec::new();
    for side in ["source", "native"] {
        server.reset(&case)?;
        let path = output.join(side);
        let mut fields = v::fields(request)?;
        fields.insert("output".into(), v::path_value(&path));
        fields.insert("api_host".into(), v::text(&server.url));
        fields.insert("keys".into(), keys.request()?);
        if let Some(namespaces) = request.get("namespaces") {
            fields.insert("namespace".into(), v::get(&namespaces, side)?);
        }
        if side == "source" {
            fields.remove("binary");
            fields.remove("runner");
        }
        let mut child = Interactive::spawn(
            &[
                worker.clone().into_os_string(),
                "--registration-side".into(),
            ],
            &environment,
            &output.join(format!("{side}-worker.stderr.log")),
        )?;
        child.send((v::object(fields)?.to_json()? + "\n").as_bytes())?;
        let raw = child.line(Duration::from_secs(170))?;
        let raw = JsonValue::parse(std::str::from_utf8(&raw)?)?;
        if !child.finish(Duration::from_secs(10))?.success() {
            return Err(format!("registration {side} worker failed").into());
        }
        let requests = server.rows()?;
        v::write(&path.join("requests.json"), &requests)?;
        let minimum = case
            .get("minimum_seconds")
            .map_or(Ok(0.0), |v| v::number(&v))?;
        if v::number(&v::get(&raw, "elapsed_seconds")?)? < minimum {
            return Err(format!("registration {side} elapsed less than original minimum").into());
        }
        observations.push(normalize(&name, &case, &raw, &requests, &keys)?);
    }
    let source = &observations[0];
    let native = &observations[1];
    for field in ["result", "records", "requests"] {
        if !v::equal(&v::get(source, field)?, &v::get(native, field)?) {
            return Err(format!(
                "registration {name} differs in {field}: source={} native={}",
                v::get(source, field)?.to_json()?,
                v::get(native, field)?.to_json()?
            )
            .into());
        }
    }
    v::object([
        ("case".into(), v::text(&name)),
        ("result".into(), v::text("PASS")),
        (
            "requests".into(),
            v::value(v::items(&v::get(source, "requests")?)?.len())?,
        ),
        ("records".into(), v::get(source, "record_count")?),
        (
            "outcome".into(),
            v::get(&v::get(source, "result")?, "outcome")?,
        ),
    ])
}
