//! Independent `logmessaged_reference.run_original` adapter for managed QA.
//!
//! Policy is derived from system/logmessaged.py, SwagLogFileFormatter and
//! SwaglogRotatingFileHandler. Only JSON lexical mechanics and transport ABI
//! are shared; neither the candidate collector nor its formatter/LogFiles runs.
//! Existing log directories are deliberately outside this fresh-worker adapter.
use crate::seal::Result;
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::{JsonValue, JsonView};
use openpilot_msgq::{Publisher, Subscriber};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const CAPACITY: usize = 10 * 1024 * 1024; // original services.py QueueSize.BIG
const PUBLISH_CHARACTERS: usize = 2 * 1024 * 1024;
const ROTATE_BYTES: u64 = 256 * 1024;
const ROTATE_INTERVAL: Duration = Duration::from_secs(60);
const RETAIN_FILES: usize = 2500;

fn key_is(key: &[u32], expected: &str) -> bool {
    key.iter().copied().eq(expected.chars().map(u32::from))
}
fn quoted(key: &[u32]) -> Result<String> {
    Ok(JsonValue::codepoints(key.to_vec())
        .ok_or("invalid Python Unicode key")?
        .to_json()?)
}
fn fixed_key(key: &[u32], value: &JsonValue) -> Vec<u32> {
    let suffix = match value.view() {
        JsonView::Text(_) => "$s",
        JsonView::Float(_) => "$f",
        JsonView::Bool(_) => "$b",
        JsonView::Integer(_) => "$i",
        JsonView::Array(_) => "$a",
        JsonView::Null | JsonView::Object(_) => "",
    };
    key.iter()
        .copied()
        .chain(suffix.chars().map(u32::from))
        .collect()
}
fn insert<T>(fields: &mut Vec<(Vec<u32>, T)>, key: Vec<u32>, value: T) {
    if let Some((_, old)) = fields.iter_mut().find(|(existing, _)| *existing == key) {
        *old = value;
    } else {
        fields.push((key, value));
    }
}

/// Only dictionaries recurse in fix_kv; dictionaries inside arrays are intact.
/// A flat arena avoids using the Rust call stack for untrusted JSON nesting.
fn message_json(value: JsonValue) -> Result<String> {
    enum Node {
        Original(JsonValue),
        Object(Vec<(Vec<u32>, usize)>),
    }
    let mut nodes = vec![Node::Original(value.clone())];
    let mut pending = vec![(0, value)];
    while let Some((index, value)) = pending.pop() {
        if let JsonView::Object(fields) = value.view() {
            let mut changed = Vec::new();
            for (key, child) in fields {
                let destination = nodes.len();
                nodes.push(Node::Original(child.clone()));
                insert(&mut changed, fixed_key(key, &child), destination);
                if child.is_object() {
                    pending.push((destination, child));
                }
            }
            nodes[index] = Node::Object(changed);
        }
    }
    enum Part<'a> {
        Node(usize),
        Key(&'a [u32]),
        Token(&'static str),
    }
    let mut parts = vec![Part::Node(0)];
    let mut output = String::new();
    while let Some(part) = parts.pop() {
        match part {
            Part::Key(key) => output.push_str(&quoted(key)?),
            Part::Token(token) => output.push_str(token),
            Part::Node(index) => match &nodes[index] {
                Node::Original(value) => output.push_str(&value.to_json()?),
                Node::Object(fields) => {
                    output.push('{');
                    parts.push(Part::Token("}"));
                    for (index, (key, child)) in fields.iter().enumerate().rev() {
                        parts.push(Part::Node(*child));
                        parts.push(Part::Token(": "));
                        parts.push(Part::Key(key));
                        if index > 0 {
                            parts.push(Part::Token(", "));
                        }
                    }
                }
            },
        }
    }
    Ok(output)
}

fn formatted(record: &str, id: impl FnOnce() -> Result<String>) -> Result<String> {
    let parsed = JsonValue::parse(record)?;
    let JsonView::Object(fields) = parsed.view() else {
        return Err("source file formatter requires an object containing msg".into());
    };
    let msg = fields
        .iter()
        .find(|(key, _)| key_is(key, "msg"))
        .map(|(_, value)| value.clone())
        .ok_or("source file formatter missing msg")?;
    let mut output = Vec::new();
    for (key, value) in fields {
        if !key_is(key, "msg") {
            output.push((key.to_vec(), value.to_json()?));
        }
    }
    // Python deletes msg, then assigns the transformed key and id. Assignment
    // replaces an existing key in place, including type-suffixed collisions.
    insert(
        &mut output,
        fixed_key(&"msg".chars().map(u32::from).collect::<Vec<_>>(), &msg),
        message_json(msg)?,
    );
    insert(
        &mut output,
        "id".chars().map(u32::from).collect(),
        JsonValue::text(&id()?).to_json()?,
    );
    let mut result = String::from("{");
    for (index, (key, value)) in output.iter().enumerate() {
        if index > 0 {
            result.push_str(", ");
        }
        result.push_str(&quoted(key)?);
        result.push_str(": ");
        result.push_str(value);
    }
    result.push('}');
    Ok(result)
}

fn uuid4() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

struct Files {
    root: PathBuf,
    stream: Option<File>,
    position: u64,
    index: u64,
    opened_at: Duration,
    names: VecDeque<PathBuf>,
}
impl Files {
    fn new(root: &Path) -> Result<Self> {
        let mut files = Self {
            root: root.to_owned(),
            stream: None,
            position: 0,
            index: 0,
            opened_at: Duration::ZERO,
            names: VecDeque::new(),
        };
        files.open(Duration::ZERO)?;
        Ok(files)
    }
    fn open(&mut self, now: Duration) -> Result<()> {
        self.stream = None; // close the old file before opening the new index
        let path = self.root.join(format!("swaglog.{:010}", self.index));
        let stream = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.index = self
            .index
            .checked_add(1)
            .ok_or("source log index overflow")?;
        self.position = 0;
        self.opened_at = now;
        self.stream = Some(stream);
        self.names.push_back(path);
        while self.names.len() > RETAIN_FILES {
            let old = self
                .names
                .pop_front()
                .ok_or("source rotation history absent")?;
            match fs::remove_file(old) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn emit(&mut self, record: &str, now: Duration) -> Result<()> {
        // Original BaseRotatingHandler checks rollover before formatting. An
        // invalid record can therefore leave a newly rotated empty file.
        if self.position >= ROTATE_BYTES || now.saturating_sub(self.opened_at) >= ROTATE_INTERVAL {
            self.open(now)?;
        }
        let line = formatted(record, uuid4)?;
        let stream = self.stream.as_mut().ok_or("source log stream absent")?;
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        self.position = self
            .position
            .checked_add(line.len() as u64 + 1)
            .ok_or("source log position overflow")?;
        Ok(())
    }
}

fn fresh_directory(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || fs::read_dir(root)?.next().transpose()?.is_some()
            {
                return Err(
                    "source collector requires an absent or empty real private log directory"
                        .into(),
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(root)?,
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn publish(publisher: &mut Publisher, record: &str, error: bool) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut root = message.init_root::<event::Builder>();
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        root.set_log_mono_time(now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64);
        root.set_valid(true);
        if error {
            root.set_error_log_message(record);
        } else {
            root.set_log_message(record);
        }
    }
    let bytes = capnp::serialize::write_message_to_words(&message);
    // Original msgq_msg_send asserts that three aligned messages fit its queue.
    // In particular, 2Mi Unicode characters can exceed this byte boundary even
    // though main's character-count guard admits them (independent capture).
    let aligned = bytes
        .len()
        .checked_add(15)
        .ok_or("source message length overflow")?
        & !7;
    if aligned
        .checked_mul(3)
        .ok_or("source message size overflow")?
        > CAPACITY
    {
        std::process::abort();
    }
    publisher.send(&bytes)?;
    Ok(())
}

/// Run only in the isolated managed source-collector worker. The caller owns its
/// private IPC namespace and endpoint cleanup. SIGINT closes collector resources
/// then reproduces the original signal exit; it is not a successful return.
pub fn run(endpoint: &str, root: &Path) -> Result<()> {
    run_inner(endpoint, root, None)
}

/// Reproduce the readiness checker's subscriber-before-publisher schedule.
/// The parent owns a private listener. `R` reports that the dummy subscriber
/// exists; parent `1` releases publisher construction, acknowledged with `1`.
pub fn run_gated(endpoint: &str, root: &Path, gate_path: &Path) -> Result<()> {
    run_inner(endpoint, root, Some(gate_path))
}

fn run_inner(endpoint: &str, root: &Path, gate_path: Option<&Path>) -> Result<()> {
    fresh_directory(root)?;
    let clock = Instant::now();
    let mut files = Files::new(root)?;
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PULL)?;
    socket.bind(endpoint)?;
    // Wake only to observe the process signal flag, including delivery on a
    // ZMQ I/O thread. This does not add an external receive deadline or retry.
    socket.set_rcvtimeo(100)?;
    let mut log = Publisher::for_runtime("logMessage", CAPACITY)?;
    // Retain the original wrapper's queue_owner throughout collection. Its
    // reader is deliberately invalidated by the subsequent publisher reset.
    let queue_owner = gate_path
        .map(|_| Subscriber::for_runtime("errorLogMessage", false, CAPACITY))
        .transpose()?;
    let mut gate = if let Some(path) = gate_path {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        stream.write_all(b"R")?;
        let mut release = [0];
        stream.read_exact(&mut release)?;
        if release != *b"1" {
            return Err("source collector publisher initialization gate closed".into());
        }
        Some(stream)
    } else {
        None
    };
    let mut errors = Publisher::for_runtime("errorLogMessage", CAPACITY)?;
    if let Some(stream) = &mut gate {
        stream.write_all(b"1")?;
    }
    drop(gate);
    let interrupted = Arc::new(AtomicBool::new(false));
    let registration =
        signal_hook::flag::register(signal_hook::consts::SIGINT, interrupted.clone())?;
    let result = (|| {
        while !interrupted.load(Ordering::Relaxed) {
            let parts = match socket.recv_multipart(0) {
                Ok(parts) => parts,
                Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => continue,
                Err(error) => return Err(error.into()),
            };
            let bytes = parts.concat();
            let (&level, body) = bytes
                .split_first()
                .ok_or("source collector empty multipart input")?;
            let record = String::from_utf8_lossy(body);
            if level >= 20 {
                files.emit(&record, clock.elapsed())?;
            }
            let characters = record.chars().count();
            if characters > PUBLISH_CHARACTERS {
                println!("WARNING: log too big to publish {characters}");
                println!("{}", record.chars().take(100).collect::<String>());
                continue;
            }
            publish(&mut log, &record, false)?;
            if level >= 40 {
                publish(&mut errors, &record, true)?;
            }
        }
        Ok(())
    })();
    drop(socket);
    drop(context);
    drop(files);
    drop(log);
    drop(errors);
    drop(queue_owner);
    signal_hook::low_level::unregister(registration);
    if interrupted.load(Ordering::Relaxed) {
        signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGINT)?;
        return Err("default SIGINT action unexpectedly returned".into());
    }
    result
}

/// Retained shared source closure. This adapter does not retire these helpers;
/// absence, changed content or an unreadable source is always an error.
pub fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    [
        (
            "rust/tools/logmessaged_reference.py",
            "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
        ),
        (
            "rust/tools/logmessaged_native.py",
            "3366e64eb36b0bad9b10023c46217f61c66aae0fe823120ae23d597d2456c872",
        ),
        (
            "openpilot/system/logmessaged.py",
            "93d94801185350ef4470bd482d37ed7f6d466e9b100ad10aa16fa6e8570fb02a",
        ),
        (
            "openpilot/common/logging_extra.py",
            "ce9b3b3d8498d76e1c4c0352a3e98e72759bb7f367ff8629c1329c46040595c3",
        ),
        (
            "openpilot/common/swaglog.py",
            "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
        ),
        (
            "openpilot/cereal/services.py",
            "a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb",
        ),
        (
            "openpilot/cereal/messaging/__init__.py",
            "1938c80c71dba0855700015bc03371786a711c67bfe3a0c5e073ed790d5e10a9",
        ),
        (
            "openpilot/cereal/log.capnp",
            "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
        ),
        (
            "msgq_repo/msgq/msgq.cc",
            "5910ffb3bc2a4f023b4bbc612e41b76ddeaace2eb84ae8b127e8736905d7a6c2",
        ),
    ]
    .into_iter()
    .map(|(name, expected)| {
        let hash = crate::seal::digest(&repository.join(name))?;
        if hash != expected {
            return Err(format!("changed managed collector source {name}").into());
        }
        Ok((name.to_owned(), hash))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    const ZERO: &str = "00000000000000000000000000000000";
    fn fixture(name: &str, expected: &str) -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/managed-entry-collector")
            .join(name);
        let raw = fs::read(path).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&raw)), expected);
        serde_json::from_slice(&raw).unwrap()
    }
    fn unhex(text: &str) -> Vec<u8> {
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn all_original_formatter_inputs_keep_bytes_and_error_before_entropy_phase() {
        let original = fixture(
            "format-original.json",
            "4551aa5f64ff22118ce35d29888b9bc6b2be09404df1331609fac9374b747748",
        );
        assert_eq!(original["cases"].as_array().unwrap().len(), 18);
        for row in original["cases"].as_array().unwrap() {
            let input = String::from_utf8(unhex(row["input_hex"].as_str().unwrap())).unwrap();
            let entropy = std::cell::Cell::new(false);
            let output = formatted(&input, || {
                entropy.set(true);
                Ok(ZERO.into())
            });
            if let Some(expected) = row.get("result_hex") {
                assert_eq!(
                    output.unwrap().as_bytes(),
                    unhex(expected.as_str().unwrap()),
                    "{}",
                    row["name"]
                );
                assert!(entropy.get());
            } else {
                assert!(output.is_err(), "{}", row["name"]);
                assert!(
                    !entropy.get(),
                    "UUID must follow parsing/msg transformation"
                );
            }
        }
    }
    fn snapshot(root: &Path) -> BTreeMap<String, String> {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let mut output = String::new();
                for line in fs::read_to_string(&path).unwrap().split_inclusive('\n') {
                    let parsed = JsonValue::parse(line).unwrap();
                    let id = parsed.get("id").unwrap().to_utf8().unwrap();
                    assert_eq!(id.len(), 32);
                    output.push_str(&line.replace(&format!("\"{id}\""), &format!("\"{ZERO}\"")));
                }
                (
                    path.file_name().unwrap().to_str().unwrap().to_owned(),
                    output,
                )
            })
            .collect()
    }
    #[test]
    fn original_prewrite_size_interval_and_full_default_retention_stay_exact() {
        let original = fixture(
            "rotation-original.json",
            "4ef9573ca652429c53c91c8a73d3ae7d97ec981608206f45d82d20a7ec6e8124",
        );
        let directory = tempfile::tempdir().unwrap();
        let mut files = Files::new(directory.path()).unwrap();
        files.emit(r#"{"msg":"first"}"#, Duration::ZERO).unwrap();
        files
            .emit(r#"{"msg":"before-interval"}"#, Duration::from_millis(59999))
            .unwrap();
        assert_eq!(
            serde_json::to_value(snapshot(directory.path())).unwrap(),
            original["before_interval"]
        );
        files
            .emit(r#"{"msg":"interval"}"#, Duration::from_secs(60))
            .unwrap();
        assert_eq!(
            serde_json::to_value(snapshot(directory.path())).unwrap(),
            original["after_interval"]
        );
        files
            .emit(
                &format!(r#"{{"msg":"{}"}}"#, "x".repeat(262144)),
                Duration::from_secs(60),
            )
            .unwrap();
        let sizes = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            serde_json::to_value(sizes).unwrap(),
            original["large_file_bytes_before_next_emit"]
        );
        files
            .emit(r#"{"msg":"after-size"}"#, Duration::from_secs(60))
            .unwrap();
        let hashes = snapshot(directory.path()).into_iter().map(|(name,text)| {
            (name,json!({"normalized_utf8_bytes":text.len(),"normalized_sha256":format!("{:x}",Sha256::digest(text.as_bytes()))}))
        }).collect::<BTreeMap<_,_>>();
        assert_eq!(
            serde_json::to_value(hashes).unwrap(),
            original["after_size"]
        );
        drop(files);
        let retention = tempfile::tempdir().unwrap();
        let mut files = Files::new(retention.path()).unwrap();
        for index in 0..2502 {
            files
                .emit(
                    r#"{"msg":"retention"}"#,
                    Duration::from_secs(61 * (index + 1)),
                )
                .unwrap();
        }
        drop(files);
        let mut names = fs::read_dir(retention.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        let actual = json!({"count":names.len(),"first":names.first().unwrap(),"last":names.last().unwrap(),"filenames_sha256":format!("{:x}",Sha256::digest((names.join("\n")+"\n").as_bytes()))});
        assert_eq!(actual, original["retention"]);
    }
    #[test]
    fn every_retained_source_pin_rejects_change_absence_and_dangling_link() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let pinned = sources(&repository).unwrap();
        assert_eq!(pinned.len(), 9);
        let copy = tempfile::tempdir().unwrap();
        for name in pinned.keys() {
            let path = copy.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(repository.join(name), path).unwrap();
        }
        assert_eq!(sources(copy.path()).unwrap(), pinned);
        for name in pinned.keys() {
            let path = copy.path().join(name);
            let original = fs::read(&path).unwrap();
            fs::write(&path, [original.as_slice(), b"\n"].concat()).unwrap();
            assert!(sources(copy.path()).is_err(), "changed {name}");
            fs::remove_file(&path).unwrap();
            assert!(sources(copy.path()).is_err(), "absent {name}");
            std::os::unix::fs::symlink("missing-retained-source", &path).unwrap();
            assert!(sources(copy.path()).is_err(), "dangling {name}");
            fs::remove_file(&path).unwrap();
            fs::write(&path, original).unwrap();
        }
        assert_eq!(sources(copy.path()).unwrap(), pinned);
    }
}
