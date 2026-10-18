//! Private worker body. No candidate Version policy or fixture answers.
use super::process::{interrupted, Process};
use super::{path, pretty, require, Request};
use crate::{
    control_tools_policy_check::json as pyjson,
    qa_process::{CommandSpec, NativeRunner},
    seal::{digest, Result},
    version_source::Session,
};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::{JsonValue, JsonView};
use openpilot_msgq::Subscriber;
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const TOPICS: [&str; 2] = ["logMessage", "errorLogMessage"];
const CAPACITY: usize = 10 * 1024 * 1024;

fn text(bytes: &[u8]) -> Result<String> {
    // subprocess(text=True) performs strict decode, then universal newlines,
    // even when check=True will subsequently reject a nonzero child status.
    Ok(std::str::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn python_line(value: &JsonValue) -> Result<String> {
    // JsonValue already writes Python json.dumps separators and ASCII escapes.
    // Adding a second separator space changes the retained source.json bytes.
    Ok(value.to_json()? + "\n")
}
/// Execute exactly one independent operation in a direct worker-group child.
/// The parent enforces10s even if the source operation itself blocks.
pub(super) fn source_once() -> Result<()> {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input)?;
    let mut source = Session::new(&std::env::current_dir()?, &std::env::vars_os().collect())?;
    let expected = source.request(&input)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(python_line(&expected)?.as_bytes())?;
    stdout.flush()?;
    Ok(())
}
struct Collector {
    output: PathBuf,
    root: PathBuf,
    prefix: String,
    child: Option<Process>,
    subscribers: Vec<Subscriber>,
    // Drop the sender before its context, as in original Peer.close().
    socket: Option<zmq::Socket>,
    context: zmq::Context,
}
impl Collector {
    fn new(request: &Request) -> Result<Self> {
        let output = path(request.output.clone()).join("collector");
        let root = output
            .join("home")
            .join(format!(".comma{}", request.prefix))
            .join("log");
        fs::create_dir_all(&root)?;
        Ok(Self {
            output,
            root,
            prefix: request.prefix.clone(),
            child: None,
            subscribers: Vec::new(),
            socket: None,
            context: zmq::Context::new(),
        })
    }
    fn start(&mut self, binary: &Path, stop: &AtomicBool) -> Result<()> {
        let mut command = Command::new(binary);
        command
            .env("HOME", self.output.join("home"))
            .env("OPENPILOT_PREFIX", &self.prefix)
            .stdin(Stdio::inherit())
            .stdout(File::create(self.output.join("stdout.log"))?)
            .stderr(File::create(self.output.join("stderr.log"))?);
        // Inherit the worker's owned group. No nested sessions or process groups.
        self.child = Some(Process::spawn(&mut command)?);
        let namespace = Path::new("/dev/shm").join(format!("msgq_{}", self.prefix));
        let start = Instant::now();
        while !TOPICS.iter().all(|topic| namespace.join(topic).exists()) {
            interrupted(stop)?;
            if self
                .child
                .as_ref()
                .ok_or("collector was not started")?
                .exited()?
            {
                let stderr = text(&fs::read(self.output.join("stderr.log"))?)?;
                return Err(format!("collector exited before queue readiness: {stderr}").into());
            }
            require(
                start.elapsed() < Duration::from_secs(20),
                "collector queue readiness exceeded 20s",
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
        for topic in TOPICS {
            require(
                fs::metadata(namespace.join(topic))?.len() > CAPACITY as u64,
                "collector queue capacity does not match original BIG service",
            )?;
            self.subscribers
                .push(Subscriber::for_runtime(topic, false, CAPACITY)?);
        }
        let socket = self.context.socket(zmq::PUSH)?;
        socket.set_linger(0)?;
        socket.set_sndtimeo(10_000)?;
        socket.connect(&format!("ipc:///tmp/logmessage{}", self.prefix))?;
        self.socket = Some(socket);
        // Preserve Python filesystem strings without lossy UTF-8 conversion or
        // serde's platform-specific OsString object representation.
        let invocation = format!(
            "{{\"argv\": [{}], \"HOME\": {}, \"OPENPILOT_PREFIX\": {}, \"binary_sha256\": {}}}",
            crate::python_fs::json_string(binary.as_os_str())?,
            crate::python_fs::json_string(self.output.join("home").as_os_str())?,
            serde_json::to_string(&self.prefix)?,
            serde_json::to_string(&digest(binary)?)?
        );
        fs::write(self.output.join("invocation.json"), invocation)?;
        Ok(())
    }
    fn receive(&mut self, index: usize, stop: &AtomicBool) -> Result<Vec<u8>> {
        let start = Instant::now();
        loop {
            interrupted(stop)?;
            let left = Duration::from_secs(10)
                .checked_sub(start.elapsed())
                .ok_or("collector receive exceeded original 10s")?;
            if let Some(packet) =
                self.subscribers[index].receive(left.min(Duration::from_millis(50)))?
            {
                return Ok(packet);
            }
        }
    }
    fn stop(&mut self, stop: &AtomicBool) -> Result<()> {
        let child = self.child.as_mut().ok_or("collector was not started")?;
        let start = Instant::now();
        if !child.exited()? {
            child.signal(Signal::INT)?;
        }
        let code = child.wait(Duration::from_secs(10), stop)?;
        pretty(
            &self.output.join("exit.json"),
            &json!({"code":code,"shutdown_seconds":start.elapsed().as_secs_f64(),"signal":2}),
        )?;
        require(code == 0, "collector did not exit0 after SIGINT")
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        // Reap this direct child while the outer worker leader/group still owns
        // descendants. The outer controller removes IPC only after group cleanup.
        self.child = None;
        self.subscribers.clear();
        self.socket = None;
    }
}
fn fields(value: &JsonValue) -> Result<Vec<(String, JsonValue)>> {
    let JsonView::Object(fields) = value.view() else {
        return Err("expected original JSON object".into());
    };
    fields
        .into_iter()
        .map(|(key, value)| {
            let key = key
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .ok_or("non-scalar object key")?;
            Ok((key, value))
        })
        .collect()
}
fn without_logs(source: &JsonValue) -> Result<JsonValue> {
    let mut entries = Vec::new();
    for (key, value) in fields(source)? {
        if key != "logs" {
            entries.push(format!(
                "{}:{}",
                serde_json::to_string(&key)?,
                value.to_json()?
            ));
        }
    }
    Ok(JsonValue::parse(&format!("{{{}}}", entries.join(",")))?)
}
fn projection(record: &JsonValue, expected: &JsonValue) -> Result<()> {
    for (key, value) in fields(expected)? {
        let actual = record
            .get(&key)
            .ok_or_else(|| format!("missing collector record key {key}"))?;
        require(
            pyjson::equal(&actual, &value),
            &format!("collector record differs from independent source: {key}"),
        )?;
    }
    Ok(())
}
pub(super) fn run(request: &Request, stop: &AtomicBool) -> Result<Value> {
    let output = path(request.output.clone());
    let repository = path(request.repository.clone());
    let probe = path(request.probe.clone());
    let binary = path(request.collector.clone());
    let mut collector = Collector::new(request)?;
    collector.start(&binary, stop)?;
    crate::qa_endpoint::acknowledge(
        &PathBuf::from(format!("/tmp/logmessage{}", request.prefix)),
        &path(request.endpoint_channel.clone()),
        &request.nonce,
        stop,
    )?;
    let no_metadata = tempfile::Builder::new()
        .prefix("version-no-metadata-")
        .tempdir()?;
    let request_line = format!(
        "{{\"op\": \"metadata\", \"path\": {}}}\n",
        crate::python_fs::json_string(no_metadata.path().as_os_str())?
    );
    interrupted(stop)?;
    let mut source_command = CommandSpec::new_os(
        vec![
            std::env::current_exe()?.into_os_string(),
            "--source-once".into(),
        ],
        Duration::from_secs(10),
    );
    source_command.input = Some(request_line.as_bytes().to_vec());
    source_command.stdout_limit = usize::MAX;
    source_command.stderr_limit = usize::MAX;
    let source_result = NativeRunner.run_in_current_group(&source_command)?;
    let source_stdout = text(&source_result.stdout)?;
    let source_stderr = text(&source_result.stderr)?;
    require(
        source_result.code == 0,
        &format!(
            "independent source exited {}: {source_stderr}",
            source_result.code
        ),
    )?;
    let expected = pyjson::parse_text(&source_stdout)?;
    interrupted(stop)?;
    let mut command =
        CommandSpec::new_os(vec![probe.as_os_str().to_owned()], Duration::from_secs(10));
    command.input = Some(request_line.into_bytes());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    // The parent worker already owns this group. The shared API only supervises
    // this direct probe; it must not hide a nested process group from cancellation.
    let captured = NativeRunner.run_in_current_group(&command)?;
    let stdout = text(&captured.stdout)?;
    let stderr = text(&captured.stderr)?;
    require(
        captured.code == 0,
        &format!("probe returned nonzero exit {}", captured.code),
    )?;
    drop(no_metadata);
    // Original subprocess(check=True) failed above BEFORE these files existed.
    fs::write(output.join("probe.stdout"), &stdout)?;
    fs::write(output.join("probe.stderr"), &stderr)?;
    fs::write(output.join("source.json"), &source_stdout)?;
    let actual = pyjson::parse_text(&stdout)?;
    require(
        pyjson::equal(&actual, &without_logs(&expected)?),
        "probe response differs from independent missing-metadata source",
    )?;
    let logs = expected
        .get("logs")
        .ok_or("independent source omitted logs")?;
    let JsonView::Array(logs) = logs.view() else {
        return Err("independent source logs are not an array".into());
    };
    require(
        logs.len() == 1,
        "independent source must produce exactly one error log",
    )?;
    let expected_log = &logs[0];
    let mut messages = Vec::new();
    for (index, topic) in TOPICS.iter().enumerate() {
        let raw = collector.receive(index, stop)?;
        fs::write(output.join(format!("{topic}.bin")), &raw)?;
        let message = capnp::serialize::read_message(
            &mut Cursor::new(raw),
            capnp::message::ReaderOptions::new(),
        )?;
        let event = message.get_root::<event::Reader>()?;
        require(
            event.get_valid() && event.get_log_mono_time() > 0,
            "invalid collector Capnp validity/timestamp",
        )?;
        let text = match (*topic, event.which()?) {
            ("logMessage", event::LogMessage(value))
            | ("errorLogMessage", event::ErrorLogMessage(value)) => value?,
            _ => return Err("collector published wrong service union".into()),
        };
        let record = pyjson::parse_text(text.to_str()?)?;
        projection(&record, expected_log)?;
        require(
            record
                .get("ctx")
                .and_then(|ctx| ctx.get("runtime_language"))
                .is_some_and(|v| v.text_eq("rust")),
            "collector record omitted Rust runtime context",
        )?;
        messages.push(record);
    }
    require(
        pyjson::equal(&messages[0], &messages[1]),
        "collector service records differ",
    )?;
    collector.stop(stop)?;
    let mut records = Vec::new();
    for entry in fs::read_dir(&collector.root)? {
        let entry = entry?;
        if entry.file_name().as_bytes().starts_with(b"swaglog.") {
            let text = text(&fs::read(entry.path())?)?;
            for line in text.split_terminator([
                '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
            ]) {
                records.push(pyjson::parse_text(line)?);
            }
        }
    }
    require(
        records.len() == 1,
        "collector disk must contain exactly one record",
    )?;
    let record = &records[0];
    require(
        record.get("levelnum").is_some_and(|level| {
            pyjson::equal(
                &level,
                &JsonValue::parse("40").expect("literal JSON integer"),
            )
        }),
        "disk record levelnum must be40",
    )?;
    for (disk_key, expected_key) in [
        ("msg$s", "msg"),
        ("levelnum", "levelnum"),
        ("exc_info", "exc_info"),
    ] {
        let disk = record
            .get(disk_key)
            .ok_or("disk record omitted expected field")?;
        let expected = expected_log
            .get(expected_key)
            .ok_or("source omitted expected log field")?;
        require(
            pyjson::equal(&disk, &expected),
            "disk record differs from original missing-metadata log",
        )?;
    }
    let array = JsonValue::parse(&format!("[{}]", record.to_json()?))?;
    fs::write(
        output.join("records.json"),
        pyjson::pretty(&array, 0)? + "\n",
    )?;
    let mut native_sources = BTreeMap::new();
    for name in [
        "src/version_collector_check.rs",
        "src/version_collector_check/process.rs",
        "src/version_collector_check/worker.rs",
        "src/version_source.rs",
        "src/version_source/value.rs",
        "src/version_source/metadata.rs",
    ] {
        native_sources.insert(
            name,
            digest(&repository.join("rust/crates/dev-tools").join(name))?,
        );
    }
    let report = json!({"result":"PASS","logMessage":1,"errorLogMessage":1,"disk_records":1,
        "source_logs":serde_json::from_str::<Value>(&logs_to_json(&logs)?)?,
        "probe_sha256":digest(&probe)?,"collector_sha256":digest(&binary)?,
        "implementation":"rust", "worker_sha256":request.worker_sha256,
        "controller_sha256":request.controller_sha256,"source_sha256":request.sources,
        "native_source_sha256":native_sources});
    Ok(report)
}
fn logs_to_json(logs: &[JsonValue]) -> Result<String> {
    Ok(format!(
        "[{}]",
        logs.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))
}
