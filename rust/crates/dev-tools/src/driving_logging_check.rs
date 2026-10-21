//! Native collector validation for the unchanged modeld logging path.
//! This supplements the original-only model observation; it does not claim
//! that the linked ENOTSOCK/EAGAIN failure family has retired.
use crate::seal::Result;
use capnp::{serialize, text};
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::Subscriber;
use rustix::{
    process::{kill_process, kill_process_group, Pid, Signal},
    time::{clock_gettime, ClockId},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const STARTUP: Duration = Duration::from_secs(10);
const SHUTDOWN: Duration = Duration::from_secs(5);
const DRAIN: Duration = Duration::from_millis(300);
const CAPACITY: usize = 10 * 1024 * 1024;
const TOPICS: [&str; 2] = ["logMessage", "errorLogMessage"];
const METRICS: [&str; 9] = [
    "camera_wait_ms",
    "camera_age_at_run_ms",
    "inference_ms",
    "inference_thread_cpu_ms",
    "postprocess_ms",
    "loop_ms",
    "thread_cpu_ms",
    "dropped_frames",
    "published",
];

fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn now_ns() -> Result<u64> {
    let now = clock_gettime(ClockId::Monotonic);
    Ok(u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?)
}
fn equal_keys(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|row| {
        row.len() == names.len() && names.iter().all(|name| row.contains_key(*name))
    })
}
fn ordered_metrics(raw: &str) -> bool {
    let Some(start) = raw.find("\"metrics\": {") else {
        return false;
    };
    let source = &raw[start..];
    let mut previous = 0;
    for key in METRICS {
        let Some(position) = source.find(&format!("\"{key}\"")) else {
            return false;
        };
        if position <= previous {
            return false;
        }
        previous = position;
    }
    true
}

fn decode(packet: &[u8], topic: &str, started: u64) -> Result<(Value, String)> {
    require(packet.len() <= 2_000_000, "oversize logging publication")?;
    let message = serialize::read_message(&mut Cursor::new(packet), Default::default())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    require(root.get_valid(), "invalid logging publication")?;
    let clock = root.get_log_mono_time();
    require(
        started <= clock && clock <= now_ns()?,
        "logging publication clock outside collector lifetime",
    )?;
    let value: text::Reader<'_> = match (topic, root.which()?) {
        ("logMessage", event::LogMessage(value))
        | ("errorLogMessage", event::ErrorLogMessage(value)) => value?,
        _ => return Err("logging publication has wrong union".into()),
    };
    let raw = value.to_str()?.to_owned();
    Ok((serde_json::from_str(&raw)?, raw))
}

pub struct DrivingScenario<'a> {
    pub pid: u32,
    pub width: u64,
    pub height: u64,
    pub size: u64,
    pub mode: &'a str,
    pub frames: &'a [u64],
}

pub struct Collector {
    process: Child,
    reaped: bool,
    log: Subscriber,
    error: Subscriber,
    output: PathBuf,
    disk: PathBuf,
    source_root: PathBuf,
    source_commit: String,
    started: u64,
    records: [Vec<Value>; 2],
    raw_records: [Vec<String>; 2],
}

struct PrefixGuard(Option<OsString>);
struct StartingProcess(Option<Child>);
impl Drop for StartingProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = kill_process_group(Pid::from_child(&child), Signal::KILL);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl PrefixGuard {
    fn set(prefix: &str) -> Self {
        let previous = std::env::var_os("OPENPILOT_PREFIX");
        std::env::set_var("OPENPILOT_PREFIX", prefix);
        Self(previous)
    }
}
impl Drop for PrefixGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.0 {
            std::env::set_var("OPENPILOT_PREFIX", previous);
        } else {
            std::env::remove_var("OPENPILOT_PREFIX");
        }
    }
}

impl Collector {
    pub fn start(
        binary: &Path,
        output: &Path,
        environment: &[(OsString, OsString)],
        prefix: &str,
        source_root: &Path,
        source_commit: &str,
    ) -> Result<Self> {
        require(
            source_commit.len() == 40 && source_commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "collector requires exact source commit",
        )?;
        fs::create_dir(output)?;
        let disk = output.join("disk");
        let started = now_ns()?;
        let process = Command::new(binary)
            .arg("--log-root")
            .arg(&disk)
            .env_remove("SIMULATION")
            .env_remove("SEND_RAW_PRED")
            .env_remove("MODEL_LOG_FAULT")
            .env_remove("MODEL_LOG_FAULT_AUDIT")
            .envs(environment.iter().cloned())
            .stdout(Stdio::from(File::create(output.join("stdout.log"))?))
            .stderr(Stdio::from(File::create(output.join("stderr.log"))?))
            .process_group(0)
            .spawn()?;
        let mut starting = StartingProcess(Some(process));
        let ready = Instant::now() + STARTUP;
        let queues = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        while !TOPICS.iter().all(|name| queues.join(name).is_file()) {
            if starting
                .0
                .as_mut()
                .ok_or("collector owner lost")?
                .try_wait()?
                .is_some()
            {
                // try_wait reaped the leader. Never signal its now-stale PID.
                starting.0.take();
                return Err(
                    "model logging collector did not open both queues in ten seconds".into(),
                );
            }
            if Instant::now() >= ready {
                return Err(
                    "model logging collector did not open both queues in ten seconds".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // The runtime subscriber resolves a namespace from the process env.
        // Restore it immediately after both reader descriptors are open.
        let (log, error) = {
            let _prefix = PrefixGuard::set(prefix);
            (
                Subscriber::for_runtime(TOPICS[0], false, CAPACITY)?,
                Subscriber::for_runtime(TOPICS[1], false, CAPACITY)?,
            )
        };
        Ok(Self {
            process: starting.0.take().ok_or("collector owner lost")?,
            reaped: false,
            log,
            error,
            output: output.into(),
            disk,
            source_root: source_root.into(),
            source_commit: source_commit.into(),
            started,
            records: [Vec::new(), Vec::new()],
            raw_records: [Vec::new(), Vec::new()],
        })
    }

    fn source_site(&self, record: &Value) -> Result<()> {
        let relative = record["pathname"]
            .as_str()
            .ok_or("missing log source path")?;
        let path = Path::new(relative);
        require(
            !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_))),
            "logging source escaped checkout",
        )?;
        let file = self.source_root.join("rust").join(path);
        let rust_root = self.source_root.join("rust").canonicalize()?;
        require(
            file.canonicalize()?.starts_with(&rust_root),
            "logging source alias escaped checkout",
        )?;
        let descriptor = rustix::fs::open(
            &file,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut descriptor = File::from(descriptor);
        let metadata = descriptor.metadata()?;
        require(
            metadata.is_file() && metadata.len() <= 4_000_000,
            "logging source is not a bounded regular file",
        )?;
        let mut content = String::new();
        Read::by_ref(&mut descriptor)
            .take(4_000_001)
            .read_to_string(&mut content)?;
        require(
            content.len() as u64 == metadata.len(),
            "logging source changed while reading",
        )?;
        let line = record["lineno"].as_u64().ok_or("missing source line")?;
        let source = content
            .lines()
            .nth(
                usize::try_from(line)?
                    .checked_sub(1)
                    .ok_or("zero log line")?,
            )
            .ok_or("log line outside source")?;
        require(
            source.contains("log_site!"),
            "log site no longer occurs at recorded source line",
        )?;
        require(
            path.file_name().and_then(|name| name.to_str()) == record["filename"].as_str(),
            "log filename does not match source path",
        )
    }

    fn drain_topic(&mut self, index: usize) -> Result<()> {
        while let Some(packet) = if index == 0 {
            self.log.receive(Duration::ZERO)?
        } else {
            self.error.receive(Duration::ZERO)?
        } {
            let (record, raw) = decode(&packet, TOPICS[index], self.started)?;
            require(
                record["ctx"]["runtime_language"] == "rust",
                "non-Rust logging context",
            )?;
            require(
                record["ctx"]["source_commit"].as_str() == Some(self.source_commit.as_str()),
                "logging source commit differs from checkout",
            )?;
            require(
                record["process"] == record["thread"],
                "logging thread differs from process",
            )?;
            self.source_site(&record)?;
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.output.join(format!("{}.bin", TOPICS[index])))?
                .write_all(&packet)?;
            self.raw_records[index].push(raw);
            self.records[index].push(record);
        }
        Ok(())
    }
    pub fn drain(&mut self) -> Result<()> {
        self.drain_topic(0)?;
        self.drain_topic(1)
    }

    fn timing(&self, record: &Value, raw: &str) -> Result<bool> {
        let message = &record["msg"];
        if message["event"] != "runtimeTiming" {
            return Ok(false);
        }
        require(
            equal_keys(
                message,
                &[
                    "event",
                    "component",
                    "pid",
                    "mono_time",
                    "seconds",
                    "frames",
                    "metrics",
                    "scheduler",
                    "schedstats_enabled",
                    "backend",
                    "usbgpu",
                    "frame_id",
                ],
            ),
            "runtimeTiming field roster changed",
        )?;
        require(
            record["levelnum"] == 20
                && message["component"] == "modeld"
                && message["pid"] == record["process"]
                && message["usbgpu"] == false
                && message["backend"] == "openpilot_driving_modeld::runtime::DrivingRuntime<'_>"
                && message.get("communication").is_none(),
            "runtimeTiming source identity changed",
        )?;
        require(
            equal_keys(&message["metrics"], &METRICS) && ordered_metrics(raw),
            "runtimeTiming metric roster/order changed",
        )?;
        let frames = message["frames"]
            .as_u64()
            .ok_or("runtimeTiming frames absent")?;
        for metric in METRICS {
            let entry = &message["metrics"][metric];
            require(
                equal_keys(entry, &["mean", "max", "count"])
                    && entry["count"].as_u64() == Some(frames),
                "runtimeTiming aggregate count changed",
            )?;
        }
        require(
            message["metrics"]["published"]["max"]
                .as_i64()
                .is_some_and(|v| v == 0 || v == 1)
                && message["metrics"]["dropped_frames"]["max"]
                    .as_i64()
                    .is_some_and(|v| v >= 0),
            "runtimeTiming discrete maxima changed",
        )?;
        let value = |metric: &str| -> Result<f64> {
            Ok(message["metrics"][metric]["max"]
                .as_f64()
                .ok_or("runtimeTiming maximum not numeric")?)
        };
        require(
            0.0 <= value("inference_thread_cpu_ms")?
                && value("inference_thread_cpu_ms")? <= value("inference_ms")? + 1.0
                && 0.0 <= value("thread_cpu_ms")?
                && value("thread_cpu_ms")? <= value("loop_ms")? + 1.0,
            "runtimeTiming CPU/sample relation changed",
        )?;
        Ok(true)
    }

    fn source_calls(&self, scenario: &DrivingScenario<'_>) -> Result<Value> {
        let pid = u64::from(scenario.pid);
        let actual = self.records[0]
            .iter()
            .filter(|row| row["process"].as_u64() == Some(pid))
            .filter_map(|row| {
                row["msg"]
                    .as_str()
                    .map(|text| (row["levelnum"].as_u64().unwrap_or(0), text.to_owned()))
            })
            .collect::<Vec<_>>();
        require(
            !actual.is_empty() && actual[0] == (30, "modeld init".into()),
            "modeld init was not first source log",
        )?;
        let mut expected = vec![
            (30, "modeld init".into()),
            (30, format!("vision stream set up, use_wide_camera: True, main_wide_camera: {}, use_extra_client: {}",
                if scenario.mode == "wide" { "True" } else { "False" },
                if scenario.mode == "dual" { "True" } else { "False" })),
            (30, format!("connected main cam with buffer size: {} ({} x {})",
                scenario.size, scenario.width, scenario.height)),
            (30, "loading model".into()),
            (20, "modeld got CarParams: ".into()),
        ];
        if scenario.mode == "dual" {
            expected.push((
                30,
                format!(
                    "connected extra cam with buffer size: {} ({} x {})",
                    scenario.size, scenario.width, scenario.height
                ),
            ));
        }
        let loaded = actual
            .iter()
            .filter(|(_, text)| text.starts_with("models loaded in "))
            .collect::<Vec<_>>();
        require(loaded.len() == 1, "model load log multiplicity changed")?;
        let loading_seconds = loaded[0]
            .1
            .strip_prefix("models loaded in ")
            .and_then(|value| value.strip_suffix("s, modeld starting"))
            .ok_or("model load log shape changed")?;
        require(
            loading_seconds
                .split_once('.')
                .is_some_and(|(whole, fractional)| {
                    !whole.is_empty()
                        && whole.bytes().all(|b| b.is_ascii_digit())
                        && fractional.len() == 1
                        && fractional.bytes().all(|b| b.is_ascii_digit())
                }),
            "model load duration precision changed",
        )?;
        expected.push((*loaded[0]).clone());
        let loading = self.records[0]
            .iter()
            .find(|row| row["process"].as_u64() == Some(pid) && row["msg"] == "loading model")
            .ok_or("loading source record missing")?;
        let loaded_record = self.records[0]
            .iter()
            .find(|row| {
                row["process"].as_u64() == Some(pid)
                    && row["msg"].as_str() == Some(loaded[0].1.as_str())
            })
            .ok_or("loaded source record missing")?;
        let duration = loading_seconds.parse::<f64>()?;
        let observed = loaded_record["created"]
            .as_f64()
            .ok_or("loaded clock missing")?
            - loading["created"].as_f64().ok_or("loading clock missing")?;
        require(
            (duration - observed).abs() <= 0.2,
            "model load duration differs from source clocks",
        )?;
        let mut previous = 0_u64;
        for frame in scenario.frames {
            let dropped = frame.saturating_sub(previous + 1);
            if dropped > 0 {
                expected.push((
                    40,
                    format!("camera dropped {dropped} frames; advancing model history"),
                ));
            }
            previous = *frame;
        }
        let debug = (10, "camera pair unavailable or out of sync".to_owned());
        expected.extend(std::iter::repeat_n(
            debug.clone(),
            actual.iter().filter(|row| *row == &debug).count(),
        ));
        let counts = |rows: &[(u64, String)]| {
            let mut count = BTreeMap::new();
            for row in rows {
                *count.entry(row.clone()).or_insert(0_usize) += 1;
            }
            count
        };
        require(
            counts(&actual) == counts(&expected),
            "model source log multiset changed",
        )?;
        require(
            self.records[0]
                .iter()
                .filter(|row| row["msg"] == "got SIGINT" && row["levelnum"] == 30)
                .count()
                == 1,
            "SIGINT source log changed",
        )?;
        let startup = actual
            .iter()
            .filter(|(_, text)| {
                [
                    "connected main cam",
                    "connected extra cam",
                    "loading model",
                    "models loaded in",
                    "modeld got CarParams",
                ]
                .iter()
                .any(|prefix| text.starts_with(prefix))
            })
            .collect::<Vec<_>>();
        let mut order = vec!["connected main cam"];
        if scenario.mode == "dual" {
            order.push("connected extra cam");
        }
        order.extend(["loading model", "models loaded in", "modeld got CarParams"]);
        require(
            startup.len() == order.len()
                && startup
                    .iter()
                    .zip(order)
                    .all(|((_, text), prefix)| text.starts_with(prefix)),
            "model source startup order changed",
        )?;
        let pending_raw =
            include_bytes!("../tests/fixtures/driving-logging-source/original-pending-egpu.json");
        require(
            format!("{:x}", Sha256::digest(pending_raw))
                == "ee68d5e5af6054c49b798c7eccec1bb9dadf01657b6706bb2c713750bc737ccb",
            "original source-only pending callsite inventory changed",
        )?;
        let pending: Value = serde_json::from_slice(pending_raw)?;
        require(
            pending.as_array().is_some_and(|rows| rows.len() == 12),
            "original source-only pending callsite count changed",
        )?;
        Ok(json!({"result":"pass", "compared_records": actual.len(),
            "startup_order":"model load before CarParams",
            "compared_lines":[226,271,278,280,283,339,369,405,449,579],
            "pending_egpu_lines":pending}))
    }

    pub fn finish(&mut self, pids: &[u32], scenario: &DrivingScenario<'_>) -> Result<Value> {
        let deadline = Instant::now() + DRAIN;
        while Instant::now() < deadline {
            self.drain()?;
            std::thread::sleep(Duration::from_millis(10));
        }
        kill_process(Pid::from_child(&self.process), Signal::TERM)?;
        let end = Instant::now() + SHUTDOWN;
        let status = loop {
            if let Some(status) = self.process.try_wait()? {
                self.reaped = true;
                break status;
            }
            if Instant::now() >= end {
                return Err("collector shutdown exceeded five seconds".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        require(status.success(), "collector shutdown failed")?;
        self.drain()?;
        let records = &self.records[0];
        require(
            !records.is_empty()
                && records.iter().all(|record| {
                    record["process"]
                        .as_u64()
                        .and_then(|pid| u32::try_from(pid).ok())
                        .is_some_and(|pid| pids.contains(&pid))
                }),
            "source process roster changed",
        )?;
        let errors = records
            .iter()
            .filter(|record| record["levelnum"].as_u64().is_some_and(|n| n >= 40))
            .collect::<Vec<_>>();
        let count = |values: Vec<&Value>| -> Result<BTreeMap<String, usize>> {
            let mut result = BTreeMap::new();
            for value in values {
                *result.entry(serde_json::to_string(value)?).or_insert(0) += 1;
            }
            Ok(result)
        };
        require(
            count(errors.clone())? == count(self.records[1].iter().collect())?,
            "errorLogMessage differs from mirrored errors",
        )?;
        let mut timing = 0;
        for (record, raw) in records.iter().zip(&self.raw_records[0]) {
            timing += usize::from(self.timing(record, raw)?);
        }
        require(timing > 0, "model loop did not publish runtimeTiming")?;
        require(
            records.iter().any(|row| {
                row["levelnum"] == 10 && row["msg"] == "camera pair unavailable or out of sync"
            }) && records.iter().any(|row| {
                row["levelnum"] == 40
                    && row["msg"]
                        .as_str()
                        .is_some_and(|text| text.starts_with("camera dropped "))
            }) && records.iter().any(|row| {
                row["levelnum"] == 20
                    && row["msg"]
                        .as_str()
                        .is_some_and(|text| text.starts_with("modeld got CarParams:"))
            }),
            "model logging source event roster changed",
        )?;
        let mut disk_records = 0_usize;
        for entry in fs::read_dir(&self.disk)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("swaglog."))
            {
                require(
                    entry.file_type()?.is_file(),
                    "collector disk record not regular",
                )?;
                let metadata = entry.metadata()?;
                require(
                    metadata.len() <= 32_000_000,
                    "collector disk file exceeds bound",
                )?;
                for line in fs::read_to_string(entry.path())?.lines() {
                    let _: Value = serde_json::from_str(line)?;
                    disk_records += 1;
                }
            }
        }
        require(
            disk_records
                == records
                    .iter()
                    .filter(|record| record["levelnum"].as_u64().is_some_and(|n| n >= 20))
                    .count(),
            "collector disk/IPC record counts differ",
        )?;
        let source_calls = self.source_calls(scenario)?;
        for (index, topic) in TOPICS.iter().enumerate() {
            fs::write(
                self.output.join(format!("{topic}.json")),
                format!("{}\n", serde_json::to_string_pretty(&self.records[index])?),
            )?;
        }
        let report = json!({"result":"pass", "component":"modeld", "log_messages":records.len(),
            "error_messages":errors.len(), "runtime_timing_events":timing, "disk_records":disk_records,
            "pids":pids, "source_commit":self.source_commit, "collector_exit":status.code(),
            "source_calls":source_calls});
        fs::write(
            self.output.join("report.json"),
            format!("{}\n", serde_json::to_string_pretty(&report)?),
        )?;
        Ok(report)
    }
    pub fn records(&self, topic: &str) -> Option<&[Value]> {
        TOPICS
            .iter()
            .position(|candidate| *candidate == topic)
            .map(|index| self.records[index].as_slice())
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.process), Signal::KILL);
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}
