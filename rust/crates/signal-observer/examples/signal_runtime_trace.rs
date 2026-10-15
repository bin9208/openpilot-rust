//! Isolated real VisionIPC/worker lifecycle gate, never a vehicle timing result.
use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer, VisionStream};
use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::{
    backend,
    runtime::{self, Config, Error},
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
fn command() -> Result<Command, Error> {
    let binary = env::current_exe()?;
    if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut c = Command::new(words.next().ok_or("empty target runner")?);
            c.args(words).arg(binary);
            return Ok(c);
        }
    }
    Ok(Command::new(binary))
}
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn wait_for(mut condition: impl FnMut() -> bool) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        if Instant::now() >= deadline {
            return Err("signal runtime fixture timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
fn worker(root: &Path, lock_only: bool, startup: bool) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    let config = Config {
        directory: root.join("worker"),
        start: root.join("start.json"),
        prepare: root.join("prepare.json"),
        display: root.join("display.json"),
        frames: if lock_only || startup { None } else { Some(2) },
        duration: Some(Duration::from_secs(30)),
    };
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(if startup {
            "startup-events.jsonl"
        } else if lock_only {
            "lock-events.jsonl"
        } else {
            "events.jsonl"
        }))?;
    let selected = AtomicU8::new(if startup { 1 } else { 2 });
    runtime::run_classical(
        &config,
        &stop,
        &mut || {
            if root.join("mode-off").exists() {
                Mode::Off
            } else {
                Mode::from_parameter(i64::from(selected.load(Ordering::Relaxed)))
            }
        },
        &mut |name, fields| {
            if startup && name == "signalTrackingShadowLoaded" {
                selected.store(2, Ordering::Relaxed);
            }
            writeln!(log, "{}", json!({"event":name,"fields":fields}))?;
            Ok(())
        },
    )
}
fn controller(root: &Path) -> Result<(), Error> {
    // A child that acknowledges TERM but remains alive must be killed and joined
    // before a replacement could own GPU resources.
    let ready = root.join("ignore-term-ready");
    let mut stubborn = Guard(command()?.arg("ignore-term").arg(&ready).spawn()?);
    wait_for(|| ready.exists())?;
    let began = Instant::now();
    let status = backend::stop(&mut stubborn.0)?;
    use std::os::unix::process::ExitStatusExt;
    if status.signal() != Some(9)
        || began.elapsed() < Duration::from_secs(2)
        || stubborn.0.try_wait()?.is_none()
    {
        return Err("backend escalation failed to join the old owner".into());
    }

    let server = VisionServer::new("camerad")?;
    let layout = VisionLayout {
        width: 1344,
        height: 760,
        stride: 1408,
        uv_offset: 1408 * 760 + 128,
        len: 1408 * 760 + 128 + 1408 * 380,
    };
    let images = server.create_stream(VisionStream::Road, 2, layout)?;
    let mut bytes = vec![128; layout.len];
    for y in 0..760 {
        bytes[y * layout.stride..y * layout.stride + 1344].fill(119);
    }
    for image in &images {
        image.write(0, &bytes)?;
    }
    server.start_listener()?;
    fs::create_dir(root.join("worker"))?;
    for flag in [
        "start_guard_enabled",
        "stop_prepare_enabled",
        "display_enabled",
    ] {
        fs::write(root.join("worker").join(flag), b"1")?;
    }
    let mut worker = Guard(command()?.arg("worker").arg(root).spawn()?);
    let events = root.join("events.jsonl");
    wait_for(|| {
        records(&events)
            .iter()
            .any(|v| v["event"] == "signalTrackingShadowLoaded")
    })?;
    let status = command()?.arg("lock-only").arg(root).status()?;
    if !status.success() || !records(&root.join("lock-events.jsonl")).is_empty() {
        return Err("exclusive worker lock admitted a second observer".into());
    }
    let publish = |id: u32, age: u64| -> Result<(), Error> {
        let now = runtime::monotonic_ns();
        images[0].publish(VisionMetadata {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            uv_offset: layout.uv_offset,
            len: layout.len,
            frame_id: id,
            timestamp_sof: now.saturating_sub(age),
            timestamp_eof: now.saturating_sub(age),
            valid: true,
            received: true,
            index: 0,
            fd: 0,
        })?;
        Ok(())
    };
    // Keep the stale identity visible until the connected worker consumes it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !records(&events)
        .iter()
        .any(|v| v["fields"]["reason"] == "stale_input")
    {
        publish(1, 151_000_000)?;
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("stale input gate was not exercised".into());
        }
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while !records(&events)
        .iter()
        .any(|v| v["event"] == "signalTrackingShadow")
    {
        publish(10, 0)?;
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("first native frame was not processed".into());
        }
    }
    let first = records(&events)
        .into_iter()
        .find(|v| v["event"] == "signalTrackingShadow")
        .ok_or("missing first record")?;
    for _ in 0..25 {
        publish(10, 0)?;
        thread::sleep(Duration::from_millis(20));
    }
    if records(&events)
        .iter()
        .filter(|v| v["event"] == "signalTrackingShadow")
        .count()
        != 1
    {
        return Err("duplicate capture produced new observation".into());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while worker.0.try_wait()?.is_none() {
        publish(9, 0)?;
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("rollback frame did not complete worker".into());
        }
    }
    if !worker.0.wait()?.success() {
        return Err("native observer child failed".into());
    }
    let all = records(&events);
    let result = all
        .iter()
        .filter(|v| v["event"] == "signalTrackingShadow")
        .collect::<Vec<_>>();
    if result.len() != 2
        || result[1]["fields"]["frame_gap"] != -1
        || result[1]["fields"]["session"] == first["fields"]["session"]
    {
        return Err("rollback did not reset tracker/session".into());
    }
    for (name, engine) in [
        ("start.json", false),
        ("prepare.json", false),
        ("display.json", true),
    ] {
        let payload: Value = serde_json::from_slice(&fs::read(root.join(name))?)?;
        if payload["frame_id"] != 9
            || payload["stream"] != "road"
            || payload["size"] != json!([1344, 760])
            || payload["session"] != result[1]["fields"]["session"]
            || payload.get("control_permission").is_some()
            || engine && payload["engine"] != "trial"
        {
            return Err("worker publication envelope mismatch".into());
        }
    }
    let latest: Value =
        serde_json::from_slice(&fs::read(root.join("worker/tracking_latest.json"))?)?;
    if !result.iter().any(|entry| entry["fields"] == latest)
        || root.join("worker/tracking_latest.tmp").exists()
    {
        return Err("latest snapshot is not a complete published record".into());
    }
    let mut switched = Guard(command()?.arg("lock-only").arg(root).spawn()?);
    wait_for(|| {
        records(&root.join("lock-events.jsonl"))
            .iter()
            .any(|v| v["event"] == "signalTrackingShadowLoaded")
    })?;
    fs::write(root.join("mode-off"), b"1")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while switched.0.try_wait()?.is_none() {
        if Instant::now() > deadline {
            return Err("mode change did not stop observer".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !switched.0.wait()?.success() {
        return Err("mode change worker failed".into());
    }
    fs::remove_file(root.join("mode-off"))?;
    fs::remove_file(root.join("lock-events.jsonl"))?;
    let mut changed = Guard(command()?.arg("lock-only").arg(root).spawn()?);
    wait_for(|| {
        records(&root.join("lock-events.jsonl"))
            .iter()
            .any(|v| v["event"] == "signalTrackingShadowLoaded")
    })?;
    // Source snapshots this file independently of the forced production
    // comparison tracker. A change must return even if mode is unchanged.
    fs::write(root.join("worker/daytime_comparison_enabled"), b"1")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while changed.0.try_wait()?.is_none() {
        if Instant::now() > deadline {
            return Err("comparison selection change did not stop observer".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !changed.0.wait()?.success() {
        return Err("comparison selection worker failed".into());
    }
    let mut startup = Guard(command()?.arg("startup-worker").arg(root).spawn()?);
    wait_for(|| {
        records(&root.join("startup-events.jsonl"))
            .iter()
            .any(|v| v["event"] == "signalTrackingShadowLoaded")
    })?;
    thread::sleep(Duration::from_millis(100));
    if startup.0.try_wait()?.is_some() {
        return Err("initial mode was frozen before Loaded instead of after it".into());
    }
    fs::write(root.join("mode-off"), b"1")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while startup.0.try_wait()?.is_none() {
        if Instant::now() > deadline {
            return Err("startup mode change worker did not stop".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    if !startup.0.wait()?.success() {
        return Err("startup mode change child failed".into());
    }
    println!(
        "{}",
        json!({"frames":2,"stale_input":true,"duplicate_rejected":true,"rollback_reset":true,"exclusive_lock":true,"backend_kill_join":true,"mode_exit":true,"comparison_exit":true,"atomic_latest":true,"startup_mode_snapshot":true,"publications":3,"control_permission":false})
    );
    Ok(())
}
fn run() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("worker") => worker(
            &PathBuf::from(args.next().ok_or("missing worker directory")?),
            false,
            false,
        ),
        Some("lock-only") => worker(
            &PathBuf::from(args.next().ok_or("missing worker directory")?),
            true,
            false,
        ),
        Some("startup-worker") => worker(
            &PathBuf::from(args.next().ok_or("missing worker directory")?),
            false,
            true,
        ),
        Some("controller") => controller(&PathBuf::from(
            args.next().ok_or("missing controller directory")?,
        )),
        Some("ignore-term") => {
            let flag = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal_hook::consts::SIGTERM, flag)?;
            fs::write(args.next().ok_or("missing ready path")?, b"ready")?;
            loop {
                thread::sleep(Duration::from_millis(20));
            }
        }
        None => {
            let prefix = format!("rust-probe-signal-live-{}", std::process::id());
            let namespace = PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
            let root = env::temp_dir().join(&prefix);
            fs::create_dir(&namespace)?;
            fs::create_dir(&root)?;
            let result = command()?
                .arg("controller")
                .arg(&root)
                .env("OPENPILOT_PREFIX", prefix)
                .env_remove("CEREAL_FAKE")
                .status();
            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&namespace);
            if !result?.success() {
                return Err("signal runtime controller failed".into());
            }
            Ok(())
        }
        _ => Err("unknown fixture stage".into()),
    }
}
fn main() -> Result<(), Error> {
    run()
}
