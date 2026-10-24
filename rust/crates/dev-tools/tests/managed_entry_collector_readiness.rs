//! Actual queue-reset ordering, independently controlled by a parent gate.
//! Separate executable: this test owns the process-global msgq namespace.
#![cfg(feature = "managed-entry-native")]
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::{managed_entry_collector_source, seal::Result};
use openpilot_msgq::Subscriber;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    os::unix::{fs::FileExt, net::UnixListener, process::ExitStatusExt},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const CAPACITY: usize = 10 * 1024 * 1024;
struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn alive(&mut self) -> bool {
        if self.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            self.0 = None;
            false
        } else {
            true
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Prefix(Option<std::ffi::OsString>);
impl Drop for Prefix {
    fn drop(&mut self) {
        if let Some(value) = &self.0 {
            std::env::set_var("OPENPILOT_PREFIX", value);
        } else {
            std::env::remove_var("OPENPILOT_PREFIX");
        }
    }
}
fn header(path: &Path) -> (u64, u64) {
    // Unchanged msgq.h: num_readers, write_pointer, write_uid, all u64.
    let mut bytes = [0; 24];
    File::open(path)
        .unwrap()
        .read_exact_at(&mut bytes, 0)
        .unwrap();
    (
        u64::from_ne_bytes(bytes[..8].try_into().unwrap()),
        u64::from_ne_bytes(bytes[16..24].try_into().unwrap()),
    )
}
fn receive(subscriber: &mut Subscriber, expected: &[u8], error: bool) {
    let packet = subscriber
        .receive(Duration::from_secs(2))
        .unwrap()
        .expect("actual source publication missing");
    let message = capnp::serialize::read_message(
        &mut Cursor::new(packet),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let value = message.get_root::<event::Reader>().unwrap();
    assert!(value.get_valid());
    assert!(value.get_log_mono_time() > 0);
    let text = match value.which().unwrap() {
        event::LogMessage(text) if !error => text.unwrap(),
        event::ErrorLogMessage(text) if error => text.unwrap(),
        _ => panic!("wrong full cereal union"),
    };
    assert_eq!(text.as_bytes(), expected);
}
#[test]
fn real_subscriber_precedes_publisher_reset_and_refreshes_before_error_delivery() {
    let _: fn(&str, &Path, &Path) -> Result<()> = managed_entry_collector_source::run_gated;
    let worker = std::env::var_os("MANAGED_ENTRY_WORKER_BINARY")
        .expect("prerequisite: freshly built check-managed-entry in MANAGED_ENTRY_WORKER_BINARY");
    let temporary = tempfile::Builder::new()
        .prefix("managed-gate-")
        .tempdir_in("/tmp")
        .unwrap();
    let namespace = tempfile::Builder::new()
        .prefix("msgq_mgate-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let name = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let _prefix = Prefix(std::env::var_os("OPENPILOT_PREFIX"));
    std::env::set_var("OPENPILOT_PREFIX", name);
    let gate_path = temporary.path().join("gate");
    let listener = UnixListener::bind(&gate_path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("ipc://{}", temporary.path().join("socket").display());
    let root = temporary.path().join("logs");
    let mut child = OwnedChild(Some(
        Command::new(worker)
            .args(["--managed-collector-gated", &endpoint])
            .arg(&root)
            .arg(&gate_path)
            .env("OPENPILOT_PREFIX", name)
            .stdout(Stdio::from(
                File::create(temporary.path().join("stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                File::create(temporary.path().join("stderr")).unwrap(),
            ))
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut gate = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    child.alive(),
                    "collector exited before gate: {}",
                    fs::read_to_string(temporary.path().join("stderr")).unwrap()
                );
                assert!(Instant::now() < deadline, "collector gate connect deadline");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept gate: {error}"),
        }
    };
    gate.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    gate.set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut byte = [0];
    gate.read_exact(&mut byte).unwrap();
    assert_eq!(byte, *b"R");
    let queue = namespace.path().join("errorLogMessage");
    let before = header(&queue);
    assert!(before.0 >= 1, "real dummy subscriber must own a reader");
    assert_eq!(before.1, 0, "publisher not initialized before release");
    let mut log = Subscriber::for_runtime("logMessage", false, CAPACITY).unwrap();
    let mut errors = Subscriber::for_runtime("errorLogMessage", false, CAPACITY).unwrap();
    let subscribed = header(&queue);
    assert!(subscribed.0 >= 2);
    assert_eq!(subscribed.1, 0);
    gate.write_all(b"1").unwrap();
    gate.read_exact(&mut byte).unwrap();
    assert_eq!(byte, *b"1");
    let reset = header(&queue);
    assert_eq!(
        reset.0, 0,
        "actual publisher construction resets old readers"
    );
    assert_ne!(reset.1, 0);
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PUSH).unwrap();
    sender.set_linger(0).unwrap();
    sender.set_sndtimeo(2000).unwrap();
    sender.connect(&endpoint).unwrap();
    let marker = br#"{"msg":"ready"}"#;
    sender
        .send_multipart([&[10][..], marker.as_slice()], 0)
        .unwrap();
    receive(&mut log, marker, false);
    assert!(errors.receive(Duration::ZERO).unwrap().is_none());
    let refreshed = header(&queue);
    assert!(refreshed.0 >= 1, "old subscriber must refresh after reset");
    assert_eq!(refreshed.1, reset.1);
    let crash = br#"{"msg":"crash","ctx":{"site":"gate"}}"#;
    sender
        .send_multipart([&[40][..], crash.as_slice()], 0)
        .unwrap();
    receive(&mut log, crash, false);
    receive(&mut errors, crash, true);
    let raw = fs::read(&queue).unwrap();
    assert!(raw
        .windows(b"\"crash\"".len())
        .any(|bytes| bytes == b"\"crash\""));
    println!(
        "{}",
        serde_json::json!({"before":before,"subscribed":subscribed,"reset":reset,"refreshed":refreshed,"error_queue_sha256":format!("{:x}",Sha256::digest(&raw))})
    );
    let pid = rustix::process::Pid::from_raw(child.0.as_ref().unwrap().id() as i32).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.as_mut().unwrap().try_wait().unwrap() {
            child.0 = None;
            assert_eq!(status.signal(), Some(2));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "collector SIGINT cleanup timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let disk = fs::read(root.join("swaglog.0000000000")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&disk).unwrap();
    assert_eq!(value["msg$s"], "crash");
    assert_eq!(value["ctx"]["site"], "gate");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}
