//! Isolated-process tests for the Python-compatible process-retained producer.
//! The parent collector binds only after emit returned / all worker threads exited.
use openpilot_logging::{
    log_site,
    producer::{close_runtime, emit_runtime, Delivery, Logger, RuntimeLogScope},
    record::{Level, Record},
    Error,
};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn fields(values: &[(&str, &str)]) -> openpilot_logging::Fields {
    values
        .iter()
        .map(|(key, value)| {
            (
                (*key).into(),
                openpilot_logging::Value::Text((*value).into()),
            )
        })
        .collect()
}
fn record(text: &str) -> Record {
    Record::text(Level::Info, text.into())
}
fn emit_facade(logger: &mut Logger, text: &str) {
    assert_eq!(
        logger.emit(log_site!(), record(text)).unwrap(),
        Delivery::Sent
    );
}
fn closed(logger: &mut Logger) {
    assert!(matches!(
        logger.emit(log_site!(), record("must-not-send")),
        Err(Error::Transport(zmq::Error::ENOTSOCK))
    ));
    assert_eq!(
        logger
            .emit(log_site!(), Record::text(Level::NotSet, "filtered".into()))
            .unwrap(),
        Delivery::Filtered
    );
}
fn ready(text: &str) {
    println!("{text}");
    std::io::stdout().flush().unwrap();
}
fn acknowledge() {
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
    assert_eq!(byte, [b'!']);
}
#[expect(
    unsafe_code,
    reason = "test-only controlled fork with one Rust application thread and no held logging locks; matches existing logging_fork fixture"
)]
fn fork_worker(logger: &mut Logger, mode: &str) {
    // SAFETY: one application thread, completed emissions and no held Rust
    // logging locks. Only owned copied libzmq handles may be forgotten in child.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
        if mode != "fork-drop" {
            emit_facade(logger, "child-record");
            ready("CHILD");
            acknowledge();
        }
        logger.close();
        logger.close();
        closed(logger);
        // Facade close is local: child can still create a different facade.
        if mode != "fork-drop" {
            let mut other = Logger::for_runtime_retained().unwrap();
            emit_facade(&mut other, "child-other");
            ready("CHILD-OTHER");
            acknowledge();
        }
        close_runtime().unwrap();
        // SAFETY: owned fork child; no inherited Rust destructors on _exit.
        unsafe { libc::_exit(0) };
    }
    let mut status = 0;
    // SAFETY: positive owned, unreaped direct child.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}
fn worker(mode: &str) {
    let _scope = RuntimeLogScope;
    if mode == "retry" {
        std::env::set_var("OPENPILOT_LOG_CONTEXT", "bad JSON");
        assert!(matches!(
            Logger::for_runtime_retained(),
            Err(Error::Context(_))
        ));
        std::env::set_var("OPENPILOT_LOG_CONTEXT", r#"{"version":"retained-version"}"#);
    }
    let mut held = Logger::for_runtime_retained().unwrap();
    match mode {
        "context" => {
            held.bind(fields(&[("local", "base"), ("version", "shadowed-local")]));
            {
                let _binding = held.context(fields(&[("local", "scoped")]));
                emit_facade(&mut held, "context-scoped");
            }
            emit_facade(&mut held, "context-restored");
            let mut other = Logger::for_runtime_retained().unwrap();
            other.bind(fields(&[("local", "other")]));
            emit_facade(&mut other, "second-facade");
        }
        "close" => {
            emit_facade(&mut held, "facade-before");
            held.close();
            held.close();
            closed(&mut held);
            let mut other = Logger::for_runtime_retained().unwrap();
            emit_facade(&mut other, "new-facade");
            assert_eq!(
                emit_runtime(log_site!(), record("direct-runtime")).unwrap(),
                Delivery::Sent
            );
        }
        "threads" => {
            drop(held);
            for i in 0..3 {
                thread::Builder::new()
                    .name(format!("facade-{i}"))
                    .spawn(move || {
                        let mut logger = Logger::for_runtime_retained().unwrap();
                        logger.bind(fields(&[("local", &format!("thread-{i}"))]));
                        emit_facade(&mut logger, &format!("thread-{i}"));
                    })
                    .unwrap()
                    .join()
                    .unwrap();
            }
            held = Logger::for_runtime_retained().unwrap();
        }
        "retry" => {
            emit_facade(&mut held, "retained-record");
            std::env::set_var(
                "OPENPILOT_LOG_CONTEXT",
                "bad after successful initialization",
            );
            assert!(matches!(
                Logger::for_runtime_retained(),
                Err(Error::Context(_))
            ));
            std::env::set_var("OPENPILOT_LOG_CONTEXT", r#"{"version":"retained-version"}"#);
        }
        "global-close" | "ordinary-independent" => {
            emit_facade(&mut held, "retained-record");
        }
        "fork-open" | "fork-closed" | "fork-drop" => {
            emit_facade(&mut held, "parent-before");
            if mode == "fork-closed" {
                held.close();
                closed(&mut held);
            }
        }
        _ => panic!("unknown facade mode"),
    }
    ready("EMITTED");
    acknowledge();
    if mode.starts_with("fork") {
        fork_worker(&mut held, mode);
        if mode == "fork-closed" {
            closed(&mut held);
        } // child reconnect cannot reopen parent's facade.
        let mut other = Logger::for_runtime_retained().unwrap();
        emit_facade(&mut other, "parent-after");
        ready("AFTER");
        acknowledge();
    }
    if mode == "ordinary-independent" {
        let mut ordinary = openpilot_logging::producer::Factory::for_runtime()
            .unwrap()
            .logger();
        emit_facade(&mut ordinary, "ordinary-before");
        ready("ORDINARY-BEFORE");
        acknowledge();
        close_runtime().unwrap();
        emit_facade(&mut ordinary, "ordinary-after");
        ready("ORDINARY-AFTER");
        acknowledge();
    }
    close_runtime().unwrap();
    close_runtime().unwrap();
    closed(&mut held);
    let mut later = Logger::for_runtime_retained().unwrap();
    closed(&mut later);
    assert!(matches!(
        emit_runtime(log_site!(), record("global-closed")),
        Err(Error::Transport(zmq::Error::ENOTSOCK))
    ));
    ready("CLOSED");
}
struct OwnedProbe {
    child: Child,
    reader: Option<JoinHandle<()>>,
    reaped: bool,
}
impl OwnedProbe {
    fn wait(&mut self) -> ExitStatus {
        let began = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                self.reaped = true;
                return status;
            }
            assert!(
                began.elapsed() < Duration::from_secs(3),
                "runtime producer child did not stop"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn ack(&mut self) {
        self.child.stdin.as_mut().unwrap().write_all(b"!").unwrap();
    }
}
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        if !self.reaped {
            // The unreaped group leader still reserves this numeric identity;
            // include the controlled fork child if a test assertion failed.
            let _ = rustix::process::kill_process_group(
                rustix::process::Pid::from_child(&self.child),
                rustix::process::Signal::KILL,
            );
            let _ = self.child.wait();
            self.reaped = true;
        }
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}
struct SocketPath(PathBuf);
impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn next_line(lines: &Receiver<String>, expected: &str, root: &Path) {
    assert_eq!(
        lines
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| panic!(
                "{e}; stderr={}",
                fs::read_to_string(root.join("stderr")).unwrap()
            )),
        expected
    );
}

fn receive(
    socket: &zmq::Socket,
    text: &str,
    pid: Option<u32>,
    thread: Option<&str>,
    local: Option<&str>,
) -> u32 {
    let raw = socket.recv_bytes(0).unwrap();
    assert_eq!(raw[0], 20);
    let value: serde_json::Value = serde_json::from_slice(&raw[1..]).unwrap();
    assert_eq!(value["msg"], text);
    assert_eq!(value["levelnum"], 20);
    assert_eq!(value["level"], "INFO");
    assert_eq!(value["name"], "swaglog");
    assert!(value.get("exc_info").is_none());
    assert_eq!(value["ctx"]["version"], "retained-version");
    assert_eq!(value["ctx"]["runtime_language"], "rust");
    if let Some(local) = local {
        assert_eq!(value["ctx"]["local"], local);
    } else {
        assert!(value["ctx"].get("local").is_none());
    }
    let actual = u32::try_from(value["process"].as_u64().unwrap()).unwrap();
    if let Some(pid) = pid {
        assert_eq!(actual, pid);
    }
    if let Some(thread) = thread {
        assert_eq!(value["threadName"], thread);
    }
    assert!(value["pathname"]
        .as_str()
        .unwrap()
        .ends_with("tests/retained_facade/main.rs"));
    assert!(value["funcName"]
        .as_str()
        .unwrap()
        .ends_with(if text == "direct-runtime" {
            "::worker"
        } else {
            "::emit_facade"
        }));
    actual
}
fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let prefix = format!(
        "_facade_{}_{}",
        std::process::id(),
        root.path().file_name().unwrap().to_str().unwrap()
    );
    let socket_path = SocketPath(format!("/tmp/logmessage{prefix}").into());
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--worker", mode])
        .env("OPENPILOT_PREFIX", prefix)
        .env("OPENPILOT_LOG_CONTEXT", r#"{"version":"retained-version"}"#)
        .env_remove("MANAGER_DAEMON")
        .env_remove("LOGPRINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(root.path().join("stderr")).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut owned = OwnedProbe {
        child,
        reader: None,
        reaped: false,
    };
    let stdout = owned.child.stdout.take().unwrap();
    let (send, lines) = mpsc::channel();
    owned.reader = Some(thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if send.send(line.unwrap() + "\n").is_err() {
                break;
            }
        }
    }));
    next_line(&lines, "EMITTED\n", root.path());
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL).unwrap();
    collector.set_linger(0).unwrap();
    collector.set_rcvtimeo(2000).unwrap(); // Original receive deadline unchanged.
    collector
        .bind(&format!("ipc://{}", socket_path.0.display()))
        .unwrap();

    let pid = owned.child.id();
    match mode {
        "context" => {
            receive(
                &collector,
                "context-scoped",
                Some(pid),
                None,
                Some("scoped"),
            );
            receive(
                &collector,
                "context-restored",
                Some(pid),
                None,
                Some("base"),
            );
            receive(&collector, "second-facade", Some(pid), None, Some("other"));
        }
        "close" => {
            for text in ["facade-before", "new-facade", "direct-runtime"] {
                receive(&collector, text, Some(pid), None, None);
            }
        }
        "threads" => {
            for i in 0..3 {
                receive(
                    &collector,
                    &format!("thread-{i}"),
                    Some(pid),
                    Some(&format!("facade-{i}")),
                    Some(&format!("thread-{i}")),
                );
            }
        }
        _ => {
            receive(
                &collector,
                if mode.starts_with("fork") {
                    "parent-before"
                } else {
                    "retained-record"
                },
                Some(pid),
                None,
                None,
            );
        }
    }
    owned.ack();
    if mode.starts_with("fork") {
        if mode != "fork-drop" {
            next_line(&lines, "CHILD\n", root.path());
            let fork_pid = receive(&collector, "child-record", None, None, None);
            assert_ne!(fork_pid, pid);
            owned.ack();
            next_line(&lines, "CHILD-OTHER\n", root.path());
            receive(&collector, "child-other", Some(fork_pid), None, None);
            owned.ack();
        }
        next_line(&lines, "AFTER\n", root.path());
        receive(&collector, "parent-after", Some(pid), None, None);
        owned.ack();
    }
    if mode == "ordinary-independent" {
        next_line(&lines, "ORDINARY-BEFORE\n", root.path());
        receive(&collector, "ordinary-before", Some(pid), None, None);
        owned.ack();
        next_line(&lines, "ORDINARY-AFTER\n", root.path());
        receive(&collector, "ordinary-after", Some(pid), None, None);
        owned.ack();
    }
    next_line(&lines, "CLOSED\n", root.path());
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    assert!(owned.wait().success());
    println!("PASS retained Logger facade {mode}");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(&args[2]);
        return;
    }
    let mut failed = Vec::new();
    for mode in [
        "context",
        "close",
        "threads",
        "retry",
        "global-close",
        "ordinary-independent",
        "fork-open",
        "fork-closed",
        "fork-drop",
    ] {
        if std::panic::catch_unwind(|| check(mode)).is_err() {
            failed.push(mode);
        }
    }
    assert!(failed.is_empty(), "retained facade failures: {failed:?}");
}
