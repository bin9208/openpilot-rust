//! Paced actual msgq input, full-schema route capture and owned logger lifecycle.
use super::{require, save, Result};
use openpilot_msgq::Publisher;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Settings {
    pub services: Vec<String>,
    pub audio: bool,
    pub test: bool,
    pub parameters: BTreeMap<String, String>,
    pub runner: Vec<String>,
}
impl Settings {
    pub fn new(services: &[&str]) -> Self {
        Self {
            services: services.iter().map(|v| (*v).into()).collect(),
            audio: false,
            test: true,
            parameters: BTreeMap::new(),
            runner: Vec::new(),
        }
    }
}
pub(super) fn capacity(service: &str) -> Result<usize> {
    Ok(openpilot_messaging::services::lookup(service)
        .ok_or("unknown logger fixture service")?
        .queue_size)
}
pub(super) struct Owned(pub(super) Option<Child>);
impl Drop for Owned {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Owned {
    pub(super) fn status(&mut self) -> Result<Option<i32>> {
        let Some(child) = self.0.as_mut() else {
            return Err("logger was already reaped".into());
        };
        if let Some(status) = child.try_wait()? {
            self.0 = None;
            Ok(Some(
                status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)),
            ))
        } else {
            Ok(None)
        }
    }
    pub(super) fn alive(&mut self) -> Result<()> {
        require(
            self.status()?.is_none(),
            "logger exited before input completion",
        )
    }
    pub(super) fn pid(&self) -> Result<rustix::process::Pid> {
        Ok(rustix::process::Pid::from_child(
            self.0.as_ref().ok_or("logger owner absent")?,
        ))
    }
    pub(super) fn wait(&mut self, timeout: Duration) -> Result<i32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(code) = self.status()? {
                return Ok(code);
            }
            require(Instant::now() < deadline, "logger shutdown exceeded bound")?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
pub(super) struct Peer {
    child: Owned,
    pub root: PathBuf,
    pub params: PathBuf,
    pub route: String,
    pub started: Instant,
    publishers: BTreeMap<String, Publisher>,
    pub inputs: Vec<(String, Vec<u8>)>,
}
impl Peer {
    pub fn start(binary: &Path, root: &Path, settings: &Settings) -> Result<Self> {
        fs::create_dir_all(root.parent().ok_or("logger output parent missing")?)?;
        fs::create_dir(root)?;
        let prefix = std::env::var("OPENPILOT_PREFIX")?;
        require(
            prefix.starts_with("rust-probe-logger-")
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "logger worker namespace is not private",
        )?;
        let params = root.join("params").join(&prefix);
        fs::create_dir_all(&params)?;
        let mut values = BTreeMap::from([
            ("RecordRoadCam", "2"),
            ("RecordFront", "1"),
            ("RecordAudio", if settings.audio { "1" } else { "0" }),
            ("RouteCount", "42"),
            ("DongleId", "host-qa-fixture"),
            ("AccessToken", "redact-this-fixture"),
            ("GitCommit", "1111111111111111111111111111111111111111"),
            ("GitCommitDate", "source-fixture-date"),
            ("GitBranch", "host-qa"),
            ("GitRemote", "source-fixture-remote"),
            ("AthenadRecentlyViewedRoutes", "old-route"),
        ])
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect::<BTreeMap<_, _>>();
        values.extend(settings.parameters.clone());
        for (key, value) in values {
            fs::write(params.join(key), value)?;
        }
        let services = settings
            .services
            .iter()
            .map(String::as_str)
            .chain(["logMessage"])
            .collect::<BTreeSet<_>>();
        let mut publishers = BTreeMap::new();
        for service in services {
            publishers.insert(
                service.into(),
                Publisher::for_runtime(service, capacity(service)?)?,
            );
        }
        let mut command = if let Some((runner, args)) = settings.runner.split_first() {
            let mut command = Command::new(runner);
            command.args(args).arg(binary);
            command
        } else {
            Command::new(binary)
        };
        command
            .env("PARAMS_ROOT", root.join("params"))
            .env("LOG_ROOT", root.join("logs"))
            .env_remove("ROUTE_UPLOAD_QA_IPC_ROOT");
        if settings.test {
            command
                .env("LOGGERD_TEST", "1")
                .env("LOGGERD_SEGMENT_LENGTH", "60");
        } else {
            command
                .env_remove("LOGGERD_TEST")
                .env_remove("LOGGERD_SEGMENT_LENGTH");
        }
        let stdout = File::create(root.join("stdout.log"))?;
        let stderr = File::create(root.join("stderr.log"))?;
        save(
            &root.join("invocation.json"),
            &json!({"argv":settings.runner.iter().cloned().chain([binary.to_string_lossy().into_owned()]).collect::<Vec<_>>(),"binary_sha256":crate::seal::digest(binary)?,"environment":{"OPENPILOT_PREFIX":prefix,"PARAMS_ROOT":root.join("params"),"LOG_ROOT":root.join("logs")},"test_mode":settings.test,"record_audio":settings.audio}),
        )?;
        let started = Instant::now();
        let child = Owned(Some(
            command
                .stdout(stdout)
                .stderr(stderr)
                .stdin(Stdio::null())
                .spawn()?,
        ));
        let mut peer = Self {
            child,
            root: root.into(),
            params,
            route: String::new(),
            started,
            publishers,
            inputs: Vec::new(),
        };
        let path = peer.params.join("CurrentRoute");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            peer.child.alive()?;
            if path.exists() {
                break;
            }
            require(
                Instant::now() < deadline,
                "CurrentRoute readiness exceeded bound",
            )?;
            std::thread::sleep(Duration::from_millis(5));
        }
        peer.route = fs::read_to_string(path)?;
        require(
            !peer.route.is_empty()
                && peer
                    .route
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "logger route identifier contains unsafe bytes",
        )?;
        peer.barrier()?;
        Ok(peer)
    }
    pub fn segment(&self, index: usize) -> PathBuf {
        self.root
            .join("logs")
            .join(format!("{}--{index}", self.route))
    }
    pub fn publish(&mut self, service: &str, packet: &[u8], record: bool) -> Result<()> {
        self.child.alive()?;
        self.publishers
            .get_mut(service)
            .ok_or("fixture service publisher missing")?
            .send(packet)?;
        if record {
            self.inputs.push((service.into(), packet.into()));
        }
        Ok(())
    }
    pub fn wait_readers(&mut self, service: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.child.alive()?;
            if self
                .publishers
                .get_mut(service)
                .ok_or("fixture publisher absent")?
                .readers_caught_up()
            {
                return Ok(());
            }
            require(
                Instant::now() < deadline,
                "logger reader acknowledgement exceeded bound",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn send(&mut self, service: &str, packet: &[u8]) -> Result<()> {
        self.publish(service, packet, true)?;
        self.wait_readers(service)?;
        self.await_idle(Duration::from_secs(10))
    }
    pub fn barrier(&mut self) -> Result<()> {
        self.send("logMessage", &super::fixtures::barrier())
    }
    pub fn await_idle(&mut self, timeout: Duration) -> Result<()> {
        await_idle_pid(self.child.pid()?, || self.child.alive(), timeout)
    }
    pub fn signal(&self, signal: rustix::process::Signal) -> Result<()> {
        rustix::process::kill_process(self.child.pid()?, signal)?;
        Ok(())
    }
    pub fn stopped(&mut self) -> Result<()> {
        self.signal(rustix::process::Signal::STOP)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let pid = self.child.pid()?;
        loop {
            self.child.alive()?;
            match rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::STOPPED | rustix::process::WaitIdOptions::NOHANG,
            ) {
                Ok(Some(status)) => {
                    require(
                        status.stopped()
                            && status.stopping_signal()
                                == Some(rustix::process::Signal::STOP.as_raw()),
                        "logger stop notification differs",
                    )?;
                    return Ok(());
                }
                Ok(None) | Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            require(
                Instant::now() < deadline,
                "logger stop acknowledgement exceeded bound",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn wait(&mut self, timeout: Duration) -> Result<i32> {
        self.child.wait(timeout)
    }
    pub fn stop(&mut self, signal: rustix::process::Signal) -> Result<Vec<Value>> {
        self.barrier()?;
        self.signal(signal)?;
        require(
            self.wait(Duration::from_secs(20))? == 0,
            "logger did not exit successfully",
        )?;
        let logs = self.logs()?;
        for index in 0..logs.len() {
            for entry in fs::read_dir(self.segment(index))? {
                require(
                    entry?.path().extension().is_none_or(|v| v != "lock"),
                    "logger retained a lock after successful shutdown",
                )?;
            }
        }
        self.cleanup()?;
        Ok(logs)
    }
    pub fn logs(&self) -> Result<Vec<Value>> {
        let inputs = self.root.join("inputs");
        fs::create_dir_all(&inputs)?;
        let mut index = Vec::new();
        for (number, (service, packet)) in self.inputs.iter().enumerate() {
            let name = format!("{number:04}-{service}.capnp");
            fs::write(inputs.join(&name), packet)?;
            index.push(json!({"service":service,"path":name,"sha256":super::hash(packet)}));
        }
        save(&inputs.join("index.json"), &json!(index))?;
        let mut directories = fs::read_dir(self.root.join("logs"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        directories.sort_by_key(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.rsplit_once("--"))
                .and_then(|(_, n)| n.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        });
        let mut rows = Vec::new();
        for (number, directory) in directories.iter().enumerate() {
            require(
                *directory == self.segment(number),
                "logger segment sequence differs",
            )?;
            let mut row = json!({});
            for name in ["rlog", "qlog"] {
                let compressed = fs::read(directory.join(format!("{name}.zst")))?;
                require(
                    compressed.len() <= 64 << 20,
                    "compressed logger output exceeded bound",
                )?;
                let (raw, records) = crate::bootlog_artifact_check::decode_zstd(&compressed)?;
                fs::write(directory.join(format!("{name}.capnp")), raw)?;
                row[name] = records;
            }
            let mut files = fs::read_dir(directory)?
                .map(|entry| -> std::io::Result<String> {
                    entry?
                        .file_name()
                        .into_string()
                        .map_err(|_| std::io::Error::other("logger file name is not UTF-8"))
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            files.retain(|p| !p.ends_with(".capnp"));
            files.sort();
            row["files"] = json!(files);
            rows.push(row);
        }
        save(&self.root.join("decoded.json"), &json!(rows))?;
        Ok(rows)
    }
    pub fn cleanup(&mut self) -> Result<()> {
        if let Some(child) = self.child.0.as_mut() {
            child.kill()?;
            child.wait()?;
            self.child.0 = None;
        }
        Ok(())
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!(
                "logger QA cleanup: {}",
                error.to_string().chars().take(512).collect::<String>()
            );
        }
    }
}

/// Read-only complete evidence after the worker and actual logger have reaped.
pub(super) struct Evidence {
    pub root: PathBuf,
    pub route: String,
    pub inputs: Vec<(String, Vec<u8>)>,
}
impl Evidence {
    pub fn load(root: &Path, with_inputs: bool) -> Result<Self> {
        let paths = fs::read_dir(root.join("params"))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        require(
            paths.len() == 1,
            "logger evidence Params namespace count differs",
        )?;
        let route = fs::read_to_string(paths[0].join("CurrentRoute"))?;
        let mut inputs = Vec::new();
        if with_inputs {
            let index: Vec<Value> =
                serde_json::from_slice(&fs::read(root.join("inputs/index.json"))?)?;
            for row in index {
                let name = row["path"].as_str().ok_or("input packet name absent")?;
                require(
                    !name.contains('/') && !name.contains(".."),
                    "input packet path escaped evidence",
                )?;
                let packet = fs::read(root.join("inputs").join(name))?;
                require(
                    row["sha256"] == super::hash(&packet),
                    "input packet byte seal changed",
                )?;
                inputs.push((
                    row["service"]
                        .as_str()
                        .ok_or("input packet service absent")?
                        .into(),
                    packet,
                ));
            }
        }
        Ok(Self {
            root: root.into(),
            route,
            inputs,
        })
    }
    pub fn segment(&self, index: usize) -> PathBuf {
        self.root
            .join("logs")
            .join(format!("{}--{index}", self.route))
    }
    pub fn records(&self) -> Result<Vec<Value>> {
        Ok(serde_json::from_slice(&fs::read(
            self.root.join("decoded.json"),
        )?)?)
    }
}

/// The same observer is exercised against the independent busy-recipient fixture.
pub(super) fn await_idle_pid(
    pid: rustix::process::Pid,
    mut alive: impl FnMut() -> Result<()>,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let pid = pid.as_raw_pid();
    let directory = PathBuf::from(format!("/proc/{pid}"));
    let ppoll = match std::env::consts::ARCH {
        "x86_64" => "271",
        "aarch64" => "73",
        _ => return Err("unsupported logger idle observer architecture".into()),
    };
    loop {
        alive()?;
        let channel = fs::read_to_string(directory.join("wchan"))?;
        if channel.trim() == "hrtimer_nanosleep" {
            return Ok(());
        }
        if !["", "0"].contains(&channel.trim()) {
            let raw = fs::read_to_string(directory.join("syscall"))?;
            let fields = raw.split_whitespace().collect::<Vec<_>>();
            if fields.len() >= 3 && fields[0] == ppoll && fields[2] == "0x0" {
                return Ok(());
            }
        }
        require(
            Instant::now() < deadline,
            "logger did not complete processing before its next input",
        )?;
        std::thread::sleep(Duration::from_micros(100));
    }
}
