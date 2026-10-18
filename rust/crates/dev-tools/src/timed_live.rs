//! Actual timed controllers with private process, filesystem, Msgq and ZMQ
//! ownership. The source daemon imports only the independently ported policy.
use crate::{
    argparse_scan::{scan_os_with_positionals, ArgumentError, OptionSpec},
    control_tools_policy_check::json as pyjson,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    timed_check::Options,
    timed_fixtures::{self, Fixture},
    timed_reference,
};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::JsonValue;
use openpilot_msgq::{Publisher, Subscriber};
use rustix::process::Signal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::BufRead,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "timed_live_process.rs"]
mod process;
#[path = "timed_live_proxy.rs"]
mod proxy;
#[path = "timed_live_source.rs"]
mod source;
const CAPACITY: usize = 250 * 1024; // original SERVICE_LIST SMALL for all three topics
#[derive(Clone, Copy)]
pub enum Kind {
    Daemon,
    DateDaemon,
    Lifecycle,
    Shutdown,
}
fn require(ok: bool, reason: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn node(value: &Value) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(value)?)?)
}
fn array(rows: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        rows.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
fn object(rows: &[(&str, JsonValue)]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "{{{}}}",
        rows.iter()
            .map(|(key, value)| Ok(format!(
                "{}:{}",
                serde_json::to_string(key)?,
                value.to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing {key}").into())
}
fn write(path: &Path, value: &JsonValue) -> Result<()> {
    fs::write(path, format!("{}\n", pyjson::pretty(value, 0)?))?;
    Ok(())
}
fn read_text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn monotonic_ns() -> Result<u64> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(u64::try_from(now.tv_sec)?
        .checked_mul(1_000_000_000)
        .and_then(|sec| sec.checked_add(now.tv_nsec as u64))
        .ok_or("monotonic clock overflow")?)
}
fn monotonic() -> Result<f64> {
    Ok(monotonic_ns()? as f64 / 1e9)
}
fn wall_ns() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .try_into()?)
}
fn clock_wire(wall: u64, mono: u64, valid: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(mono);
    event.set_valid(valid);
    event.init_clocks().set_wall_time_nanos(wall);
    capnp::serialize::write_message_to_words(&message)
}
pub fn sources(root: &Path) -> Result<()> {
    let bytes = include_bytes!("../tests/fixtures/timed-tools/live-manifest.json");
    require(
        format!("{:x}", Sha256::digest(bytes))
            == "1d7def8d1d7690622368f697c960786ebd3c42c3b9ccd78f6c9a43163b662d2f",
        "live source provenance changed",
    )?;
    let value: Value = serde_json::from_slice(bytes)?;
    let pins = value["sources"]
        .as_object()
        .ok_or("missing live source pins")?;
    require(pins.len() == 25, "live source pin set changed")?;
    for (name, expected) in pins {
        let bytes = match fs::read(root.join(name)) {
            Ok(bytes) => bytes,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && timed_reference::retired_source(name) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        require(
            expected.as_str() == Some(&format!("{:x}", Sha256::digest(bytes))),
            &format!("live source changed: {name}"),
        )?;
    }
    timed_reference::sources(root)
}
fn encode_path(path: &Path) -> Value {
    json!(path
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}
fn decode_path(value: &Value) -> Result<PathBuf> {
    let text = value.as_str().ok_or("missing path bytes")?;
    require(text.len().is_multiple_of(2), "odd path hex")?;
    Ok(OsString::from_vec(
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
            .collect::<Result<Vec<_>>>()?,
    )
    .into())
}
fn receipt_write(output: &Path, receipt: &Value) -> Result<()> {
    use std::io::Write;
    // A killed worker must leave either the complete old receipt or the
    // complete new receipt, never a truncated ownership document.
    let mut file = tempfile::NamedTempFile::new_in(output)?;
    file.write_all(format!("{}\n", serde_json::to_string_pretty(receipt)?).as_bytes())?;
    file.persist(output.join("ownership.json"))?;
    Ok(())
}
fn cleanup_receipt(output: &Path, invocation: &str) -> Result<()> {
    let receipt = output.join("ownership.json");
    if !receipt.is_file() {
        return Ok(());
    }
    let value: Value = serde_json::from_slice(&fs::read(receipt)?)?;
    // Reused output may describe a previous invocation, even on failed spawn.
    if value["invocation"].as_str() != Some(invocation) {
        return Ok(());
    }
    let prefix = value["prefix"].as_str().ok_or("missing owned prefix")?;
    require(
        prefix.starts_with("timed_")
            && prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
        "invalid owned prefix",
    )?;
    // Receipt is written by our worker before IPC/child creation, never by the
    // candidate. The outer Runner has killed/reaped that worker's whole group.
    let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
    let endpoint = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    if value["owns_namespace"] == true && shm.exists() {
        fs::remove_dir_all(shm)?;
    }
    if value["owns_endpoint"] == true && endpoint.exists() {
        fs::remove_file(endpoint)?;
    }
    Ok(())
}
fn invoke_raw(options: &Options, worker: &Path, case: &Value) -> Result<JsonValue> {
    sources(&options.root)?;
    fs::create_dir_all(&options.output)?;
    let invocation = tempfile::Builder::new()
        .prefix(".timed-invocation-")
        .tempfile_in(&options.output)?;
    let token = invocation
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid invocation token")?
        .to_owned();
    let mut spec =
        CommandSpec::new_os(vec![worker.as_os_str().to_owned()], Duration::from_secs(60));
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.env.insert("TIMED_WORKER_TOKEN".into(), token.clone());
    let request = format!(
        "{}\n",
        json!({"root":encode_path(&options.root),"binary":encode_path(&options.binary),"output":encode_path(&options.output),"reference":encode_path(&options.reference),"command":encode_path(&options.command),"case":case})
    );
    if case["kind"] == "lifecycle" {
        // Original lifecycle children inherit the caller's stdin. Keep the
        // internal worker request out of that stream entirely.
        spec.os_argv
            .as_mut()
            .ok_or("missing worker argv")?
            .push("--inherit-input".into());
        spec.env.insert("TIMED_WORKER_REQUEST".into(), request);
        spec.inherit_stdin = true;
    } else {
        spec.input = Some(request.into_bytes());
    }
    let captured = NativeRunner.run(&spec);
    // Runtime children inherit the outer worker's group, so cleanup is safe
    // after either normal exit or the outer runner's deadline/error cleanup.
    let cleanup = cleanup_receipt(&options.output, &token);
    let captured = captured?;
    require(
        captured.code == 0,
        &format!(
            "timed worker failed {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    cleanup?;
    pyjson::parse(&fs::read(options.output.join("result.json"))?)
}
pub fn invoke(options: &Options, worker: &Path, case: &Value) -> Result<Value> {
    Ok(serde_json::from_str(
        &invoke_raw(options, worker, case)?.to_json()?,
    )?)
}

struct Gps<'a> {
    service: &'a str,
    age: f64,
    fix: bool,
    millis: i64,
    longitude: f64,
    file: Option<&'a str>,
    valid: bool,
}
struct Session {
    _directory: tempfile::TempDir,
    fixture: Fixture,
    namespace: PathBuf,
    endpoint: PathBuf,
    owns_namespace: bool,
    owns_endpoint: bool,
    publisher: Option<Publisher>,
    receiver: Option<Subscriber>,
    collector: Option<zmq::Socket>,
    context: Option<zmq::Context>,
    child: Option<process::Process>,
    output: PathBuf,
    clocks: usize,
    timeout: Duration,
    receipt: Value,
}
impl Session {
    fn new(options: &Options, zone: &str, timeout: Duration) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let fixture = Fixture::new(directory.path(), zone, &options.command)?;
        for (key, value) in &fixture.environment {
            std::env::set_var(key, value);
        }
        for key in ["ZMQ", "CEREAL_FAKE", "LOGPRINT", "TIMED_COMMAND_FAIL"] {
            std::env::remove_var(key);
        }
        let namespace = PathBuf::from(format!("/dev/shm/msgq_{}", fixture.prefix));
        let endpoint = PathBuf::from(format!("/tmp/logmessage{}", fixture.prefix));
        let receipt = json!({"invocation":std::env::var("TIMED_WORKER_TOKEN")?,"prefix":fixture.prefix,"worker_pid":std::process::id(),"shared_memory":namespace,"log_endpoint":endpoint,"owns_namespace":false,"owns_endpoint":false,"source_implementation":"native source reference","reference_sha256":format!("{:x}",Sha256::digest(fs::read(&options.reference)?)),"command_sha256":format!("{:x}",Sha256::digest(fs::read(&options.command)?))});
        receipt_write(&options.output, &receipt)?;
        let mut session = Self {
            _directory: directory,
            fixture,
            namespace,
            endpoint,
            owns_namespace: false,
            owns_endpoint: false,
            publisher: None,
            receiver: None,
            collector: None,
            context: None,
            child: None,
            output: options.output.clone(),
            clocks: 0,
            timeout,
            receipt,
        };
        fs::create_dir(&session.namespace)?;
        session.owns_namespace = true;
        session.receipt["owns_namespace"] = json!(true);
        receipt_write(&session.output, &session.receipt)?;
        Ok(session)
    }
    fn open(&mut self, service: &str, logging: bool) -> Result<()> {
        self.publisher = Some(Publisher::for_runtime(service, CAPACITY)?);
        self.receiver = Some(Subscriber::for_runtime("clocks", false, CAPACITY)?);
        if logging {
            let context = zmq::Context::new();
            let socket = context.socket(zmq::PULL)?;
            socket.set_linger(0)?;
            socket.bind(&format!("ipc://{}", self.endpoint.display()))?;
            self.owns_endpoint = true;
            self.receipt["owns_endpoint"] = json!(true);
            self.context = Some(context);
            self.collector = Some(socket);
            receipt_write(&self.output, &self.receipt)?;
        }
        Ok(())
    }
    fn spawn(
        &mut self,
        options: &Options,
        implementation: &str,
        args: &[&str],
        log: &str,
        input: bool,
    ) -> Result<()> {
        let mut argv = if implementation == "rust" {
            vec![options.binary.as_os_str().to_owned()]
        } else {
            vec![options.reference.as_os_str().to_owned(), "--live".into()]
        };
        argv.extend(args.iter().map(OsString::from));
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(30));
        self.fixture.configure(&mut spec);
        spec.env_remove
            .extend(["TIMED_WORKER_REQUEST", "TIMED_WORKER_TOKEN"].map(OsString::from));
        spec.inherit_stdin = !input;
        if input {
            spec.input =
                Some(format!("{}\n", serde_json::to_string(&self.fixture.config)?).into_bytes());
        }
        self.child = Some(process::Process::spawn(&spec, &self.output.join(log))?);
        let pid = self.child()?.id();
        self.receipt["child_pid"] = json!(pid);
        self.receipt["child_implementation"] = json!(if implementation == "rust" {
            "native runtime"
        } else {
            "native source reference"
        });
        self.receipt["executed_sha256"] = json!(format!(
            "{:x}",
            Sha256::digest(fs::read(if implementation == "rust" {
                &options.binary
            } else {
                &options.reference
            })?)
        ));
        self.receipt["child_group"] = json!(rustix::process::getpgid(Some(
            rustix::process::Pid::from_raw(pid.try_into()?).ok_or("invalid child PID")?
        ))?
        .as_raw_nonzero()
        .get());
        receipt_write(&self.output, &self.receipt)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self
            .publisher
            .as_mut()
            .ok_or("missing GPS publisher")?
            .readers_caught_up()
        {
            require(
                Instant::now() < deadline,
                "timed subscriber not ready within five seconds",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn child(&mut self) -> Result<&mut process::Process> {
        self.child
            .as_mut()
            .ok_or_else(|| "missing live child".into())
    }
    fn raw_clock(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let bytes = self
            .receiver
            .as_mut()
            .ok_or("missing clocks subscriber")?
            .receive(self.timeout)?;
        if let Some(bytes) = &bytes {
            fs::write(self.output.join(name), bytes)?;
        }
        Ok(bytes)
    }
    fn clock(&mut self, write_before: bool) -> Result<Option<(JsonValue, usize)>> {
        let bytes = self
            .receiver
            .as_mut()
            .ok_or("missing clocks subscriber")?
            .receive(self.timeout)?;
        let Some(bytes) = bytes else { return Ok(None) };
        let path = self.output.join(format!("clocks-{}.capnp", self.clocks));
        if write_before {
            fs::write(&path, &bytes)?;
        }
        let mut raw = bytes.as_slice();
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut raw,
            capnp::message::ReaderOptions::new(),
        )?;
        let event = message.get_root::<event::Reader<'_>>()?;
        let clocks = match event.which()? {
            event::Which::Clocks(value) => value?,
            _ => return Err("unexpected clocks event".into()),
        };
        let row = object(&[
            ("time", node(&json!(monotonic()?))?),
            ("valid", node(&json!(event.get_valid()))?),
            ("wall", node(&json!(clocks.get_wall_time_nanos()))?),
            ("mono", node(&json!(event.get_log_mono_time()))?),
        ])?;
        if !write_before {
            fs::write(path, &bytes)?;
        }
        self.clocks += 1;
        Ok(Some((row, bytes.len())))
    }
    fn gps(&mut self, gps: Gps<'_>) -> Result<()> {
        let Gps {
            service,
            age,
            fix,
            millis,
            longitude,
            file,
            valid,
        } = gps;
        let now = monotonic_ns()?;
        let mono = now
            .checked_sub((age * 1e9) as u64)
            .ok_or("GPS monotonic underflow")?;
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(mono);
        event.set_valid(valid);
        let mut gps = if service == "gpsLocationExternal" {
            event.init_gps_location_external()
        } else {
            event.init_gps_location()
        };
        gps.set_has_fix(fix);
        gps.set_longitude(longitude);
        gps.set_unix_timestamp_millis(millis);
        let bytes = capnp::serialize::write_message_to_words(&message);
        if let Some(file) = file {
            fs::write(self.output.join(file), &bytes)?;
        }
        self.publisher
            .as_mut()
            .ok_or("missing GPS publisher")?
            .send(&bytes)?;
        Ok(())
    }
    fn record(&self) -> Result<JsonValue> {
        let collector = self.collector.as_ref().ok_or("missing log collector")?;
        require(
            collector.poll(zmq::POLLIN, 3000)? != 0,
            "missing log record",
        )?;
        let raw = collector.recv_bytes(0)?;
        let record = pyjson::parse(raw.get(1..).unwrap_or_default())?;
        let level = raw.first().ok_or("empty log frame")?;
        require(
            pyjson::equal(&field(&record, "levelnum")?, &node(&json!(level))?)
                && field(&record, "name")?.text_eq("swaglog"),
            "log level/name mismatch",
        )?;
        require(
            !field(&record, "pathname")?
                .to_utf8()
                .ok_or("log pathname not text")?
                .is_empty()
                && field(&record, "lineno")?.to_json()?.parse::<f64>()? > 0.
                && !field(&record, "funcName")?
                    .to_utf8()
                    .ok_or("log function not text")?
                    .is_empty(),
            "log source metadata absent",
        )?;
        if self.receipt["child_implementation"] == "native source reference" {
            // Original Python LogFormatter insertion order is observable in
            // the controller's report. Keep our actual Rust source metadata.
            let keys = [
                "msg",
                "ctx",
                "level",
                "levelnum",
                "name",
                "filename",
                "lineno",
                "pathname",
                "module",
                "funcName",
                "host",
                "process",
                "thread",
                "threadName",
                "created",
            ];
            let mut fields = keys
                .into_iter()
                .map(|key| Ok((key, field(&record, key)?)))
                .collect::<Result<Vec<_>>>()?;
            if let Some(exception) = record.get("exc_info") {
                fields.push(("exc_info", exception));
            }
            return object(&fields);
        }
        Ok(record)
    }
    fn commands(&self) -> Result<JsonValue> {
        node(&json!(timed_fixtures::commands(&self.fixture.directory)?))
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.child.take();
        self.collector.take();
        self.context.take();
        self.receiver.take();
        self.publisher.take();
        if self.owns_namespace {
            let _ = fs::remove_dir_all(&self.namespace);
        }
        if self.owns_endpoint {
            let _ = fs::remove_file(&self.endpoint);
        }
    }
}

fn daemon(options: &Options, case: &Value) -> Result<JsonValue> {
    let implementation = case["implementation"]
        .as_str()
        .ok_or("missing implementation")?;
    let external = case["external"] == true;
    let service = if external {
        "gpsLocationExternal"
    } else {
        "gpsLocation"
    };
    let mut session = Session::new(options, "UTC", Duration::from_millis(1500))?;
    fs::write(session.fixture.params.join("TimezoneSource"), b"app")?;
    fs::write(
        session.fixture.params.join("UbloxAvailable"),
        if external { b"1" } else { b"0" },
    )?;
    session.fixture.config["live"] = json!(true);
    session.fixture.config["monotonic"] = Value::Null;
    session.open(service, true)?;
    let started = monotonic()?;
    session.spawn(options, implementation, &[], "daemon.log", true)?;
    require(
        session.child()?.poll()?.is_none(),
        "timed exited before first clock",
    )?;
    let mut packets = vec![];
    let mut records = vec![];
    let receive = |session: &mut Session| -> Result<JsonValue> {
        let (packet, _) = session.clock(true)?.ok_or("missing daemon clocks")?;
        require(
            field(&packet, "valid")?.to_json()? == "true"
                && pyjson::equal(
                    &field(&packet, "wall")?,
                    &node(&session.fixture.config["wall"])?,
                ),
            "daemon clock validity/wall mismatch",
        )?;
        let age = monotonic()? - field(&packet, "mono")?.to_json()?.parse::<f64>()? / 1e9;
        require(
            (0.0..1.5).contains(&age),
            "daemon clock age outside original tolerance",
        )?;
        Ok(packet)
    };
    packets.push(receive(&mut session)?);
    for (age, fix, diff) in [(3., true, 30), (0., false, 30), (0., true, 0)] {
        let file = format!("gps-{}.capnp", packets.len());
        session.gps(Gps {
            service,
            age,
            fix,
            millis: 1790000000000 + diff * 1000,
            longitude: 127.,
            file: Some(&file),
            valid: false,
        })?;
        packets.push(receive(&mut session)?);
        require(
            pyjson::sequence(&session.commands()?)?.is_empty(),
            "unexpected early date command",
        )?;
    }
    let sleep_started = monotonic()?;
    let record = session.record()?;
    require(
        field(&record, "msg")?.text_eq("Time diff too small: 0.0s"),
        "small-diff log differs",
    )?;
    records.push(record);
    require(
        pyjson::sequence(&session.commands()?)?.is_empty(),
        "unexpected small-diff date command",
    )?;
    require(
        session
            .receiver
            .as_mut()
            .ok_or("missing clocks subscriber")?
            .receive(Duration::from_millis(1500))?
            .is_none(),
        "loop failed to retain ten-second sleep",
    )?;
    while monotonic()? - sleep_started < 9.6 {
        std::thread::sleep(Duration::from_millis(50));
    }
    session.gps(Gps {
        service,
        age: 0.,
        fix: true,
        millis: 1790000030000,
        longitude: 127.,
        file: Some("gps-4.capnp"),
        valid: false,
    })?;
    packets.push(receive(&mut session)?);
    let sleep_elapsed = monotonic()? - sleep_started;
    require(
        sleep_elapsed > 9.8 && sleep_elapsed < 12.,
        "GPS sleep duration changed",
    )?;
    let record = session.record()?;
    require(
        field(&record, "msg")?.text_eq("Setting system time from GPS (diff 30.0s)"),
        "setting-time log differs",
    )?;
    records.push(record);
    let end = Instant::now() + Duration::from_secs(3);
    while pyjson::sequence(&session.commands()?)?.is_empty() {
        require(Instant::now() < end, "missing date command")?;
        std::thread::sleep(Duration::from_millis(10));
    }
    let commands = session.commands()?;
    require(
        pyjson::equal(&commands, &node(&json!([["date", "-s", "@1790000030"]]))?),
        "date command differs",
    )?;
    require(
        !session.fixture.params.join("TimezoneName").exists(),
        "app timezone overwritten",
    )?;
    session.child()?.signal(Signal::TERM)?;
    let code = session.child()?.wait(Duration::from_secs(12))?;
    require(
        code == if implementation == "rust" { 0 } else { -15 },
        "daemon termination differs",
    )?;
    object(&[
        ("passed", node(&json!(true))?),
        ("implementation", JsonValue::text(implementation)),
        ("service", JsonValue::text(service)),
        ("packets", array(&packets)?),
        ("records", array(&records)?),
        ("commands", commands),
        ("sleep_elapsed", node(&json!(sleep_elapsed))?),
        ("exit_code", node(&json!(code))?),
        ("elapsed", node(&json!(monotonic()? - started))?),
    ])
}
fn expected_date(options: &Options, zone: &str, millis: &str) -> Result<Value> {
    let temporary = tempfile::tempdir()?;
    let mut config = timed_fixtures::default_config(Path::new("<root>"));
    // Keep Python's arbitrary integer through the independent policy preflight;
    // only the real GPS schema assignment below imposes the Int64 boundary.
    const INTEGER: &str = "__TIMED_RAW_MILLISECONDS__";
    config["actions"] = json!([{"kind":"step","monotonic":10_000_000_000i64,"gps":{"updated":true,"has_fix":true,"log_mono_time":10_000_000_000i64,"longitude":127.,"unix_timestamp_millis":INTEGER}}]);
    let request = json!({"timezone":zone,"config":config,"loop":true,"initial":{"params_hex":{"TimezoneSource":"617070"},"sudo_exists":true}});
    let mut fixture = Fixture::new(temporary.path(), zone, &options.command)?;
    fixture.prepare(&request)?;
    let mut spec = CommandSpec::new_os(
        vec![options.reference.as_os_str().to_owned()],
        Duration::from_secs(30),
    );
    fixture.configure(&mut spec);
    spec.env_remove
        .extend(["TIMED_WORKER_REQUEST", "TIMED_WORKER_TOKEN"].map(OsString::from));
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let raw = serde_json::to_string(&json!({"config":fixture.config,"loop":true}))?
        .replace(&serde_json::to_string(INTEGER)?, millis);
    spec.input = Some(format!("{raw}\n").into_bytes());
    let captured = NativeRunner.run_in_current_group(&spec)?;
    require(
        captured.code == 0,
        &format!(
            "date policy preflight failed: {}",
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    let source: Value = serde_json::from_slice(&captured.stdout)?;
    Ok(source["errors"][0]["message"].clone())
}
fn date(options: &Options, case: &Value) -> Result<JsonValue> {
    let implementation = case["implementation"]
        .as_str()
        .ok_or("missing implementation")?;
    let zone = case["zone"].as_str().ok_or("missing timezone")?;
    let millis = if let Some(text) = case["milliseconds_text"].as_str() {
        crate::stats_clock_check::integer(text).map_err(|error| format!("ValueError: {error}"))?
    } else {
        case["milliseconds"]
            .as_i64()
            .ok_or("missing GPS milliseconds")?
            .to_string()
    };
    let error = expected_date(options, zone, &millis)?;
    let mut session = Session::new(options, zone, Duration::from_millis(2500))?;
    fs::write(session.fixture.params.join("TimezoneSource"), b"app")?;
    session.fixture.config["live"] = json!(true);
    session.fixture.config["monotonic"] = Value::Null;
    session.open("gpsLocation", false)?;
    session.spawn(options, implementation, &[], "process.log", true)?;
    let mut packets = vec![];
    packets.push(session.clock(false)?.ok_or("initial clocks absent")?.1);
    let millis = millis
        .parse::<i64>()
        .map_err(|_| "OverflowError: Python int too large to convert to C integer")?;
    session.gps(Gps {
        service: "gpsLocation",
        age: 0.,
        fix: true,
        millis,
        longitude: 0.,
        file: Some("gps.capnp"),
        valid: true,
    })?;
    let (passed, code) = if !error.is_null() {
        let code = match session.child()?.wait(Duration::from_secs(4)) {
            Ok(code) => code,
            Err(_) => {
                session.child()?.kill()?;
                session.child()?.wait(Duration::from_secs(3))?
            }
        };
        (code == 1, code)
    } else {
        let mut continued = true;
        for _ in 0..2 {
            if let Some((_, size)) = session.clock(false)? {
                packets.push(size);
            } else {
                continued = false;
                break;
            }
        }
        continued &= session.child()?.poll()?.is_none();
        session.child()?.signal(Signal::INT)?;
        let code = session.child()?.wait(Duration::from_secs(4))?;
        (
            continued && code == if implementation == "rust" { 0 } else { -2 },
            code,
        )
    };
    let commands = session.commands()?;
    let passed = passed && pyjson::sequence(&commands)?.is_empty();
    object(&[
        ("zone", JsonValue::text(zone)),
        ("unix_timestamp_millis", node(&json!(millis))?),
        ("implementation", JsonValue::text(implementation)),
        ("source_error", node(&error)?),
        ("exit_code", node(&json!(code))?),
        ("clock_packet_sizes", node(&json!(packets))?),
        ("commands", commands),
        ("passed", node(&json!(passed))?),
    ])
}
fn sleep_phase(session: &mut Session) -> Result<JsonValue> {
    let end = Instant::now() + Duration::from_secs(5);
    let mut observations = vec![];
    let proc = PathBuf::from(format!("/proc/{}", session.child()?.id()));
    loop {
        require(
            session.child()?.poll()?.is_none(),
            "source exited before GPS sleep",
        )?;
        let state = object(&[
            (
                "wchan",
                JsonValue::text(read_text(&proc.join("wchan"))?.trim()),
            ),
            (
                "syscall",
                JsonValue::text(read_text(&proc.join("syscall"))?.trim()),
            ),
        ])?;
        if observations.last().is_none_or(|previous: &JsonValue| {
            previous
                .get("state")
                .is_none_or(|previous| !pyjson::equal(&previous, &state))
        }) {
            observations.push(object(&[
                ("at", node(&json!(monotonic()?))?),
                ("state", state.clone()),
            ])?);
            write(
                &session.output.join("sleep-readiness.json"),
                &array(&observations)?,
            )?;
        }
        if field(&state, "wchan")?.text_eq("hrtimer_nanosleep") {
            return Ok(state);
        }
        require(
            Instant::now() < end,
            "source did not enter GPS sleep within five seconds",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn shutdown(options: &Options, case: &Value) -> Result<JsonValue> {
    let mode = case["mode"].as_str().ok_or("missing shutdown mode")?;
    let implementation = case["implementation"]
        .as_str()
        .ok_or("missing implementation")?;
    let signal_name = case["signal"].as_str().ok_or("missing signal")?;
    let signal = match signal_name {
        "SIGINT" => Signal::INT,
        "SIGTERM" => Signal::TERM,
        _ => return Err("unsupported shutdown signal".into()),
    };
    let server = proxy::Proxy::new()?;
    let mut session = Session::new(options, "UTC", Duration::from_secs(5))?;
    session.fixture.config["live"] = json!(true);
    session.fixture.config["monotonic"] = Value::Null;
    session.fixture.config["wall"] = json!(wall_ns()?);
    if mode == "gps" {
        fs::write(session.fixture.params.join("TimezoneSource"), b"app")?;
    }
    for key in [
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        session
            .fixture
            .environment
            .insert(key.into(), server.address.clone().into());
    }
    for key in ["no_proxy", "NO_PROXY"] {
        session.fixture.environment.insert(key.into(), "".into());
    }
    session.open("gpsLocation", false)?;
    session.spawn(options, implementation, &[], "process.log", true)?;
    require(
        session.raw_clock("clocks.capnp")?.is_some(),
        "missing shutdown clocks",
    )?;
    let phase = if mode == "gps" {
        let millis = i64::try_from(wall_ns()? / 1_000_000)?
            .checked_add(30000)
            .ok_or("GPS epoch overflow")?;
        session.gps(Gps {
            service: "gpsLocation",
            age: 0.,
            fix: true,
            millis,
            longitude: 0.,
            file: None,
            valid: true,
        })?;
        let end = Instant::now() + Duration::from_secs(5);
        while pyjson::sequence(&session.commands()?)?.is_empty() {
            require(Instant::now() < end, "missing GPS command")?;
            std::thread::sleep(Duration::from_millis(10));
        }
        if implementation != "rust" {
            sleep_phase(&mut session)?
        } else {
            JsonValue::parse("null")?
        }
    } else {
        require(
            server.wait_started(),
            "proxy not contacted within five seconds",
        )?;
        std::thread::sleep(Duration::from_secs(6));
        require(
            server.requests.lock().map_err(|_| "proxy capture mutex")?[0]["chunks"]
                .as_u64()
                .unwrap_or(0)
                > 40,
            "proxy body did not progress",
        )?;
        JsonValue::parse("null")?
    };
    require(
        session.child()?.poll()?.is_none(),
        "timed exited before shutdown signal",
    )?;
    let started = monotonic()?;
    session.child()?.signal(signal)?;
    let code = match session.child()?.wait(Duration::from_secs(12)) {
        Ok(code) => code,
        Err(_) => {
            session.child()?.kill()?;
            session.child()?.wait(Duration::from_secs(3))?
        }
    };
    let elapsed = monotonic()? - started;
    let closed = mode == "gps" || server.wait_closed();
    let rows = server
        .requests
        .lock()
        .map_err(|_| "proxy capture mutex")?
        .clone();
    let requests = rows
        .iter()
        .map(|row| {
            object(&[
                ("path", node(&row["path"])?),
                ("started", node(&row["started"])?),
                ("chunks", node(&row["chunks"])?),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let phase_ok = mode != "gps"
        || implementation == "rust"
        || field(&phase, "wchan")?.text_eq("hrtimer_nanosleep");
    let expected = if implementation == "rust" {
        0
    } else {
        -(signal.as_raw())
    };
    object(&[
        ("mode", JsonValue::text(mode)),
        ("implementation", JsonValue::text(implementation)),
        ("signal", JsonValue::text(signal_name)),
        ("elapsed", node(&json!(elapsed))?),
        ("exit_code", node(&json!(code))?),
        ("peer_closed", node(&json!(closed))?),
        ("requests", array(&requests)?),
        ("commands", session.commands()?),
        ("source_phase", phase),
        (
            "passed",
            node(&json!(
                elapsed < 2. && code == expected && closed && phase_ok
            ))?,
        ),
    ])
}
fn lifecycle(options: &Options) -> Result<JsonValue> {
    let mut results = vec![];
    for mode in ["bounded", "sigint", "sigterm"] {
        let mut session = Session::new(options, "UTC", Duration::from_secs(5))?;
        fs::write(session.fixture.params.join("TimezoneSource"), b"app")?;
        session.open("gpsLocation", false)?;
        session.spawn(
            options,
            "rust",
            if mode == "bounded" {
                &["--cycles", "2"]
            } else {
                &[]
            },
            &format!("{mode}.log"),
            false,
        )?;
        let raw = session
            .raw_clock(&format!("{mode}.capnp"))?
            .ok_or("missing lifecycle clocks")?;
        let mut slice = raw.as_slice();
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut slice,
            capnp::message::ReaderOptions::new(),
        )?;
        let event = message.get_root::<event::Reader<'_>>()?;
        let clock = match event.which()? {
            event::Which::Clocks(value) => value?,
            _ => return Err("unexpected lifecycle clocks event".into()),
        };
        require(
            (clock.get_wall_time_nanos() as f64 / 1e9 - wall_ns()? as f64 / 1e9).abs() < 3.,
            "lifecycle wall time differs",
        )?;
        let age = monotonic()? - event.get_log_mono_time() as f64 / 1e9;
        require((0.0..3.).contains(&age), "lifecycle monotonic age differs")?;
        if mode != "bounded" {
            session.child()?.signal(if mode == "sigint" {
                Signal::INT
            } else {
                Signal::TERM
            })?;
        }
        let code = session.child()?.wait(Duration::from_secs(5))?;
        let commands = session.commands()?;
        require(
            code == 0 && pyjson::sequence(&commands)?.is_empty(),
            "lifecycle exit/commands differ",
        )?;
        results.push(object(&[
            ("mode", JsonValue::text(mode)),
            ("exit_code", node(&json!(code))?),
            ("commands", commands),
            ("packet_bytes", node(&json!(raw.len()))?),
        ])?);
    }
    for (args, expected) in [
        (vec!["--help"], 0),
        (vec!["--cycles", "0"], 1),
        (vec!["--unknown"], 1),
        (vec!["--cycles", "1", "extra"], 1),
    ] {
        let mut argv = vec![timed_fixtures::lexical(&options.binary).into_os_string()];
        argv.extend(args.iter().map(OsString::from));
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(5));
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        spec.inherit_stdin = true;
        spec.env_remove
            .extend(["TIMED_WORKER_REQUEST", "TIMED_WORKER_TOKEN"].map(OsString::from));
        let captured = NativeRunner.run_in_current_group(&spec)?;
        let stdout = std::str::from_utf8(&captured.stdout)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let stderr = std::str::from_utf8(&captured.stderr)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        require(captured.code == expected, "lifecycle CLI exit differs")?;
        results.push(object(&[
            ("arguments", node(&json!(args))?),
            ("exit_code", node(&json!(captured.code))?),
            ("stdout", JsonValue::text(&stdout)),
            ("stderr", JsonValue::text(&stderr)),
        ])?);
    }
    object(&[
        ("passed", node(&json!(true))?),
        ("executions", node(&json!(results.len()))?),
        ("results", array(&results)?),
    ])
}
pub fn worker_cli() -> Result<()> {
    let line =
        if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--inherit-input")) {
            std::env::var("TIMED_WORKER_REQUEST")?
        } else {
            std::io::stdin()
                .lock()
                .lines()
                .next()
                .ok_or("missing timed worker request")??
        };
    let value: Value = serde_json::from_str(&line)?;
    let options = Options {
        root: decode_path(&value["root"])?,
        binary: decode_path(&value["binary"])?,
        output: decode_path(&value["output"])?,
        reference: decode_path(&value["reference"])?,
        command: decode_path(&value["command"])?,
        worker: vec![],
    };
    sources(&options.root)?;
    fs::create_dir_all(&options.output)?;
    let case = &value["case"];
    let result = match case["kind"].as_str() {
        Some("daemon") => daemon(&options, case)?,
        Some("date-daemon") => date(&options, case)?,
        Some("shutdown") => shutdown(&options, case)?,
        Some("lifecycle") => lifecycle(&options)?,
        _ => return Err("unknown timed worker kind".into()),
    };
    write(&options.output.join("result.json"), &result)
}
pub fn source_cli() -> Result<()> {
    source::run()
}

pub fn cases(kind: Kind) -> Vec<Value> {
    match kind {
        Kind::Daemon=>[false,true].into_iter().flat_map(|external|["python","rust"].into_iter().map(move|implementation|json!({"kind":"daemon","external":external,"implementation":implementation}))).collect(),
        Kind::DateDaemon=>[("UTC",253402300800000i64),("UTC",253402300799999),("Etc/GMT+12",253402300800000),("Etc/GMT-14",253402250400000),("Etc/GMT-14",-62135596800001),("Etc/GMT-14",-62135560800000),("Etc/GMT+12",-62135467200000),("UTC",-62135596800000)].into_iter().flat_map(|(zone,milliseconds)|["python","rust"].into_iter().map(move|implementation|json!({"kind":"date-daemon","zone":zone,"milliseconds":milliseconds,"implementation":implementation}))).collect(),
        Kind::Shutdown=>["gps","http"].into_iter().flat_map(|mode|[("python","SIGINT"),("rust","SIGINT"),("rust","SIGTERM")].into_iter().map(move|(implementation,signal)|json!({"kind":"shutdown","mode":mode,"implementation":implementation,"signal":signal}))).collect(),
        Kind::Lifecycle=>vec![json!({"kind":"lifecycle"})],
    }
}
fn case_directory(case: &Value) -> Result<String> {
    let text = |name: &str| -> Result<&str> {
        case[name]
            .as_str()
            .ok_or_else(|| format!("missing case {name}").into())
    };
    Ok(match text("kind")? {
        "daemon" => format!(
            "{}-{}",
            text("implementation")?,
            if case["external"] == true {
                "True"
            } else {
                "False"
            }
        ),
        "date-daemon" => format!(
            "{}-{}-{}",
            text("zone")?.replace('/', "-"),
            case["milliseconds"],
            text("implementation")?
        ),
        "shutdown" => format!(
            "{}-{}-{}",
            text("mode")?,
            text("implementation")?,
            text("signal")?
        ),
        "lifecycle" => "lifecycle".into(),
        _ => return Err("unknown live case directory".into()),
    })
}
#[derive(Debug)]
struct ComparisonFailure(String);
impl std::fmt::Display for ComparisonFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("timed live comparisons failed")
    }
}
impl std::error::Error for ComparisonFailure {}
pub fn run(kind: Kind, options: &Options, worker: &Path) -> Result<String> {
    sources(&options.root)?;
    if !options.worker.is_empty() {
        let case = match kind {
            Kind::DateDaemon => {
                require(
                    options.worker.len() == 3,
                    "date worker needs three arguments",
                )?;
                let millis = crate::stats_clock_check::integer(&options.worker[1])
                    .map_err(|error| format!("ValueError: {error}"))?;
                json!({"kind":"date-daemon","zone":options.worker[0],"milliseconds_text":millis,"implementation":options.worker[2]})
            }
            Kind::Shutdown => {
                require(
                    options.worker.len() == 3,
                    "shutdown worker needs three arguments",
                )?;
                json!({"kind":"shutdown","mode":options.worker[0],"implementation":options.worker[1],"signal":options.worker[2]})
            }
            _ => return Err("worker mode is unsupported for this checker".into()),
        };
        invoke_raw(options, worker, &case)?;
        return Ok(String::new());
    }
    if matches!(kind, Kind::Lifecycle) {
        let result = invoke_raw(options, worker, &json!({"kind":"lifecycle"}))?;
        write(&options.output.join("summary.json"), &result)?;
        return Ok("PASS: real timed entrypoint publication, bounded exit, SIGINT/SIGTERM and CLI errors; zero clock commands\n".into());
    }
    fs::create_dir_all(&options.output)?;
    let scenarios = cases(kind);
    let mut results = vec![];
    if matches!(kind, Kind::Daemon) {
        for case in &scenarios {
            let mut child = options.clone();
            child.output = options.output.join(case_directory(case)?);
            results.push(invoke_raw(&child, worker, case)?);
        }
    } else {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        };
        let next = AtomicUsize::new(0);
        let output = Mutex::new(vec![None; scenarios.len()]);
        std::thread::scope(|scope| {
            for _ in 0..if matches!(kind, Kind::DateDaemon) {
                4
            } else {
                3
            } {
                let next = &next;
                let output = &output;
                let scenarios = &scenarios;
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= scenarios.len() {
                        break;
                    }
                    let case = &scenarios[index];
                    let result = (|| -> Result<JsonValue> {
                        let mut child = options.clone();
                        child.output = options.output.join(case_directory(case)?);
                        invoke_raw(&child, worker, case)
                    })()
                    .map_err(|error| error.to_string());
                    output.lock().expect("live result mutex")[index] = Some(result);
                });
            }
        });
        for value in output
            .into_inner()
            .map_err(|_| "live result mutex poisoned")?
        {
            results.push(
                value
                    .ok_or("missing live worker result")?
                    .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?,
            );
        }
    }
    if matches!(kind, Kind::Daemon) {
        let services = results
            .iter()
            .map(|row| field(row, "service"))
            .collect::<Result<Vec<_>>>()?;
        write(
            &options.output.join("summary.json"),
            &object(&[
                ("passed", node(&json!(true))?),
                ("executions", node(&json!(results.len()))?),
                ("services", array(&services)?),
            ])?,
        )?;
        return Ok("PASS: original and native continuous msgq loops, both GPS topics, real logs, ten-second sleep, command fixtures\n".into());
    }
    let failures = results
        .iter()
        .filter(|row| {
            row.get("passed")
                .is_none_or(|value| value.to_json().ok().as_deref() != Some("true"))
        })
        .count();
    write(
        &options.output.join("summary.json"),
        &object(&[
            ("passed", node(&json!(failures == 0))?),
            ("executions", node(&json!(results.len()))?),
            ("results", array(&results)?),
        ])?,
    )?;
    let text = if matches!(kind, Kind::DateDaemon) {
        format!(
            "{}\n",
            pyjson::pretty(
                &object(&[
                    ("passed", node(&json!(failures == 0))?),
                    ("executions", node(&json!(results.len()))?),
                    ("failed_cases", node(&json!(failures))?)
                ])?,
                0
            )?
        )
    } else {
        "PASS: original SIGINT and native SIGINT/SIGTERM during GPS sleep and >5s progressing proxy HTTP; sockets close\n".into()
    };
    if failures != 0 {
        return Err(Box::new(ComparisonFailure(text)));
    }
    Ok(text)
}
fn worker_arguments(
    args: &[OsString],
) -> std::result::Result<(Vec<OsString>, Vec<OsString>), ArgumentError> {
    let mut rest = vec![];
    let mut worker = vec![];
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_str().unwrap_or("");
        if text == "--" || text == "-h" || (text.starts_with("--") && "--help".starts_with(text)) {
            rest.extend_from_slice(&args[index..]);
            break;
        }
        let key = text.split('=').next().unwrap_or(text);
        if key.starts_with("--w") && "--worker".starts_with(key) {
            if text.contains('=')
                || args.len() - index - 1 < 3
                || args[index + 1..index + 4].iter().any(|arg| arg == "--")
            {
                return Err(ArgumentError(
                    "argument --worker: expected 3 arguments".into(),
                ));
            }
            worker = args[index + 1..index + 4].to_vec();
            index += 4;
        } else if key.starts_with("--")
            && ["--binary", "--output"]
                .iter()
                .any(|option| option.starts_with(key))
            && !text.contains('=')
        {
            let end = (index + 2).min(args.len());
            rest.extend_from_slice(&args[index..end]);
            index = end;
        } else {
            rest.push(args[index].clone());
            index += 1;
        }
    }
    Ok((rest, worker))
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let name = match kind {
        Kind::Daemon => "daemon",
        Kind::DateDaemon => "date_daemon",
        Kind::Lifecycle => "lifecycle",
        Kind::Shutdown => "shutdown",
    };
    let worker_allowed = matches!(kind, Kind::DateDaemon | Kind::Shutdown);
    let mut specs = vec![
        OptionSpec::value("--binary", true),
        OptionSpec::value("--output", true),
    ];
    if worker_allowed {
        specs.push(OptionSpec::value("--worker", false));
    }
    let program = format!("check_timed_{name}.py");
    let usage = format!(
        "usage: {program} [-h] --binary BINARY --output OUTPUT{}\n",
        if worker_allowed {
            format!(
                "\n{}[--worker WORKER WORKER WORKER]",
                " ".repeat(program.len() + 8)
            )
        } else {
            String::new()
        }
    );
    let (scan_args, worker_args) = if worker_allowed {
        worker_arguments(&args)
            .map_err(|error| ArgumentError(format!("{usage}{program}: error: {}", error.0)))?
    } else {
        (args.clone(), vec![])
    };
    let parsed = scan_os_with_positionals(&scan_args, &specs, 0).map_err(|error| {
        let reason = if error.0.ends_with(" required") {
            let optional = specs
                .iter()
                .map(|spec| OptionSpec::value(spec.name, false))
                .collect::<Vec<_>>();
            let values = scan_os_with_positionals(&scan_args, &optional, 0).ok();
            let missing = ["--binary", "--output"]
                .into_iter()
                .filter(|key| {
                    values
                        .as_ref()
                        .is_none_or(|parsed| !parsed.values.contains_key(*key))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("the following arguments are required: {missing}")
        } else if error.0 == "unrecognized arguments" {
            let extras = args
                .iter()
                .filter_map(|arg| arg.to_str())
                .filter(|arg| {
                    arg.starts_with("--")
                        && !["--binary", "--output", "--worker"]
                            .iter()
                            .any(|key| key.starts_with(arg.split('=').next().unwrap_or(arg)))
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("unrecognized arguments: {extras}")
        } else {
            error.0
        };
        ArgumentError(format!("{usage}{program}: error: {reason}"))
    })?;
    if parsed.help {
        print!("{usage}\noptions:\n  -h, --help{}show this help message and exit\n  --binary BINARY\n  --output OUTPUT\n{}",if worker_allowed{"            "}else{"       "},if worker_allowed{"  --worker WORKER WORKER WORKER\n"}else{""});
        return Ok(());
    }
    let helpers = std::env::current_exe()?
        .parent()
        .ok_or("missing helper directory")?
        .to_owned();
    let value = |key: &str| -> Result<OsString> {
        parsed
            .values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing {key}").into())
    };
    let worker_args = worker_args
        .into_iter()
        .map(|value| value.into_string().map_err(|_| "non-Unicode worker".into()))
        .collect::<Result<Vec<_>>>()?;
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: value("--binary")?.into(),
        output: value("--output")?.into(),
        reference: helpers.join("timed-source-reference"),
        command: helpers.join("timed-command-fixture"),
        worker: worker_args,
    };
    match run(kind, &options, &helpers.join("timed-runtime-worker")) {
        Ok(text) => print!("{text}"),
        Err(error) => {
            if let Some(failure) = error.downcast_ref::<ComparisonFailure>() {
                print!("{}", failure.0);
            }
            return Err(error);
        }
    }
    Ok(())
}
