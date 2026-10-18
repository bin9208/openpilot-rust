//! #385: signal the real QA example before the body, without another continue.
#![cfg(feature = "managed-entry-native")]
use rustix::{
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions},
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Owner {
    child: Child,
    pid: Pid,
    buffer: Vec<u8>,
    reaped: bool,
}
impl Owner {
    fn row(&mut self) -> Value {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(at) = self.buffer.iter().position(|byte| *byte == b'\n') {
                return serde_json::from_slice(&self.buffer.drain(..=at).collect::<Vec<_>>())
                    .unwrap();
            }
            let mut bytes = [0; 4096];
            match self.child.stdout.as_mut().unwrap().read(&mut bytes) {
                Ok(0) => panic!("example EOF before response"),
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("example protocol: {error}"),
            }
            assert!(
                Instant::now() < end,
                "example did not answer context SIGINT without second continue"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn advance(&mut self) {
        let input = self.child.stdin.as_mut().unwrap();
        input.write_all(b"continue\n").unwrap();
        input.flush().unwrap();
    }
    fn finish(mut self) -> i32 {
        let end = Instant::now() + Duration::from_secs(5);
        while waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        )
        .unwrap()
        .is_none()
        {
            assert!(Instant::now() < end, "example exit deadline");
            thread::sleep(Duration::from_millis(2));
        }
        let _ = kill_process_group(self.pid, Signal::KILL);
        let status = self.child.wait().unwrap();
        self.reaped = true;
        status.code().unwrap()
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(self.pid, Signal::KILL);
            let _ = self.child.wait();
        }
    }
}
struct Namespace(PathBuf);
impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn actual_example_context_sigint_skips_body_without_second_continue() {
    let binary = std::env::var_os("MANAGED_CHILD_BINARY")
        .expect("mandatory actual managed_child example binary");
    let root = tempfile::Builder::new()
        .prefix("managed-ack-")
        .tempdir()
        .unwrap();
    let prefix = format!(
        "rust-probe-{}",
        root.path().file_name().unwrap().to_str().unwrap()
    );
    // This separate integration executable contains exactly one test. Set before
    // creating any native threads; no other test/environment shares this child.
    std::env::set_var("OPENPILOT_PREFIX", &prefix);
    let namespace = Namespace(PathBuf::from(format!("/dev/shm/msgq_{prefix}")));
    fs::create_dir(&namespace.0).unwrap();
    fs::create_dir_all(root.path().join("base/openpilot/common")).unwrap();
    fs::create_dir_all(root.path().join("params").join(&prefix)).unwrap();
    fs::write(
        root.path().join("base/openpilot/common/version.h"),
        "#define VERSION \"fixture-entry\"\n",
    )
    .unwrap();
    fs::write(root.path().join("base/build.json"),r#"{"channel":"nightly","openpilot":{"git_origin":"git@github.com:commaai/openpilot.git","git_commit":"abcdefghijk"}}"#).unwrap();
    fs::write(
        root.path().join("params").join(&prefix).join("DongleId"),
        "fixture-device",
    )
    .unwrap();
    fs::write(root.path().join("prepared-input"), "prepared native body").unwrap();
    let context = zmq::Context::new();
    let log = context.socket(zmq::PULL).unwrap();
    log.set_linger(0).unwrap();
    log.set_rcvtimeo(5000).unwrap();
    let endpoint = format!("ipc://{}", root.path().join("log").display());
    log.bind(&endpoint).unwrap();
    let mut body = openpilot_msgq::Subscriber::new("managedEntryBody", false).unwrap();
    let request = root.path().join("request.json");
    fs::write(&request,json!({"root":root.path(),"prefix":prefix,"endpoint":endpoint,"dsn":"http://public:secret@127.0.0.1:1/296","process":"openpilot.fixture_managed_entry","daemon":"managed-fixture","body":"return"}).to_string()).unwrap();
    let child = Command::new(binary)
        .arg(&request)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(root.path().join("stderr")).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = Pid::from_raw(child.id() as i32).unwrap();
    let flags = fcntl_getfl(child.stdout.as_ref().unwrap()).unwrap();
    fcntl_setfl(child.stdout.as_ref().unwrap(), flags | OFlags::NONBLOCK).unwrap();
    let mut owner = Owner {
        child,
        pid,
        buffer: Vec::new(),
        reaped: false,
    };
    assert_eq!(owner.row()["ready"], true);
    owner.advance();
    assert_eq!(owner.row()["context_ready"], true);
    kill_process(pid, Signal::INT).unwrap();
    // No second continue: pending SIGINT must interrupt this callback itself.
    let response = owner.row();
    let original: Value = serde_json::from_slice(include_bytes!(
        "fixtures/managed-entry/source-boundaries-original.json"
    ))
    .unwrap();
    let original = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["case"] == "context-sigint")
        .unwrap();
    // The original wrapper labels this request-derived branch returned, even
    // though its launcher handled SIGINT before entering the body. Preserve the
    // capture but require the candidate's truthful typed Interrupted outcome.
    assert_eq!(original["outcome"], json!({"kind":"returned"}));
    assert_eq!(response["outcome"], json!({"kind":"interrupted"}));
    assert_eq!(response["steps"].as_array().unwrap().len(), 2);
    assert_eq!(response["steps"][0]["step"], "prepare");
    assert_eq!(response["steps"][1]["step"], "reset_context");
    assert_eq!(response["sdk_calls"], json!([]));
    assert!(body.receive(Duration::ZERO).unwrap().is_none());
    let records = (0..2)
        .map(|_| {
            let raw = log.recv_bytes(0).unwrap();
            serde_json::from_slice::<Value>(&raw[1..]).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(records[0]["msg"], "managed-entry-ready");
    assert_eq!(
        records[1]["msg"],
        "child openpilot.fixture_managed_entry got SIGINT"
    );
    assert!(!root
        .path()
        .join("params")
        .join(prefix)
        .join("CarrotException")
        .exists());
    owner.advance();
    assert_eq!(owner.finish(), 0);
}
