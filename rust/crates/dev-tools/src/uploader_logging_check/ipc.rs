use super::{bounded, disk_projection, require, Result};
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::{Publisher, Subscriber};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const BIG: usize = 10 << 20;
const TOPICS: [&str; 2] = ["logMessage", "errorLogMessage"];
struct OwnedChild {
    child: Option<Child>,
    status: Option<i32>,
}
impl OwnedChild {
    fn observe(&mut self) -> Result<Option<i32>> {
        if let Some(child) = &mut self.child {
            if let Some(status) = child.try_wait()? {
                self.status = Some(
                    status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                );
                self.child = None;
            }
        }
        Ok(self.status)
    }
    fn alive(&mut self) -> Result<()> {
        require(
            self.observe()?.is_none(),
            "uploader collector exited before completion",
        )
    }
    fn stop(&mut self) -> Result<i32> {
        if let Some(child) = &self.child {
            rustix::process::kill_process(
                rustix::process::Pid::from_child(child),
                rustix::process::Signal::INT,
            )?;
        }
        let start = Instant::now();
        loop {
            if let Some(code) = self.observe()? {
                return Ok(code);
            }
            require(
                start.elapsed() < Duration::from_secs(10),
                "uploader collector shutdown timeout",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Namespace {
    shm: PathBuf,
    endpoint: PathBuf,
}
impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.shm);
        let _ = fs::remove_file(&self.endpoint);
    }
}
pub(super) struct Peer {
    child: OwnedChild,
    log: Subscriber,
    error: Subscriber,
    _device: Publisher,
    _context: zmq::Context,
    sender: zmq::Socket,
    output: PathBuf,
    disk: PathBuf,
    pub endpoint: String,
    records: Vec<Value>,
    errors: Vec<Value>,
    _namespace: Namespace,
}
fn decode(packet: &[u8], topic: &str) -> Result<(Value, String)> {
    require(
        packet.len() <= 2 << 20,
        "uploader log publication exceeds bound",
    )?;
    let message = capnp::serialize::read_message(Cursor::new(packet), Default::default())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    require(root.get_valid(), "invalid uploader logging cereal event")?;
    let raw = match (topic, root.which()?) {
        ("logMessage", event::LogMessage(value))
        | ("errorLogMessage", event::ErrorLogMessage(value)) => value?,
        _ => return Err("uploader log publication union differs".into()),
    }
    .to_str()?
    .to_owned();
    Ok((serde_json::from_str(&raw)?, raw))
}
impl Peer {
    pub(super) fn start(
        collector: &Path,
        worker: &Path,
        output: &Path,
        source: bool,
        prefix: &str,
    ) -> Result<Self> {
        require(
            std::env::var("OPENPILOT_PREFIX").ok().as_deref() == Some(prefix),
            "logging worker IPC prefix differs",
        )?;
        let shm = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        let endpoint_path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        require(
            matches!(fs::symlink_metadata(&endpoint_path),Err(e) if e.kind()==std::io::ErrorKind::NotFound),
            "owned logging endpoint already exists",
        )?;
        fs::create_dir(&shm)?;
        let namespace = Namespace {
            shm: shm.clone(),
            endpoint: endpoint_path,
        };
        let endpoint = format!("ipc:///tmp/logmessage{prefix}");
        let disk = output.join("disk");
        let mut command = if source {
            let mut c = Command::new(worker);
            c.arg("--uploader-source-collector")
                .arg(&endpoint)
                .arg(&disk);
            c
        } else {
            let mut c = Command::new(collector);
            c.arg("--endpoint")
                .arg(&endpoint)
                .arg("--log-root")
                .arg(&disk);
            c
        };
        // The outer QA worker owns the process group. Children stay in it so
        // its timeout cleanup also contains a collector during partial startup.
        command
            .stdout(Stdio::from(File::create(output.join("collector.stdout"))?))
            .stderr(Stdio::from(File::create(output.join("collector.stderr"))?));
        let mut child = OwnedChild {
            child: Some(command.spawn()?),
            status: None,
        };
        let start = Instant::now();
        loop {
            child.alive()?;
            let full = TOPICS.iter().all(|name| {
                fs::metadata(shm.join(name)).is_ok_and(|m| m.is_file() && m.len() > BIG as u64)
            });
            if full {
                break;
            }
            require(
                start.elapsed() < Duration::from_secs(20),
                "collector queues missing or below original full capacity",
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
        let log = Subscriber::for_runtime("logMessage", false, BIG)?;
        let error = Subscriber::for_runtime("errorLogMessage", false, BIG)?;
        // services.py defaults deviceState to QueueSize.SMALL (250 KiB).
        let device = Publisher::for_runtime("deviceState", 250 * 1024)?;
        let context = zmq::Context::new();
        let sender = context.socket(zmq::PUSH)?;
        sender.set_linger(0)?;
        sender.set_sndtimeo(10000)?;
        sender.connect(&endpoint)?;
        Ok(Self {
            child,
            log,
            error,
            _device: device,
            _context: context,
            sender,
            output: output.into(),
            disk,
            endpoint,
            records: Vec::new(),
            errors: Vec::new(),
            _namespace: namespace,
        })
    }
    pub(super) fn synchronize(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut sequence = 0;
        while start.elapsed() < Duration::from_secs(20) {
            self.child.alive()?;
            sequence += 1;
            let marker =
                serde_json::to_string(&json!({"msg":format!("uploader-ready-{sequence}")}))?;
            self.sender
                .send_multipart([&[10_u8][..], marker.as_bytes()], 0)?;
            let retry = Instant::now();
            while retry.elapsed() < Duration::from_millis(50) {
                if let Some(packet) = self.log.receive(Duration::ZERO)? {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(self.output.join("readiness.bin"))?
                        .write_all(&packet)?;
                    let (_, raw) = decode(&packet, "logMessage")?;
                    if raw == marker {
                        require(
                            self.error.receive(Duration::ZERO)?.is_none(),
                            "unexpected error during collector readiness",
                        )?;
                        fs::write(
                            self.output.join("readiness.json"),
                            serde_json::to_vec(&json!({"marker":marker,"attempts":sequence}))?,
                        )?;
                        return Ok(());
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        Err("collector did not acknowledge uploader readiness".into())
    }
    fn drain(&mut self) -> Result<bool> {
        let mut received = false;
        for (index, topic) in TOPICS.iter().enumerate() {
            loop {
                let packet = if index == 0 {
                    self.log.receive(Duration::ZERO)?
                } else {
                    self.error.receive(Duration::ZERO)?
                };
                let Some(packet) = packet else {
                    break;
                };
                received = true;
                let (record, _) = decode(&packet, topic)?;
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.output.join(format!("{topic}.bin")))?
                    .write_all(&packet)?;
                if index == 0 {
                    self.records.push(record);
                } else {
                    self.errors.push(record);
                }
            }
        }
        Ok(received)
    }
    pub(super) fn finish(mut self) -> Result<(Vec<Value>, Vec<Value>, usize, i32)> {
        let mut last = Instant::now();
        while last.elapsed() < Duration::from_millis(250) {
            self.child.alive()?;
            if self.drain()? {
                last = Instant::now();
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let code = self.child.stop()?;
        self.drain()?;
        let mut files = fs::read_dir(&self.disk)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        files.sort();
        let mut actual = Vec::new();
        for file in files {
            if file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with("swaglog."))
            {
                let bytes = bounded(&file, 8 << 20)?;
                for line in std::str::from_utf8(&bytes)?.lines() {
                    let mut record: Value = serde_json::from_str(line)?;
                    let id = record["id"].as_str().ok_or("disk UUID absent")?;
                    require(
                        id.len() == 32
                            && id.bytes().all(|b| b.is_ascii_hexdigit())
                            && id.as_bytes()[12] == b'4'
                            && matches!(id.as_bytes()[16], b'8' | b'9' | b'a' | b'b'),
                        "disk UUID is not RFC4122 v4",
                    )?;
                    record
                        .as_object_mut()
                        .ok_or("disk log is not an object")?
                        .remove("id");
                    actual.push(record);
                }
            }
        }
        let expected = self
            .records
            .iter()
            .filter(|r| r["levelnum"].as_u64().unwrap_or(0) >= 20)
            .map(disk_projection)
            .collect::<Result<Vec<_>>>()?;
        require(
            actual == expected,
            "actual collector disk formatting differs from independent original formatter",
        )?;
        Ok((
            std::mem::take(&mut self.records),
            std::mem::take(&mut self.errors),
            actual.len(),
            code,
        ))
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
