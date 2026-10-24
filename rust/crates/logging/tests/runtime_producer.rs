//! Isolated-process tests for the Python-compatible process-retained producer.
//! The parent collector binds only after emit returned / all worker threads exited.
use openpilot_logging::{
    log_site,
    producer::{close_runtime, emit_runtime, Delivery, RuntimeLogScope},
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
fn record(text: &str) -> Record {
    Record::text(Level::Error, text.into()).with_exception("NoneType: None\n".into())
}
fn emit(text: &str) {
    assert_eq!(
        emit_runtime(log_site!(), record(text)).unwrap(),
        Delivery::Sent
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
fn fork_worker(reconnect: bool) {
    // SAFETY: only the worker main application thread exists; emit completed
    // before fork, so no Rust logging mutex/context lock is held. libzmq's old
    // handles must be forgotten in the child before reconnecting or closing.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
        if reconnect {
            emit("child-record");
            ready("CHILD");
            acknowledge();
        }
        close_runtime().unwrap();
        // SAFETY: this is the owned fork child; skip inherited Rust destructors.
        unsafe { libc::_exit(0) };
    }
    let mut status = 0;
    // SAFETY: child is this worker's positive unreaped direct child.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}
fn worker(mode: &str) {
    // Only the process root holds this guard, never an individual emit helper or
    // short-lived worker thread. It must not initialize an unused transport.
    let _scope = RuntimeLogScope;
    if mode == "scope" {
        drop(RuntimeLogScope); // An unused process-root guard is also a no-op.
    }
    close_runtime().unwrap(); // Before first emit is deliberately a no-op.
    if mode == "retry" {
        std::env::set_var("OPENPILOT_LOG_CONTEXT", "broken");
        assert!(matches!(
            emit_runtime(log_site!(), record("not queued")),
            Err(Error::Context(_))
        ));
        assert_eq!(
            emit_runtime(log_site!(), Record::text(Level::NotSet, "filtered".into())).unwrap(),
            Delivery::Filtered
        );
        std::env::set_var("OPENPILOT_LOG_CONTEXT", r#"{"version":"retained-version"}"#);
    }
    if mode == "threads" {
        for i in 0..3 {
            thread::Builder::new()
                .name(format!("retained-{i}"))
                .spawn(move || emit(&format!("thread-{i}")))
                .unwrap()
                .join()
                .unwrap();
            if i == 0 {
                // The retained Factory is initialized once by the first success;
                // later metadata calls do not rebuild a different global context.
                std::env::set_var(
                    "OPENPILOT_LOG_CONTEXT",
                    r#"{"version":"changed-after-first"}"#,
                );
            }
        }
    } else {
        emit(if mode.starts_with("fork") {
            "parent-before"
        } else {
            "retained-record"
        });
    }
    ready("EMITTED");
    acknowledge();
    if mode.starts_with("fork") {
        fork_worker(mode == "fork-reconnect");
        emit("parent-after");
        ready("AFTER");
        acknowledge();
    }
    if mode == "scope" {
        drop(_scope);
    } else {
        close_runtime().unwrap();
        close_runtime().unwrap();
    }
    // In scope mode this assertion precedes explicit close, so a no-op Drop
    // cannot pass by borrowing the close_runtime checks from other modes.
    assert!(matches!(
        emit_runtime(log_site!(), record("closed transport")),
        Err(Error::Transport(zmq::Error::ENOTSOCK))
    ));
    close_runtime().unwrap(); // Idempotent after explicit close or scope drop.
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
fn receive(socket: &zmq::Socket, text: &str, process: Option<u32>, thread: Option<&str>) -> u32 {
    let raw = socket.recv_bytes(0).unwrap();
    assert_eq!(raw[0], 40);
    let value: serde_json::Value = serde_json::from_slice(&raw[1..]).unwrap();
    assert_eq!(value["msg"], text);
    assert_eq!(value["exc_info"], "NoneType: None\n");
    assert_eq!(value["levelnum"], 40);
    assert_eq!(value["level"], "ERROR");
    assert_eq!(value["name"], "swaglog");
    assert_eq!(value["ctx"]["version"], "retained-version");
    assert_eq!(value["ctx"]["runtime_language"], "rust");
    assert!(value["pathname"]
        .as_str()
        .unwrap()
        .ends_with("tests/runtime_producer.rs"));
    assert!(value["funcName"].as_str().unwrap().ends_with("::emit"));
    let pid = u32::try_from(value["process"].as_u64().unwrap()).unwrap();
    if let Some(process) = process {
        assert_eq!(pid, process);
    }
    if let Some(thread) = thread {
        assert_eq!(value["threadName"], thread);
    }
    pid
}
fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let prefix = format!(
        "_producer_{}_{}",
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
    if mode == "threads" {
        for i in 0..3 {
            receive(
                &collector,
                &format!("thread-{i}"),
                Some(owned.child.id()),
                Some(&format!("retained-{i}")),
            );
        }
    } else {
        receive(
            &collector,
            if mode.starts_with("fork") {
                "parent-before"
            } else {
                "retained-record"
            },
            Some(owned.child.id()),
            None,
        );
    }
    owned.ack();
    if mode == "fork-reconnect" {
        next_line(&lines, "CHILD\n", root.path());
        let fork_pid = receive(&collector, "child-record", None, None);
        assert_ne!(fork_pid, owned.child.id());
        owned.ack();
    }
    if mode.starts_with("fork") {
        next_line(&lines, "AFTER\n", root.path());
        receive(&collector, "parent-after", Some(owned.child.id()), None);
        owned.ack();
    }
    next_line(&lines, "CLOSED\n", root.path());
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    assert!(owned.wait().success());
    println!("PASS runtime producer {mode}");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(&args[2]);
        return;
    }
    let mut failed = Vec::new();
    for mode in [
        "main",
        "threads",
        "retry",
        "fork-reconnect",
        "fork-drop",
        "scope",
    ] {
        if std::panic::catch_unwind(|| check(mode)).is_err() {
            failed.push(mode);
        }
    }
    assert!(failed.is_empty(), "runtime producer failures: {failed:?}");
}
