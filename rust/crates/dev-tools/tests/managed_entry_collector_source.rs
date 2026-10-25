//! Independent original formatter bytes through actual source-collector IPC.
//! One test owns the process-global msgq namespace; no candidate collector or
//! candidate disk formatter participates in the expected bytes.
#![cfg(feature = "managed-entry-native")]
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::{managed_entry_collector_source, seal::Result};
use openpilot_msgq::Subscriber;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Cursor,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

const ORIGINAL: &[u8] = include_bytes!("fixtures/managed-entry-collector/format-original.json");
const CAPACITY: usize = 10 * 1024 * 1024;

fn unhex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

/// Replace only the generated top-level ID token. Every other byte, including
/// field ordering, suffixes, escaped surrogates and float spelling, stays exact.
fn normalize_id(line: &[u8], ids: &mut BTreeSet<Vec<u8>>) -> Vec<u8> {
    let mut index = 0;
    let mut depth = 0;
    while index < line.len() {
        match line[index] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b'"' => {
                let start = index;
                index += 1;
                while line[index] != b'"' {
                    if line[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                let mut next = index + 1;
                while next < line.len() && line[next].is_ascii_whitespace() {
                    next += 1;
                }
                if depth == 1 && &line[start..=index] == b"\"id\"" && line.get(next) == Some(&b':')
                {
                    index += 1;
                    while line[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    assert_eq!(line[index], b':');
                    index += 1;
                    while line[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    assert_eq!(line[index], b'"');
                    let begin = index + 1;
                    let id = &line[begin..begin + 32];
                    assert!(id.iter().all(u8::is_ascii_hexdigit));
                    assert_eq!(id[12], b'4', "original UUID4 version");
                    assert!(b"89ab".contains(&id[16]), "original RFC4122 variant");
                    assert!(ids.insert(id.to_vec()), "UUID must be fresh per record");
                    assert_eq!(line[begin + 32], b'"');
                    let mut result = line.to_vec();
                    result[begin..begin + 32].fill(b'0');
                    return result;
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("source collector record has no generated top-level id")
}

struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn poll(&mut self) -> Option<ExitStatus> {
        let status = self.0.as_mut().unwrap().try_wait().unwrap();
        if status.is_some() {
            self.0 = None;
        }
        status
    }
    fn wait(&mut self, deadline: Instant) -> ExitStatus {
        loop {
            if let Some(status) = self.poll() {
                return status;
            }
            assert!(Instant::now() < deadline, "owned collector did not stop");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Never signal an identity after try_wait has reaped it.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn receive(subscriber: &mut Subscriber, expected: &[u8], error: bool) {
    let packet = subscriber
        .receive(Duration::from_secs(2))
        .unwrap()
        .expect("real source publication absent");
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
        _ => panic!("wrong source collector union arm"),
    };
    assert_eq!(text.as_bytes(), expected);
}

#[test]
fn real_source_collector_keeps_original_bytes_levels_and_interrupt_cleanup() {
    let _: fn(&str, &Path) -> Result<()> = managed_entry_collector_source::run;
    assert_eq!(
        format!("{:x}", Sha256::digest(ORIGINAL)),
        "4551aa5f64ff22118ce35d29888b9bc6b2be09404df1331609fac9374b747748"
    );
    let original: Value = serde_json::from_slice(ORIGINAL).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let occupied = temporary.path().join("occupied");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("swaglog.0000000000"), b"untouched").unwrap();
    let worker = std::env::var_os("MANAGED_ENTRY_WORKER_BINARY").expect("prerequisite: supply the freshly built check-managed-entry worker in MANAGED_ENTRY_WORKER_BINARY");
    let namespace = tempfile::Builder::new()
        .prefix("msgq_msrc-")
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
    let endpoint_file = temporary.path().join("socket");
    let endpoint = format!("ipc://{}", endpoint_file.display());
    let root = temporary.path().join("log");
    let previous = std::env::var_os("OPENPILOT_PREFIX");
    std::env::set_var("OPENPILOT_PREFIX", name);
    let empty_target = temporary.path().join("empty-target");
    fs::create_dir(&empty_target).unwrap();
    let symlink_root = temporary.path().join("linked-root");
    std::os::unix::fs::symlink(&empty_target, &symlink_root).unwrap();
    for (label, rejected_root) in [("occupied", &occupied), ("symlink", &symlink_root)] {
        let mut refused = OwnedChild(Some(
            Command::new(&worker)
                .args(["--managed-collector", &endpoint])
                .arg(rejected_root)
                .env("OPENPILOT_PREFIX", name)
                .stdout(Stdio::null())
                .stderr(Stdio::from(
                    File::create(temporary.path().join(format!("{label}.stderr"))).unwrap(),
                ))
                .spawn()
                .unwrap(),
        ));
        let status = refused.wait(Instant::now() + Duration::from_secs(2));
        assert!(!status.success(), "unsupported pre-existing state admitted");
        assert!(!endpoint_file.exists(), "rejected root opened ZMQ");
        assert_eq!(fs::read_dir(namespace.path()).unwrap().count(), 0);
    }
    assert_eq!(
        fs::read(occupied.join("swaglog.0000000000")).unwrap(),
        b"untouched"
    );
    assert_eq!(fs::read_dir(&occupied).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&empty_target).unwrap().count(), 0);
    let mut child = OwnedChild(Some(
        Command::new(&worker)
            .args(["--managed-collector", &endpoint])
            .arg(&root)
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
    let deadline = Instant::now() + Duration::from_secs(20);
    while !["logMessage", "errorLogMessage"]
        .iter()
        .all(|topic| namespace.path().join(topic).is_file())
        || !endpoint_file.exists()
    {
        assert!(
            child.poll().is_none(),
            "source worker exited: {}",
            fs::read_to_string(temporary.path().join("stderr")).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "source collector readiness timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut log = Subscriber::for_runtime("logMessage", false, CAPACITY).unwrap();
    let mut errors = Subscriber::for_runtime("errorLogMessage", false, CAPACITY).unwrap();
    let context = zmq::Context::new();
    let sender = context.socket(zmq::PUSH).unwrap();
    sender.set_linger(0).unwrap();
    sender.set_sndtimeo(2000).unwrap();
    sender.connect(&endpoint).unwrap();
    // The unchanged original Peer synchronizes its real publisher with a
    // debug marker before admitted test records; queue-file existence alone
    // does not prove that publisher construction has finished resetting readers.
    let marker = format!("{{\"msg\":\"ready-{name}\"}}");
    let ready_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            child.poll().is_none(),
            "source worker exited during readiness"
        );
        assert!(
            Instant::now() < ready_deadline,
            "source readiness marker absent"
        );
        sender
            .send_multipart([&[10][..], marker.as_bytes()], 0)
            .unwrap();
        if let Some(packet) = log.receive(Duration::from_millis(50)).unwrap() {
            let message = capnp::serialize::read_message(
                &mut Cursor::new(packet),
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            let value = message.get_root::<event::Reader>().unwrap();
            assert!(value.get_valid());
            match value.which().unwrap() {
                event::LogMessage(text) => assert_eq!(text.unwrap().as_bytes(), marker.as_bytes()),
                _ => panic!("wrong readiness union arm"),
            }
            break;
        }
    }
    assert!(errors.receive(Duration::ZERO).unwrap().is_none());
    let mut expected_disk = Vec::new();
    let valid = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row.get("result_hex").is_some())
        .collect::<Vec<_>>();
    assert_eq!(
        valid.len(),
        14,
        "all unchanged original successful formatter inputs"
    );
    for row in &valid {
        let input = unhex(row["input_hex"].as_str().unwrap());
        sender
            .send_multipart(
                [
                    &[][..],
                    &[20],
                    &input[..input.len() / 2],
                    &input[input.len() / 2..],
                ],
                0,
            )
            .unwrap();
        receive(&mut log, &input, false);
        assert!(errors.receive(Duration::ZERO).unwrap().is_none());
        expected_disk.extend(unhex(row["result_hex"].as_str().unwrap()));
        expected_disk.push(b'\n');
    }
    let input = unhex(valid[0]["input_hex"].as_str().unwrap());
    for level in [19, 39, 40, 255] {
        sender.send_multipart([&[level][..], &input], 0).unwrap();
        receive(&mut log, &input, false);
        if level >= 40 {
            receive(&mut errors, &input, true);
        } else {
            assert!(errors.receive(Duration::ZERO).unwrap().is_none());
        }
        if level >= 20 {
            expected_disk.extend(unhex(valid[0]["result_hex"].as_str().unwrap()));
            expected_disk.push(b'\n');
        }
    }
    let pid = rustix::process::Pid::from_raw(child.0.as_ref().unwrap().id() as i32).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::INT).unwrap();
    let status = child.wait(Instant::now() + Duration::from_secs(3));
    assert_eq!(
        status.signal(),
        Some(2),
        "unchanged original SIGINT exit is -2"
    );
    let raw = fs::read(root.join("swaglog.0000000000")).unwrap();
    let mut ids = BTreeSet::new();
    let observed = raw
        .split_inclusive(|byte| *byte == b'\n')
        .flat_map(|line| normalize_id(line, &mut ids))
        .collect::<Vec<_>>();
    assert_eq!(
        observed, expected_disk,
        "all original formatting bytes and record order"
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    drop(sender);
    drop(context);
    drop(log);
    drop(errors);
    if let Some(previous) = previous {
        std::env::set_var("OPENPILOT_PREFIX", previous);
    } else {
        std::env::remove_var("OPENPILOT_PREFIX");
    }
}
