//! One isolated comparison child. Its outer owner retains the process group
//! until all direct children, collector sockets and HTTP workers have stopped.
use super::{
    field, require,
    runtime::{artifacts, path, pretty, sources, Request},
};
use crate::{
    sdk_receiver::Receiver,
    seal::Result,
    version_collector_check::process::Process,
    version_reference_check::{
        process::{Cancellation, Probe},
        Interrupted,
    },
};
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::Subscriber;
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::{
        fs::{FileExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};

const CAPACITY: usize = 10 * 1024 * 1024;
const TOPICS: [&str; 2] = ["logMessage", "errorLogMessage"];
fn io_error(error: Box<dyn std::error::Error + Send + Sync>) -> Box<dyn std::error::Error> {
    error
}
fn decode(packet: &[u8], index: usize) -> Result<Value> {
    let message = capnp::serialize::read_message(
        Cursor::new(packet),
        capnp::message::ReaderOptions {
            traversal_limit_in_words: None,
            nesting_limit: 65536,
        },
    )?;
    let event = message.get_root::<event::Reader<'_>>()?;
    require(
        event.get_valid() && event.get_log_mono_time() > 0,
        "invalid managed collector event",
    )?;
    let text = match (index, event.which()?) {
        (0, event::LogMessage(text)) | (1, event::ErrorLogMessage(text)) => text?,
        _ => return Err("managed collector wrong union".into()),
    };
    Ok(serde_json::from_str(text.to_str()?)?)
}
fn append(path: &Path, bytes: &[u8]) -> Result<()> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}
// Header ABI is pinned with msgq.h and the collector source closure.
fn queue_header(path: &Path) -> Result<(u64, u64)> {
    let mut bytes = [0; 24];
    File::open(path)?.read_exact_at(&mut bytes, 0)?;
    Ok((
        u64::from_ne_bytes(bytes[..8].try_into()?),
        u64::from_ne_bytes(bytes[16..24].try_into()?),
    ))
}
fn gate_byte(stream: &mut UnixStream, expected: u8, cancel: &Cancellation) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    stream.set_read_timeout(Some(Duration::from_millis(20)))?;
    loop {
        cancel.check()?;
        require(
            Instant::now() < end,
            "publisher initialization gate timed out",
        )?;
        let mut byte = [0];
        match stream.read_exact(&mut byte) {
            Ok(()) => {
                return require(
                    byte[0] == expected,
                    "publisher initialization gate invalid acknowledgement",
                )
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
struct Collector {
    process: Option<Process>,
    subscribers: Vec<Subscriber>,
    socket: Option<zmq::Socket>,
    context: zmq::Context,
    output: PathBuf,
    root: PathBuf,
    gated_queue: Option<PathBuf>,
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.process.take();
        self.subscribers.clear();
        self.socket.take();
    }
}
impl Collector {
    fn start(request: &Request, cancel: &Cancellation) -> Result<Self> {
        let output = path(&request.output).join("collector");
        let root = output
            .join("home")
            .join(format!(".comma{}", request.prefix))
            .join("log");
        fs::create_dir_all(&root)?;
        let mut value = Self {
            process: None,
            subscribers: Vec::new(),
            socket: None,
            context: zmq::Context::new(),
            output,
            root,
            gated_queue: None,
        };
        let endpoint = format!("ipc:///tmp/logmessage{}", request.prefix);
        require(
            !request.gated || (request.original_collector && request.source),
            "readiness only supports source/original collector",
        )?;
        // AF_UNIX addresses are bounded independently of filesystem paths.
        // Keep the gate in an owned short directory even for long user output.
        let gate_home = if request.gated {
            Some(
                tempfile::Builder::new()
                    .prefix("managed-gate-")
                    .tempdir_in("/tmp")?,
            )
        } else {
            None
        };
        let gate_path = gate_home.as_ref().map(|home| home.path().join("gate"));
        let listener = if request.gated {
            let listener = UnixListener::bind(gate_path.as_ref().ok_or("gate path absent")?)?;
            listener.set_nonblocking(true)?;
            Some(listener)
        } else {
            None
        };
        let mut command = if request.original_collector {
            let mut command = Command::new(path(&request.worker));
            command
                .arg(if request.gated {
                    "--managed-collector-gated"
                } else {
                    "--managed-collector"
                })
                .arg(&endpoint)
                .arg(&value.root);
            if request.gated {
                command.arg(gate_path.as_ref().ok_or("gate path absent")?);
            }
            command
        } else {
            Command::new(path(&request.collector))
        };
        command
            .env("HOME", value.output.join("home"))
            .env("OPENPILOT_PREFIX", &request.prefix)
            .stdin(Stdio::null())
            .stdout(File::create(value.output.join("stdout.log"))?)
            .stderr(File::create(value.output.join("stderr.log"))?);
        value.process = Some(Process::spawn(&mut command)?);
        let shm = PathBuf::from(format!("/dev/shm/msgq_{}", request.prefix));
        let mut gate = if let Some(listener) = listener {
            let end = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                cancel.check()?;
                require(
                    !value.process.as_ref().ok_or("collector absent")?.exited()?,
                    "gated collector exited before connect",
                )?;
                require(Instant::now() < end, "publisher gate connection timed out")?;
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            gate_byte(&mut stream, b'R', cancel)?;
            Some(stream)
        } else {
            None
        };
        let before = if request.gated {
            let header = queue_header(&shm.join("errorLogMessage"))?;
            require(
                header.0 >= 1 && header.1 == 0,
                "error publisher initialized before subscriber gate",
            )?;
            Some(header)
        } else {
            None
        };
        let end = Instant::now() + Duration::from_secs(20);
        while !TOPICS.iter().all(|name| shm.join(name).exists()) {
            cancel.check()?;
            require(
                !value.process.as_ref().ok_or("collector absent")?.exited()?,
                "managed collector exited before ready",
            )?;
            require(
                Instant::now() < end,
                "managed collector queue startup timeout",
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
        for topic in TOPICS {
            require(
                fs::metadata(shm.join(topic))?.len() > CAPACITY as u64,
                "managed collector queue capacity reduced",
            )?;
            value
                .subscribers
                .push(Subscriber::for_runtime(topic, false, CAPACITY)?);
        }
        let reset = if let Some(stream) = gate.as_mut() {
            let queue = shm.join("errorLogMessage");
            let subscribed = queue_header(&queue)?;
            require(
                subscribed.0 >= 2 && subscribed.1 == 0,
                "error subscriber did not precede publisher",
            )?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            stream.write_all(b"1")?;
            gate_byte(stream, b'1', cancel)?;
            let reset = queue_header(&queue)?;
            require(
                reset.0 == 0 && reset.1 != 0,
                "publisher failed to reset preexisting readers",
            )?;
            pretty(
                &value.output.join("initialization-order.json"),
                &json!({
                    "subscriber_created":subscribed.0 >= 2,
                    "publisher_initialized_after_subscriber":subscribed.1 == 0 && reset.1 != 0,
                    "before":before,"subscribed":subscribed,"reset":reset,
                }),
            )?;
            value.gated_queue = Some(queue);
            Some(reset)
        } else {
            None
        };
        drop(gate);
        drop(gate_home);
        let socket = value.context.socket(zmq::PUSH)?;
        socket.set_linger(0)?;
        socket.set_sndtimeo(10000)?;
        socket.connect(&endpoint)?;
        value.socket = Some(socket);
        let end = Instant::now() + Duration::from_secs(20);
        let mut sequence = 0;
        loop {
            cancel.check()?;
            require(
                Instant::now() < end,
                "managed collector did not acknowledge readiness",
            )?;
            require(
                !value.process.as_ref().ok_or("collector absent")?.exited()?,
                "managed collector exited during handshake",
            )?;
            sequence += 1;
            let marker = json!({"msg":format!("ready-{}-{sequence}",request.prefix)});
            value
                .socket
                .as_ref()
                .ok_or("collector sender absent")?
                .send_multipart([&[10][..], marker.to_string().as_bytes()], 0)?;
            let retry = (Instant::now() + Duration::from_millis(50)).min(end);
            while Instant::now() < retry {
                if let Some(packet) = value.subscribers[0].receive(Duration::from_millis(1))? {
                    append(&value.output.join("readiness.bin"), &packet)?;
                    if decode(&packet, 0)? == marker {
                        require(
                            value.subscribers[1].receive(Duration::ZERO)?.is_none(),
                            "unexpected readiness error log",
                        )?;
                        if let Some(reset) = reset {
                            let refreshed = queue_header(
                                value.gated_queue.as_ref().ok_or("gated queue absent")?,
                            )?;
                            require(
                                refreshed.0 >= 1 && refreshed.1 == reset.1,
                                "error subscriber did not refresh after publisher reset",
                            )?;
                            pretty(
                                &value.output.join("refreshed-header.json"),
                                &json!({"refreshed":refreshed}),
                            )?;
                        }
                        pretty(
                            &value.output.join("readiness.json"),
                            &json!({"acknowledged_marker":marker,"attempts":sequence}),
                        )?;
                        return Ok(value);
                    }
                }
            }
        }
    }
    fn receive(&mut self, index: usize, output: &Path, cancel: &Cancellation) -> Result<Value> {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            cancel.check()?;
            require(Instant::now() < end, "managed collector event timeout")?;
            if let Some(packet) = self.subscribers[index].receive(Duration::from_millis(20))? {
                append(&output.join(format!("{}.bin", TOPICS[index])), &packet)?;
                return decode(&packet, index);
            }
        }
    }
    fn stop(&mut self, cancel: &Cancellation, original: bool) -> Result<Vec<Value>> {
        if let Some(queue) = &self.gated_queue {
            let raw = fs::read(queue)?;
            let exited = self.process.as_ref().ok_or("collector absent")?.exited()?;
            require(!exited, "gated collector exited before diagnostic")?;
            let contains_crash = raw
                .windows(b"\"crash\"".len())
                .any(|window| window == b"\"crash\"");
            pretty(
                &self.output.join("queue-diagnostic.json"),
                &json!({
                    "collector_returncode_before_close":Value::Null,
                    "error_queue_contains_crash":contains_crash,
                    "error_queue_sha256":crate::seal::digest(queue)?,
                    "namespace":std::env::var("OPENPILOT_PREFIX")?,
                }),
            )?;
            require(
                contains_crash,
                "gated error queue did not retain actual crash",
            )?;
        }
        for subscriber in &mut self.subscribers {
            require(
                subscriber.receive(Duration::ZERO)?.is_none(),
                "unexpected managed log after scenario",
            )?;
        }
        let process = self.process.as_mut().ok_or("collector absent")?;
        let at = Instant::now();
        if !process.exited()? {
            process.signal(Signal::INT)?;
        }
        let code = process.wait(Duration::from_secs(10), &cancel.flag())?;
        require(
            code == 0 || (original && code == -2),
            "managed collector shutdown failed",
        )?;
        pretty(
            &self.output.join("exit.json"),
            &json!({"code":code,"shutdown_seconds":at.elapsed().as_secs_f64()}),
        )?;
        let mut files = fs::read_dir(&self.root)?
            .map(|entry| entry.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        files.sort();
        let mut disk = Vec::new();
        for file in files {
            if file
                .file_name()
                .is_some_and(|s| s.as_encoded_bytes().starts_with(b"swaglog."))
            {
                for line in fs::read_to_string(file)?.lines() {
                    disk.push(serde_json::from_str(line)?);
                }
            }
        }
        Ok(disk)
    }
}
fn fixture(root: &Path, prefix: &str, scenario: &Value) -> Result<()> {
    fs::create_dir_all(root.join("base/openpilot/common"))?;
    let params = root.join("params").join(prefix);
    fs::create_dir_all(&params)?;
    fs::write(
        root.join("base/openpilot/common/version.h"),
        "#define VERSION \"fixture-entry\"\n",
    )?;
    let origin = if scenario.get("reporting") == Some(&Value::Bool(false)) {
        "git@github.com:fixture/fork.git"
    } else {
        "git@github.com:commaai/openpilot.git"
    };
    pretty(
        &root.join("base/build.json"),
        &json!({"channel":"nightly","openpilot":{"git_origin":origin,"git_commit":"abcdefghijk"}}),
    )?;
    fs::write(params.join("DongleId"), "fixture-device")?;
    fs::write(root.join("prepared-input"), "prepared native body")?;
    fs::write(root.join("params-blocked"), "not a directory")?;
    for (key, name) in [
        ("sent", "CarrotExceptionSent"),
        ("prior", "CarrotException"),
    ] {
        if let Some(value) = scenario.get(key) {
            fs::write(
                params.join(name),
                value.as_str().ok_or("invalid fixture parameter")?,
            )?;
        }
    }
    Ok(())
}
fn line(child: &mut Probe, output: &Path, cancel: &Cancellation) -> Result<Value> {
    let bytes = child.line_until(cancel, Instant::now() + Duration::from_secs(15))?;
    append(&output.join("responses.jsonl"), &bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn execute(request: &Request, cancel: &Cancellation) -> Result<Value> {
    let root = path(&request.root);
    let output = path(&request.output);
    let mut collector = Collector::start(request, cancel)?;
    // Bind cleanup identity while the actual collector is ready. The outer
    // owner acknowledges its held inode before any source/candidate can run.
    crate::qa_endpoint::acknowledge(
        Path::new(&format!("/tmp/logmessage{}", request.prefix)),
        &path(&request.endpoint_channel),
        &request.nonce,
        &cancel.flag(),
    )?;
    let mut receiver = Receiver::bind(Duration::from_secs(10)).map_err(io_error)?;
    let mut body_subscriber = Subscriber::for_runtime("managedEntryBody", false, 1024 * 1024)?;
    fixture(&root, &request.prefix, &request.scenario)?;
    let mut child_request = json!({"root":root,"prefix":request.prefix,"endpoint":format!("ipc:///tmp/logmessage{}",request.prefix),"dsn":receiver.dsn(),"process":"openpilot.fixture_managed_entry","daemon":"managed-fixture"});
    child_request
        .as_object_mut()
        .ok_or("request not object")?
        .extend(
            request
                .scenario
                .as_object()
                .ok_or("scenario not object")?
                .clone(),
        );
    let child_path = output.join("request.json");
    pretty(&child_path, &child_request)?;
    let argv = if request.source {
        vec![
            path(&request.worker).into_os_string(),
            "--managed-source".into(),
            child_path.into_os_string(),
        ]
    } else {
        let mut argv = request
            .runner
            .iter()
            .map(|v| path(v).into_os_string())
            .collect::<Vec<_>>();
        argv.extend([
            path(&request.binary).into_os_string(),
            child_path.into_os_string(),
        ]);
        argv
    };
    let environment = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
    let mut child = Probe::spawn_in_worker(
        &argv,
        &path(&request.repository),
        &environment,
        File::create(output.join("stderr.txt"))?,
    )?;
    let ready = line(&mut child, &output, cancel)?;
    require(
        ready["ready"] == true
            && ready["reporting_enabled"]
                == child_request
                    .get("reporting")
                    .cloned()
                    .unwrap_or(json!(true)),
        "managed child incorrect ready response",
    )?;
    require(
        collector.receive(0, &output, cancel)?["msg"] == "managed-entry-ready",
        "managed ready log missing",
    )?;
    child.write(b"continue\n", cancel)?;
    let mut response = line(&mut child, &output, cancel)?;
    if response.get("context_ready") == Some(&Value::Bool(true)) {
        if request.source {
            require(
                response["context_replaced"] == true,
                "managed source context not replaced",
            )?;
        }
        require(
            body_subscriber.receive(Duration::ZERO)?.is_none(),
            "body publication before continue",
        )?;
        child.write(b"continue\n", cancel)?;
        response = line(&mut child, &output, cancel)?;
    }
    let mut signal_seconds = None;
    if response.get("waiting_for_sigint") == Some(&Value::Bool(true)) {
        let at = Instant::now();
        child.signal(Signal::INT)?;
        response = line(&mut child, &output, cancel)?;
        require(
            at.elapsed() < Duration::from_secs(5),
            "managed SIGINT response exceeded5s",
        )?;
        signal_seconds = Some(at.elapsed().as_secs_f64());
    }
    let body = field(&response, "steps")?
        .as_array()
        .ok_or("managed steps not array")?
        .iter()
        .any(|step| step["step"] == "body");
    let kind = field(field(&response, "outcome")?, "kind")?
        .as_str()
        .ok_or("managed outcome not text")?;
    let mut expected = Vec::new();
    if body {
        expected.push(("managed body entered".to_owned(), 20));
    }
    if kind == "interrupted" {
        expected.push((
            format!(
                "child {} got SIGINT",
                child_request["process"].as_str().ok_or("process missing")?
            ),
            30,
        ));
    } else if ["raised", "reporting_failed"].contains(&kind)
        && child_request.get("close_logger") != Some(&Value::Bool(true))
    {
        expected.push(("crash".into(), 40));
    }
    if child_request
        .get("sdk_failure")
        .and_then(Value::as_str)
        .is_some_and(|s| ["capture_exception", "flush"].contains(&s))
    {
        expected.push(("sentry exception".into(), 40));
    }
    let mut records = json!({"logMessage":[],"errorLogMessage":[]});
    for (message, level) in expected {
        let value = collector.receive(0, &output, cancel)?;
        require(
            value["msg"] == message && value["levelnum"] == level,
            "managed policy log message/level differs",
        )?;
        records["logMessage"]
            .as_array_mut()
            .ok_or("record array absent")?
            .push(value.clone());
        if level >= 40 {
            let error = collector.receive(1, &output, cancel)?;
            require(error == value, "managed error publication differs")?;
            records["errorLogMessage"]
                .as_array_mut()
                .ok_or("error array absent")?
                .push(error);
        }
    }
    if body {
        let packet = body_subscriber
            .receive(Duration::from_secs(5))?
            .ok_or("managed body publication missing")?;
        require(
            packet == b"managed body IPC",
            "managed body publication differs",
        )?;
        fs::write(output.join("body-message.bin"), packet)?;
    } else {
        require(
            body_subscriber.receive(Duration::ZERO)?.is_none(),
            "unexpected managed body publication",
        )?;
    }
    let mut captures = Vec::new();
    let actual_capture = field(&response, "sdk_calls")?
        .as_array()
        .ok_or("SDK calls not array")?
        .iter()
        .any(|call| call["op"] == "capture_exception")
        && child_request.get("sdk_failure").is_none();
    if request.source {
        let boundary = field(&response, "external_sdk_boundary")?;
        require(
            boundary["kind"] == "captured-original-sdk-2.55.0-selected-fields",
            "unknown source external SDK boundary",
        )?;
        captures = field(boundary, "events")?
            .as_array()
            .ok_or("source SDK events not array")?
            .iter()
            .map(|v| json!({"event":v}))
            .collect();
    } else if child_request.get("real_sdk") == Some(&Value::Bool(true))
        && ready["reporting_enabled"] == true
        && actual_capture
    {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            cancel.check()?;
            require(Instant::now() < end, "managed native SDK event timeout")?;
            match receiver.receive(Duration::from_millis(20)) {
                Ok(capture) => {
                    fs::write(output.join("sdk-envelope.bin"), &capture.wire)?;
                    let event: Value = serde_json::from_str(&capture.event.to_json()?)?;
                    captures.push(json!({"event":event,"path":capture.path,"auth":capture.auth}));
                    break;
                }
                Err(error)
                    if error.downcast_ref::<RecvTimeoutError>()
                        == Some(&RecvTimeoutError::Timeout) => {}
                Err(error) => return Err(io_error(error)),
            }
        }
    }
    receiver.require_empty().map_err(io_error)?;
    child.write(b"continue\n", cancel)?;
    let code = child.finish_status(cancel, Duration::from_secs(5))?;
    require(
        code == if ["returned", "interrupted"].contains(&kind) {
            0
        } else {
            1
        },
        "managed child exit status differs",
    )?;
    let parameter = root
        .join("params")
        .join(&request.prefix)
        .join("CarrotException");
    let params = if parameter.is_file() {
        Some(fs::read_to_string(parameter)?)
    } else {
        None
    };
    fs::set_permissions(root.join("params"), fs::Permissions::from_mode(0o700))?;
    let disk = collector.stop(cancel, request.original_collector)?;
    let wire = records["logMessage"]
        .as_array()
        .ok_or("wire array absent")?;
    require(disk.len() == wire.len(), "managed disk/wire counts differ")?;
    for (stored, wire) in disk.iter().zip(wire) {
        let mut stored = stored.as_object().ok_or("disk record not object")?.clone();
        require(stored.remove("id").is_some(), "disk record id missing")?;
        let message = stored.remove("msg$s").ok_or("disk typed message missing")?;
        stored.insert("msg".into(), message);
        require(
            Value::Object(stored) == *wire,
            "managed disk record differs from real wire",
        )?;
    }
    receiver.close().map_err(io_error)?;
    // close joins the owner and every accepted request. A disconnected empty
    // channel is expected here; a last envelope/error must still fail.
    match receiver.receive(Duration::ZERO) {
        Err(error)
            if matches!(
                error.downcast_ref::<RecvTimeoutError>(),
                Some(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout)
            ) => {}
        Err(error) => return Err(io_error(error)),
        Ok(_) => return Err("unexpected managed SDK envelope during teardown".into()),
    }
    let mut result = json!({"result":"PASS","response":response,"ready":ready,"exit_code":code,"signal_response_seconds":signal_seconds,"records":records,"disk":disk,"params":params,"captures":captures});
    if request.source {
        result["sdk_boundary"] = json!("captured-original-sdk-2.55.0-selected-fields");
    }
    pretty(&output.join("result.json"), &result)?;
    Ok(result)
}
pub(super) fn run(request_path: &Path) -> Result<Value> {
    let request: Request = serde_json::from_slice(&fs::read(request_path)?)?;
    require(
        sources(&path(&request.repository))? == request.sources,
        "managed worker source differs",
    )?;
    require(
        artifacts(
            &path(&request.binary),
            &path(&request.collector),
            &path(&request.binding),
            &path(&request.worker),
        )? == request.artifacts,
        "managed worker artifact differs",
    )?;
    require(
        std::env::var("OPENPILOT_PREFIX")? == request.prefix,
        "managed worker namespace differs",
    )?;
    let cancel = Cancellation::new()?;
    let result = execute(&request, &cancel);
    if let Some(signal) = cancel.signal() {
        return Err(Interrupted(signal).into());
    }
    Ok(json!({"nonce":request.nonce,"report":result?}))
}
