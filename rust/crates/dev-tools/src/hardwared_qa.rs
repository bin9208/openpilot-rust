//! Independent original-source QA for the Rust hardwared trace and live daemon.
//!
//! Trace expectations are admitted original Python results. Live daemon output
//! is checked against the unchanged checker contract, never captured as a golden.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Cursor, Read, Seek, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TRACE_CASES: usize = 12;
const TRACE_INPUT_ROWS: usize = 891;
const TRACE_OUTPUT_ROWS: usize = 890;

#[derive(Clone, Debug)]
pub struct TraceOptions {
    pub source_root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
    pub require_retired: bool,
}

#[derive(Clone, Debug)]
pub struct TraceCase {
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug)]
pub struct DaemonOptions {
    pub source_root: PathBuf,
    pub binary: PathBuf,
    /// Isolated runtime filesystem root passed to the daemon.
    pub root: PathBuf,
    pub output: PathBuf,
    pub launcher: PathBuf,
    pub require_retired: bool,
}

fn startup_ok() -> Value {
    json!({
        "up_to_date":true,"no_excessive_actuation":true,"not_uninstalling":true,
        "accepted_terms":true,"free_space":true,"completed_training":true,
        "not_driver_view":true,"not_taking_snapshot":true,"registered_device":true
    })
}

fn policy_steps(changes: Vec<Value>) -> Vec<Value> {
    changes.into_iter().enumerate().map(|(i, change)| {
        let mut row = json!({
            "now":1000.0 + i as f64 * 0.5,"frame":i as i64 * 5,
            "panda_updated":true,"panda_present":true,"panda_receive_time":1000.0 + i as f64 * 0.5,
            "ignition":true,"in_car":true,"offroad_temperature":60.0,"pmic_temperature":60.0,
            "startup":startup_ok(),"booted":true
        });
        if let (Some(base), Some(overrides)) = (row.as_object_mut(), change.as_object()) {
            for (key, value) in overrides { base.insert(key.clone(), value.clone()); }
        }
        row
    }).collect()
}

/// Generate the original ordered case set independently of expected outputs.
pub fn cases() -> Vec<TraceCase> {
    let mut out = Vec::with_capacity(TRACE_CASES);
    let temperatures = [40.0, 60.0, 69.9, 75.0, 80.0, 95.0, 100.0, 110.0, 70.0];
    for device in ["tici", "tizi", "mici"] {
        let mut steps = Vec::with_capacity(216);
        for ignition in [false, true, false] {
            for _ in 0..8 {
                steps.extend(
                    temperatures
                        .iter()
                        .map(|temperature| json!([temperature, ignition])),
                );
            }
        }
        out.push(TraceCase {
            name: format!("fan-{device}"),
            input: json!({"kind":"fan","device":device,"steps":steps}),
        });
    }
    let normal = policy_steps(vec![
        json!({}),
        json!({}),
        json!({}),
        json!({}),
        json!({"cycle_requested":true}),
        json!({}),
        json!({}),
        json!({}),
        json!({"ignition":false}),
        json!({}),
        json!({}),
        json!({"panda_updated":false,"panda_receive_time":990.0}),
    ]);
    out.push(TraceCase {
        name: "normal-cycle-disconnect".into(),
        input: json!({"kind":"policy","device":"tici","steps":normal}),
    });
    out.push(TraceCase { name:"panda-disconnect-timeout".into(), input:json!({"kind":"policy","device":"tici","steps":policy_steps(vec![
        json!({}),json!({}),json!({}),json!({"now":1006.0,"panda_updated":false,"panda_receive_time":1001.0}),
        json!({"now":1006.001,"panda_updated":false,"panda_receive_time":1001.0}),
    ])}) });
    let blocked = |key: &str| {
        let mut state = startup_ok();
        state[key] = Value::Bool(false);
        state
    };
    out.push(TraceCase { name:"startup-block-and-boot-latch".into(), input:json!({"kind":"policy","device":"mici","steps":policy_steps(vec![
        json!({"booted":false}),json!({"booted":false}),json!({"startup":blocked("accepted_terms")}),json!({}),
        json!({"booted":false}),json!({"startup":blocked("free_space")}),json!({"ignition":false}),
        json!({"startup":blocked("not_driver_view")}),json!({}),
    ])}) });
    let mut thermal = vec![json!({}); 4];
    thermal.extend(vec![
        json!({"offroad_temperature":100.0,"pmic_temperature":115.0});
        70
    ]);
    thermal.extend(vec![
        json!({"offroad_temperature":60.0,"pmic_temperature":60.0});
        80
    ]);
    out.push(TraceCase {
        name: "thermal-hysteresis".into(),
        input: json!({"kind":"policy","device":"mici","steps":policy_steps(thermal)}),
    });
    out.push(TraceCase { name:"offroad-danger-boundary".into(), input:json!({"kind":"policy","device":"tici","steps":policy_steps(vec![
        json!({"offroad_temperature":75.0,"ignition":false}),json!({"offroad_temperature":76.0,"ignition":false}),json!({}),json!({}),json!({}),
    ])}) });
    out.push(TraceCase {
        name: "tesla-awake-after-start".into(),
        input: json!({"kind":"policy","device":"tizi","steps":policy_steps([
        vec![json!({"tesla":true});4],vec![json!({"tesla":true,"ignition":false});5]
    ].concat())}),
    });
    out.push(TraceCase { name:"ignition-edge-between-ticks".into(), input:json!({"kind":"policy","device":"tici","steps":policy_steps(vec![
        json!({}),json!({}),json!({}),json!({"frame":11,"ignition":false}),json!({"frame":12,"ignition":false}),json!({"frame":13}),
    ])}) });
    let shutdown = json!({"now":4000.0,"ignition":false,"in_car":true,"off_ts":0.0,"started_seen":true,
        "max_offroad_minutes":1800,"disable":false,"force":false});
    let power_data = [
        (10.0, Some(12000.0), false, 10.0, json!({})),
        (20.0, Some(12000.0), false, 10.0, json!({})),
        (30.0, Some(14000.0), true, 10.0, json!({})),
        (40.0, None, false, 10.0, json!({})),
        (50.0, Some(11000.0), false, 10.0, json!({})),
        (60.0, Some(11000.0), false, -10.0, json!({})),
        (49.0, Some(11000.0), true, 10.0, json!({})),
        (
            70.0,
            Some(12000.0),
            false,
            10.0,
            json!({"force":true,"started_seen":false,"now":3600.0}),
        ),
        (
            80.0,
            Some(12000.0),
            false,
            10.0,
            json!({"force":true,"started_seen":false,"now":3600.001}),
        ),
        (
            90.0,
            Some(12000.0),
            false,
            10.0,
            json!({"force":true,"off_ts":null}),
        ),
        (
            100.0,
            Some(12000.0),
            false,
            10.0,
            json!({"max_offroad_minutes":1,"now":300.0}),
        ),
        (
            110.0,
            Some(12000.0),
            false,
            10.0,
            json!({"max_offroad_minutes":1,"now":300.001}),
        ),
        (
            120.0,
            Some(12000.0),
            false,
            10.0,
            json!({"max_offroad_minutes":1,"disable":true}),
        ),
    ];
    let steps = power_data.into_iter().map(|(now, voltage, ignition, power, changes)| {
        let mut shutdown = shutdown.clone();
        if let Some(fields) = changes.as_object() { for (key,value) in fields { shutdown[key]=value.clone(); } }
        json!({"now":now,"voltage":voltage,"ignition":ignition,"power":power,"shutdown":shutdown})
    }).collect::<Vec<_>>();
    out.push(TraceCase {
        name: "power-integration-shutdown-boundaries".into(),
        input: json!({"kind":"power","capacity":30000000.0,"steps":steps}),
    });
    out.push(TraceCase { name:"low-voltage-shutdown".into(), input:json!({"kind":"power","capacity":3000000.0,"steps":(0..30).map(|i| json!({
        "now":1000.0+i as f64,"voltage":10000.0,"ignition":false,"power":100.0,
        "shutdown":{"now":1000.0+i as f64,"ignition":false,"in_car":true,"off_ts":0.0,"started_seen":true,
          "max_offroad_minutes":1800,"disable":false,"force":false}
    })).collect::<Vec<_>>()}) });
    debug_assert_eq!(out.len(), TRACE_CASES);
    debug_assert_eq!(
        out.iter()
            .map(|c| c.input["steps"].as_array().unwrap().len())
            .sum::<usize>(),
        TRACE_INPUT_ROWS
    );
    out
}

fn compare_value(expected: &Value, actual: &Value, path: &str) -> Result<()> {
    match expected {
        Value::Object(fields) => {
            let observed = actual
                .as_object()
                .ok_or_else(|| format!("{path}: expected object"))?;
            for (key, expected) in fields {
                compare_value(
                    expected,
                    observed
                        .get(key)
                        .ok_or_else(|| format!("{path}.{key}: missing"))?,
                    &format!("{path}.{key}"),
                )?;
            }
        }
        Value::Array(values) => {
            let observed = actual
                .as_array()
                .ok_or_else(|| format!("{path}: expected array"))?;
            if values.len() != observed.len() {
                return Err(
                    format!("{path}: length {} != {}", values.len(), observed.len()).into(),
                );
            }
            for (i, (left, right)) in values.iter().zip(observed).enumerate() {
                compare_value(left, right, &format!("{path}[{i}]"))?;
            }
        }
        Value::Number(number) if number.is_f64() => {
            let left = number.as_f64().ok_or("expected float")?;
            let right = actual
                .as_f64()
                .ok_or_else(|| format!("{path}: expected float value"))?;
            let difference = (left - right).abs();
            if !difference.is_finite()
                || difference > 1e-8f64.max(1e-12 * left.abs().max(right.abs()))
            {
                return Err(format!("{path}: {left:?} != {right:?}").into());
            }
        }
        _ if expected != actual => return Err(format!("{path}: {expected} != {actual}").into()),
        _ => {}
    }
    Ok(())
}

fn file_sha(path: &Path) -> Result<String> {
    Ok(format!("{:x}", sha2::Sha256::digest(fs::read(path)?)))
}

fn future_path(path: &Path) -> Result<PathBuf> {
    let mut cursor = path;
    let mut suffix = Vec::new();
    while !cursor.exists() {
        let name = cursor
            .file_name()
            .ok_or("output path has no existing ancestor")?;
        suffix.push(name.to_os_string());
        cursor = cursor.parent().ok_or("output path has no parent")?;
    }
    let mut resolved = cursor.canonicalize()?;
    for name in suffix.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn require_absent(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(format!("{label} remains after owned cleanup: {}", path.display()).into()),
        Err(error) => Err(error.into()),
    }
}

struct OwnedReport {
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
    committed: bool,
}

fn report_identity(metadata: &std::fs::Metadata) -> (u64, u64, u32, (i64, i64), (i64, i64)) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.mode(),
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
    )
}

impl OwnedReport {
    fn verify(&self, expected: &[u8]) -> Result<()> {
        let held = self.file.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        let identity = (self.dev, self.ino, self.mode, self.modified, self.changed);
        if !path.is_file()
            || path.file_type().is_symlink()
            || report_identity(&held) != identity
            || report_identity(&path) != identity
            || held.len() != expected.len() as u64
        {
            return Err("hardwared report ownership changed".into());
        }
        let mut file = self.file.try_clone()?;
        file.seek(std::io::SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes != expected
            || report_identity(&self.file.metadata()?) != identity
            || !fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| report_identity(&metadata) == identity)
        {
            return Err("hardwared report bytes or identity changed".into());
        }
        Ok(())
    }
}

impl Drop for OwnedReport {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(path) = fs::symlink_metadata(&self.path) {
            if path.is_file()
                && !path.file_type().is_symlink()
                && (path.dev(), path.ino()) == (self.dev, self.ino)
            {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn finalize(
    path: &Path,
    report: &Value,
    verify: impl Fn() -> Result<()>,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<()> {
    verify()?;
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    let metadata = file.metadata()?;
    let mut publication = OwnedReport {
        path: path.to_path_buf(),
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        file,
        committed: false,
    };
    publication.file.write_all(&bytes)?;
    publication.file.flush()?;
    let metadata = publication.file.metadata()?;
    publication.mode = metadata.mode();
    publication.modified = (metadata.mtime(), metadata.mtime_nsec());
    publication.changed = (metadata.ctime(), metadata.ctime_nsec());
    publication.verify(&bytes)?;
    emit(report)?;
    verify()?;
    publication.verify(&bytes)?;
    publication.committed = true;
    Ok(())
}

/// Compare every original AST result with the actual native trace executable.
pub fn run_trace(options: &TraceOptions) -> Result<Value> {
    run_trace_with_emit(options, |_| Ok(()))
}

pub fn run_trace_with_emit(
    options: &TraceOptions,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<Value> {
    let checkout = options.source_root.canonicalize()?;
    let evidence = checkout.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = options.binary.canonicalize()?;
    let receipt = crate::hardwared_capture::validate(
        &checkout,
        &evidence,
        &binary,
        &options.runner,
        None,
        options.require_retired,
    )?;
    let requested_output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        std::env::current_dir()?.join(&options.output)
    };
    let output = future_path(&requested_output)?;
    if output.starts_with(&evidence) || evidence.starts_with(&output) {
        return Err("hardwared output cannot overlap original capture".into());
    }
    let original = receipt.original_json_bytes()?;
    let mut report = run_trace_admitted(options, &binary, &original, &output)?;
    report["capture"] = receipt.report();
    finalize(
        &output.join("result.json"),
        &report,
        || receipt.verify(),
        emit,
    )?;
    Ok(report)
}

fn run_trace_admitted(
    options: &TraceOptions,
    binary: &Path,
    original_source: &[u8],
    output: &Path,
) -> Result<Value> {
    let cases = cases();
    let source_sha = format!("{:x}", sha2::Sha256::digest(original_source));
    let binary_sha = file_sha(binary)?;
    let originals: Vec<Value> = serde_json::from_slice(original_source)?;
    if originals.len() != TRACE_CASES || cases.len() != TRACE_CASES {
        return Err("hardwared original case roster differs".into());
    }
    let mut input = Vec::new();
    let mut total = 0usize;
    for (case, original) in cases.iter().zip(&originals) {
        if original["name"] != case.name || original["input"] != case.input {
            return Err(format!("original hardwared input differs for {}", case.name).into());
        }
        total += original["expected"]
            .as_array()
            .ok_or("original hardwared result is not array")?
            .len();
        input.extend(serde_json::to_vec(&case.input)?);
        input.push(b'\n');
    }
    if total != TRACE_OUTPUT_ROWS {
        return Err("hardwared original output row count differs".into());
    }
    fs::create_dir_all(output)?;
    fs::write(output.join("input.jsonl"), &input)?;
    let mut argv = options.runner.clone();
    argv.push(binary.as_os_str().to_owned());
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(5 * 60));
    command.input = Some(input);
    command.stdout_limit = 32 * 1024 * 1024;
    command.stderr_limit = 4 * 1024 * 1024;
    let captured = NativeRunner.run_to_files(
        &command,
        &output.join("native.stdout"),
        &output.join("native.stderr"),
    )?;
    if captured.code != 0 {
        return Err(format!("hardwared trace ELF exited {}", captured.code).into());
    }
    let text = fs::read_to_string(output.join("native.stdout"))?;
    let actual = text
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if actual.len() != TRACE_CASES {
        return Err("hardwared trace output line count differs".into());
    }
    let mut results = Vec::new();
    for ((case, source), observed) in cases.iter().zip(&originals).zip(actual.iter()) {
        compare_value(&source["expected"], observed, &case.name)?;
        results.push(json!({"name":case.name,"rows":source["expected"].as_array().unwrap().len(),"result":"PASS"}));
    }
    if file_sha(binary)? != binary_sha {
        return Err("hardwared trace binary changed during comparison".into());
    }
    let report = json!({"result":"PASS","case_count":TRACE_CASES,"input_rows":TRACE_INPUT_ROWS,
        "observation_count":total,"binary_sha256":binary_sha,"original_source_sha256":source_sha,
        "source":"captured original hardwared_source AST outputs","cases":results});
    Ok(report)
}

fn write_event(
    build: impl FnOnce(&mut openpilot_cereal::log_capnp::event::Builder<'_>),
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
    build(&mut event);
    capnp::serialize::write_message_to_words(&message)
}

fn monotonic_ns() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

fn send_inputs(
    publishers: &mut openpilot_messaging::runtime::PubMaster,
    ignition: bool,
    engaged: bool,
) -> Result<u64> {
    use openpilot_cereal::log_capnp::panda_state::{HarnessStatus, PandaType};
    let now = monotonic_ns();
    let panda = write_event(|event| {
        event.set_log_mono_time(now);
        event.set_valid(false);
        let mut list = event.reborrow().init_panda_states(1);
        let mut state = list.reborrow().get(0);
        state.set_panda_type(PandaType::Dos);
        state.set_harness_status(HarnessStatus::Normal);
        state.set_ignition_line(ignition);
    });
    let peripheral = write_event(|event| {
        event.set_log_mono_time(now);
        event.set_valid(false);
        let mut state = event.reborrow().init_peripheral_state();
        state.set_panda_type(PandaType::Dos);
        state.set_voltage(12000);
    });
    let selfdrive = write_event(|event| {
        event.set_log_mono_time(now);
        event.set_valid(false);
        event.reborrow().init_selfdrive_state().set_enabled(engaged);
    });
    publishers.send("pandaStates", &panda)?;
    publishers.send("peripheralState", &peripheral)?;
    publishers.send("selfdriveState", &selfdrive)?;
    Ok(now)
}

enum PublisherCommand {
    State(bool, bool),
    Stop,
}

enum PublisherEvent {
    Tick {
        ignition: bool,
        engaged: bool,
        mono_ns: u64,
    },
    Failed(String),
}

struct PublisherWorker {
    commands: mpsc::Sender<PublisherCommand>,
    events: mpsc::Receiver<PublisherEvent>,
    done: mpsc::Receiver<()>,
    worker: Option<JoinHandle<()>>,
    state: (bool, bool),
}

impl PublisherWorker {
    fn start() -> Result<Self> {
        use openpilot_messaging::runtime::PubMaster;
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("hardwared-qa-publisher".into())
            .spawn(move || {
                let mut ready_sent = false;
                let result = (|| -> Result<()> {
                    let mut publishers = PubMaster::for_runtime(&[
                        "pandaStates",
                        "peripheralState",
                        "selfdriveState",
                        "gpsLocationExternal",
                    ])?;
                    if !publishers.wait_for_readers(
                        "pandaStates",
                        Duration::from_secs(8),
                        Duration::from_millis(20),
                    )? {
                        return Err("hardwared never attached pandaStates".into());
                    }
                    ready_tx
                        .send(Ok(()))
                        .map_err(|_| "hardwared publisher readiness receiver closed")?;
                    ready_sent = true;
                    let mut state = (false, false);
                    let mut next = Instant::now();
                    loop {
                        let timeout = next.saturating_duration_since(Instant::now());
                        match command_rx.recv_timeout(timeout) {
                            Ok(PublisherCommand::State(ignition, engaged)) => {
                                state = (ignition, engaged);
                                continue;
                            }
                            Ok(PublisherCommand::Stop)
                            | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                        let mono_ns = match send_inputs(&mut publishers, state.0, state.1) {
                            Ok(mono_ns) => mono_ns,
                            Err(error) => {
                                let _ = event_tx.send(PublisherEvent::Failed(error.to_string()));
                                break;
                            }
                        };
                        if event_tx
                            .send(PublisherEvent::Tick {
                                ignition: state.0,
                                engaged: state.1,
                                mono_ns,
                            })
                            .is_err()
                        {
                            break;
                        }
                        next += Duration::from_millis(100);
                        if next <= Instant::now() {
                            next = Instant::now() + Duration::from_millis(100);
                        }
                    }
                    Ok(())
                })();
                if !ready_sent {
                    let ready_error = result
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "hardwared publisher stopped before readiness".into());
                    let _ = ready_tx.send(Err(ready_error));
                } else if let Err(error) = result {
                    let _ = event_tx.send(PublisherEvent::Failed(error.to_string()));
                }
                let _ = done_tx.send(());
            })?;
        match ready_rx.recv_timeout(Duration::from_secs(9)) {
            Ok(Ok(())) => Ok(Self {
                commands: command_tx,
                events: event_rx,
                done: done_rx,
                worker: Some(worker),
                state: (false, false),
            }),
            Ok(Err(error)) => {
                if done_rx.recv_timeout(Duration::from_secs(1)).is_ok() {
                    let _ = worker.join();
                }
                Err(error.into())
            }
            Err(error) => {
                let _ = command_tx.send(PublisherCommand::Stop);
                if done_rx.recv_timeout(Duration::from_secs(1)).is_ok() {
                    let _ = worker.join();
                }
                Err(format!("hardwared publisher readiness timeout: {error}").into())
            }
        }
    }

    fn set_state(&mut self, ignition: bool, engaged: bool) -> Result<()> {
        if self.state != (ignition, engaged) {
            self.commands
                .send(PublisherCommand::State(ignition, engaged))
                .map_err(|_| "hardwared publisher worker stopped")?;
            self.state = (ignition, engaged);
        }
        Ok(())
    }

    fn collect(&self, history: &mut Vec<Value>, input_count: &mut usize) -> Result<()> {
        loop {
            match self.events.try_recv() {
                Ok(PublisherEvent::Tick {
                    ignition,
                    engaged,
                    mono_ns,
                }) => {
                    *input_count += 3;
                    history.push(json!({"mono_ns":mono_ns,"ignition":ignition,"engaged":engaged,
                        "event_valid":false,"publisher_thread":"hardwared-qa-publisher",
                        "publishers_opened":["pandaStates","peripheralState","selfdriveState","gpsLocationExternal"],
                        "services_sent":["pandaStates","peripheralState","selfdriveState"],
                        "gpsLocationExternal_event_sent":false}));
                }
                Ok(PublisherEvent::Failed(error)) => return Err(error.into()),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if self.worker.is_some() {
            let _ = self.commands.send(PublisherCommand::Stop);
            self.done
                .recv_timeout(Duration::from_secs(1))
                .map_err(|error| format!("hardwared publisher stop deadline: {error}"))?;
            let worker = self
                .worker
                .take()
                .ok_or("hardwared publisher worker missing")?;
            worker
                .join()
                .map_err(|_| "hardwared publisher thread panicked")?;
        }
        Ok(())
    }
}

impl Drop for PublisherWorker {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.commands.send(PublisherCommand::Stop);
            if self.done.recv_timeout(Duration::from_secs(1)).is_ok() {
                let _ = worker.join();
            }
        }
    }
}

fn value_packet(raw: &[u8]) -> Result<Value> {
    use openpilot_cereal::log_capnp::{event, init_data::DeviceType};
    let message = capnp::serialize::read_message(
        &mut Cursor::new(raw),
        capnp::message::ReaderOptions::new(),
    )?;
    let event = message.get_root::<event::Reader>()?;
    let event_valid = event.get_valid();
    let mono = event.get_log_mono_time();
    let event::Which::DeviceState(state) = event.which()? else {
        return Err("hardwared published a non-deviceState event".into());
    };
    let state = state?;
    if state.get_device_type()? != DeviceType::Pc {
        return Err("hardwared did not publish PC device type".into());
    }
    let cpu = state
        .get_cpu_usage_percent()?
        .iter()
        .map(i64::from)
        .collect::<Vec<_>>();
    let value = json!({
        "valid":event_valid,"logMonoTime":mono,"started":state.get_started(),
        "startedMonoTime":state.get_started_mono_time(),"memoryUsagePercent":state.get_memory_usage_percent(),
        "cpuUsagePercent":cpu,"fanSpeedPercentDesired":state.get_fan_speed_percent_desired(),
        "thermalStatus":state.get_thermal_status()? as u16
    });
    if !event_valid {
        return Err("native hardwared deviceState publication is invalid".into());
    }
    let age = (monotonic_ns() as i128 - mono as i128) as f64 / 1e9;
    if !(0.0..2.0).contains(&age) {
        return Err(format!("stale deviceState publication: {age}s").into());
    }
    Ok(value)
}

struct OwnedNamespace {
    path: PathBuf,
    identity: (u64, u64),
}
impl Drop for OwnedNamespace {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && (metadata.dev(), metadata.ino()) == self.identity
        }) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct OwnedRuntimeRoot {
    path: PathBuf,
    identity: (u64, u64),
}

impl Drop for OwnedRuntimeRoot {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && (metadata.dev(), metadata.ino()) == self.identity
        }) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn drain_zmq(
    logs: &zmq::Socket,
    stats: &zmq::Socket,
    records: &mut Vec<Value>,
    metrics: &mut Vec<String>,
) -> Result<()> {
    loop {
        match logs.recv_bytes(zmq::DONTWAIT) {
            Ok(raw) => {
                let parsed = raw
                    .get(1..)
                    .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
                records.push(parsed.unwrap_or_else(
                    || json!({"raw_hex":raw.iter().map(|b|format!("{b:02x}")).collect::<String>()}),
                ));
            }
            Err(zmq::Error::EAGAIN) => break,
            Err(error) => return Err(error.into()),
        }
    }
    loop {
        match stats.recv_string(zmq::DONTWAIT) {
            Ok(Ok(metric)) => metrics.push(metric),
            Ok(Err(raw)) => metrics.push(format!(
                "non-utf8:{}",
                raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )),
            Err(zmq::Error::EAGAIN) => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

struct PeerEnvironment {
    saved: [(&'static str, Option<OsString>); 3],
}

impl PeerEnvironment {
    fn set(prefix: &str) -> Self {
        let saved = [
            ("OPENPILOT_PREFIX", std::env::var_os("OPENPILOT_PREFIX")),
            ("ZMQ", std::env::var_os("ZMQ")),
            ("CEREAL_FAKE", std::env::var_os("CEREAL_FAKE")),
        ];
        std::env::set_var("OPENPILOT_PREFIX", prefix);
        std::env::remove_var("ZMQ");
        std::env::remove_var("CEREAL_FAKE");
        Self { saved }
    }
}

impl Drop for PeerEnvironment {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

struct SocketPath {
    path: PathBuf,
    identity: Option<(u64, u64)>,
}

impl SocketPath {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            identity: None,
        }
    }

    fn capture(&mut self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.file_type().is_socket() {
            return Err("hardwared peer endpoint is not a Unix socket".into());
        }
        self.identity = Some((metadata.dev(), metadata.ino()));
        Ok(())
    }

    fn cleanup(&mut self) -> Result<Value> {
        let identity = self
            .identity
            .ok_or("hardwared peer socket was not captured")?;
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.file_type().is_socket() || (metadata.dev(), metadata.ino()) != identity {
            return Err("hardwared peer socket path was replaced".into());
        }
        fs::remove_file(&self.path)?;
        match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err("hardwared peer socket path reappeared after unlink".into()),
            Err(error) => return Err(error.into()),
        }
        self.identity = None;
        Ok(json!({"path":self.path,"device":identity.0,"inode":identity.1,"absent":true}))
    }
}

impl Drop for SocketPath {
    fn drop(&mut self) {
        if let Some(identity) = self.identity {
            if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
                metadata.file_type().is_socket() && (metadata.dev(), metadata.ino()) == identity
            }) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn write_private_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("proc"))?;
    fs::create_dir_all(root.join("dev"))?;
    fs::create_dir_all(root.join("data/params/d"))?;
    fs::create_dir_all(root.join("data/media/0/realdata"))?;
    for (path, bytes) in [
        (
            "proc/stat",
            b"cpu 100 0 0 100 0 0 0 0 0 0\ncpu0 100 0 0 100 0 0 0 0 0 0\n".as_slice(),
        ),
        (
            "proc/meminfo",
            b"MemTotal: 1000000 kB\nMemAvailable: 500000 kB\n".as_slice(),
        ),
        ("data/params/d/HasAcceptedTerms", b"2".as_slice()),
        (
            "data/params/d/CompletedTrainingVersion",
            b"0.2.0".as_slice(),
        ),
        ("data/params/d/MaxTimeOffroadMin", b"1800".as_slice()),
        ("dev/kmsg", b"".as_slice()),
    ] {
        fs::write(root.join(path), bytes)?;
    }
    fs::create_dir(root.join("no-command-directory"))?;
    Ok(())
}

fn params_snapshot(params: &Path) -> Result<Value> {
    let mut values = BTreeMap::new();
    for entry in fs::read_dir(params)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("unexpected non-regular entry in hardwared Params root".into());
        }
        let bytes = fs::read(&path)?;
        values.insert(
            path.file_name()
                .ok_or("Params entry has no basename")?
                .to_string_lossy()
                .into_owned(),
            json!({"bytes":bytes,"utf8":std::str::from_utf8(&bytes).ok()}),
        );
    }
    Ok(json!(values))
}

struct ProcessGuard {
    child: Option<Child>,
    pid: rustix::process::Pid,
}
impl ProcessGuard {
    fn spawn(
        binary: &Path,
        root: &Path,
        launcher: &Path,
        prefix: &str,
        log: &Path,
    ) -> Result<Self> {
        let stdout = File::create(log)?;
        let stderr = stdout.try_clone()?;
        let mut command = Command::new(binary);
        command
            .args(["--root"])
            .arg(root)
            .args(["--launcher"])
            .arg(launcher)
            .env_clear()
            .env("OPENPILOT_PREFIX", prefix)
            .env("HOME", root)
            .env("PATH", root.join("no-command-directory"))
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .process_group(0);
        let child = command.spawn()?;
        let pid = rustix::process::Pid::from_child(&child);
        Ok(Self {
            child: Some(child),
            pid,
        })
    }
    fn stop(&mut self) -> Result<i32> {
        if self.child.is_none() {
            return Err("hardwared process already reaped".into());
        }
        rustix::process::kill_process(self.pid, rustix::process::Signal::TERM)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = self
                .child
                .as_mut()
                .ok_or("hardwared process reaped")?
                .try_wait()?;
            if let Some(status) = status {
                self.child.take();
                return status
                    .code()
                    .ok_or_else(|| "hardwared ended by signal".into());
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL)?;
        let kill_deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let status = self
                .child
                .as_mut()
                .ok_or("hardwared process reaped")?
                .try_wait()?;
            if let Some(status) = status {
                self.child.take();
                return Err(format!("hardwared ignored SIGTERM (status {status})").into());
            }
            if Instant::now() >= kill_deadline {
                return Err("hardwared did not exit after process-group SIGKILL deadline".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL);
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        }
    }
}

fn peer(root: &Path, output: &Path, prefix: &str) -> Result<Value> {
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _lock = ENV_LOCK.lock().map_err(|_| "hardwared env lock poisoned")?;
    let _environment = PeerEnvironment::set(prefix);
    let service = openpilot_messaging::services::lookup("deviceState")
        .ok_or("deviceState service metadata missing")?;
    let mut subscriber =
        openpilot_msgq::Subscriber::for_runtime("deviceState", false, service.queue_size)?;
    let context = zmq::Context::new();
    let mut log_cleanup = SocketPath::new(PathBuf::from(format!("/tmp/logmessage{prefix}")));
    let logs = context.socket(zmq::PULL).map_err(|e| e.to_string())?;
    logs.set_linger(0).map_err(|e| e.to_string())?;
    let log_endpoint = format!("ipc:///tmp/logmessage{prefix}");
    logs.bind(&log_endpoint).map_err(|e| e.to_string())?;
    log_cleanup.capture()?;
    let mut stats_cleanup = SocketPath::new(root.join("stats"));
    let stats = context.socket(zmq::PULL).map_err(|e| e.to_string())?;
    stats.set_linger(0).map_err(|e| e.to_string())?;
    stats
        .bind(&format!("ipc://{}", root.join("stats").display()))
        .map_err(|e| e.to_string())?;
    stats_cleanup.capture()?;
    let mut publisher = PublisherWorker::start()?;
    let mut packets = Vec::new();
    let mut records = Vec::new();
    let mut metrics = Vec::new();
    let mut input_history = Vec::new();
    let send_receive = |publisher: &mut PublisherWorker,
                        subscriber: &mut openpilot_msgq::Subscriber,
                        ignition,
                        engaged,
                        packets: &mut Vec<Value>,
                        records: &mut Vec<Value>,
                        metrics: &mut Vec<String>,
                        input_count: &mut usize,
                        input_history: &mut Vec<Value>|
     -> Result<Option<Value>> {
        publisher.set_state(ignition, engaged)?;
        drain_zmq(&logs, &stats, records, metrics)?;
        let received = subscriber.receive(Duration::from_millis(180))?;
        publisher.collect(input_history, input_count)?;
        let Some(raw) = received else {
            return Ok(None);
        };
        let value = value_packet(&raw)?;
        if value["memoryUsagePercent"] != 50 || value["cpuUsagePercent"] != json!([0]) {
            return Err("hardwared CPU/memory observations differ".into());
        }
        fs::write(
            output.join(format!("deviceState-{:03}.capnp", packets.len())),
            &raw,
        )?;
        packets.push(value.clone());
        Ok(Some(value))
    };
    let until = |publisher: &mut PublisherWorker,
                 subscriber: &mut openpilot_msgq::Subscriber,
                 target: bool,
                 ignition: bool,
                 engaged: bool,
                 packets: &mut Vec<Value>,
                 records: &mut Vec<Value>,
                 metrics: &mut Vec<String>,
                 input_count: &mut usize,
                 input_history: &mut Vec<Value>|
     -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(value) = send_receive(
                publisher,
                subscriber,
                ignition,
                engaged,
                packets,
                records,
                metrics,
                input_count,
                input_history,
            )? {
                if value["started"] == target {
                    return Ok(value);
                }
            }
            if Instant::now() >= deadline {
                return Err(format!("deviceState transition to started={target} missing").into());
            }
        }
    };
    let mut transitions = Vec::new();
    let mut input_count = 0usize;
    transitions.push(until(
        &mut publisher,
        &mut subscriber,
        false,
        false,
        false,
        &mut packets,
        &mut records,
        &mut metrics,
        &mut input_count,
        &mut input_history,
    )?);
    transitions.push(until(
        &mut publisher,
        &mut subscriber,
        true,
        true,
        true,
        &mut packets,
        &mut records,
        &mut metrics,
        &mut input_count,
        &mut input_history,
    )?);
    let params = root.join("data/params/d");
    let engaged_deadline = Instant::now() + Duration::from_secs(3);
    while fs::read_to_string(params.join("IsEngaged")).unwrap_or_default() != "1" {
        if Instant::now() >= engaged_deadline {
            return Err("hardwared did not persist IsEngaged".into());
        }
        let _ = send_receive(
            &mut publisher,
            &mut subscriber,
            true,
            true,
            &mut packets,
            &mut records,
            &mut metrics,
            &mut input_count,
            &mut input_history,
        )?;
    }
    fs::write(params.join("OnroadCycleRequested"), b"1")?;
    transitions.push(until(
        &mut publisher,
        &mut subscriber,
        false,
        true,
        true,
        &mut packets,
        &mut records,
        &mut metrics,
        &mut input_count,
        &mut input_history,
    )?);
    transitions.push(until(
        &mut publisher,
        &mut subscriber,
        true,
        true,
        true,
        &mut packets,
        &mut records,
        &mut metrics,
        &mut input_count,
        &mut input_history,
    )?);
    transitions.push(until(
        &mut publisher,
        &mut subscriber,
        false,
        false,
        false,
        &mut packets,
        &mut records,
        &mut metrics,
        &mut input_count,
        &mut input_history,
    )?);
    if transitions[1]["fanSpeedPercentDesired"] != 30
        || transitions[4]["fanSpeedPercentDesired"] != 0
    {
        return Err("hardwared fan output mismatch across ignition".into());
    }
    if fs::read_to_string(params.join("OnroadCycleRequested"))?.trim() != "0" {
        return Err("hardwared did not clear cycle request".into());
    }
    let status: Value = serde_json::from_slice(&fs::read(params.join("LastOffroadStatusPacket"))?)?;
    if status["deviceState"]["deviceState"]["started"] != true
        || status["deviceState"]["deviceState"]
            .get("deprecated")
            .is_some()
    {
        return Err("hardwared status packet lifecycle contract differs".into());
    }
    if !params.join("UptimeOffroad").is_file() || !params.join("UptimeOnroad").is_file() {
        return Err("hardwared uptime Params missing".into());
    }
    let network_deadline = Instant::now() + Duration::from_secs(3);
    while fs::read_to_string(params.join("NetworkMetered"))
        .unwrap_or_default()
        .trim()
        != "0"
    {
        if Instant::now() >= network_deadline {
            return Err("hardwared did not persist NetworkMetered".into());
        }
        let _ = send_receive(
            &mut publisher,
            &mut subscriber,
            false,
            false,
            &mut packets,
            &mut records,
            &mut metrics,
            &mut input_count,
            &mut input_history,
        )?;
    }
    for _ in 0..4 {
        let _ = send_receive(
            &mut publisher,
            &mut subscriber,
            false,
            false,
            &mut packets,
            &mut records,
            &mut metrics,
            &mut input_count,
            &mut input_history,
        )?;
    }
    drop(until);
    drop(send_receive);
    publisher.stop()?;
    publisher.collect(&mut input_history, &mut input_count)?;
    let logs_ok = records
        .iter()
        .any(|row| row["msg"]["event"] == "STATUS_PACKET");
    let power = metrics.iter().any(|s| s.starts_with("power_draw:"));
    let voltage = metrics.iter().any(|s| s.starts_with("car_voltage:"));
    if !logs_ok || !power || !voltage {
        return Err("hardwared log/stats telemetry missing".into());
    }
    if input_count < 15 {
        return Err("hardwared test did not send enough invalid-flag inputs".into());
    }
    let kmsg = fs::read_to_string(root.join("dev/kmsg"))?;
    if !kmsg.contains("engaged: True") || !kmsg.contains("engaged: False") {
        return Err("hardwared engaged kmsg lifecycle markers missing".into());
    }
    if fs::read_dir(root.join("no-command-directory"))?
        .next()
        .is_some()
    {
        return Err("hardwared unexpectedly invoked a PATH command".into());
    }
    let params_after = params_snapshot(&params)?;
    fs::write(
        output.join("records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    fs::write(
        output.join("metrics.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    fs::write(
        output.join("params.json"),
        serde_json::to_vec_pretty(&params_after)?,
    )?;
    fs::write(
        output.join("input-history.json"),
        serde_json::to_vec_pretty(&input_history)?,
    )?;
    drop(subscriber);
    drop(logs);
    drop(stats);
    drop(context);
    let log_socket = log_cleanup.cleanup()?;
    let stats_socket = stats_cleanup.cleanup()?;
    Ok(
        json!({"packets":packets,"transitions":transitions,"invalid_input_events":input_count,
        "input_history":input_history,"status_packet":status,"params_after":params_after,"kmsg":kmsg,
        "records":records,"metrics":metrics,"publisher_flags_valid":false,
        "socket_cleanup":[log_socket,stats_socket],"peer_prefix":prefix}),
    )
}

/// Run the live IPC suite against the actual daemon ELF in a private PC root.
/// Captured daemon artifacts are never used as expected answers.
pub fn run_daemon(options: &DaemonOptions) -> Result<Value> {
    run_daemon_with_emit(options, |_| Ok(()))
}

pub fn run_daemon_with_emit(
    options: &DaemonOptions,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<Value> {
    let checkout = options.source_root.canonicalize()?;
    let evidence = checkout.join("rust/crates/dev-tools/tests/fixtures/hardwared-original");
    let binary = options.binary.canonicalize()?;
    let launcher = options.launcher.canonicalize()?;
    let receipt = crate::hardwared_capture::validate(
        &checkout,
        &evidence,
        &binary,
        &[],
        Some(&launcher),
        options.require_retired,
    )?;
    let requested_output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        std::env::current_dir()?.join(&options.output)
    };
    let output = future_path(&requested_output)?;
    if output.starts_with(&evidence) || evidence.starts_with(&output) {
        return Err("hardwared output cannot overlap original capture".into());
    }
    let requested_root = if options.root.is_absolute() {
        options.root.clone()
    } else {
        std::env::current_dir()?.join(&options.root)
    };
    let runtime_root = future_path(&requested_root)?;
    if runtime_root.starts_with(&checkout)
        || checkout.starts_with(&runtime_root)
        || output.starts_with(&runtime_root)
        || runtime_root.starts_with(&output)
    {
        return Err("hardwared private root and output must be isolated from source checkout and each other".into());
    }
    let mut report = run_daemon_admitted(&binary, &launcher, &runtime_root, &output)?;
    report["capture"] = receipt.report();
    finalize(
        &output.join("result.json"),
        &report,
        || receipt.verify(),
        emit,
    )?;
    Ok(report)
}

fn run_daemon_admitted(
    binary: &Path,
    launcher: &Path,
    runtime_root: &Path,
    output: &Path,
) -> Result<Value> {
    fs::create_dir_all(output)?;
    fs::create_dir(runtime_root)?;
    let root_metadata = fs::symlink_metadata(runtime_root)?;
    let owned_runtime_root = OwnedRuntimeRoot {
        path: runtime_root.to_path_buf(),
        identity: (root_metadata.dev(), root_metadata.ino()),
    };
    write_private_root(runtime_root)?;
    let namespace = tempfile::Builder::new()
        .prefix("msgq_hardwared-")
        .tempdir_in("/dev/shm")?;
    let namespace_path = namespace.path().to_path_buf();
    let prefix = namespace_path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid hardwared msgq namespace")?
        .strip_prefix("msgq_")
        .ok_or("invalid hardwared msgq namespace")?
        .to_owned();
    let namespace_id = fs::symlink_metadata(&namespace_path)?;
    let owned_namespace = OwnedNamespace {
        path: namespace.keep(),
        identity: (namespace_id.dev(), namespace_id.ino()),
    };
    let binary_sha = file_sha(binary)?;
    let mut process = ProcessGuard::spawn(
        binary,
        runtime_root,
        launcher,
        &prefix,
        &output.join("daemon.log"),
    )?;
    // Every peer wait is bounded by an absolute per-transition deadline. Running
    // synchronously avoids an unbounded join during error cleanup.
    let peer_value = peer(runtime_root, output, &prefix)?;
    let exit = process.stop()?;
    if exit != 0 {
        return Err(format!("hardwared SIGTERM exit {exit}").into());
    }
    if process.child.is_some() {
        return Err("hardwared child was not reaped after SIGTERM".into());
    }
    if fs::symlink_metadata(&owned_namespace.path).map_or(true, |m| {
        !m.is_dir() || m.file_type().is_symlink() || (m.dev(), m.ino()) != owned_namespace.identity
    }) {
        return Err("hardwared msgq namespace identity changed".into());
    }
    if file_sha(binary)? != binary_sha {
        return Err("hardwared daemon ELF changed during live IPC run".into());
    }
    drop(process);
    drop(owned_namespace);
    require_absent(&namespace_path, "hardwared msgq namespace")?;
    drop(owned_runtime_root);
    require_absent(runtime_root, "hardwared private root")?;
    let report = json!({"result":"PASS","exit_code":exit,"actual_board_access":false,
        "actual_host_commands":false,"binary_sha256":binary_sha,"peer":peer_value,
        "daemon_log_sha256":format!("{:x}",sha2::Sha256::digest(fs::read(output.join("daemon.log"))?)),
        "cleanup":{"daemon_reaped":true,"namespace":{"path":namespace_path,"absent":true},
          "runtime_root":{"path":runtime_root,"absent":true}}});
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn socket_cleanup_preserves_a_foreign_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("endpoint.sock");
        let original = UnixListener::bind(&path).unwrap();
        let mut cleanup = SocketPath::new(path.clone());
        cleanup.capture().unwrap();
        let moved = directory.path().join("original.sock");
        fs::rename(&path, &moved).unwrap();
        let foreign = UnixListener::bind(&path).unwrap();
        assert!(cleanup.cleanup().is_err());
        assert!(path.exists());
        drop(cleanup);
        assert!(path.exists());
        drop(foreign);
        fs::remove_file(path).unwrap();
        drop(original);
        fs::remove_file(moved).unwrap();
    }

    #[test]
    fn generated_hardwared_corpus_has_original_order_and_891_inputs() {
        let all = cases();
        assert_eq!(all.len(), 12);
        assert_eq!(
            all.iter()
                .map(|case| case.input["steps"].as_array().unwrap().len())
                .sum::<usize>(),
            TRACE_INPUT_ROWS
        );
        assert_eq!(TRACE_OUTPUT_ROWS, 890);
        assert_eq!(all[0].name, "fan-tici");
        assert_eq!(all[3].name, "normal-cycle-disconnect");
        assert_eq!(all[11].name, "low-voltage-shutdown");
    }
}
