//! CLI-private actual journald IPC owner. Prefixes change only between fully
//! closed peers; no subscriber or child from an earlier case remains alive.
use capnp::serialize::{read_message, write_message_to_words};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::{
    argparse_scan::{scan_os, OptionSpec},
    journald_check,
    path_resolve::resolve_non_strict,
    seal::{digest, Result},
};
use openpilot_log_format::{LogFiles, RotationSettings};
use openpilot_msgq::{Publisher, Subscriber};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
fn check(value: bool, message: impl Into<String>) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
fn monotonic() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
fn status(value: std::process::ExitStatus) -> i32 {
    value.code().unwrap_or_else(|| -value.signal().unwrap_or(1))
}
fn signal(pid: u32, value: rustix::process::Signal) -> Result<()> {
    let pid = rustix::process::Pid::from_raw(i32::try_from(pid)?).ok_or("invalid owned pid")?;
    match rustix::process::kill_process(pid, value) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn wait(child: &mut Child, timeout: Duration) -> Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            return Err("journald owned child deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn log_packet(topic: &str, text: &str) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut e = message.init_root::<event::Builder>();
    e.set_valid(true);
    e.set_log_mono_time(monotonic());
    if topic == "logMessage" {
        e.set_log_message(text)
    } else {
        e.set_error_log_message(text)
    }
    write_message_to_words(&message)
}
fn collector(endpoint: &str, root: &Path) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PULL)?;
    socket.set_rcvtimeo(100)?;
    socket.bind(endpoint)?;
    let mut publisher = Publisher::for_runtime("logMessage", 10 * 1024 * 1024)?;
    let mut errors = Publisher::for_runtime("errorLogMessage", 10 * 1024 * 1024)?;
    let mut files = LogFiles::new(&root.join("swaglog"), RotationSettings::default(), || {
        monotonic() as f64 / 1e9
    })?;
    while !stop.load(Ordering::Relaxed) {
        let parts = match socket.recv_multipart(0) {
            Ok(v) => v,
            Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => continue,
            Err(e) => return Err(e.into()),
        };
        let raw = parts.concat();
        let (&level, raw) = raw.split_first().ok_or("empty log payload")?;
        let record = String::from_utf8_lossy(raw);
        if level >= 20 {
            files.emit(&record)?
        }
        if record.chars().count() > 2 * 1024 * 1024 {
            continue;
        }
        publisher.send(&log_packet("logMessage", &record))?;
        if level >= 40 {
            errors.send(&log_packet("errorLogMessage", &record))?
        }
    }
    files.close()?;
    Ok(())
}
fn reference_packet(text: &str) -> Result<Vec<u8>> {
    let expected: Value = serde_json::from_str(&journald_check::entry_json(text)?)?;
    let row = &expected["androidLog"];
    let ts = row["ts"]
        .as_u64()
        .ok_or("source timestamp is outside uint64")?;
    let priority = u8::try_from(
        row["priority"]
            .as_u64()
            .ok_or("source priority is negative")?,
    )?;
    let pid = i32::try_from(row["pid"].as_i64().ok_or("source pid is outside int64")?)?;
    let tag = if row.get("tag").is_some() {
        Some(
            row["tag"]
                .as_str()
                .ok_or("source tag is not Unicode text")?,
        )
    } else {
        None
    };
    let mut message = capnp::message::Builder::new_default();
    let mut e = message.init_root::<event::Builder>();
    e.set_valid(false);
    e.set_log_mono_time(monotonic());
    let mut log = e.init_android_log();
    log.set_ts(ts);
    log.set_priority(priority);
    log.set_pid(pid);
    if let Some(tag) = tag {
        log.set_tag(tag)
    }
    log.set_message(
        row["message"]
            .as_str()
            .ok_or("source message is not text")?,
    );
    Ok(write_message_to_words(&message))
}
/// Independent source loop; only the mechanical transport/schema is shared.
/// No candidate journald decode/parse_line/packet code is linked as an oracle.
fn reference(root: &Path) -> Result<()> {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        fs::{fcntl_getfl, fcntl_setfl, OFlags},
    };
    let interrupted = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, interrupted.clone())?;
    // SIGTERM deliberately keeps original abrupt-parent/orphan behavior. Peer
    // owns and stops that observed child afterwards; nothing escapes cleanup.
    let mut publisher = Publisher::for_runtime("androidLog", 256000)?;
    let mut child = Command::new("journalctl")
        .args(["-f", "-o", "json"])
        .stdout(Stdio::piped())
        .spawn()?;
    let result = (|| -> Result<()> {
        let mut output = child.stdout.take().ok_or("journal stdout unavailable")?;
        fcntl_setfl(&output, fcntl_getfl(&output)? | OFlags::NONBLOCK)?;
        let context = zmq::Context::new();
        let logger = context.socket(zmq::PUSH)?;
        logger.set_linger(1000)?;
        let prefix = std::env::var("OPENPILOT_PREFIX")?;
        logger.connect(&format!("ipc:///tmp/logmessage{prefix}"))?;
        let mut pending = vec![];
        let mut eof = false;
        loop {
            if interrupted.load(Ordering::Relaxed) {
                return Ok(());
            }
            let text = std::str::from_utf8(&pending);
            if let Err(e) = text {
                if eof || e.error_len().is_some() {
                    return Err(e.into());
                }
            }
            let end = pending.iter().position(|b| matches!(b, b'\r' | b'\n'));
            let size = end.and_then(|i| {
                if pending[i] == b'\r' && i + 1 == pending.len() && !eof {
                    None
                } else {
                    Some(
                        i + 1
                            + usize::from(
                                pending[i] == b'\r' && pending.get(i + 1) == Some(&b'\n'),
                            ),
                    )
                }
            });
            let bytes = if let Some(size) = size {
                Some(pending.drain(..size).collect::<Vec<_>>())
            } else if eof {
                if pending.is_empty() {
                    return Ok(());
                }
                Some(std::mem::take(&mut pending))
            } else {
                None
            };
            let Some(bytes) = bytes else {
                let mut fds = [PollFd::new(&output, PollFlags::IN)];
                match poll(
                    &mut fds,
                    Some(&Timespec {
                        tv_sec: 0,
                        tv_nsec: 100_000_000,
                    }),
                ) {
                    Ok(0) | Err(rustix::io::Errno::INTR) => continue,
                    Err(e) => return Err(e.into()),
                    Ok(_) => {}
                }
                let mut chunk = [0; 8192];
                match output.read(&mut chunk) {
                    Ok(0) => eof = true,
                    Ok(n) => pending.extend_from_slice(&chunk[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                        ) => {}
                    Err(e) => return Err(e.into()),
                }
                continue;
            };
            let text = std::str::from_utf8(&bytes)?.trim();
            if text.is_empty() {
                continue;
            }
            match journald_check::entry_json(text) {
                Ok(_) => publisher.send(&reference_packet(text)?)?,
                Err(e) if e.kind == "JSONDecodeError" => {
                    let payload = json!({"msg":"failed to parse journalctl output","level":"ERROR","levelnum":40,"exc_info":e.to_string(),"process":std::process::id(),"filename":"journald.py","funcName":"main","pathname":root.join("openpilot/system/journald.py")});
                    let mut bytes = vec![40];
                    bytes.extend(serde_json::to_vec(&payload)?);
                    logger.send(bytes, 0)?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    })();
    if child.try_wait()?.is_none() {
        signal(child.id(), rustix::process::Signal::TERM)?
    }
    if wait(&mut child, Duration::from_secs(5)).is_err() {
        child.kill()?;
        child.wait()?;
        return Err("source journal child terminate deadline".into());
    }
    result?;
    if interrupted.load(Ordering::Relaxed) {
        signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGINT)?
    }
    Ok(())
}
fn project(wire: &[u8], topic: &str, sent: u64) -> Result<(Value, u64)> {
    let message = read_message(
        Cursor::new(wire),
        capnp::message::ReaderOptions {
            traversal_limit_in_words: None,
            nesting_limit: 65536,
        },
    )?;
    let e = message.get_root::<event::Reader<'_>>()?;
    let now = monotonic();
    let time = e.get_log_mono_time();
    check(
        sent.saturating_sub(1000) <= time && time <= now.saturating_add(1000),
        "journal monotonic timestamp outside send/receive interval",
    )?;
    let value = match (topic, e.which()?) {
        ("androidLog", event::AndroidLog(v)) => {
            let log = v?;
            let mut row = json!({"id":log.get_id(),"ts":log.get_ts(),"priority":log.get_priority(),"pid":log.get_pid(),"tid":log.get_tid(),"message":log.get_message()?.to_str()?});
            if log.has_tag() {
                row["tag"] = json!(log.get_tag()?.to_str()?)
            }
            json!({"valid":e.get_valid(),"androidLog":row})
        }
        ("logMessage", event::LogMessage(v)) => {
            json!({"valid":e.get_valid(),"logMessage":v?.to_str()?})
        }
        ("errorLogMessage", event::ErrorLogMessage(v)) => {
            json!({"valid":e.get_valid(),"errorLogMessage":v?.to_str()?})
        }
        _ => return Err("journal publication has incorrect union".into()),
    };
    Ok((value, time))
}
struct Peer {
    output: PathBuf,
    prefix: String,
    shm: PathBuf,
    endpoint: PathBuf,
    log_root: PathBuf,
    process: Option<Child>,
    collector: Option<Child>,
    child_pid: Option<u32>,
    child_pidfd: Option<rustix::fd::OwnedFd>,
    subscribers: BTreeMap<String, Subscriber>,
    records: BTreeMap<String, Vec<Value>>,
    sent: u64,
    closed: bool,
    owns_shm: bool,
    owns_endpoint: bool,
}
impl Peer {
    fn new(root: &Path, output: &Path, binary: Option<&Path>, fixture: &Path) -> Result<Self> {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(output)?;
        // Paths may contain dots, Unicode or arbitrary Unix bytes. Namespace
        // identity uses their exact bytes, while the component remains ASCII.
        let prefix = format!(
            "journal-native-{}-{}",
            std::process::id(),
            &digest_string(output.as_os_str().as_encoded_bytes())[..20]
        );
        let mut peer = Self {
            output: output.into(),
            shm: Path::new("/dev/shm").join(format!("msgq_{prefix}")),
            endpoint: PathBuf::from(format!("/tmp/logmessage{prefix}")),
            prefix,
            log_root: output.join("logs"),
            process: None,
            collector: None,
            child_pid: None,
            child_pidfd: None,
            subscribers: BTreeMap::new(),
            records: ["androidLog", "logMessage", "errorLogMessage"]
                .map(|s| (s.into(), vec![]))
                .into(),
            sent: monotonic(),
            closed: false,
            owns_shm: false,
            owns_endpoint: false,
        };
        fs::create_dir(&peer.shm)?;
        peer.owns_shm = true;
        fs::create_dir_all(&peer.log_root)?;
        let bin = output.join("bin");
        fs::create_dir(&bin)?;
        symlink(fixture, bin.join("journalctl"))?;
        let mut path = bin.into_os_string();
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        std::env::set_var("OPENPILOT_PREFIX", &peer.prefix);
        let spawn = |name: &str, argv: &[std::ffi::OsString], piped: bool| -> Result<Child> {
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..])
                .current_dir(root)
                .env("OPENPILOT_PREFIX", &peer.prefix)
                .env("HOME", output.join("home"))
                .env("PATH", &path)
                .env("JOURNAL_FIXTURE_TRACE", output.join("child.jsonl"))
                .stdout(File::create(output.join(format!("{name}.stdout")))?)
                .stderr(File::create(output.join(format!("{name}.stderr")))?);
            if piped {
                cmd.stdin(Stdio::piped());
            }
            Ok(cmd.spawn()?)
        };
        let this = std::env::current_exe()?;
        let collector_argv = vec![
            this.clone().into_os_string(),
            "--collector".into(),
            peer.endpoint.clone().into_os_string(),
            peer.log_root.clone().into_os_string(),
        ];
        match fs::symlink_metadata(&peer.endpoint) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => return Err("journal endpoint already exists".into()),
        }
        peer.collector = Some(spawn("collector", &collector_argv, false)?);
        peer.owns_endpoint = true;
        peer.wait_ready(|| {
            Ok(peer.shm.join("logMessage").exists() && peer.shm.join("errorLogMessage").exists())
        })?;
        let argv = if let Some(binary) = binary {
            vec![binary.as_os_str().to_owned()]
        } else {
            vec![
                this.into_os_string(),
                "--reference-daemon".into(),
                root.as_os_str().to_owned(),
            ]
        };
        peer.process = Some(spawn("journal", &argv, true)?);
        peer.wait_ready(|| {
            Ok(peer.shm.join("androidLog").exists()
                && fs::read(output.join("child.jsonl")).is_ok_and(|v| v.ends_with(b"\n")))
        })?;
        let first = fs::read_to_string(output.join("child.jsonl"))?;
        let first: Value =
            serde_json::from_str(first.lines().next().ok_or("empty journal trace")?)?;
        let child_pid = u32::try_from(first["pid"].as_u64().ok_or("missing child pid")?)?;
        check(
            first["event"] == "started" && first["ppid"] == peer.process.as_ref().unwrap().id(),
            "journal child parentage differs",
        )?;
        peer.pin_child(child_pid)?;
        for (topic, capacity) in [
            ("androidLog", 256000),
            ("logMessage", 10 * 1024 * 1024),
            ("errorLogMessage", 10 * 1024 * 1024),
        ] {
            let mut sub = Subscriber::for_runtime(topic, false, capacity)?;
            check(
                sub.receive(Duration::ZERO)?.is_none(),
                "unexpected initial journal packet",
            )?;
            peer.subscribers.insert(topic.into(), sub);
        }
        let argv_json = |args: &[std::ffi::OsString]| -> Result<String> {
            Ok(format!(
                "[{}]",
                args.iter()
                    .map(|s| openpilot_dev_tools::python_fs::json_string(s))
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            ))
        };
        let path_json = |path: &Path| openpilot_dev_tools::python_fs::json_string(path.as_os_str());
        let binary_hash = binary
            .map(digest)
            .transpose()?
            .map(|v| format!("\"{v}\""))
            .unwrap_or_else(|| "null".into());
        fs::write(output.join("invocation.json"),format!(
            "{{\"argv\":{},\"collector\":{},\"environment\":{{\"OPENPILOT_PREFIX\":{},\"HOME\":{},\"PATH\":{},\"JOURNAL_FIXTURE_TRACE\":{}}},\"source_sha256\":\"{}\",\"binary_sha256\":{},\"fixture_sha256\":\"{}\",\"checker_sha256\":\"{}\"}}\n",
            argv_json(&argv)?,argv_json(&collector_argv)?,serde_json::to_string(&peer.prefix)?,
            path_json(&output.join("home"))?,openpilot_dev_tools::python_fs::json_string(&path)?,
            path_json(&output.join("child.jsonl"))?,digest(&root.join("openpilot/system/journald.py"))?,
            binary_hash,digest(fixture)?,digest(&std::env::current_exe()?)?
        ))?;
        Ok(peer)
    }
    fn wait_ready(&self, mut predicate: impl FnMut() -> Result<bool>) -> Result<()> {
        let start = Instant::now();
        while !predicate()? {
            for (name, child) in [
                ("collector", self.collector.as_ref()),
                ("journal", self.process.as_ref()),
            ] {
                let Some(child) = child else { continue };
                let pid = rustix::process::Pid::from_raw(i32::try_from(child.id())?)
                    .ok_or("invalid owned leader")?;
                let fd = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())?;
                let mut fds = [rustix::event::PollFd::new(
                    &fd,
                    rustix::event::PollFlags::IN,
                )];
                if rustix::event::poll(
                    &mut fds,
                    Some(&rustix::event::Timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    }),
                )? > 0
                {
                    let diagnostic =
                        fs::read(self.output.join(format!("{name}.stderr"))).unwrap_or_default();
                    return Err(format!(
                        "journal {name} exited before readiness: {}",
                        String::from_utf8_lossy(&diagnostic)
                    )
                    .into());
                }
            }
            check(
                start.elapsed() < Duration::from_secs(15),
                format!("journal readiness timeout: {}", self.output.display()),
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    fn command(&mut self, value: &Value) -> Result<()> {
        let input = self
            .process
            .as_mut()
            .ok_or("journal process absent")?
            .stdin
            .as_mut()
            .ok_or("journal stdin absent")?;
        writeln!(input, "{value}")?;
        input.flush()?;
        Ok(())
    }
    fn send(&mut self, payload: &[u8]) -> Result<()> {
        use base64::Engine;
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.output.join("input.bin"))?
            .write_all(payload)?;
        self.sent = monotonic();
        self.command(&json!({"op":"write","base64":base64::engine::general_purpose::STANDARD.encode(payload)}))
    }
    fn receive(&mut self, topic: &str) -> Result<Value> {
        let wire = self
            .subscribers
            .get_mut(topic)
            .ok_or("unknown topic")?
            .receive(Duration::from_secs(5))?
            .ok_or_else(|| format!("missing {topic}: {}", self.output.display()))?;
        let (value, time) = project(&wire, topic, self.sent)?;
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.output.join(format!("{topic}.bin")))?
            .write_all(&wire)?;
        writeln!(
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.output.join("timestamps.jsonl"))?,
            "{}",
            json!({"topic":topic,"sent_ns":self.sent,"event_ns":time,"received_ns":monotonic()})
        )?;
        self.records.get_mut(topic).unwrap().push(value.clone());
        Ok(value)
    }
    fn finish(
        &mut self,
        command: Option<Value>,
        signum: Option<rustix::process::Signal>,
        orphan: bool,
    ) -> Result<Value> {
        let start = Instant::now();
        if let Some(command) = command {
            self.command(&command)?
        }
        let child = self.process.as_mut().ok_or("journal process absent")?;
        if let Some(signum) = signum {
            signal(child.id(), signum)?
        }
        let exit = wait(child, Duration::from_secs(5));
        if exit.is_err() {
            self.timeout_snapshot()?
        }
        let exit = status(exit?);
        let alive = !self.child_exited()?;
        let trace = fs::read_to_string(self.output.join("child.jsonl"))?
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        check(
            alive == orphan,
            format!("unexpected journal child survival {alive}/{orphan}: {trace:?}"),
        )?;
        let value = json!({"exit":exit,"elapsed":start.elapsed().as_secs_f64(),"child_survived":alive,"trace":trace});
        pretty(&self.output.join("exit.json"), &value)?;
        Ok(value)
    }
    fn timeout_snapshot(&self) -> Result<()> {
        for (label, pid) in [
            ("journal", self.process.as_ref().map(Child::id)),
            ("child", self.child_pid),
        ] {
            let Some(pid) = pid else { continue };
            for name in ["status", "wchan", "syscall"] {
                let text = fs::read_to_string(format!("/proc/{pid}/{name}"))
                    .unwrap_or_else(|e| e.to_string());
                fs::write(
                    self.output.join(format!("timeout-{label}-{name}.txt")),
                    text,
                )?;
            }
            let fd = PathBuf::from(format!("/proc/{pid}/fd"));
            let mut descriptors = BTreeMap::new();
            if let Ok(entries) = fs::read_dir(fd) {
                for e in entries.flatten() {
                    descriptors.insert(
                        e.file_name().to_string_lossy().into_owned(),
                        fs::read_link(e.path())
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|e| e.to_string()),
                    );
                }
            }
            pretty(
                &self.output.join(format!("timeout-{label}-fds.json")),
                &json!(descriptors),
            )?;
        }
        Ok(())
    }
    fn pin_child(&mut self, pid: u32) -> Result<()> {
        let process = self.process.as_ref().ok_or("journal leader missing")?;
        let pid_value = rustix::process::Pid::from_raw(i32::try_from(pid)?)
            .ok_or("invalid journal child pid")?;
        let pidfd = rustix::process::pidfd_open(pid_value, rustix::process::PidfdFlags::empty())?;
        // Pin first, then verify parentage while the leader remains owned and
        // unreaped. A later numeric PID reuse cannot redirect this descriptor.
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
        let tail = stat.rsplit_once(')').ok_or("invalid journal child stat")?.1;
        let ppid = tail
            .split_whitespace()
            .nth(1)
            .ok_or("missing journal child parent")?
            .parse::<u32>()?;
        check(
            ppid == process.id(),
            "journal child no longer belongs to owned leader",
        )?;
        self.child_pid = Some(pid);
        self.child_pidfd = Some(pidfd);
        Ok(())
    }
    fn child_exited(&self) -> Result<bool> {
        use rustix::event::{poll, PollFd, PollFlags, Timespec};
        let Some(fd) = &self.child_pidfd else {
            return Ok(true);
        };
        let mut fds = [PollFd::new(fd, PollFlags::IN)];
        Ok(poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )? > 0)
    }
    fn stop_pinned_child(&self) -> Result<()> {
        use rustix::event::{poll, PollFd, PollFlags, Timespec};
        let Some(fd) = &self.child_pidfd else {
            return Ok(());
        };
        if self.child_exited()? {
            return Ok(());
        }
        match rustix::process::pidfd_send_signal(fd, rustix::process::Signal::TERM) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.into()),
        }
        let mut fds = [PollFd::new(fd, PollFlags::IN)];
        if poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 5,
                tv_nsec: 0,
            }),
        )? == 0
        {
            rustix::process::pidfd_send_signal(fd, rustix::process::Signal::KILL)?;
            check(
                poll(
                    &mut fds,
                    Some(&Timespec {
                        tv_sec: 5,
                        tv_nsec: 0,
                    }),
                )? > 0,
                "journal descendant kill deadline",
            )?;
            return Err("journal descendant termination deadline".into());
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        // Constructor failures may have a startup record before normal pidfd
        // assignment. Acquire it while the daemon leader is still unreaped.
        if self.child_pidfd.is_none() {
            if let Ok(trace) = fs::read_to_string(self.output.join("child.jsonl")) {
                if let Some(Ok(started)) = trace.lines().next().map(serde_json::from_str::<Value>) {
                    if self
                        .process
                        .as_ref()
                        .is_some_and(|p| started["ppid"] == p.id())
                    {
                        if let Some(pid) =
                            started["pid"].as_u64().and_then(|v| u32::try_from(v).ok())
                        {
                            // Already-exited children cannot be signaled and
                            // need no descendant cleanup; never fall back to PID.
                            let _ = self.pin_child(pid);
                        }
                    }
                }
            }
        }
        if let Some(child) = &mut self.process {
            if child.try_wait()?.is_none() {
                child.kill()?;
                wait(child, Duration::from_secs(5))?;
            }
            child.stdin.take();
        }
        let descendant = self.stop_pinned_child();
        if let Some(collector) = &mut self.collector {
            if collector.try_wait()?.is_none() {
                signal(collector.id(), rustix::process::Signal::INT)?;
                if wait(collector, Duration::from_secs(5)).is_err() {
                    collector.kill()?;
                    collector.wait()?;
                }
            }
        }
        pretty(&self.output.join("records.json"), &json!(self.records))?;
        self.subscribers.clear();
        if self.owns_shm {
            match fs::remove_dir_all(&self.shm) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        if self.owns_endpoint {
            match fs::remove_file(&self.endpoint) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.closed = true;
        descendant
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            let _ = fs::write(self.output.join("cleanup-error.txt"), e.to_string());
        }
    }
}
fn digest_string(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn expected(text: &str) -> Result<Value> {
    Ok(serde_json::from_str(&journald_check::entry_json(text)?)?)
}
fn no_packet(peer: &mut Peer, topic: &str) -> Result<()> {
    check(
        peer.subscribers
            .get_mut(topic)
            .ok_or("unknown topic")?
            .receive(Duration::ZERO)?
            .is_none(),
        format!("extra journal {topic}"),
    )
}
fn source_commit(root: &Path) -> Result<String> {
    let value = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()?;
    check(value.status.success(), "git identity failed")?;
    Ok(String::from_utf8(value.stdout)?.trim().into())
}
fn normal(root: &Path, binary: Option<&Path>, output: &Path, fixture: &Path) -> Result<Value> {
    let mut peer = Peer::new(root, output, binary, fixture)?;
    let result = (|| -> Result<Value> {
        for text in journald_check::valid_records()? {
            peer.send(format!("  {text} \n").as_bytes())?;
            check(
                peer.receive("androidLog")? == expected(&text)?,
                "journal entry differs",
            )?;
        }
        peer.send(b"\r\n \t\r{}\r{}\r\n{\"MESSAGE\":\"\xed\x95")?;
        check(
            peer.receive("androidLog")? == expected("{}")?,
            "first CR entry",
        )?;
        check(
            peer.receive("androidLog")? == expected("{}")?,
            "second CR entry",
        )?;
        peer.send(b"\x9c\"}\n")?;
        check(
            peer.receive("androidLog")? == expected("{\"MESSAGE\":\"한\"}")?,
            "fragmented UTF8 entry",
        )?;
        let commit = if binary.is_some() {
            source_commit(root)?
        } else {
            String::new()
        };
        for mut payload in journald_check::malformed_records() {
            payload.extend_from_slice(b"{}\n");
            peer.send(&payload)?;
            check(
                peer.receive("androidLog")? == expected("{}")?,
                "parse recovery entry",
            )?;
            let log = peer.receive("logMessage")?;
            let error = peer.receive("errorLogMessage")?;
            let record: Value =
                serde_json::from_str(log["logMessage"].as_str().ok_or("missing log text")?)?;
            let other: Value = serde_json::from_str(
                error["errorLogMessage"]
                    .as_str()
                    .ok_or("missing error text")?,
            )?;
            check(record == other, "recoverable error publications differ")?;
            check(
                record["msg"] == "failed to parse journalctl output"
                    && record["level"] == "ERROR"
                    && record["levelnum"] == 40
                    && record["exc_info"].as_str().is_some_and(|s| !s.is_empty()),
                "incorrect recoverable error record",
            )?;
            check(
                record["process"] == peer.process.as_ref().unwrap().id(),
                "incorrect error process",
            )?;
            if binary.is_none() {
                check(
                    record["filename"] == "journald.py" && record["funcName"] == "main",
                    "reference error site differs",
                )?
            } else {
                check(
                    record["filename"]
                        .as_str()
                        .is_some_and(|s| s.ends_with(".rs"))
                        && record["module"]
                            .as_str()
                            .is_some_and(|s| s.contains("journald")),
                    "native error site differs",
                )?;
                check(
                    record["ctx"]["daemon"] == "journald"
                        && record["ctx"]["runtime_language"] == "rust"
                        && record["ctx"]["source_commit"] == commit,
                    "native error context differs",
                )?;
                let path = PathBuf::from(
                    record["pathname"]
                        .as_str()
                        .ok_or("native pathname missing")?,
                );
                let path = if path.is_absolute() {
                    path
                } else {
                    root.join("rust").join(path)
                };
                let source = fs::read_to_string(path)?;
                let line = record["lineno"].as_u64().ok_or("native line missing")? as usize;
                check(
                    source
                        .lines()
                        .nth(line.checked_sub(1).ok_or("zero native line")?)
                        .is_some_and(|s| s.contains("log_site!")),
                    "native log line no longer source site",
                )?;
            }
        }
        peer.send(b"{\"MESSAGE\":\"last without newline\"}")?;
        peer.command(&json!({"op":"close"}))?;
        check(
            peer.receive("androidLog")? == expected("{\"MESSAGE\":\"last without newline\"}")?,
            "last partial line",
        )?;
        let outcome = peer.finish(None, None, false)?;
        check(
            outcome["exit"] == 0
                && outcome["trace"]
                    .as_array()
                    .and_then(|v| v.last())
                    .is_some_and(|v| v["event"] == "signal-15"),
            "normal exit/child termination differs",
        )?;
        no_packet(&mut peer, "androidLog")?;
        // Stop collector before inspecting its buffered disk output, after all
        // publication acknowledgements establish it has processed every record.
        if let Some(collector) = &mut peer.collector {
            signal(collector.id(), rustix::process::Signal::INT)?;
            wait(collector, Duration::from_secs(5))?;
        }
        let mut count = 0;
        for p in fs::read_dir(&peer.log_root)? {
            let path = p?.path();
            if !path
                .file_name()
                .is_some_and(|n| n.as_encoded_bytes().starts_with(b"swaglog."))
            {
                continue;
            }
            for row in fs::read_to_string(path)?.lines() {
                let row: Value = serde_json::from_str(row)?;
                check(
                    row["levelnum"] == 40 && row["msg$s"] == "failed to parse journalctl output",
                    "disk recoverable record differs",
                )?;
                count += 1;
            }
        }
        check(
            count == peer.records["errorLogMessage"].len(),
            "disk error record count differs",
        )?;
        let mut value = outcome;
        value["messages"] = json!(peer.records["androidLog"].len());
        value["errors"] = json!(peer.records["errorLogMessage"].len());
        value["disk_records"] = json!(count);
        Ok(value)
    })();
    let cleanup = peer.close();
    cleanup?;
    result
}
fn fatal(
    root: &Path,
    binary: Option<&Path>,
    output: &Path,
    fixture: &Path,
    text: &[u8],
) -> Result<Value> {
    let mut peer = Peer::new(root, output, binary, fixture)?;
    let result = (|| -> Result<Value> {
        let mut payload = text.to_vec();
        payload.extend_from_slice(b"\n{}\n");
        peer.send(&payload)?;
        let outcome = peer.finish(None, None, false)?;
        check(
            outcome["exit"] != 0
                && outcome["trace"]
                    .as_array()
                    .and_then(|v| v.last())
                    .is_some_and(|v| v["event"] == "signal-15"),
            "fatal exit or child cleanup differs",
        )?;
        no_packet(&mut peer, "androidLog")?;
        no_packet(&mut peer, "errorLogMessage")?;
        check(
            !fs::read(output.join("journal.stderr"))?.is_empty(),
            "fatal diagnostic absent",
        )?;
        Ok(outcome)
    })();
    let cleanup = peer.close();
    cleanup?;
    result
}
fn lifecycle(root: &Path, binary: Option<&Path>, output: &Path, fixture: &Path) -> Result<Value> {
    let mut results = serde_json::Map::new();
    for (name, signum, command) in [
        ("sigint", Some(rustix::process::Signal::INT), None),
        ("sigterm", Some(rustix::process::Signal::TERM), None),
        ("child-exit", None, Some(json!({"op":"exit","status":17}))),
        ("empty-eof", None, Some(json!({"op":"close"}))),
    ] {
        let mut peer = Peer::new(root, &output.join(name), binary, fixture)?;
        let orphan = binary.is_none() && signum == Some(rustix::process::Signal::TERM);
        let result = peer.finish(command.clone(), signum, orphan);
        if let Ok(value) = &result {
            if command.is_some() || binary.is_some() {
                check(value["exit"] == 0, "lifecycle exit differs")?
            }
        }
        peer.close()?;
        results.insert(name.into(), result?);
    }
    Ok(Value::Object(results))
}
const ORIGINAL_SOURCE_PINS: [(&str, &str); 10] = [
    (
        "rust/tools/check_journald.py",
        "010e2c863f9d1ee69903a118eb96b07ad0c9bc286385bcde8af2c5f522ca019f",
    ),
    (
        "rust/tools/journald_peer.py",
        "ca6a07a8e559d4abc643df85e51b7849595d1747ef42cfe35bf32c8d44399104",
    ),
    (
        "rust/tools/journalctl_fixture.py",
        "d735cdb55a379e1ec175b6f515a9855c757743b41652fa93e14632de3c858768",
    ),
    (
        "rust/tools/journal_fixture_input.py",
        "bc1091d07c967c13bbf7d9cc9b8ec104ee5f09365d241ed9f38ff199929ac01e",
    ),
    (
        "rust/tools/logmessaged_reference.py",
        "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
    ),
    (
        "rust/tools/tests/test_journal_fixture.py",
        "46114b442a90f5f4ac23130a935beb322218d6379974f2abfb22ebe7a492f1f6",
    ),
    (
        "openpilot/system/journald.py",
        "29ce87e32684f6ae635a24ff5877f7e8b406353cf221080eb8aa163a49f0a396",
    ),
    (
        "openpilot/system/logmessaged.py",
        "93d94801185350ef4470bd482d37ed7f6d466e9b100ad10aa16fa6e8570fb02a",
    ),
    (
        "openpilot/common/logging_extra.py",
        "ce9b3b3d8498d76e1c4c0352a3e98e72759bb7f367ff8629c1329c46040595c3",
    ),
    (
        "openpilot/common/swaglog.py",
        "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
    ),
];

const RETIRED_TOOL_SOURCES: [&str; 5] = [
    "rust/tools/check_journald.py",
    "rust/tools/journald_peer.py",
    "rust/tools/journal_fixture_input.py",
    "rust/tools/journalctl_fixture.py",
    "rust/tools/tests/test_journal_fixture.py",
];
fn source_identities(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut records = BTreeMap::new();
    for (path, pin) in ORIGINAL_SOURCE_PINS {
        let source = root.join(path);
        match fs::symlink_metadata(&source) {
            Ok(_) => {
                check(
                    digest(&source)? == pin,
                    format!("journald source drift: {path}"),
                )?;
                records.insert(path.to_owned(), pin.to_owned());
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && RETIRED_TOOL_SOURCES.contains(&path) =>
            {
                records.insert(path.to_owned(), format!("retired:{pin}"));
            }
            Err(e) => return Err(e.into()),
        }
    }
    for path in [
        "rust/crates/dev-tools/src/journald_check.rs",
        "rust/crates/dev-tools/src/journald_fixture.rs",
        "rust/crates/dev-tools/src/journald_native.rs",
        "rust/crates/dev-tools/src/bin/check_journald.rs",
        "rust/crates/dev-tools/src/bin/journalctl_fixture.rs",
    ] {
        records.insert(path.into(), digest(&root.join(path))?);
    }
    Ok(records)
}
pub fn cli() -> Result<()> {
    let argv = std::env::args_os().skip(1).collect::<Vec<_>>();
    if argv.first().is_some_and(|s| s == "--collector") {
        check(argv.len() == 3, "collector expects endpoint/root")?;
        return collector(
            &format!("ipc://{}", Path::new(&argv[1]).display()),
            Path::new(&argv[2]),
        );
    }
    if argv.first().is_some_and(|s| s == "--reference-daemon") {
        check(argv.len() == 2, "reference expects source root")?;
        return reference(Path::new(&argv[1]));
    }
    let args = scan_os(
        &argv,
        &[
            OptionSpec::value("--binary", false),
            OptionSpec::value("--output", true),
            OptionSpec::value("--fixture", false),
        ],
    )?;
    if args.help {
        println!("usage: check-journald [--binary BINARY] --output OUTPUT [--fixture JOURNALCTL_FIXTURE]");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = resolve_non_strict(&root)?;
    let binary = args
        .values
        .get("--binary")
        .map(|v| resolve_non_strict(Path::new(v)))
        .transpose()?;
    let output = resolve_non_strict(Path::new(&args.values["--output"]))?;
    let fixture = args
        .values
        .get("--fixture")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name("journalctl-fixture"));
    let fixture = resolve_non_strict(&fixture)?;
    fs::create_dir_all(&output)?;
    let inputs = source_identities(&root)?;
    pretty(
        &output.join("inputs.json"),
        &json!({"sources":inputs,"checker_sha256":digest(&std::env::current_exe()?)?,"fixture_sha256":digest(&fixture)?,"candidate_sha256":binary.as_deref().map(digest).transpose()?}),
    )?;
    let mut results = serde_json::Map::new();
    let mut sides = vec![("original", None)];
    if let Some(binary) = binary.as_deref() {
        sides.push(("rust", Some(binary)))
    }
    for (side, binary) in sides {
        let destination = output.join(side);
        let mut records = serde_json::Map::new();
        records.insert(
            "normal".into(),
            normal(&root, binary, &destination.join("normal"), &fixture)?,
        );
        for (i, text) in journald_check::fatal_records().iter().enumerate() {
            records.insert(
                format!("fatal-{i}"),
                fatal(
                    &root,
                    binary,
                    &destination.join(format!("fatal-{i}")),
                    &fixture,
                    text,
                )?,
            );
        }
        records.insert(
            "lifecycle".into(),
            lifecycle(&root, binary, &destination.join("lifecycle"), &fixture)?,
        );
        results.insert(side.into(), Value::Object(records));
        pretty(&output.join("report.json"), &Value::Object(results.clone()))?;
    }
    if binary.is_some() {
        check(
            results["original"]["normal"]["messages"] == results["rust"]["normal"]["messages"],
            "source/candidate journal counts differ",
        )?
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(output: &Path) -> Peer {
        fs::create_dir_all(output).unwrap();
        Peer {
            output: output.into(),
            prefix: "owned-journal-test".into(),
            shm: output.join("namespace"),
            endpoint: output.join("endpoint"),
            log_root: output.join("logs"),
            process: None,
            collector: None,
            child_pid: None,
            child_pidfd: None,
            subscribers: BTreeMap::new(),
            records: BTreeMap::new(),
            sent: monotonic(),
            closed: false,
            owns_shm: false,
            owns_endpoint: false,
        }
    }
    #[test]
    fn exited_collector_is_detected_without_reaping_it_in_readiness() {
        let temp = tempfile::tempdir().unwrap();
        let mut peer = owner(temp.path());
        fs::write(
            temp.path().join("collector.stderr"),
            "owned collector failure",
        )
        .unwrap();
        peer.collector = Some(Command::new("/bin/false").spawn().unwrap());
        let error = peer.wait_ready(|| Ok(false)).unwrap_err().to_string();
        check(
            error.contains("collector exited before readiness: owned collector failure"),
            error,
        )
        .unwrap();
        // The owner still reaps this exact Child. Readiness only polls pidfd.
        assert_eq!(
            peer.collector.as_mut().unwrap().wait().unwrap().code(),
            Some(1)
        );
        peer.close().unwrap();
    }
    #[test]
    fn descendant_pidfd_cleanup_never_signals_a_recycled_numeric_identity() {
        let temp = tempfile::tempdir().unwrap();
        let mut peer = owner(temp.path());
        let mut original = Command::new("/bin/true").spawn().unwrap();
        let pid = rustix::process::Pid::from_raw(i32::try_from(original.id()).unwrap()).unwrap();
        let pinned =
            rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).unwrap();
        original.wait().unwrap();
        let mut unrelated = Command::new("/bin/sleep").arg("60").spawn().unwrap();
        // Model numeric PID reuse without exhausting host PIDs. The pinned
        // descriptor is the original exited child; the number now names a
        // different test-owned process and must never be used for cleanup.
        peer.child_pid = Some(unrelated.id());
        peer.child_pidfd = Some(pinned);
        peer.close().unwrap();
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
    }
    #[test]
    fn failed_constructor_does_not_remove_unowned_namespace_or_endpoint() {
        let temp = tempfile::tempdir().unwrap();
        let mut peer = owner(temp.path());
        fs::create_dir(&peer.shm).unwrap();
        fs::write(peer.shm.join("marker"), "other namespace").unwrap();
        fs::write(&peer.endpoint, "other endpoint").unwrap();
        peer.close().unwrap();
        assert_eq!(
            fs::read(peer.shm.join("marker")).unwrap(),
            b"other namespace"
        );
        assert_eq!(fs::read(&peer.endpoint).unwrap(), b"other endpoint");
    }
    #[test]
    fn owned_descendant_is_terminated_and_observed_exited_before_ipc_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let mut peer = owner(temp.path());
        let pid_file = temp.path().join("descendant-pid");
        peer.process = Some(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 60 & child=$!; printf '%s' \"$child\" > \"$1\"; wait")
                .arg("journal-owned-peer")
                .arg(&pid_file)
                .spawn()
                .unwrap(),
        );
        let started = Instant::now();
        while !fs::read_to_string(&pid_file).is_ok_and(|s| !s.is_empty()) {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        peer.pin_child(fs::read_to_string(pid_file).unwrap().parse().unwrap())
            .unwrap();
        fs::create_dir(&peer.shm).unwrap();
        fs::write(&peer.endpoint, "owned").unwrap();
        peer.owns_shm = true;
        peer.owns_endpoint = true;
        peer.close().unwrap();
        assert!(peer.child_exited().unwrap());
        assert!(!peer.shm.exists());
        assert!(!peer.endpoint.exists());
    }
}
