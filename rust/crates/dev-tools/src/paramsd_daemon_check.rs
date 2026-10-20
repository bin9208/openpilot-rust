//! Native controller for the unchanged paramsd daemon corpus. Input JSON is
//! captured by the independent original checker and sealed by the caller.
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::paramsd_startup_gate::StartupGate;
use openpilot_messaging::{runtime::PubMaster, services};
use openpilot_msgq::Subscriber;
use rustix::{
    process::{kill_process, kill_process_group, Pid, Signal},
    time::{clock_gettime, ClockId},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const STARTUP: Duration = Duration::from_secs(10);
const SHUTDOWN: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct Frame {
    messages: Vec<Vec<u8>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    car: Vec<u8>,
    seed: BTreeMap<String, Vec<u8>>,
    debug: bool,
    replay: bool,
    simulation: bool,
    gps: String,
    frames: Vec<Frame>,
}

pub struct Options {
    pub daemon: PathBuf,
    pub library: PathBuf,
    pub continuous_input: PathBuf,
    pub restart_input: PathBuf,
    pub evidence: PathBuf,
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let mut signals = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            registrations: Vec::with_capacity(2),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signals.registrations.push(signal_hook::flag::register(
                signal,
                Arc::clone(&signals.cancelled),
            )?);
        }
        Ok(signals)
    }

    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("paramsd native controller cancelled".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

struct Daemon {
    child: Child,
    reaped: bool,
}
impl Daemon {
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            // try_wait reaps the child. Its numeric PID/group may be recycled,
            // so Drop must never signal it after this observation.
            self.reaped = true;
            return Err(format!("paramsd exited before input: {status}").into());
        }
        Ok(())
    }

    fn stop(&mut self, signal: Signal, signals: &Signals) -> Result<(i32, f64)> {
        let before = Instant::now();
        signals.check()?;
        kill_process(Pid::from_child(&self.child), signal)?;
        loop {
            signals.check()?;
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                if !status.success() {
                    return Err(format!("paramsd shutdown: {status}").into());
                }
                return Ok((
                    status.code().ok_or("paramsd exit signal")?,
                    before.elapsed().as_secs_f64(),
                ));
            }
            if before.elapsed() >= SHUTDOWN {
                return Err("paramsd shutdown exceeded five seconds".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn monotonic_ns() -> Result<u64> {
    let now = clock_gettime(ClockId::Monotonic);
    Ok(u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?)
}

fn wait_until(
    daemon: &mut Daemon,
    signals: &Signals,
    description: &str,
    mut ready: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let before = Instant::now();
    loop {
        signals.check()?;
        daemon.alive()?;
        if ready()? {
            return Ok(());
        }
        if before.elapsed() >= STARTUP {
            return Err(format!("{description} missing after ten seconds").into());
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn receive_packet(subscriber: &mut Subscriber, signals: &Signals) -> Result<Vec<u8>> {
    let before = Instant::now();
    loop {
        signals.check()?;
        let remaining = Duration::from_secs(5).saturating_sub(before.elapsed());
        if remaining.is_zero() {
            return Err("liveParameters publication timeout".into());
        }
        if let Some(packet) = subscriber.receive(remaining.min(Duration::from_millis(50)))? {
            // Once received, let the caller persist this raw packet before a
            // newly latched cancellation ends the controller.
            return Ok(packet);
        }
    }
}

fn reader_ready(shm: &Path, service: &str, daemon_pid: u32) -> Result<bool> {
    let mut header = [0_u8; 123 * 8];
    File::open(shm.join(service))?.read_exact(&mut header)?;
    let word = |index: usize| -> u64 {
        u64::from_le_bytes(header[index * 8..][..8].try_into().expect("header word"))
    };
    let count = usize::try_from(word(0).min(40))?;
    Ok((0..count).any(|index| word(43 + index) != 0 && word(83 + index) as u32 == daemon_pid))
}

fn pose_cursor(shm: &Path) -> Result<u64> {
    let mut header = [0_u8; 16];
    File::open(shm.join("livePose"))?.read_exact(&mut header)?;
    Ok(u64::from_le_bytes(header[8..16].try_into()?))
}

fn packet_service(packet: &[u8]) -> Result<&'static str> {
    let message = serialize::read_message(packet, ReaderOptions::new())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    Ok(match root.which()? {
        event::CarState(_) => "carState",
        event::LiveCalibration(_) => "liveCalibration",
        event::GpsLocation(_) => "gpsLocation",
        event::GpsLocationExternal(_) => "gpsLocationExternal",
        event::LivePose(_) => "livePose",
        _ => return Err("paramsd input includes unexpected service".into()),
    })
}

fn packet_time(packet: &[u8]) -> Result<u64> {
    let message = serialize::read_message(packet, ReaderOptions::new())?;
    Ok(message.get_root::<event::Reader<'_>>()?.get_log_mono_time())
}

fn input(path: &Path, restart: bool) -> Result<Case> {
    let case: Case = serde_json::from_slice(&fs::read(path)?)?;
    require(
        case.name == if restart { "restart" } else { "continuous" },
        "wrong paramsd case",
    )?;
    require(
        case.frames.len() == if restart { 9 } else { 1203 },
        "wrong paramsd frame count",
    )?;
    require(
        case.frames[0].messages.is_empty(),
        "first paramsd frame must be empty",
    )?;
    require(
        case.debug && case.replay && case.simulation,
        "paramsd source flags changed",
    )?;
    require(
        case.gps
            == if restart {
                "gpsLocation"
            } else {
                "gpsLocationExternal"
            },
        "paramsd GPS source changed",
    )?;
    for frame in &case.frames[1..] {
        require(
            frame.messages.len() == 4,
            "paramsd frame requires four inputs",
        )?;
        for (packet, service) in
            frame
                .messages
                .iter()
                .zip(["carState", "liveCalibration", &case.gps, "livePose"])
        {
            require(
                packet_service(packet)? == service,
                "paramsd input service order changed",
            )?;
        }
    }
    Ok(case)
}

fn phase(
    options: &Options,
    case: &Case,
    restart: bool,
    previous_saved: Option<&[u8]>,
    signals: &Signals,
) -> Result<Vec<u8>> {
    signals.check()?;
    let output = options
        .evidence
        .join(if restart { "restart" } else { "continuous" });
    fs::create_dir(&output)?;
    let private = tempfile::tempdir()?;
    let shm = tempfile::Builder::new()
        .prefix("msgq_rust-probe-params-")
        .tempdir_in("/dev/shm")?;
    let prefix = shm
        .path()
        .file_name()
        .ok_or("msgq namespace")?
        .to_str()
        .ok_or("msgq namespace UTF-8")?
        .strip_prefix("msgq_")
        .ok_or("msgq namespace prefix")?;
    let params = private.path().join("params");
    let memory = private.path().join("memory");
    let named_params = params.join(prefix);
    let named_memory = memory.join(prefix);
    fs::create_dir_all(&named_params)?;
    fs::create_dir_all(&named_memory)?;
    fs::write(named_memory.join("LastGPSPosition"), b"old position")?;
    fs::write(
        named_params.join("UbloxAvailable"),
        if restart { b"0" } else { b"1" },
    )?;
    if restart {
        require(
            case.seed.len() == 2
                && case.seed.contains_key("LiveParametersV2")
                && case.seed.contains_key("CarParamsPrevRoute"),
            "restart seed changed",
        )?;
        fs::write(
            named_params.join("LiveParametersV2"),
            previous_saved.ok_or("continuous saved bytes missing")?,
        )?;
        fs::write(
            named_params.join("CarParamsPrevRoute"),
            &case.seed["CarParamsPrevRoute"],
        )?;
    } else {
        require(case.seed.is_empty(), "continuous seed changed")?;
    }
    let position = named_memory.join("LastGPSPosition");
    let mut gate = StartupGate::new(&shm.path().join("livePose"), &output)?;
    let trace = output.join("poll.jsonl");
    let mut command = Command::new(&options.daemon);
    command
        .arg("--memory-root")
        .arg(&memory)
        .env("OPENPILOT_PREFIX", prefix)
        .env("PARAMS_ROOT", &params)
        .env("DEBUG", "1")
        .env("REPLAY", "1")
        .env("SIMULATION", "1")
        .env("PATH", private.path().join("no-programs"))
        .env_remove("ZMQ")
        .env_remove("CEREAL_FAKE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(output.join("stdout.log"))?))
        .stderr(Stdio::from(File::create(output.join("stderr.log"))?))
        .process_group(0);
    gate.attach(&mut command, &options.library, &position, &trace)?;
    let mut daemon = Daemon {
        child: command.spawn()?,
        reaped: false,
    };
    drop(command);
    let names = ["carState", "liveCalibration", case.gps.as_str(), "livePose"];
    wait_until(&mut daemon, signals, "paramsd msgq endpoints", || {
        Ok(names
            .iter()
            .chain(["liveParameters"].iter())
            .all(|name| shm.path().join(name).exists()))
    })?;
    require(
        fs::read(&position)? == b"old position",
        "paramsd entered main loop before CarParams",
    )?;
    let executable = fs::read_link(format!("/proc/{}/exe", daemon.child.id()))?;
    require(
        executable == options.daemon.canonicalize()?,
        "paramsd executable identity changed",
    )?;
    let maps = fs::read_to_string(format!("/proc/{}/maps", daemon.child.id()))?;
    require(
        !["libpython", "ekf_sym_pyx", "libcar.so"]
            .iter()
            .any(|name| maps.contains(name)),
        "Python runtime loaded by paramsd",
    )?;
    fs::write(output.join("maps.txt"), maps)?;
    std::env::set_var("OPENPILOT_PREFIX", prefix);
    std::env::set_var("PARAMS_ROOT", &params);
    std::env::set_var("SIMULATION", "1");
    std::env::remove_var("ZMQ");
    std::env::remove_var("CEREAL_FAKE");
    let mut publisher = PubMaster::for_runtime(&names)?;
    let capacity = services::lookup("liveParameters")
        .ok_or("liveParameters service")?
        .queue_size;
    let mut subscriber = Subscriber::for_runtime("liveParameters", false, capacity)?;
    let _ = subscriber.receive(Duration::ZERO)?;
    fs::write(named_params.join("CarParams"), &case.car)?;
    let daemon_pid = daemon.child.id();
    wait_until(&mut daemon, signals, "paramsd input readers", || {
        Ok(names
            .iter()
            .map(|name| reader_ready(shm.path(), name, daemon_pid))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .all(|ready| ready))
    })?;
    fs::write(
        output.join("reader-readiness.json"),
        serde_json::to_vec_pretty(
            &json!({"services":names,"daemon_pid":daemon.child.id(),"all_ready":true}),
        )?,
    )?;
    gate.wait_batch_cancellable(&signals.cancelled)?;
    let rows: Vec<serde_json::Value> = fs::read_to_string(&trace)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.iter()
            .any(|row| row["event"] == "gate" && row["completed"] == 1),
        "second poll gate not held after one empty update",
    )?;
    require(
        rows.iter()
            .filter(|row| row["event"] == "return" && row["result"] == 0)
            .map(|row| row["completed"].as_u64())
            .collect::<Vec<_>>()
            == [Some(1)],
        "first empty update count changed",
    )?;
    let mut packets = Vec::with_capacity(case.frames.len() - 1);
    let mut packet_log = File::create(output.join("packets.jsonl"))?;
    for (index, frame) in case.frames[1..].iter().enumerate() {
        signals.check()?;
        if index != 0 {
            gate.wait_batch_cancellable(&signals.cancelled)?;
        }
        let previous = pose_cursor(shm.path())?;
        let before = monotonic_ns()?;
        for packet in &frame.messages {
            publisher.send(packet_service(packet)?, packet)?;
        }
        gate.release_cancellable(
            previous,
            u64::try_from(frame.messages[3].len())?,
            &signals.cancelled,
        )?;
        let packet = receive_packet(&mut subscriber, signals)?;
        let after = monotonic_ns()?;
        packet_log.write_all(&serde_json::to_vec(&packet)?)?;
        packet_log.write_all(b"\n")?;
        packet_log.flush()?;
        signals.check()?;
        require(
            (before..=after).contains(&packet_time(&packet)?),
            "publication timestamp outside input transaction",
        )?;
        packets.push(packet);
    }
    gate.finish_cancellable(&signals.cancelled)?;
    let signum = if restart { Signal::TERM } else { Signal::INT };
    let (exit, shutdown_seconds) = daemon.stop(signum, signals)?;
    let saved = fs::read(named_params.join("LiveParametersV2"))?;
    // The original reports this index only for the continuous persistence
    // assertion. Restart verifies the seeded bytes independently.
    let persisted_index = if restart {
        None
    } else {
        packets.iter().position(|packet| packet == &saved)
    };
    if restart {
        require(
            previous_saved == Some(saved.as_slice()),
            "restart changed persisted parameters",
        )?;
    } else {
        require(
            persisted_index == Some(1199),
            "1200-frame persistence index changed",
        )?;
    }
    require(
        gate.report().batches_queued_before_release == packets.len() as u32,
        "not every pose batch passed the committed-cursor gate",
    )?;
    let gps = fs::read(&position)?;
    require(
        gps == b"{\"latitude\": 37.5, \"longitude\": 127.0, \"bearing\": 22.5}",
        "GPS position bytes changed",
    )?;
    fs::write(output.join("gps.json"), &gps)?;
    fs::write(output.join("saved.bin"), &saved)?;
    fs::write(output.join("packets.json"), serde_json::to_vec(&packets)?)?;
    fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "pass":true,"case":case.name,"publications":packets.len(),"native_executable":executable,
            "persisted_packet_index":persisted_index,"gps_service":case.gps,
            "signal":if restart {15} else {2},"exit":exit,"shutdown_seconds":shutdown_seconds,
            "no_python_runtime":true,"startup_poll":gate.report(),
            "input_queued_before_release":gate.report().input_queued_before_release,
            "batches_queued_before_release":gate.report().batches_queued_before_release,
        }))?,
    )?;
    Ok(saved)
}

pub fn run(options: &Options) -> Result<()> {
    let signals = Signals::new()?;
    signals.check()?;
    require(
        options.daemon.is_absolute()
            && options.library.is_absolute()
            && options.continuous_input.is_absolute()
            && options.restart_input.is_absolute()
            && options.evidence.is_absolute(),
        "all paramsd controller paths must be absolute",
    )?;
    require(
        options.daemon.is_file() && options.library.is_file(),
        "source-built daemon and phase hook required",
    )?;
    let sources = [
        &options.daemon,
        &options.library,
        &options.continuous_input,
        &options.restart_input,
    ];
    let before = sources
        .iter()
        .map(|path| openpilot_dev_tools::seal::digest(path))
        .collect::<Result<Vec<_>>>()?;
    let continuous = input(&options.continuous_input, false)?;
    let restart = input(&options.restart_input, true)?;
    fs::create_dir(&options.evidence)?;
    let saved = phase(options, &continuous, false, None, &signals)?;
    phase(options, &restart, true, Some(&saved), &signals)?;
    signals.check()?;
    let after = sources
        .iter()
        .map(|path| openpilot_dev_tools::seal::digest(path))
        .collect::<Result<Vec<_>>>()?;
    require(
        before == after,
        "paramsd controller source or binary changed during run",
    )?;
    signals.check()?;
    fs::write(
        options.evidence.join("source-identities.json"),
        serde_json::to_vec_pretty(&json!({
            "daemon":before[0], "phase_hook":before[1],
            "continuous_input":before[2], "restart_input":before[3],
            "unchanged_after_run":true
        }))?,
    )?;
    fs::write(
        options.evidence.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "pass":true,
            "continuous_publications":1202,
            "restart_publications":8,
            "native_source_inputs_unchanged":true,
            "source_comparison":"pending independent original comparator"
        }))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_exited_child_is_marked_reaped_before_cleanup() {
        let child = Command::new("/bin/true").process_group(0).spawn().unwrap();
        let mut daemon = Daemon {
            child,
            reaped: false,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match daemon.alive() {
                Ok(()) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(()) => panic!("short-lived child did not exit"),
                Err(error) => {
                    assert!(error.to_string().contains("exited before input"));
                    assert!(daemon.reaped, "Drop would signal a reaped group number");
                    break;
                }
            }
        }
    }
}
