//! Actual native collector/daemon worker. The parent owns its process group,
//! private namespace and all nested retrace helpers throughout error cleanup.
use super::{
    common::{array, boolean, field, number, object, path, pretty, require, Request},
    sources,
};
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    sdk_receiver::Receiver,
    seal::{digest, Result},
    tombstone_source::Session,
    version_collector_check::process::Process,
    version_reference_check::{process::Cancellation, Interrupted},
};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::{JsonValue as J, JsonView};
use openpilot_msgq::Subscriber;
use rustix::process::Signal;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};
const STAMP: &str = "2020-01-02--03-04-05";
const TRACE: &str =
    "header\n#0 0xdead system (p=0x1) at sys.c\n#1 0xbeef model (a=1) at selfdrive/modeld.cc:7\n";
pub(super) const BODY:&str="ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\nSignal: 11\nProcMaps:\n omitted\nProcStatus:\n kept\nCoreDump: omitted\n";
const CAPACITY: usize = 10 * 1024 * 1024;
const TOPICS: [&str; 2] = ["logMessage", "errorLogMessage"];
fn io_error(error: Box<dyn std::error::Error + Send + Sync>) -> Box<dyn std::error::Error> {
    error
}
fn argv(values: &[OsString]) -> Result<J> {
    array(
        &values
            .iter()
            .map(|v| crate::python_fs::json_value(v))
            .collect::<Vec<_>>(),
    )
}
fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: Option<u32>) -> Result<()> {
    fs::write(path, bytes)?;
    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}
fn fixture(repository: &Path, root: &Path, namespace: &str, held: bool) -> Result<()> {
    for name in [
        "apport".into(),
        "logs".into(),
        "base/openpilot/common".into(),
        format!("params/{namespace}"),
        "source-params/fixture".into(),
        "bin".into(),
    ] {
        fs::create_dir_all(root.join(name))?;
    }
    write(
        &root.join("base/build.json"),
        r#"{"channel":"nightly","openpilot":{"git_origin":"git@github.com:commaai/openpilot.git","git_commit":"abcdefghijk"}}"#,
        None,
    )?;
    write(
        &root.join("base/openpilot/common/version.h"),
        "#define VERSION \"fixture-runtime\"\n",
        None,
    )?;
    write(
        &root.join("params").join(namespace).join("DongleId"),
        "fixture-device",
        None,
    )?;
    write(
        &root.join("source-params/fixture/DongleId"),
        "fixture-device",
        None,
    )?;
    write(&root.join("trace"), TRACE, None)?;
    let name = if held {
        "apport-held"
    } else {
        "apport-runtime"
    };
    let helper = repository
        .join("rust/crates/dev-tools/src/tombstone_runtime_check")
        .join(name)
        .canonicalize()?;
    let expected = if held {
        include_bytes!("apport-held").as_slice()
    } else {
        include_bytes!("apport-runtime").as_slice()
    };
    require(
        fs::read(&helper)? == expected && fs::metadata(&helper)?.permissions().mode() & 0o111 != 0,
        "changed immutable tombstone shell helper",
    )?;
    symlink(helper, root.join("bin/apport-retrace"))?;
    for name in ["old.crash", ".hidden.crash"] {
        write(&root.join("apport").join(name), BODY, Some(0o640))?;
    }
    Ok(())
}
fn environment(root: &Path) -> BTreeMap<OsString, OsString> {
    let mut env = std::env::vars_os().collect::<BTreeMap<_, _>>();
    env.insert("PARAMS_ROOT".into(), root.join("params").into_os_string());
    let mut bin = root.join("bin").into_os_string();
    bin.push(":/usr/bin:/bin");
    env.insert("PATH".into(), bin);
    for (key, file) in [
        ("FIXTURE_INPUT", "retrace.input"),
        ("FIXTURE_TRACE", "trace"),
        ("FIXTURE_PID", "helper.pid"),
        ("FIXTURE_DESCENDANT", "descendant.pid"),
    ] {
        env.insert(key.into(), root.join(file).into_os_string());
    }
    env
}
fn child(
    argv: &[OsString],
    cwd: &Path,
    env: &BTreeMap<OsString, OsString>,
    stdout: &Path,
    stderr: &Path,
    session: bool,
) -> Result<Process> {
    let first = argv.first().ok_or("empty live child command")?;
    let mut command = Command::new(first);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::inherit())
        .stderr(File::create(stderr)?)
        .stdout(File::create(stdout)?);
    if session {
        openpilot_process_limits::new_session(&mut command);
    }
    Process::spawn(&mut command)
}
fn wait_until(
    process: &Process,
    seconds: u64,
    cancel: &Cancellation,
    mut ready: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        cancel.check()?;
        if ready()? {
            return Ok(());
        }
        require(!process.exited()?, "live child exited before readiness")?;
        require(Instant::now() < end, "live child readiness timed out")?;
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn wait_sleep(process: &Process, stderr: &Path, cancel: &Cancellation) -> Result<f64> {
    let pid = process.reserved_pid()?.as_raw_pid();
    wait_until(process, 20, cancel, || {
        Ok(fs::read_to_string(stderr)?.contains("tombstoned: ready")
            && fs::read_to_string(format!("/proc/{pid}/wchan"))?.trim() == "hrtimer_nanosleep")
    })?;
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(now.tv_sec as f64 + now.tv_nsec as f64 / 1e9)
}
struct Collector {
    process: Option<Process>,
    subscribers: Vec<Subscriber>,
    socket: Option<zmq::Socket>,
    _context: zmq::Context,
    root: PathBuf,
    output: PathBuf,
}
impl Collector {
    fn start(binary: &Path, output: &Path, prefix: &str, cancel: &Cancellation) -> Result<Self> {
        fs::create_dir(output)?;
        let root = output
            .join("home")
            .join(format!(".comma{prefix}"))
            .join("log");
        fs::create_dir_all(&root)?;
        let mut value = Self {
            process: None,
            subscribers: Vec::new(),
            socket: None,
            _context: zmq::Context::new(),
            root,
            output: output.to_owned(),
        };
        let mut env = std::env::vars_os().collect::<BTreeMap<_, _>>();
        env.insert("HOME".into(), output.join("home").into_os_string());
        env.insert("OPENPILOT_PREFIX".into(), prefix.into());
        let args = vec![binary.as_os_str().to_owned()];
        value.process = Some(child(
            &args,
            &std::env::current_dir()?,
            &env,
            &output.join("stdout.log"),
            &output.join("stderr.log"),
            false,
        )?);
        let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        wait_until(
            value.process.as_ref().ok_or("collector absent")?,
            20,
            cancel,
            || Ok(TOPICS.iter().all(|topic| shm.join(topic).exists())),
        )?;
        for topic in TOPICS {
            require(
                fs::metadata(shm.join(topic))?.len() > CAPACITY as u64,
                "collector queue below original BIG capacity",
            )?;
            value
                .subscribers
                .push(Subscriber::for_runtime(topic, false, CAPACITY)?);
        }
        let socket = value._context.socket(zmq::PUSH)?;
        socket.set_linger(0)?;
        socket.set_sndtimeo(10000)?;
        socket.connect(&format!("ipc:///tmp/logmessage{prefix}"))?;
        value.socket = Some(socket);
        pretty(
            &output.join("invocation.json"),
            &object(&[
                ("argv", argv(&args)?),
                (
                    "HOME",
                    crate::python_fs::json_value(output.join("home").as_os_str()),
                ),
                ("OPENPILOT_PREFIX", J::text(prefix)),
                ("binary_sha256", J::text(&digest(binary)?)),
            ])?,
        )?;
        Ok(value)
    }
    fn receive(&mut self, index: usize, output: &Path, cancel: &Cancellation) -> Result<J> {
        let end = Instant::now() + Duration::from_secs(10);
        let packet = loop {
            cancel.check()?;
            let left = end
                .checked_duration_since(Instant::now())
                .ok_or("tombstone wire receive exceeded original10s")?;
            if let Some(packet) =
                self.subscribers[index].receive(left.min(Duration::from_millis(20)))?
            {
                break packet;
            }
        };
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(output)?
            .write_all(&packet)?;
        let reader = capnp::serialize::read_message(
            Cursor::new(&packet),
            capnp::message::ReaderOptions {
                traversal_limit_in_words: None,
                nesting_limit: 65536,
            },
        )?;
        let event = reader.get_root::<event::Reader<'_>>()?;
        require(
            event.get_valid() && event.get_log_mono_time() > 0,
            "invalid or zero-time tombstone wire Event",
        )?;
        let text = match (index, event.which()?) {
            (0, event::LogMessage(text)) => text?,
            (1, event::ErrorLogMessage(text)) => text?,
            _ => return Err("incorrect tombstone wire union".into()),
        };
        Ok(J::parse(text.to_str()?)?)
    }
    fn stop(&mut self, cancel: &Cancellation) -> Result<f64> {
        let process = self.process.as_mut().ok_or("collector closed")?;
        let at = Instant::now();
        if !process.exited()? {
            process.signal(Signal::INT)?;
        }
        let code = process.wait(Duration::from_secs(10), &cancel.flag())?;
        let elapsed = at.elapsed().as_secs_f64();
        pretty(
            &self.output.join("exit.json"),
            &object(&[
                ("code", number(code)?),
                ("shutdown_seconds", number(elapsed)?),
                ("signal", number(2)?),
            ])?,
        )?;
        require(code == 0, "collector SIGINT did not exit0")?;
        Ok(elapsed)
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.process.take();
        self.subscribers.clear();
        self.socket.take();
    }
}
fn event(receiver: &Receiver, cancel: &Cancellation) -> Result<J> {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        cancel.check()?;
        let left = end
            .checked_duration_since(Instant::now())
            .ok_or("SDK event exceeded original5s")?;
        match receiver.receive(left.min(Duration::from_millis(20))) {
            Ok(capture) => return Ok(capture.event),
            Err(error)
                if error.downcast_ref::<RecvTimeoutError>() == Some(&RecvTimeoutError::Timeout) => {
            }
            Err(error) => return Err(io_error(error)),
        }
    }
}
fn source_report(
    root: &Path,
    environment: &BTreeMap<OsString, OsString>,
    output: &Path,
    binding: &Path,
    cancel: &Cancellation,
) -> Result<J> {
    write(&root.join("source.crash"), BODY, None)?;
    // Retain the original source-child admission phase. The legacy binding is
    // sealed provenance only; the independent native Params source is executed.
    digest(binding)?;
    let requests = [
        object(&[
            ("op", J::text("configure")),
            (
                "base",
                crate::python_fs::json_value(root.join("base").as_os_str()),
            ),
            (
                "params",
                crate::python_fs::json_value(root.join("source-params").as_os_str()),
            ),
            ("pc", boolean(false)),
            ("device", J::text("tici")),
        ])?,
        object(&[
            ("op", J::text("report")),
            (
                "path",
                crate::python_fs::json_value(root.join("source.crash").as_os_str()),
            ),
            (
                "root",
                crate::python_fs::json_value(root.join("source-logs").as_os_str()),
            ),
            ("stamp", J::text(STAMP)),
        ])?,
    ];
    let mut source = Session::new(root, environment)?;
    source.set_stderr(File::create(output.join("source.stderr"))?)?;
    let mut stdout = File::create(output.join("source.stdout"))?;
    let mut last = None;
    for request in requests {
        let response = source.request_controlled(
            request.to_json()?.as_bytes(),
            Instant::now() + Duration::from_secs(40),
            cancel.flag(),
        )?;
        writeln!(stdout, "{}", response.to_json()?)?;
        stdout.flush()?;
        last = Some(response);
    }
    last.ok_or_else(|| "no source report response".into())
}
fn sdk_call(calls: &J, op: &str, key: Option<&str>) -> Result<J> {
    let JsonView::Array(values) = calls.view() else {
        return Err("source calls are not array".into());
    };
    for value in values {
        if field(&value, "op")?.text_eq(op) {
            let fields = field(&value, "fields")?;
            if key.is_none_or(|key| fields.get("key").is_some_and(|v| v.text_eq(key))) {
                return Ok(fields);
            }
        }
    }
    Err(format!("source did not call {op}").into())
}
fn runtime(request: &Request, cancel: &Cancellation) -> Result<J> {
    let root = path(&request.root);
    let output = path(&request.output);
    let repository = path(&request.repository);
    fixture(&repository, &root, &request.prefix, false)?;
    let mut receiver = Receiver::bind(Duration::from_secs(5)).map_err(io_error)?;
    let mut collector = Collector::start(
        &path(&request.collector),
        &output.join("collector"),
        &request.prefix,
        cancel,
    )?;
    let env = environment(&root);
    let mut command = vec![
        OsString::from_vec(request.daemon.clone()),
        "--base-dir".into(),
        root.join("base").into_os_string(),
        "--apport-dir".into(),
        root.join("apport").into_os_string(),
        "--log-root".into(),
        root.join("logs").into_os_string(),
        "--cycles".into(),
        if request.enabled { "3" } else { "2" }.into(),
        "--local-sentry-dsn".into(),
        receiver.dsn().into(),
    ];
    if request.enabled {
        command.extend(["--local-reporting-device".into(), "tici".into()]);
    }
    let stderr = output.join("daemon.stderr");
    let mut daemon = child(
        &command,
        &root,
        &env,
        &output.join("daemon.stdout"),
        &stderr,
        false,
    )?;
    let entered = wait_sleep(&daemon, &stderr, cancel)?;
    require(
        !root.join("apport/old.crash").exists() && root.join("apport/.hidden.crash").exists(),
        "first daemon scan did not preserve original hidden-file boundary",
    )?;
    let mut raw = b"new-\xff-".to_vec();
    raw.extend_from_slice("한".as_bytes());
    raw.extend_from_slice(b"-\xe2\x82.crash");
    let new = root.join("apport").join(OsString::from_vec(raw));
    write(&new, BODY, Some(0o640))?;
    write(&root.join("apport/invalid.crash"), b"bad\xff", Some(0o640))?;
    write(
        &root.join("apport/tombstone-unknown"),
        b"unknown",
        Some(0o640),
    )?;
    write(
        &root.join("apport/wrong-mode.crash"),
        b"ignored",
        Some(0o600),
    )?;
    let created = Instant::now();
    let mut records = [Vec::new(), Vec::new()];
    let delay = if request.enabled {
        for (index, count) in [(0, 6), (1, 3)] {
            for _ in 0..count {
                records[index].push(collector.receive(
                    index,
                    &output.join(format!("{}.bin", TOPICS[index])),
                    cancel,
                )?);
            }
        }
        let delay = created.elapsed().as_secs_f64();
        require(
            (4.0..9.5).contains(&delay),
            format!("original real scan delay outside4..9.5: {delay}"),
        )?;
        let event = event(&receiver, cancel)?;
        require(
            crate::control_tools_policy_check::json::equal(
                &field(&event, "user")?,
                &object(&[("id", J::text("fixture-device"))])?,
            ),
            "SDK user differs",
        )?;
        let tags = field(&event, "tags")?;
        require(
            matches!(field(&tags, "dirty")?.view(), JsonView::Bool(false))
                && field(&tags, "device")?.text_eq("tici"),
            "SDK dirty/device tags differ",
        )?;
        let source = source_report(&root, &env, &output, &path(&request.binding), cancel)?;
        require(
            crate::control_tools_policy_check::json::equal(
                &field(&source, "result")?,
                &J::parse("{\"value\":null}")?,
            ),
            "independent source report failed",
        )?;
        let calls = field(&source, "calls")?;
        let expected_message = field(&sdk_call(&calls, "capture_message", None)?, "message")?;
        let expected_body = field(
            &sdk_call(&calls, "set_extra", Some("tombstone"))?,
            "value_json",
        )?;
        let JsonView::Text(points) = expected_body.view() else {
            return Err("source extra body is not JSON text".into());
        };
        let expected_body = J::parse_codepoints(points)?;
        require(
            crate::control_tools_policy_check::json::equal(
                &field(&event, "message")?,
                &expected_message,
            ),
            "runtime SDK report message differs from independent source",
        )?;
        let extra = field(&event, "extra")?;
        require(
            crate::control_tools_policy_check::json::equal(
                &field(&extra, "tombstone")?,
                &expected_body,
            ),
            "runtime SDK report body differs from independent source",
        )?;
        let filename = crate::python_fs::json_value(new.as_os_str());
        require(
            crate::control_tools_policy_check::json::equal(
                &field(&extra, "tombstone_fn")?,
                &filename,
            ),
            "raw tombstone basename was lost",
        )?;
        let mut reporting = "reporting new tombstone "
            .chars()
            .map(u32::from)
            .collect::<Vec<_>>();
        reporting.extend(crate::python_fs::codepoints(new.as_os_str()));
        let reporting = J::codepoints(reporting).ok_or("invalid raw path")?;
        require(
            records[0].iter().any(|row| {
                row.get("msg")
                    .is_some_and(|v| crate::control_tools_policy_check::json::equal(&v, &reporting))
            }),
            "missing raw-path reporting log",
        )?;
        let copies = fs::read_dir(root.join("logs/crash"))?.collect::<std::io::Result<Vec<_>>>()?;
        require(copies.len() == 1, "wrong crash copy count")?;
        let copy = copies[0].path();
        require(
            fs::read(&copy)? == BODY.as_bytes()
                && fs::metadata(&copy)?.permissions().mode() & 0o7777 == 0o640,
            "crash copy contents/mode differ",
        )?;
        require(
            !new.exists()
                && root.join("apport/invalid.crash").exists()
                && root.join("apport/tombstone-unknown").exists(),
            "enabled report/delete boundaries differ",
        )?;
        fs::copy(copy, output.join("copied.crash"))?;
        pretty(&output.join("sdk-event.json"), &event)?;
        number(delay)?
    } else {
        J::parse("null")?
    };
    require(
        daemon.wait(Duration::from_secs(15), &cancel.flag())? == 0,
        "actual tombstoned did not exit0",
    )?;
    require(
        root.join("apport/wrong-mode.crash").exists(),
        "wrong-mode crash unexpectedly removed",
    )?;
    if !request.enabled {
        require(
            !new.exists()
                && !root.join("apport/invalid.crash").exists()
                && !root.join("apport/tombstone-unknown").exists()
                && !root.join("logs/crash").exists(),
            "disabled scan did not delete only eligible new files",
        )?;
        receiver.require_empty().map_err(io_error)?;
    }
    for sub in &mut collector.subscribers {
        require(
            sub.receive(Duration::ZERO)?.is_none(),
            "duplicate subsequent-cycle publication",
        )?;
    }
    let shutdown = collector.stop(cancel)?;
    let mut disk = Vec::new();
    for entry in fs::read_dir(&collector.root)? {
        let entry = entry?;
        if entry.file_name().as_bytes().starts_with(b"swaglog.") {
            for line in fs::read_to_string(entry.path())?.lines() {
                disk.push(J::parse(line)?);
            }
        }
    }
    require(
        disk.len() == if request.enabled { 6 } else { 0 },
        "disk record count differs from original",
    )?;
    for row in &disk {
        require(
            field(&field(row, "ctx")?, "runtime_language")?.text_eq("rust"),
            "non-native log metadata",
        )?;
    }
    pretty(
        &output.join("records.json"),
        &object(&[
            (
                "wire",
                object(&[
                    ("logMessage", array(&records[0])?),
                    ("errorLogMessage", array(&records[1])?),
                ])?,
            ),
            ("disk", array(&disk)?),
        ])?,
    )?;
    let report = object(&[
        ("result", J::text("PASS")),
        ("reporting_enabled", boolean(request.enabled)),
        ("argv", argv(&command)?),
        ("first_scan_sleep_at", number(entered)?),
        ("new_crash_report_delay_seconds", delay),
        ("log_messages", number(records[0].len())?),
        ("error_messages", number(records[1].len())?),
        ("disk_records", number(disk.len())?),
        ("collector_shutdown_seconds", number(shutdown)?),
        ("subsequent_cycle_duplicates", number(0)?),
    ])?;
    pretty(&output.join("manifest.json"), &report)?;
    receiver.close().map_err(io_error)?;
    Ok(report)
}
fn startup(request: &Request) -> Result<J> {
    let root = path(&request.root);
    fixture(&path(&request.repository), &root, "fixture", false)?;
    let before = fs::read(root.join("apport/old.crash"))?;
    let mut rows = Vec::new();
    for (index, args) in [
        vec!["--cycles", "0"],
        vec!["--local-reporting-device", "tici"],
        vec!["--local-sentry-dsn", "https://fixture@example.invalid/1"],
        vec!["--cycles", "1"],
    ]
    .iter()
    .enumerate()
    {
        if index == 3 {
            write(&root.join("base/build.json"), "{", None)?;
        }
        let mut argv = vec![
            OsString::from_vec(request.daemon.clone()),
            "--apport-dir".into(),
            root.join("apport").into_os_string(),
            "--base-dir".into(),
            root.join("base").into_os_string(),
        ];
        argv.extend(args.iter().map(OsString::from));
        let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(5));
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        spec.inherit_stdin = true;
        let result = NativeRunner.run_in_current_group(&spec)?;
        require(
            result.code != 0 && fs::read(root.join("apport/old.crash"))? == before,
            "invalid startup touched original crash or exited0",
        )?;
        let mut fields = vec![
            ("argv", self::argv(&argv)?),
            ("exit_code", number(result.code)?),
        ];
        if index < 3 {
            fields.push(("stdout", J::text(std::str::from_utf8(&result.stdout)?)));
        }
        fields.push(("stderr", J::text(std::str::from_utf8(&result.stderr)?)));
        rows.push(object(&fields)?);
    }
    array(&rows)
}
pub(crate) fn source_main(root: &Path, binding: &Path, cancel: &Cancellation) -> Result<()> {
    digest(binding)?;
    let mut session = Session::new(&std::env::current_dir()?, &std::env::vars_os().collect())?;
    let configure = object(&[
        ("op", J::text("configure")),
        (
            "base",
            crate::python_fs::json_value(root.join("base").as_os_str()),
        ),
        (
            "params",
            crate::python_fs::json_value(root.join("params").as_os_str()),
        ),
        ("pc", boolean(false)),
        ("device", J::text("tici")),
    ])?;
    session.request_controlled(
        configure.to_json()?.as_bytes(),
        Instant::now() + Duration::from_secs(40),
        cancel.flag(),
    )?;
    let start = object(&[
        ("op", J::text("start")),
        (
            "apport",
            crate::python_fs::json_value(root.join("apport").as_os_str()),
        ),
        (
            "root",
            crate::python_fs::json_value(root.join("logs").as_os_str()),
        ),
        ("stamp", J::text(STAMP)),
    ])?;
    session.request_controlled(
        start.to_json()?.as_bytes(),
        Instant::now() + Duration::from_secs(40),
        cancel.flag(),
    )?;
    loop {
        fs::write(root.join("ready"), b"")?;
        std::thread::sleep(Duration::from_secs(5));
        cancel.check()?;
        session.request_controlled(
            b"{\"op\":\"cycle\"}",
            Instant::now() + Duration::from_secs(40),
            cancel.flag(),
        )?;
    }
}
pub(super) fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let cancel = Cancellation::new()?;
    let result = (|| {
        if args.len() == 3 && args[0] == "--source-main" {
            return source_main(Path::new(&args[2]), Path::new(&args[1]), &cancel);
        }
        require(
            args.len() == 2 && args[0] == "--request",
            "expected private --request FILE",
        )?;
        let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
        require(
            digest(&std::env::current_exe()?)? == request.worker_sha256,
            "live worker executable changed",
        )?;
        require(
            sources(&path(&request.repository))? == request.sources,
            "live worker source changed before execution",
        )?;
        require(
            std::env::var("OPENPILOT_PREFIX")? == request.prefix,
            "worker namespace mismatch",
        )?;
        let report = match request.kind.as_str() {
            "runtime" => runtime(&request, &cancel)?,
            "startup" => startup(&request)?,
            "shutdown-source" | "shutdown-native" => shutdown(&request, &cancel)?,
            _ => return Err("unknown live worker case".into()),
        };
        println!(
            "{}",
            object(&[("nonce", J::text(&request.nonce)), ("report", report)])?.to_json()?
        );
        Ok(())
    })();
    if let Some(signal) = cancel.signal() {
        return Err(Interrupted(signal).into());
    }
    result
}

fn backslash_text(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.push_str(text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                out.push_str(std::str::from_utf8(&rest[..valid]).expect("validated prefix"));
                rest = &rest[valid..];
                let count = error.error_len().unwrap_or(rest.len());
                for byte in &rest[..count] {
                    out.push_str(&format!("\\x{byte:02x}"));
                }
                rest = &rest[count..];
            }
        }
    }
    out
}
fn processes(token: &str) -> Result<Vec<J>> {
    let needle = format!("TOMBSTONE_SHUTDOWN_FIXTURE={token}");
    let mut rows = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|v| v.parse::<i32>().ok())
        else {
            continue;
        };
        let read = (|| -> std::io::Result<Option<J>> {
            let environment = fs::read(entry.path().join("environ"))?;
            if !environment
                .split(|b| *b == 0)
                .any(|v| v == needle.as_bytes())
            {
                return Ok(None);
            }
            let stat = fs::read_to_string(entry.path().join("stat"))?;
            let fields = stat
                .rsplit_once(") ")
                .ok_or_else(|| std::io::Error::other("invalid owned proc stat"))?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            let cmdline = fs::read(entry.path().join("cmdline"))?;
            let value = object(&[
                (
                    "pid",
                    number(pid).map_err(|error| std::io::Error::other(error.to_string()))?,
                ),
                (
                    "state",
                    J::text(
                        fields
                            .first()
                            .ok_or_else(|| std::io::Error::other("missing owned proc state"))?,
                    ),
                ),
                (
                    "ppid",
                    number(
                        fields
                            .get(1)
                            .ok_or_else(|| std::io::Error::other("missing owned ppid"))?,
                    )
                    .map_err(|error| std::io::Error::other(error.to_string()))?,
                ),
                (
                    "group",
                    number(
                        fields
                            .get(2)
                            .ok_or_else(|| std::io::Error::other("missing owned pgrp"))?,
                    )
                    .map_err(|error| std::io::Error::other(error.to_string()))?,
                ),
                (
                    "command",
                    J::text(&backslash_text(&cmdline).replace('\0', " ")),
                ),
            ])
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            Ok(Some(value))
        })();
        match read {
            Ok(Some(row)) => rows.push(row),
            Ok(None) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) => {}
            Err(error) if error.raw_os_error() == Some(3) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(rows)
}
struct RetainedProcesses(Vec<rustix::fd::OwnedFd>);
impl Drop for RetainedProcesses {
    fn drop(&mut self) {
        for identity in &self.0 {
            let _ = rustix::process::pidfd_send_signal(identity, Signal::KILL);
        }
    }
}
fn shutdown(request: &Request, cancel: &Cancellation) -> Result<J> {
    let root = path(&request.root);
    let output = path(&request.output);
    let repository = path(&request.repository);
    let source = request.kind == "shutdown-source";
    fixture(
        &repository,
        &root,
        if source { "fixture" } else { &request.prefix },
        true,
    )?;
    let mut receiver = Receiver::bind(Duration::from_secs(5)).map_err(io_error)?;
    let mut env = environment(&root);
    env.insert(
        "TOMBSTONE_SHUTDOWN_FIXTURE".into(),
        request.nonce.clone().into(),
    );
    let command = if source {
        vec![
            OsString::from_vec(request.worker.clone()),
            "--source-main".into(),
            OsString::from_vec(request.binding.clone()),
            root.as_os_str().to_owned(),
        ]
    } else {
        let mut args = request
            .runner
            .iter()
            .map(|v| OsString::from_vec(v.clone()))
            .collect::<Vec<_>>();
        args.extend([
            OsString::from_vec(request.daemon.clone()),
            "--base-dir".into(),
            root.join("base").into_os_string(),
            "--apport-dir".into(),
            root.join("apport").into_os_string(),
            "--log-root".into(),
            root.join("logs").into_os_string(),
            "--local-sentry-dsn".into(),
            receiver.dsn().into(),
            "--local-reporting-device".into(),
            "tici".into(),
        ]);
        args
    };
    let stderr = output.join("stderr.txt");
    let mut process = child(
        &command,
        &repository,
        &env,
        &output.join("stdout.txt"),
        &stderr,
        true,
    )?;
    let pid = process.reserved_pid()?.as_raw_pid();
    wait_until(&process, 12, cancel, || {
        Ok(if source {
            root.join("ready").exists()
        } else {
            fs::read_to_string(&stderr)?.contains("tombstoned: ready")
        })
    })?;
    write(&root.join("apport/new.crash"), BODY, Some(0o640))?;
    wait_until(&process, 12, cancel, || {
        Ok(root.join("descendant.pid").exists())
    })?;
    let before = processes(&request.nonce)?;
    require(
        before.len() >= 3,
        "held retrace did not acknowledge root/helper/descendant",
    )?;
    let mut retained = RetainedProcesses(Vec::new());
    for row in &before {
        let pid = field(row, "pid")?.to_json()?.parse::<i32>()?;
        if let Some(pid) = rustix::process::Pid::from_raw(pid) {
            if let Ok(identity) =
                rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())
            {
                // Bind the token AFTER opening the retained identity. A process
                // can exit/recycle its number between the observation and here.
                let tag = format!("TOMBSTONE_SHUTDOWN_FIXTURE={}", request.nonce);
                if fs::read(format!("/proc/{}/environ", pid.as_raw_pid()))
                    .is_ok_and(|env| env.split(|b| *b == 0).any(|v| v == tag.as_bytes()))
                {
                    retained.0.push(identity);
                }
            }
        }
    }
    receiver.require_empty().map_err(io_error)?;
    let signal = match request.signal {
        2 => Signal::INT,
        15 => Signal::TERM,
        _ => return Err("unsupported shutdown fixture signal".into()),
    };
    let begin = Instant::now();
    process.signal(signal)?;
    let result = process.wait(Duration::from_secs(5), &cancel.flag());
    let seconds = begin.elapsed().as_secs_f64();
    let (code, timed_out) = match result {
        Ok(code) => (number(code)?, false),
        Err(error) if error.to_string().contains("did not exit within") => {
            (J::parse("null")?, true)
        }
        Err(error) => return Err(error),
    };
    let after = processes(&request.nonce)?;
    let active = after
        .iter()
        .filter(|row| {
            !row.get("state").is_some_and(|v| v.text_eq("Z"))
                && !row
                    .get("pid")
                    .is_some_and(|v| v.to_json().is_ok_and(|v| v == pid.to_string()))
        })
        .cloned()
        .collect::<Vec<_>>();
    let http_events = match receiver.require_empty() {
        Ok(()) => 0,
        Err(_) => 1,
    };
    let report = object(&[
        ("kind", J::text(if source { "source" } else { "native" })),
        (
            "signal",
            J::text(if request.signal == 2 {
                "SIGINT"
            } else {
                "SIGTERM"
            }),
        ),
        ("argv", argv(&command)?),
        ("before", array(&before)?),
        ("after", array(&after)?),
        ("exited_within_5_seconds", boolean(!timed_out)),
        ("seconds", number(seconds)?),
        ("exit_code", code),
        ("active_descendants_after_stop", array(&active)?),
        ("http_events", number(http_events)?),
        (
            "crash_retained",
            boolean(root.join("apport/new.crash").exists()),
        ),
        ("crash_copied", boolean(root.join("logs/crash").exists())),
    ])?;
    pretty(&output.join("result.json"), &report)?;
    drop(process);
    drop(retained);
    receiver.close().map_err(io_error)?;
    Ok(report)
}
