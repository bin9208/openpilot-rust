//! Actual Athena context binding after a malformed private Params value.
//! The collector deliberately starts after the operation and emitting thread end.
use openpilot_athena::{
    logging,
    state::{Config, Shared},
};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const WARNING: &str =
    "Failed to cast param DongleId with value=b'\\xff' from type t=<ParamKeyType.STRING: 0>";

fn bind(root: &Path) {
    let shared = Shared::new(Config {
        host: "ws://127.0.0.1:1/unused".into(),
        handlers: 0,
        log_root: root.join("routes"),
        swaglog_root: root.join("logs"),
        stats_root: root.join("stats"),
        persist_root: root.join("persist"),
        basedir: root.join("repository"),
        process_launcher: root.join("absent-launcher"),
        pc: true,
    })
    .unwrap();
    shared.params.put("DongleId", b"\xff").unwrap();
    shared
        .factory
        .bind_global(
            [(
                "owned_context".into(),
                openpilot_logging::Value::Text("before-bind".into()),
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
    logging::bind(&shared).unwrap();
    let context = shared.factory.logger().context_snapshot().unwrap();
    let context: serde_json::Value = serde_json::from_str(&context.to_json().unwrap()).unwrap();
    assert_eq!(context["dongle_id"], serde_json::Value::Null);
    assert_eq!(context["version"], "owned-version");
    assert_eq!(context["origin"], "github.com/owned/fixture");
    assert_eq!(context["branch"], "owned-channel");
    assert_eq!(context["commit"], "owned-commit");
    assert_eq!(context["dirty"], false);
    assert_eq!(context["owned_context"], "before-bind");
    fs::write(
        root.join("bound-context.json"),
        serde_json::to_vec(&context).unwrap(),
    )
    .unwrap();
}

fn worker(root: &Path, threaded: bool) {
    let _logs = openpilot_logging::producer::RuntimeLogScope;
    if threaded {
        let root = root.to_owned();
        thread::Builder::new()
            .name("athena-context".into())
            .spawn(move || bind(&root))
            .unwrap()
            .join()
            .unwrap();
    } else {
        bind(root);
    }
    println!("COMPLETED");
    std::io::stdout().flush().unwrap();
    let mut ack = [0];
    std::io::stdin().read_exact(&mut ack).unwrap();
    assert_eq!(ack, [b'!']);
}

struct Owner {
    child: Child,
    reader: Option<JoinHandle<()>>,
    reaped: bool,
}
impl Drop for Owner {
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
        let _ = fs::remove_file(&self.0);
    }
}

fn check(threaded: bool) {
    let root = tempfile::tempdir().unwrap();
    let prefix = format!(
        "_athenactx_{}_{}",
        std::process::id(),
        root.path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_start_matches('.')
    );
    let endpoint = SocketPath(format!("/tmp/logmessage{prefix}").into());
    fs::create_dir_all(root.path().join("repository")).unwrap();
    fs::create_dir_all(root.path().join("bin")).unwrap();
    fs::write(root.path().join("repository/build.json"),
        r#"{"channel":"owned-channel","openpilot":{"version":"owned-version","git_commit":"owned-commit","git_origin":"https://github.com/owned/fixture.git"}}"#).unwrap();
    let stderr = root.path().join("stderr");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--worker", if threaded { "thread" } else { "main" }])
        .arg(root.path())
        .env("OPENPILOT_PREFIX", prefix)
        .env("PARAMS_ROOT", root.path().join("params"))
        .env("PATH", root.path().join("bin"))
        .env_remove("OPENPILOT_LOG_CONTEXT")
        .env_remove("MANAGER_DAEMON")
        .env_remove("ATHENA_LOG_CONTEXT")
        .env_remove("LOGPRINT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(&stderr).unwrap());
    let mut owner = Owner {
        child: command.spawn().unwrap(),
        reader: None,
        reaped: false,
    };
    let stdout = owner.child.stdout.take().unwrap();
    let (send, ready) = mpsc::sync_channel(1);
    owner.reader = Some(thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line);
        let _ = send.send(result.map(|_| line));
    }));
    assert_eq!(
        ready
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
        "COMPLETED\n",
        "context operation failed: {}",
        fs::read_to_string(&stderr).unwrap()
    );
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL).unwrap();
    collector.set_linger(0).unwrap();
    collector.set_rcvtimeo(2000).unwrap();
    collector
        .bind(&format!("ipc://{}", endpoint.0.display()))
        .unwrap();
    let packet = collector.recv_bytes(0).unwrap_or_else(|error| {
        panic!(
            "Athena context warning lost after bind (threaded={threaded}): {error}; stderr={}",
            fs::read_to_string(&stderr).unwrap()
        )
    });
    assert_eq!(packet[0], 30);
    let value: serde_json::Value = serde_json::from_slice(&packet[1..]).unwrap();
    assert_eq!(value["msg"], WARNING);
    assert_eq!(value["levelnum"], 30);
    assert_eq!(value["process"], owner.child.id());
    assert_eq!(value["ctx"]["owned_context"], "before-bind");
    assert!(value["pathname"]
        .as_str()
        .unwrap()
        .ends_with("params-typed/src/lib.rs"));
    assert!(value["funcName"]
        .as_str()
        .unwrap()
        .ends_with("::get_string"));
    assert_eq!(collector.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN));
    owner.child.stdin.take().unwrap().write_all(b"!").unwrap();
    let began = Instant::now();
    loop {
        if let Some(status) = owner.child.try_wait().unwrap() {
            owner.reaped = true;
            assert!(status.success());
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(5));
    }
    println!("PASS Athena context lifetime threaded={threaded}");
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--worker") {
        worker(Path::new(&args[3]), args[2] == "thread");
        return;
    }
    let mut failed = Vec::new();
    for threaded in [false, true] {
        if std::panic::catch_unwind(|| check(threaded)).is_err() {
            failed.push(threaded);
        }
    }
    assert!(
        failed.is_empty(),
        "Athena context lifetime failures: {failed:?}"
    );
}
