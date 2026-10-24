//! The original global Swaglog handler retains queued records after the helper
//! returns, including after its calling thread exits. Bind only after emission
//! so this checks ownership rather than depending on IPC connection timing.
use openpilot_runtime_version::{get_build_metadata, Error};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct OwnedProbe {
    child: Child,
    reader: Option<JoinHandle<()>>,
}
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}
struct SocketPath(PathBuf);
impl Drop for SocketPath {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("temporary metadata IPC cleanup: {error}"),
        }
    }
}
fn emit(root: &Path) {
    assert!(matches!(
        get_build_metadata(root),
        Err(Error::InvalidMetadata)
    ));
}
fn worker(root: &Path, mode: &str) {
    let _runtime_logs = openpilot_runtime_version::RuntimeLogScope;
    match mode {
        "main" => emit(root),
        "threads" => {
            for number in 0..3 {
                let root = root.to_owned();
                thread::Builder::new()
                    .name(format!("metadata-{number}"))
                    .spawn(move || emit(&root))
                    .unwrap()
                    .join()
                    .unwrap();
            }
        }
        _ => panic!("unknown worker mode"),
    }
    println!("EMITTED");
    std::io::stdout().flush().unwrap();
    let mut acknowledgment = [0];
    std::io::stdin().read_exact(&mut acknowledgment).unwrap();
    assert_eq!(acknowledgment, [b'!']);
}
fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let prefix = format!(
        "_version_{}_{}",
        std::process::id(),
        root.path().file_name().unwrap().to_str().unwrap()
    );
    let socket_path = SocketPath(PathBuf::from(format!("/tmp/logmessage{prefix}")));
    let stderr = std::fs::File::create(root.path().join("stderr")).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--worker", mode])
        .arg(root.path())
        .env("OPENPILOT_PREFIX", &prefix)
        .env_remove("OPENPILOT_LOG_CONTEXT")
        .env_remove("MANAGER_DAEMON")
        .env_remove("LOGPRINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .unwrap();
    let mut owned = OwnedProbe {
        child,
        reader: None,
    };
    let stdout = owned.child.stdout.take().unwrap();
    let (ready, receipt) = mpsc::sync_channel(1);
    owned.reader = Some(thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = ready.send(result.map(|_| line));
    }));
    assert_eq!(
        receipt
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        "EMITTED\n",
        "metadata calls must return before binding the collector"
    );
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL).unwrap();
    collector.set_linger(0).unwrap();
    // This is the original check_version_reference.py deadline, unchanged.
    collector.set_rcvtimeo(2000).unwrap();
    collector
        .bind(&format!("ipc://{}", socket_path.0.display()))
        .unwrap();
    let count = if mode == "threads" { 3 } else { 1 };
    for number in 0..count {
        let packet = collector.recv_bytes(0).unwrap_or_else(|error| {
            panic!(
                "metadata error log {number}/{count} lost after {mode} caller returned: {error}; stderr={}",
                std::fs::read_to_string(root.path().join("stderr")).unwrap()
            )
        });
        assert_eq!(packet[0], 40);
        let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
        assert_eq!(record["msg"], "unable to get build metadata");
        assert_eq!(record["exc_info"], "NoneType: None\n");
        assert_eq!(record["levelnum"], 40);
        assert_eq!(record["level"], "ERROR");
        assert_eq!(record["name"], "swaglog");
        assert_eq!(record["ctx"]["runtime_language"], "rust");
        assert_eq!(record["process"], owned.child.id());
        assert!(record["pathname"]
            .as_str()
            .unwrap()
            .ends_with("runtime-version/src/lib.rs"));
        assert!(record["funcName"]
            .as_str()
            .unwrap()
            .ends_with("::get_build_metadata"));
        if mode == "threads" {
            assert_eq!(record["threadName"], format!("metadata-{number}"));
        }
    }
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    owned.child.stdin.take().unwrap().write_all(b"!").unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = owned.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "metadata probe did not exit"
        );
        thread::sleep(Duration::from_millis(5));
    }
    println!("PASS retained metadata log after {mode} caller returned ({count} records)");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(Path::new(&args[3]), &args[2]);
        return;
    }
    let mut failed = Vec::new();
    for mode in ["main", "threads"] {
        if std::panic::catch_unwind(|| check(mode)).is_err() {
            failed.push(mode);
        }
    }
    assert!(
        failed.is_empty(),
        "metadata log lifetime failures: {failed:?}"
    );
}
