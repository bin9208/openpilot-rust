//! Real public-operation lifetime regression. The collector binds only after
//! operation completion (and after the calling thread joins in thread mode).
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
    reaped: bool,
}
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}
struct SocketPath(PathBuf);
impl Drop for SocketPath {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => eprintln!("owned IPC cleanup: {error}"),
        }
    }
}
fn worker(root: &Path, mode: &str) {
    let _runtime_logs = openpilot_logging::producer::RuntimeLogScope;
    if mode == "thread" {
        let root = root.to_owned();
        thread::Builder::new()
            .name("operation-log".into())
            .spawn(move || operation(&root))
            .unwrap()
            .join()
            .unwrap();
    } else {
        assert_eq!(mode, "main");
        operation(root);
    }
    println!("COMPLETED");
    std::io::stdout().flush().unwrap();
    let mut acknowledgment = [0];
    std::io::stdin().read_exact(&mut acknowledgment).unwrap();
    assert_eq!(acknowledgment, [b'!']);
}
fn check(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let prefix = format!(
        "_params_log_lifetime_{}_{}",
        std::process::id(),
        root.path().file_name().unwrap().to_str().unwrap()
    );
    let socket_path = SocketPath(PathBuf::from(format!("/tmp/logmessage{prefix}")));
    let stderr_path = root.path().join("stderr");
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--worker", mode])
        .arg(root.path())
        .env("OPENPILOT_PREFIX", &prefix)
        .env_remove("OPENPILOT_LOG_CONTEXT")
        .env_remove("MANAGER_DAEMON")
        .env_remove("LOGPRINT")
        .env("LOG_LIFETIME_FIXTURE_ROOT", root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let mut owned = OwnedProbe {
        child,
        reader: None,
        reaped: false,
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
        "COMPLETED\n",
        "operation must finish before collector binds; stderr={}",
        std::fs::read_to_string(&stderr_path).unwrap()
    );
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL).unwrap();
    collector.set_linger(0).unwrap();
    // Existing original logging receive deadline; no retry or enlarged timeout.
    collector.set_rcvtimeo(2000).unwrap();
    collector
        .bind(&format!("ipc://{}", socket_path.0.display()))
        .unwrap();
    let packet = collector.recv_bytes(0).unwrap_or_else(|error| {
        panic!(
            "params_log_lifetime log lost after {mode} operation returned: {error}; stderr={}",
            std::fs::read_to_string(&stderr_path).unwrap()
        )
    });
    assert_eq!(packet[0], LEVEL);
    let record: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(record["msg"], EXPECTED_MESSAGE);
    assert_eq!(record["levelnum"], LEVEL);
    assert_eq!(record["level"], LEVEL_NAME);
    assert_eq!(record["name"], "swaglog");
    assert!(record.get("exc_info").is_none());
    assert_eq!(record["ctx"]["runtime_language"], "rust");
    assert_eq!(record["process"], owned.child.id());
    assert!(record["pathname"].as_str().unwrap().ends_with(SITE_SUFFIX));
    assert!(record["funcName"]
        .as_str()
        .unwrap()
        .ends_with(FUNCTION_SUFFIX));
    if mode == "thread" {
        assert_eq!(record["threadName"], "operation-log");
    }
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    owned.child.stdin.take().unwrap().write_all(b"!").unwrap();
    let began = Instant::now();
    loop {
        if let Some(status) = owned.child.try_wait().unwrap() {
            owned.reaped = true;
            assert!(status.success());
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "operation probe did not exit"
        );
        thread::sleep(Duration::from_millis(5));
    }
    println!("PASS params_log_lifetime retained log after {mode} operation completion");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(Path::new(&args[3]), &args[2]);
        return;
    }

    let mut failed = Vec::new();
    for mode in ["main", "thread"] {
        if std::panic::catch_unwind(|| check(mode)).is_err() {
            failed.push(mode);
        }
    }
    assert!(
        failed.is_empty(),
        "params_log_lifetime lifetime failures: {failed:?}"
    );
}

const LEVEL: u8 = 30;
const LEVEL_NAME: &str = "WARNING";
// Original params_pyx.pyx warning; manager_init iterates all_keys byte keys.
const EXPECTED_MESSAGE: &str = "Failed to cast param b'LongitudinalPersonalityMax' with value=b'not-an-int' from type t=<ParamKeyType.INT: 2>";
const SITE_SUFFIX: &str = "manager/src/diagnostics.rs";
const FUNCTION_SUFFIX: &str = "::cast_failed";
fn operation(root: &Path) {
    let params = openpilot_params::Params::open(&root.join("params"), "d").unwrap();
    let info = openpilot_params::metadata("LongitudinalPersonalityMax").unwrap();
    assert_eq!(info.kind, 2);
    assert_eq!(info.default, Some("3"));
    params.put(info.name, b"not-an-int").unwrap();
    openpilot_manager::parameters::set_defaults(&params, false).unwrap();
    assert_eq!(
        params.get(info.name).unwrap().as_deref(),
        Some(b"3".as_slice())
    );
    // No second warning when defaults are read again after the repair write.
    openpilot_manager::parameters::set_defaults(&params, false).unwrap();
}
