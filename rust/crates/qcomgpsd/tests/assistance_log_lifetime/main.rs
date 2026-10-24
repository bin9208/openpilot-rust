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
        "_assistance_log_lifetime_{}_{}",
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
            "assistance_log_lifetime log lost after {mode} operation returned: {error}; stderr={}",
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
    println!("PASS assistance_log_lifetime retained log after {mode} operation completion");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(Path::new(&args[3]), &args[2]);
        return;
    }
    if args.get(1).is_some_and(|arg| arg == "-m") {
        mmcli_fixture(&args[1..]);
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
        "assistance_log_lifetime lifetime failures: {failed:?}"
    );
}

const LEVEL: u8 = 20;
const LEVEL_NAME: &str = "INFO";
// Original qcomgpsd.py inject_assistance success event after check_output.
const EXPECTED_MESSAGE: &str = "successfully loaded assistance data";
const SITE_SUFFIX: &str = "qcomgpsd/src/assistance.rs";
const FUNCTION_SUFFIX: &str = "::inject";
fn operation(root: &Path) {
    use openpilot_qcomgpsd::{assistance, at::AtPort, config::Config};
    let assistance = root.join("assistance.bin");
    std::fs::write(&assistance, b"private assistance fixture").unwrap();
    let config = Config {
        at: AtPort {
            path: root.join("unused-at"),
            lock: root.join("unused-lock"),
        },
        diagnostic: root.join("unused-diag"),
        nmea: root.join("unused-nmea"),
        root: root.into(),
        assistance: assistance.clone(),
        assistance_url: "http://127.0.0.1:9/unused".into(),
        alternate: None,
        mmcli: std::env::current_exe().unwrap(),
        systemd: root.join("unused-systemd"),
        cold_start: false,
    };
    assistance::inject(&config, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    let command: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.join("mmcli.json")).unwrap()).unwrap();
    assert_eq!(
        command,
        [
            "-m",
            "any",
            "--timeout",
            "30",
            &format!("--location-inject-assistance-data={}", assistance.display())
        ]
    );
    assert_eq!(
        std::fs::read(assistance).unwrap(),
        b"private assistance fixture"
    );
}
fn mmcli_fixture(args: &[String]) {
    let root = PathBuf::from(std::env::var_os("LOG_LIFETIME_FIXTURE_ROOT").unwrap());
    let assistance = root.join("assistance.bin");
    assert_eq!(
        args,
        [
            "-m",
            "any",
            "--timeout",
            "30",
            &format!("--location-inject-assistance-data={}", assistance.display())
        ]
    );
    assert_eq!(
        std::fs::read(assistance).unwrap(),
        b"private assistance fixture"
    );
    // Exclusive creation also proves the first successful command was not retried.
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("mmcli.json"))
        .unwrap();
    output
        .write_all(&serde_json::to_vec(args).unwrap())
        .unwrap();
}
