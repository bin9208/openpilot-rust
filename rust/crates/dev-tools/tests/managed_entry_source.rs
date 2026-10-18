//! One isolated test process owns this namespace. Original child fields are
//! expectations only; every source request executes a new interactive worker.
#![cfg(feature = "managed-entry-native")]
use openpilot_dev_tools::managed_entry_source as source;
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild {
    child: Child,
    pid: Pid,
    reaped: bool,
    buffer: Vec<u8>,
}
impl OwnedChild {
    fn line(&mut self) -> Value {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(at) = self.buffer.iter().position(|b| *b == b'\n') {
                let row = self.buffer.drain(..=at).collect::<Vec<_>>();
                return serde_json::from_slice(&row).unwrap();
            }
            let mut bytes = [0; 4096];
            match self.child.stdout.as_mut().unwrap().read(&mut bytes) {
                Ok(0) => panic!("source child EOF before protocol row"),
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("source protocol read: {error}"),
            }
            assert!(Instant::now() < end, "source protocol deadline");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn advance(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
    }
    fn finish(mut self) -> i32 {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let status = waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
            )
            .unwrap();
            if status.is_some() {
                break;
            }
            assert!(
                Instant::now() < end,
                "source did not exit after final acknowledgement"
            );
            thread::sleep(Duration::from_millis(2));
        }
        // The unreaped leader reserves this group until descendants are stopped.
        let _ = kill_process_group(self.pid, Signal::KILL);
        let result = self.child.wait().unwrap();
        self.reaped = true;
        result.code().expect("source child terminated by a signal")
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(self.pid, Signal::KILL);
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}
struct CaseEvidence(Option<tempfile::TempDir>);
impl CaseEvidence {
    fn path(&self) -> &Path {
        self.0.as_ref().unwrap().path()
    }
}
impl Drop for CaseEvidence {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(directory) = self.0.take() {
                eprintln!(
                    "managed source failure evidence: {}",
                    directory.keep().display()
                );
            }
        }
    }
}
struct Cleanup {
    params: PathBuf,
    namespace: PathBuf,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.params, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.namespace);
    }
}
fn normalized(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(normalized).collect()),
        Value::Object(values) => {
            let capture = values.get("op").is_some_and(|v| v == "capture_exception");
            Value::Object(
                values
                    .iter()
                    .filter_map(|(key, value)| {
                        if [
                            "exception",
                            "traceback",
                            "message",
                            "kind",
                            "cmdline",
                            "comm",
                            "secondary",
                            "exception_type",
                        ]
                        .contains(&key.as_str())
                        {
                            return None;
                        }
                        let value = if capture && key == "fields" {
                            json!({"captured_exception":true})
                        } else if key == "value_json" {
                            serde_json::from_str(value.as_str().unwrap()).unwrap()
                        } else {
                            normalized(value)
                        };
                        Some((key.clone(), value))
                    })
                    .collect(),
            )
        }
        other => other.clone(),
    }
}
fn log(socket: &zmq::Socket) -> Value {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        match socket.recv_bytes(0) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes[1..]).unwrap();
                assert_eq!(u64::from(bytes[0]), value["levelnum"].as_u64().unwrap());
                return value;
            }
            Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => {}
            Err(error) => panic!("source log receive: {error}"),
        }
        assert!(Instant::now() < end, "source logging deadline");
    }
}

#[test]
fn all22_original_source_cases_execute_policy_files_signals_context_and_body_ipc() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let pinned = source::sources(&repository).unwrap();
    let source_guard = tempfile::tempdir().unwrap();
    for name in pinned.keys() {
        let target = source_guard.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository.join(name), target).unwrap();
    }
    assert_eq!(source::sources(source_guard.path()).unwrap(), pinned);
    for name in pinned.keys() {
        let guarded = source_guard.path().join(name);
        let original = fs::read(&guarded).unwrap();
        fs::write(&guarded, b"changed original source").unwrap();
        assert!(
            source::sources(source_guard.path()).is_err(),
            "changed {name}"
        );
        fs::remove_file(&guarded).unwrap();
        if matches!(
            name.as_str(),
            "rust/tools/check_managed_entry.py" | "rust/tools/managed_entry_reference.py"
        ) {
            let mut without = pinned.clone();
            without.remove(name);
            assert_eq!(
                source::sources(source_guard.path()).unwrap(),
                without,
                "retired {name}"
            );
        } else {
            assert!(
                source::sources(source_guard.path()).is_err(),
                "missing live {name}"
            );
        }
        std::os::unix::fs::symlink(source_guard.path().join("absent"), &guarded).unwrap();
        assert!(
            source::sources(source_guard.path()).is_err(),
            "dangling {name}"
        );
        fs::remove_file(&guarded).unwrap();
        fs::write(&guarded, original).unwrap();
    }
    assert_eq!(source::sources(source_guard.path()).unwrap(), pinned);
    let retired = [
        (
            "rust/tools/check_managed_entry.py",
            "89d2ce945fe94955d9b764f6a71434ac02298713a8983513bb9990318e6acc44",
        ),
        (
            "rust/tools/managed_entry_reference.py",
            "3f502d9c478e5f801f84b52ab5b613cb55f5d161b68434568e3942a72ac642d8",
        ),
    ];
    let mut saved = Vec::new();
    let mut remaining = pinned.clone();
    let mut historical = BTreeMap::new();
    for (name, expected) in retired {
        let path = source_guard.path().join(name);
        let bytes = if pinned.contains_key(name) {
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            Some(bytes)
        } else {
            None
        };
        saved.push((path, bytes));
        remaining.remove(name);
        historical.insert(name.to_owned(), expected.to_owned());
    }
    assert_eq!(source::sources(source_guard.path()).unwrap(), remaining);
    assert_eq!(
        source::historical_sources(source_guard.path()).unwrap(),
        historical
    );
    for (path, bytes) in saved {
        fs::write(&path, b"reintroduced changed helper").unwrap();
        assert!(source::sources(source_guard.path()).is_err());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(source_guard.path().join("absent"), &path).unwrap();
        assert!(source::sources(source_guard.path()).is_err());
        fs::remove_file(&path).unwrap();
        if let Some(bytes) = bytes {
            fs::write(path, bytes).unwrap();
        }
    }
    assert_eq!(source::sources(source_guard.path()).unwrap(), pinned);
    assert_eq!(
        source::historical_sources(source_guard.path()).unwrap(),
        source::historical_sources(&repository).unwrap()
    );
    let bytes = include_bytes!("fixtures/managed-entry/original-fields.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "6b0ca0e35bbc9bb51b2ff78e9560aefbed5bc40e8d65a072346cad69d1251271"
    );
    let captured: Value = serde_json::from_slice(bytes).unwrap();
    let rows = captured["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 44);
    let mut cases = BTreeMap::new();
    for row in rows {
        let name = row["case"].as_str().unwrap();
        let selected = json!({"request":row["request"],"ready":row["ready"],"exit_code":row["exit_code"],"params":row["params"],"outcome":row["outcome"],"steps":row["steps"],"sdk_calls":row["sdk_calls"],"records":row["records"],"captures":row["captures"]});
        if let Some(previous) = cases.insert(name.to_owned(), selected.clone()) {
            assert_eq!(
                previous, selected,
                "two independent source collector sets differ: {name}"
            );
        }
    }
    assert_eq!(cases.len(), 22);
    let extra_bytes = include_bytes!("fixtures/managed-entry/source-boundaries-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(extra_bytes)),
        "389dfd2359e8540288af0cdc452b2d12bfb37b587fa2c1db06bbc311e17612bb"
    );
    let extra: Value = serde_json::from_slice(extra_bytes).unwrap();
    assert_eq!(extra["provenance"]["candidate_executed"], false);
    for row in extra["cases"].as_array().unwrap() {
        cases.insert(
            format!("source-boundary-{}", row["case"].as_str().unwrap()),
            row.clone(),
        );
    }
    let namespace_owner = tempfile::Builder::new()
        .prefix("managed-source-")
        .tempdir()
        .unwrap();
    let prefix = format!(
        "rust-probe-{}",
        namespace_owner
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    );
    // This target contains exactly one test. Set once before any native ZMQ
    // threads exist; only owned source children inherit this private namespace.
    let previous_prefix = std::env::var_os("OPENPILOT_PREFIX");
    std::env::set_var("OPENPILOT_PREFIX", &prefix);
    for (name, expected) in cases {
        let root = CaseEvidence(Some(tempfile::tempdir().unwrap()));
        let cleanup = Cleanup {
            params: root.path().join("params"),
            namespace: PathBuf::from(format!("/dev/shm/msgq_{prefix}")),
        };
        for directory in [
            "base/openpilot/common".to_owned(),
            format!("params/{prefix}"),
        ] {
            fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        fs::write(
            root.path().join("base/openpilot/common/version.h"),
            "#define VERSION \"fixture-entry\"\n",
        )
        .unwrap();
        let request = &expected["request"];
        let origin = if request.get("reporting") == Some(&Value::Bool(false)) {
            "git@github.com:fixture/fork.git"
        } else {
            "git@github.com:commaai/openpilot.git"
        };
        fs::write(root.path().join("base/build.json"),json!({"channel":"nightly","openpilot":{"git_origin":origin,"git_commit":"abcdefghijk"}}).to_string()).unwrap();
        let dongle = if let Some(text) = expected["fixture"]["dongle_hex"].as_str() {
            text.as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
        } else {
            b"fixture-device".to_vec()
        };
        fs::write(
            root.path().join(format!("params/{prefix}/DongleId")),
            dongle,
        )
        .unwrap();
        fs::write(root.path().join("prepared-input"), "prepared native body").unwrap();
        fs::write(root.path().join("params-blocked"), "not a directory").unwrap();
        for (key, file) in [
            ("sent", "CarrotExceptionSent"),
            ("prior", "CarrotException"),
        ] {
            if let Some(value) = request.get(key) {
                fs::write(
                    root.path().join(format!("params/{prefix}/{file}")),
                    value.as_str().unwrap(),
                )
                .unwrap();
            }
        }
        let endpoint = format!("ipc://{}", root.path().join("source-log").display());
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL).unwrap();
        socket.set_linger(0).unwrap();
        socket.set_rcvtimeo(100).unwrap();
        socket.bind(&endpoint).unwrap();
        fs::create_dir_all(&cleanup.namespace).unwrap();
        let mut body = openpilot_msgq::Subscriber::new("managedEntryBody", false).unwrap();
        let mut request = request.as_object().unwrap().clone();
        request.insert("root".into(), json!(root.path()));
        request.insert("prefix".into(), json!(prefix));
        request.insert("endpoint".into(), json!(endpoint));
        request.insert("dsn".into(), json!("http://public:secret@127.0.0.1:1/296"));
        let request_file = root.path().join("request.json");
        fs::write(&request_file, Value::Object(request.clone()).to_string()).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_check-managed-entry"))
            .args([
                std::ffi::OsStr::new("--managed-source"),
                request_file.as_os_str(),
            ])
            .env("OPENPILOT_PREFIX", &prefix)
            .env("OPENPILOT_RUST_ROOT", &repository)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.path().join("stderr")).unwrap())
            .process_group(0)
            .spawn()
            .unwrap();
        let flags = fcntl_getfl(child.stdout.as_ref().unwrap()).unwrap();
        fcntl_setfl(child.stdout.as_ref().unwrap(), flags | OFlags::NONBLOCK).unwrap();
        let pid = Pid::from_raw(child.id() as i32).unwrap();
        let mut child = OwnedChild {
            child,
            pid,
            reaped: false,
            buffer: Vec::new(),
        };
        let ready = child.line();
        assert_eq!(ready, expected["ready"], "{name}");
        assert_eq!(log(&socket)["msg"], "managed-entry-ready");
        child.advance();
        let mut response = child.line();
        if response.get("context_ready") == Some(&Value::Bool(true)) {
            assert_eq!(response["context_replaced"], true, "{name}");
            assert!(body.receive(Duration::ZERO).unwrap().is_none());
            if expected["fixture"]["sigint_at_context"] == true {
                let start = Instant::now();
                kill_process(pid, Signal::INT).unwrap();
                response = child.line();
                assert!(start.elapsed() < Duration::from_secs(5), "{name}");
            } else {
                child.advance();
                response = child.line();
            }
        }
        if response.get("waiting_for_sigint") == Some(&Value::Bool(true)) {
            let start = Instant::now();
            kill_process(pid, Signal::INT).unwrap();
            response = child.line();
            assert!(start.elapsed() < Duration::from_secs(5), "{name}");
        }
        let actual_outcome = if response["outcome"].get("stage").is_some() {
            json!({"kind":response["outcome"]["kind"],"stage":response["outcome"]["stage"]})
        } else {
            json!({"kind":response["outcome"]["kind"]})
        };
        assert_eq!(actual_outcome, expected["outcome"], "{name}");
        assert_eq!(normalized(&response["steps"]), expected["steps"], "{name}");
        assert_eq!(
            normalized(&response["sdk_calls"]),
            expected["sdk_calls"],
            "{name}"
        );
        let has_body = response["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["step"] == "body");
        if has_body {
            assert_eq!(
                body.receive(Duration::from_secs(5)).unwrap().unwrap(),
                b"managed body IPC"
            );
        } else {
            assert!(body.receive(Duration::ZERO).unwrap().is_none());
        }
        let mut observed = Vec::new();
        for _ in expected["records"]["logMessage"].as_array().unwrap() {
            let record = log(&socket);
            observed.push(json!({"msg":record["msg"],"levelnum":record["levelnum"],"ctx":record["ctx"],"has_exception":record.get("exc_info").is_some()}));
        }
        assert_eq!(
            Value::Array(observed),
            expected["records"]["logMessage"],
            "{name}"
        );
        assert_eq!(
            response["external_sdk_boundary"]["kind"],
            "captured-original-sdk-2.55.0-selected-fields"
        );
        assert_eq!(
            response["external_sdk_boundary"]["events"], expected["captures"],
            "{name}"
        );
        let process = request["process"].as_str().unwrap();
        if request.get("prepare").is_none() && !process.contains('\0') {
            assert_eq!(
                response["comm"],
                process.chars().take(15).collect::<String>(),
                "{name}"
            );
            assert_eq!(
                response["cmdline"].as_str().unwrap().trim(),
                process,
                "{name}"
            );
            assert_eq!(
                fs::read_to_string(format!("/proc/{}/comm", pid.as_raw_pid()))
                    .unwrap()
                    .trim(),
                process.chars().take(15).collect::<String>(),
                "{name}"
            );
        }
        child.advance();
        assert_eq!(json!(child.finish()), expected["exit_code"], "{name}");
        let parameter = root.path().join(format!("params/{prefix}/CarrotException"));
        let value = if parameter.is_file() {
            json!(fs::read_to_string(parameter).unwrap())
        } else {
            Value::Null
        };
        assert_eq!(value, expected["params"], "{name}");
        assert!(
            matches!(socket.recv_bytes(zmq::DONTWAIT), Err(zmq::Error::EAGAIN)),
            "{name}: unexpected source logs"
        );
        drop(body);
        drop(socket);
        drop(context);
        drop(cleanup);
        println!("PASS source:{name}");
    }
    match previous_prefix {
        Some(value) => std::env::set_var("OPENPILOT_PREFIX", value),
        None => std::env::remove_var("OPENPILOT_PREFIX"),
    }
}
