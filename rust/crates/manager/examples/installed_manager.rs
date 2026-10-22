//! Drives the installed manager with genuine native children in owned host storage.
mod installed_probe;

use installed_probe::{packet, Probe};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{
    runtime::SubMaster,
    state::{Options, Poll},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn send_metric(socket: &zmq::Socket) -> Result<(), zmq::Error> {
    loop {
        match socket.send("invalid_manager_probe_metric", 0) {
            Ok(()) => return Ok(()),
            Err(zmq::Error::EINTR) => eprintln!("probe metric send interrupted; retrying"),
            Err(error) => return Err(error),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let output = PathBuf::from(arguments.first().ok_or("output directory missing")?);
    let root = PathBuf::from(arguments.get(1).ok_or("source root missing")?);
    let binaries = PathBuf::from(arguments.get(2).ok_or("binary directory missing")?);
    let mode = arguments
        .get(3)
        .and_then(|value| value.to_str())
        .ok_or("mode missing")?;
    let runtime_mode = if mode.starts_with("stdout-")
        || mode.starts_with("handoff-")
        || matches!(mode, "assets-override" | "log-route")
    {
        "normal"
    } else {
        mode
    };
    let mut probe = Probe::new(&output, &root, &binaries, runtime_mode)?;
    if mode == "assets-override" {
        probe.model_assets_override(&output.join("explicit-native-assets"));
    }
    if mode == "log-route" {
        probe.enable_route_logger(&binaries)?;
    }
    let _recipient = match mode {
        "stdout-full" => probe.stdout_pressure(true)?,
        "stdout-closed" => probe.stdout_pressure(false)?,
        _ => None,
    };
    if mode.starts_with("handoff-") {
        probe.handoff(&binaries.join("manager_handoff"))?;
        if mode == "handoff-overlay-int" {
            probe.handoff(&binaries.join("manager_handoff"))?;
        }
    }
    let mut upstream = openpilot_messaging::runtime::PubMaster::isolated(&[
        "deviceState",
        "carParams",
        "pandaStates",
    ])?;
    let mut observer = SubMaster::isolated(
        &["managerState", "logMessage", "errorLogMessage"],
        Options {
            poll: Poll::One("managerState".into()),
            ..Options::default()
        },
    )?;
    probe.launch()?;
    probe.transfer_lock()?;
    if runtime_mode != "normal" {
        let status = probe.wait()?;
        let text = std::fs::read_to_string(output.join("manager.log"))?;
        let expected = match mode {
            "missing-daemon" => "native daemon unavailable: statsd",
            "missing-bootlog" | "missing-bootlog-ui" => "No such file or directory",
            _ => return Err("unknown probe mode".into()),
        };
        assert!(text.contains(expected), "{text}");
        assert_eq!(status.success(), mode == "missing-daemon");
        if mode == "missing-daemon" {
            assert_eq!(
                probe.params.get("CarrotException")?.as_deref(),
                Some(b"exception".as_slice())
            );
        } else {
            assert!(probe
                .log_files()?
                .iter()
                .any(|path| std::fs::read_to_string(path)
                    .is_ok_and(|log| log.contains("Manager failed to start"))));
        }
        assert!(probe.lock_released()?);
        probe.write_summary(serde_json::json!({"mode":mode,"exit_code":status.code(),"explicit_error":expected,"lock_released":true,"managed_exception":mode == "missing-daemon"}))?;
        println!("PASS installed manager {mode}: {expected}");
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut pids = Vec::new();
    let mut states = Vec::new();
    let mut stats_pid = None;
    while pids.len() != 2 {
        assert!(
            Instant::now() < deadline,
            "manager startup timeout: {}",
            std::fs::read_to_string(output.join("manager.log"))?
        );
        for topic in ["carParams", "pandaStates", "deviceState"] {
            upstream.send(topic, &packet(topic, false)?)?;
        }
        observer.update(Duration::from_millis(500))?;
        let event::ManagerState(message) =
            observer.state.topic("managerState")?.event()?.which()?
        else {
            return Err("managerState union missing".into());
        };
        let message = message?;
        pids.clear();
        states.clear();
        for state in message.get_processes()? {
            let name = state.get_name()?.to_str()?;
            states.push(serde_json::json!({"name":name,"pid":state.get_pid(),"running":state.get_running(),"wanted":state.get_should_be_running()}));
            if matches!(name, "logmessaged" | "statsd") && state.get_running() {
                pids.push(state.get_pid());
                if name == "statsd" {
                    stats_pid = Some(state.get_pid());
                }
            }
        }
    }
    let selected_assets = if mode == "assets-override" {
        output.join("explicit-native-assets")
    } else {
        output.join("bin/usbgpu-assets")
    };
    let expected_assets = [
        b"USBGPU_ASSETS_ROOT=".as_slice(),
        selected_assets.as_os_str().as_encoded_bytes(),
    ]
    .concat();
    let stats_environment = std::fs::read(format!(
        "/proc/{}/environ",
        stats_pid.ok_or("stats PID missing")?
    ))?;
    assert!(
        stats_environment
            .split(|byte| *byte == 0)
            .any(|value| value == expected_assets),
        "native child did not inherit selected GPU assets"
    );
    assert!(probe.lock_released()?);
    assert_eq!(
        probe.params.get("DongleId")?.as_deref(),
        Some(b"UnregisteredDevice".as_slice())
    );
    let commit = String::from_utf8(probe.params.get("GitCommit")?.ok_or("GitCommit missing")?)?;
    assert_eq!(commit.len(), 40);
    let context = zmq::Context::new();
    let metrics = context.socket(zmq::PUSH)?;
    metrics.set_linger(0)?;
    metrics.set_sndtimeo(1000)?;
    metrics.connect(&probe.stats_endpoint()?)?;
    send_metric(&metrics)?;
    let mut child_record = None;
    for started in [true, false] {
        let deadline = Instant::now() + Duration::from_secs(15);
        while probe.params.get_bool("IsOnroad")? != started || child_record.is_none() {
            assert!(Instant::now() < deadline, "transition or child log timeout");
            if child_record.is_none() {
                send_metric(&metrics)?;
            }
            upstream.send("deviceState", &packet("deviceState", started)?)?;
            observer.update(Duration::from_millis(300))?;
            if !observer.state.topic("logMessage")?.updated {
                continue;
            }
            if let event::LogMessage(value) =
                observer.state.topic("logMessage")?.event()?.which()?
            {
                let record: serde_json::Value = serde_json::from_str(value?.to_str()?)?;
                if record["ctx"]["daemon"] == "statsd" {
                    assert_eq!(record["ctx"]["runtime_implementation"], "statsd-rs");
                    assert_eq!(record["ctx"]["commit"], commit);
                    assert_eq!(record["ctx"]["device"], "pc");
                    child_record = Some(record);
                }
            }
        }
        if started && mode == "log-route" {
            let deadline = Instant::now() + Duration::from_secs(15);
            while probe.params.get("CurrentRoute")?.is_none() {
                assert!(
                    Instant::now() < deadline,
                    "manager did not start route logger"
                );
                upstream.send("deviceState", &packet("deviceState", true)?)?;
                observer.update(Duration::from_millis(100))?;
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut logger_pid = None;
            while Instant::now() < deadline {
                send_metric(&metrics)?;
                upstream.send("deviceState", &packet("deviceState", true)?)?;
                observer.update(Duration::from_millis(100))?;
                if let event::ManagerState(message) =
                    observer.state.topic("managerState")?.event()?.which()?
                {
                    for state in message?.get_processes()? {
                        if state.get_name()?.to_str()? == "loggerd" && state.get_running() {
                            logger_pid = Some(state.get_pid());
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            pids.push(logger_pid.ok_or("managed route logger PID missing")?);
            metrics.send([0xff_u8].as_slice(), 0)?;
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut crash_seen = false;
            let mut replacement = None;
            while !crash_seen || replacement.is_none() {
                assert!(
                    Instant::now() < deadline,
                    "managed stats failure/restart timeout"
                );
                upstream.send("deviceState", &packet("deviceState", true)?)?;
                observer.update(Duration::from_millis(100))?;
                if observer.state.topic("errorLogMessage")?.updated {
                    if let event::ErrorLogMessage(value) =
                        observer.state.topic("errorLogMessage")?.event()?.which()?
                    {
                        let record: serde_json::Value = serde_json::from_str(value?.to_str()?)?;
                        crash_seen |=
                            record["msg"] == "crash" && record["ctx"]["daemon"] == "statsd";
                    }
                }
                if let event::ManagerState(message) =
                    observer.state.topic("managerState")?.event()?.which()?
                {
                    for state in message?.get_processes()? {
                        if state.get_name()?.to_str()? == "statsd"
                            && state.get_running()
                            && Some(state.get_pid()) != stats_pid
                        {
                            replacement = Some(state.get_pid());
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            pids.push(replacement.ok_or("replacement stats PID missing")?);
        }
    }
    drop(metrics);
    drop(context);
    let boot = probe
        .params
        .get("CurrentBootlog")?
        .ok_or("native bootlog did not finish")?;
    assert!(!boot.is_empty());
    if mode.starts_with("handoff-") {
        assert!(std::process::Command::new("/bin/kill")
            .args(["-STOP", &stats_pid.ok_or("stats PID missing")?.to_string()])
            .status()?
            .success());
    }
    let stop_time = Instant::now();
    if mode.ends_with("-int") {
        probe.signal("-INT")?;
    } else {
        probe.terminate()?;
    }
    let status = probe.wait()?;
    let cleanup_seconds = stop_time.elapsed().as_secs_f64();
    if mode.starts_with("handoff-") {
        assert!(
            cleanup_seconds >= 4.5,
            "manager killed before cleanup: {cleanup_seconds}"
        );
    }
    assert!(!status.success(), "SIGTERM is source interruption");
    for pid in &pids {
        assert!(
            !PathBuf::from(format!("/proc/{pid}")).exists(),
            "owned child remains: {pid}"
        );
    }
    assert!(probe
        .log_files()?
        .iter()
        .any(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)));
    probe.write_summary(serde_json::json!({"mode":mode,"usbgpu_assets_root":selected_assets,"cleanup_seconds":cleanup_seconds,"exit_code":status.code(),"manager_state":states,"native_children_reaped":pids,"lock_released":true,"git_commit":commit,"bootlog_id":String::from_utf8(boot)?,"offroad_onroad_offroad":true,"native_child_log":child_record}))?;
    println!("PASS installed manager startup, bootlog, lock, native IPC transitions, genuine children, inherited log and SIGTERM cleanup");
    Ok(())
}
