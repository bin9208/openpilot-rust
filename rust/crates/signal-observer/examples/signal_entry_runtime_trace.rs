//! Real native entry/automatic classical child gate, with isolated files/IPC.
//! A QCOM label tests the missing-cache failure only; no GPU is opened.
use openpilot_msgq::{VisionLayout, VisionServer, VisionStream};
use openpilot_params::Params;
use openpilot_plannerd::signal::transport::ModeCache;
use openpilot_signal_observer::{
    backend::Backend,
    entry::Entry,
    runtime::{self, Config, Error},
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};
fn command() -> Result<Command, Error> {
    let binary = env::current_exe()?;
    if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut command = Command::new(words.next().ok_or("empty runner")?);
            command.args(words).arg(binary);
            return Ok(command);
        }
    }
    Ok(Command::new(binary))
}
fn config(root: &Path) -> Config {
    Config {
        directory: root.join("worker"),
        start: root.join("start.json"),
        prepare: root.join("prepare.json"),
        display: root.join("display.json"),
        frames: None,
        duration: None,
    }
}
fn run_entry(
    root: &Path,
    child: Option<Backend>,
    duration: Option<Duration>,
    stopped: bool,
) -> Result<Vec<Value>, Error> {
    let params = Params::for_runtime_at(&root.join("params"))?;
    let stop = Arc::new(AtomicBool::new(stopped));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    let mut cache = ModeCache::default();
    let mut mode = || {
        cache.read(runtime::monotonic_ns() as f64 / 1e9, || {
            params
                .get("SignalAssistMode")
                .map(|v| runtime::parameter_value(v.as_deref().unwrap_or_default()))
        })
    };
    let mut config = config(root);
    config.duration = duration;
    let mut events = vec![];
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(if child.is_some() {
            "child.jsonl"
        } else {
            "parent.jsonl"
        }))?;
    let result = Entry {
        config: &config,
        stop: &stop,
        params: &params,
        provider: None,
        child,
    }
    .run(
        &mut mode,
        &mut |name, fields| {
            let event = json!({"event":name,"fields":fields});
            writeln!(log, "{event}")?;
            events.push(event);
            Ok(())
        },
        &mut |backend| {
            let name = match backend {
                Backend::TrackingCpu => "tracking_cpu",
                Backend::Qcom => "qcom",
                Backend::Cpu => return Err("unexpected CPU child".into()),
            };
            let mut command = command()?;
            command.arg("child").arg(name).arg(root);
            Ok(command)
        },
    );
    if let Err(error) = result {
        writeln!(log, "{}", json!({"error":error.to_string()}))?;
        return Err(error);
    }
    Ok(events)
}
fn setup(root: &Path, mode: u8, backend: &str, usb: bool) -> Result<(), Error> {
    fs::create_dir(root)?;
    fs::create_dir(root.join("worker"))?;
    for (name, value) in [
        ("enabled", "1"),
        ("neural_enabled", "1"),
        ("neural_backend", backend),
    ] {
        fs::write(root.join("worker").join(name), value)?;
    }
    let params = Params::for_runtime_at(&root.join("params"))?;
    params.put("SignalAssistMode", mode.to_string().as_bytes())?;
    for (name, value) in [
        ("UsbGpuActive", usb),
        ("DriverMonitoringEnabled", false),
        ("DriverMonitoringSessionDisabled", false),
        ("IsDriverViewEnabled", false),
    ] {
        params.put_bool(name, value)?;
    }
    Ok(())
}
fn loaded(root: &Path) -> bool {
    fs::read_to_string(root.join("child.jsonl"))
        .unwrap_or_default()
        .contains("signalTrackingShadowLoaded")
}
fn reaped(events: &[Value]) -> Result<(), Error> {
    for event in events {
        if let Some(pid) = event["fields"]["pid"].as_u64() {
            let pid = rustix::process::Pid::from_raw(i32::try_from(pid)?).ok_or("invalid PID")?;
            if rustix::process::test_kill_process(pid).is_ok() {
                return Err("entry child survived return".into());
            }
        }
    }
    Ok(())
}
fn parent_events(root: &Path) -> Result<Vec<Value>, Error> {
    Ok(fs::read_to_string(root.join("parent.jsonl"))?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<Value>, _>>()?)
}
fn controller(root: &Path) -> Result<(), Error> {
    let server = VisionServer::new("camerad")?;
    let _images = server.create_stream(
        VisionStream::Road,
        2,
        VisionLayout {
            width: 1344,
            height: 760,
            stride: 1344,
            uv_offset: 1344 * 760,
            len: 1344 * 760 * 3 / 2,
        },
    )?;
    server.start_listener()?;
    let prepare = root.join("prepare");
    setup(&prepare, 2, "invalid selection must not be read", true)?;
    let events = run_entry(&prepare, None, None, true)?;
    assert!(
        events
            .iter()
            .any(|event| event["event"] == "signalTrackingShadowLoaded")
    );
    let cpu = root.join("cpu");
    setup(&cpu, 1, "cpu", false)?;
    assert_eq!(
        run_entry(&cpu, None, None, false).unwrap_err().to_string(),
        "native signal ORT provider root is not configured"
    );
    let automatic = root.join("auto");
    setup(&automatic, 1, "auto", false)?;
    let events = run_entry(&automatic, None, Some(Duration::from_secs(4)), false)?;
    assert!(loaded(&automatic));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["fields"]["backend"], "tracking_cpu");
    reaped(&events)?;
    let ordinary = root.join("ordinary-return");
    setup(&ordinary, 1, "auto", false)?;
    let writer_root = ordinary.clone();
    let writer = thread::spawn(move || -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !loaded(&writer_root) {
            if Instant::now() > deadline {
                return Err("classical child did not load".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        fs::write(writer_root.join("worker/daytime_comparison_enabled"), b"1")
            .map_err(|e| e.to_string())?;
        Ok(())
    });
    assert_eq!(
        run_entry(&ordinary, None, Some(Duration::from_secs(25)), false)
            .unwrap_err()
            .to_string(),
        "signal backend tracking_cpu exited: 0"
    );
    reaped(&parent_events(&ordinary)?)?;
    writer
        .join()
        .map_err(|_| "comparison writer panic")?
        .map_err(std::io::Error::other)?;
    let qcom = root.join("qcom-missing");
    setup(&qcom, 1, "auto", true)?;
    assert_eq!(
        run_entry(&qcom, None, Some(Duration::from_secs(10)), false)
            .unwrap_err()
            .to_string(),
        "signal backend qcom exited: 1"
    );
    reaped(&parent_events(&qcom)?)?;
    assert!(
        fs::read_to_string(qcom.join("child.jsonl"))?
            .contains("auto neural mode requires locally validated QCOM caches")
    );
    println!(
        "{}",
        json!({"mode2_forces_classical":true,"explicit_cpu_provider_required":true,"automatic_classical_child":true,"ordinary_child_return_is_fault":true,"missing_qcom_is_fault":true,"cpu_neural_fallback":false,"gpu_access":false})
    );
    Ok(())
}
fn main() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("child") => {
            let backend = match args.next().as_deref() {
                Some("tracking_cpu") => Backend::TrackingCpu,
                Some("qcom") => Backend::Qcom,
                _ => return Err("invalid child".into()),
            };
            run_entry(
                &PathBuf::from(args.next().ok_or("missing root")?),
                Some(backend),
                None,
                false,
            )?;
            Ok(())
        }
        Some("controller") => controller(&PathBuf::from(args.next().ok_or("missing root")?)),
        None => {
            let prefix = format!("rust-probe-signal-entry-{}", std::process::id());
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
            let _ = fs::remove_dir_all(root);
            let _ = fs::remove_dir_all(namespace);
            if !result?.success() {
                return Err("native entry controller failed".into());
            }
            Ok(())
        }
        _ => Err("invalid fixture stage".into()),
    }
}
