use super::{bind, require, validate_flood, validate_runtime};
use crate::stats_daemon_check::{
    process::{check, Process},
    worker::Collector,
};
use crate::{python_fs, seal::Result};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::JsonValue;
use openpilot_msgq::Publisher;
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn paths(directory: &Path) -> Result<Vec<PathBuf>> {
    Ok(fs::read_dir(directory)?
        .map(|row| row.map(|row| row.path()))
        .collect::<std::io::Result<_>>()?)
}
fn wall_time() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .try_into()?)
}
fn device(publisher: &mut Publisher, started: bool) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_valid(true);
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        event.set_log_mono_time((now.tv_sec as u64) * 1_000_000_000 + now.tv_nsec as u64);
        event.init_device_state().set_started(started);
    }
    publisher.send(&capnp::serialize::write_message_to_words(&message))?;
    Ok(())
}
fn ticks(process: &Process<'_>) -> Result<u64> {
    let text = fs::read_to_string(format!("/proc/{}/stat", process.pid().as_raw_pid()))?;
    Ok(text
        .rsplit_once(')')
        .ok_or("invalid process stat")?
        .1
        .split_whitespace()
        .nth(11)
        .ok_or("missing user CPU ticks")?
        .parse()?)
}
fn child_command(binary: &Path, root: &Path, stdout: &Path, stderr: &Path) -> Result<Command> {
    let mut command = Command::new(binary);
    command
        .current_dir(root)
        .env_remove("STATS_RUNTIME_ENDPOINT_OWNER")
        .env_remove("STATS_ENDPOINT_OWNER")
        .stdout(Stdio::from(File::create(stdout)?))
        .stderr(Stdio::from(File::create(stderr)?));
    Ok(command)
}
fn wait_started(daemon: &Process<'_>, stats: &Path, deadline: Instant) -> Result<()> {
    while !stats.exists() {
        require(
            daemon.exited()?.is_none(),
            "production daemon exited before stats directory",
        )?;
        require(Instant::now() < deadline, "production startup exceeded10s")?;
        std::thread::sleep(Duration::from_millis(10));
    }
    require(
        daemon.exited()?.is_none(),
        "production daemon exited before endpoint acknowledgement",
    )?;
    let channel = std::env::var_os("STATS_RUNTIME_ENDPOINT_OWNER")
        .ok_or("missing statistics endpoint owner")?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    // The trusted worker observes the endpoint while this real candidate is
    // alive. The outer owner reserves its filesystem inode before ACK returns.
    // As with the source, fixed /tmp/stats requires exclusive host execution.
    crate::qa_endpoint::acknowledge(
        Path::new("/tmp/stats"),
        Path::new(&channel),
        &prefix,
        &AtomicBool::new(false),
    )
}
fn runtime(
    daemon: &Process<'_>,
    publisher: &mut Publisher,
    stats: &Path,
    root: &Path,
    binary: &Path,
    hash: &str,
    deadline: Instant,
) -> Result<JsonValue> {
    let context = zmq::Context::new();
    let producer = context.socket(zmq::PUSH)?;
    producer.set_sndtimeo(5000)?;
    producer.set_linger(10)?;
    producer.connect("ipc:///tmp/stats")?;
    let before = wall_time()?;
    producer.send("runtime:7|g", 0)?;
    let mut started = false;
    while paths(stats)?.is_empty() {
        device(publisher, started)?;
        started = !started;
        require(
            daemon.exited()?.is_none(),
            "runtime daemon exited during transitions",
        )?;
        require(
            Instant::now() < deadline,
            "runtime publication exceeded startup10s deadline",
        )?;
        std::thread::sleep(Duration::from_millis(100));
    }
    let after = wall_time()?;
    let files = paths(stats)?;
    require(files.len() == 1, "runtime did not publish exactly one file")?;
    let text = fs::read_to_string(&files[0])?;
    let stamp: u64 = text
        .rsplit(' ')
        .next()
        .ok_or("missing file timestamp")?
        .trim()
        .parse()?;
    // Validate the original file/time predicates before signalling, just as
    // the source does. The shutdown check below uses the actual child status.
    let mut observed =
        json!({"before_ns":before,"file_timestamp_ns":stamp,"after_ns":after,"file":text,"exit":0});
    validate_runtime(
        &observed,
        files.len(),
        fs::metadata(&files[0])?.permissions().mode(),
    )?;
    daemon.signal(Signal::TERM)?;
    let code = daemon.wait(Some(Duration::from_secs(5)))?;
    observed["exit"] = json!(code);
    validate_runtime(
        &observed,
        paths(stats)?.len(),
        fs::metadata(&files[0])?.permissions().mode(),
    )?;
    JsonValue::parse(&format!(
        concat!(
            "{{\"result\":\"PASS\",\"argv\":[{}],\"cwd\":{},",
            "\"binary_sha256\":{},\"before_ns\":{},\"file_timestamp_ns\":{},\"after_ns\":{},",
            "\"file\":{},\"shutdown_signal\":\"SIGTERM\",\"exit\":{}}}"
        ),
        python_fs::json_string(binary.as_os_str())?,
        python_fs::json_string(root.as_os_str())?,
        serde_json::to_string(hash)?,
        before,
        stamp,
        after,
        serde_json::to_string(&text)?,
        code
    ))
    .map_err(Into::into)
}
struct Flood<'a> {
    binaries: &'a Path,
    stats: &'a Path,
    output: &'a Path,
    binary: &'a Path,
    hash: &'a str,
    deadline: Instant,
    cancel: &'a AtomicBool,
}
fn flood(daemon: &Process<'_>, context: Flood<'_>, signum: Signal) -> Result<JsonValue> {
    let Flood {
        binaries,
        stats,
        output,
        binary,
        hash,
        deadline,
        cancel,
    } = context;
    let baseline = ticks(daemon)?;
    let producer_binary = binaries.join("examples/stats_flood");
    let mut producers = Vec::new();
    let mut bindings = Vec::new();
    for index in 0..2 {
        let mut command = Command::new(&producer_binary);
        command
            .arg("ipc:///tmp/stats")
            .env_remove("STATS_RUNTIME_ENDPOINT_OWNER")
            .env_remove("STATS_ENDPOINT_OWNER")
            .stderr(Stdio::from(File::create(
                output.join(format!("producer-{index}.stderr")),
            )?));
        bindings.push(bind(&producer_binary, "stats_flood")?);
        let mut producer = Process::spawn(&mut command, cancel, false)?;
        require(
            producer.line(Some(Duration::from_secs(10)))? == "ready",
            "flood producer did not acknowledge ready",
        )?;
        producers.push(producer);
    }
    // Diagnostic acknowledgement follows both actual ready lines. Tests can
    // cancel an idle producer fixture here without guessing startup latency.
    let children = fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id()))?;
    let mut pids = vec![std::process::id()];
    for child in children.split_whitespace() {
        pids.push(child.parse::<u32>()?);
    }
    fs::write(
        output.join("producer-ready.json"),
        serde_json::to_vec(&json!({
            "pids":pids,"prefix":std::env::var("OPENPILOT_PREFIX")?
        }))?,
    )?;
    while ticks(daemon)?.saturating_sub(baseline) < 10 {
        require(
            daemon.exited()?.is_none()
                && producers
                    .iter()
                    .all(|p| p.exited().is_ok_and(|v| v.is_none())),
            "daemon/producer exited before10 busy user ticks",
        )?;
        require(
            Instant::now() < deadline,
            "flood did not reach10 user ticks within startup10s",
        )?;
        std::thread::sleep(Duration::from_millis(10));
    }
    let busy = ticks(daemon)?
        .checked_sub(baseline)
        .ok_or("CPU ticks went backwards")?;
    let start = Instant::now();
    daemon.signal(signum)?;
    // Preserve the original timeout report (null exit) before rejecting it.
    let code = loop {
        check(cancel)?;
        if let Some(code) = daemon.exited()? {
            break Some(code);
        }
        if start.elapsed() >= Duration::from_secs(2) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let elapsed = start.elapsed().as_secs_f64();
    let alive = producers
        .iter()
        .all(|p| p.exited().is_ok_and(|v| v.is_none()));
    let observed: Value = json!({"signal":signum.as_raw(),"exit":code,"shutdown_seconds":elapsed,
        "busy_cpu_ticks":busy,"producers_alive":alive});
    let argv = python_fs::json_string(binary.as_os_str())?;
    let producer_argv = format!(
        "[{},\"ipc:///tmp/stats\"]",
        python_fs::json_string(producer_binary.as_os_str())?
    );
    let report = JsonValue::parse(&format!(
        concat!(
            "{{\"signal\":{},\"exit\":{},\"shutdown_seconds\":{},",
            "\"busy_cpu_ticks\":{},\"producers_alive\":{},\"binary_sha256\":{},\"argv\":[{}],",
            "\"producer_argv\":[{},{}]}}"
        ),
        signum.as_raw(),
        serde_json::to_string(&code)?,
        elapsed,
        busy,
        alive,
        serde_json::to_string(hash)?,
        argv,
        producer_argv,
        producer_argv
    ))?;
    // Per-signal result is an observation, not a root PASS receipt. The source
    // retains it even when its later shutdown/files assertions fail.
    fs::write(output.join("result.json"), report.to_json()?)?;
    validate_flood(&observed, paths(stats)?.len())?;
    for file in &bindings {
        file.verify()?;
    }
    drop(producers);
    Ok(report)
}
pub(super) fn run(case: &str, binaries: &Path, output: &Path, worker: &Path) -> Result<JsonValue> {
    let signum = match case {
        "runtime" => None,
        "SIGINT" => Some(Signal::INT),
        "SIGTERM" => Some(Signal::TERM),
        _ => return Err("unknown production statistics case".into()),
    };
    let cancel = AtomicBool::new(false);
    let mut collector = Collector::start(output, worker, &cancel)?;
    let root = output.join("source");
    fs::create_dir(&root)?;
    fs::write(
        root.join("build.json"),
        if signum.is_none() {
            r#"{"channel": "runtime-fixture", "openpilot": {"version": "v", "git_origin": "https://github.com/test/openpilot.git"}}"#
        } else {
            r#"{"openpilot": {"version": "v", "git_origin": "git@github.com:test/openpilot.git"}}"#
        },
    )?;
    let prefix = std::env::var_os("OPENPILOT_PREFIX").ok_or("missing prefix")?;
    let home = std::env::var_os("HOME").ok_or("missing home")?;
    let mut basename = std::ffi::OsString::from(".comma");
    basename.push(prefix);
    let stats = Path::new(&home).join(basename).join("stats");
    let mut publisher = Publisher::for_runtime("deviceState", 250 * 1024)?;
    let binary = binaries.join("statsd-rs");
    let (stdout, stderr) = if signum.is_none() {
        ("stdout.log", "stderr.log")
    } else {
        ("daemon.stdout", "daemon.stderr")
    };
    let mut command = child_command(&binary, &root, &output.join(stdout), &output.join(stderr))?;
    let bound = bind(&binary, "statsd-rs")?;
    // Preserve the original file-backed stdout and the reserved PID until Drop.
    let daemon = Process::spawn_logged(&mut command, &cancel, false)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    wait_started(&daemon, &stats, deadline)?;
    let report = if let Some(signum) = signum {
        flood(
            &daemon,
            Flood {
                binaries,
                stats: &stats,
                output,
                binary: &binary,
                hash: &bound.hash,
                deadline,
                cancel: &cancel,
            },
            signum,
        )?
    } else {
        runtime(
            &daemon,
            &mut publisher,
            &stats,
            &root,
            &binary,
            &bound.hash,
            deadline,
        )?
    };
    bound.verify()?;
    drop(daemon);
    let _records = collector.stop()?;
    Ok(report)
}
