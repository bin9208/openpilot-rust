//! Synthetic native IPC/Params/owned-child integration; never selects production daemons.
use openpilot_cereal::log_capnp::{event, panda_state::PandaType};
use openpilot_manager::{
    lifecycle::{self, Environment, ExitAction, Runtime},
    processes::{Entry, Processes},
    runtime::{self, ExitBoundary, NativeRuntime},
    Error,
};
use openpilot_manager_catalog::{Descriptor, Predicate, RustAvailability, SourceProcess};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_process_supervision::{Execution, ManagedProcess, NativeCommand, ProcessLog};
use std::{path::PathBuf, time::Duration};
struct Boundary {
    exited: bool,
}
impl ExitBoundary for Boundary {
    fn capture_exception(&mut self, error: &Error) -> Result<(), Error> {
        Err(Error::Io(std::io::Error::other(error.to_string())))
    }
    fn exit(&mut self, action: ExitAction) -> Result<(), Error> {
        assert_eq!(action, ExitAction::Shutdown);
        self.exited = true;
        Ok(())
    }
}
fn packet(topic: &str, started: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(true);
    match topic {
        "deviceState" => root.init_device_state().set_started(started),
        "carParams" => root.init_car_params().set_not_car(false),
        "pandaStates" => {
            let mut panda = root.init_panda_states(2);
            panda.reborrow().get(0).set_ignition_line(true);
            let mut known = panda.get(1);
            known.set_panda_type(PandaType::Dos);
            known.set_ignition_can(started);
        }
        _ => panic!("fixture topic"),
    }
    capnp::serialize::write_message_to_words(&message)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let output = PathBuf::from(&args[1]);
    let launcher = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    let namespace = tempfile::Builder::new()
        .prefix("msgq_rust-probe-manager-")
        .tempdir_in("/dev/shm")?;
    let prefix = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    std::env::set_var("OPENPILOT_PREFIX", prefix);
    std::env::remove_var("CEREAL_FAKE");
    let factory =
        openpilot_logging::producer::Factory::new(format!("ipc://{}/logging", output.display()))?;
    let command = NativeCommand {
        launcher: launcher.clone(),
        basedir: output.clone(),
        cwd: PathBuf::from("."),
        argv: vec!["/bin/sleep".into(), "60".into()],
    };
    let process = ManagedProcess::new(
        "fixture".into(),
        Execution::Native(command),
        ProcessLog::new(factory.logger()),
    );
    let descriptor = Descriptor {
        name: "fixture",
        source: SourceProcess::Native {
            cwd: ".",
            argv: &[],
        },
        enabled: true,
        sigkill: false,
        restart_if_crash: false,
        daemon: false,
        predicate: Predicate::Onroad,
        rust: RustAvailability::NotPorted,
    };
    let params = openpilot_params::Params::open(&output.join("params"), "d")?;
    params.put("DongleId", b"fixture")?;
    std::fs::write(
        output.join("build.json"),
        r#"{"openpilot":{"git_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
    )?;
    let mut upstream = PubMaster::isolated(&["deviceState", "carParams", "pandaStates"])?;
    let (subscriber, publisher) = runtime::subscriptions(true)?;
    let mut observer = SubMaster::isolated(
        &["managerState"],
        Options {
            poll: Poll::One("managerState".into()),
            ..Options::default()
        },
    )?;
    let mut runtime = NativeRuntime {
        params,
        processes: Processes {
            entries: vec![Entry {
                descriptor,
                process: Some(process),
            }],
        },
        subscriber,
        publisher,
        update_status: openpilot_checkout_status::UpdateStatus::new(&output, &launcher),
        watchdog_path: output.join("watchdog"),
        logger: factory.logger(),
        boundary: Boundary { exited: false },
        signals: openpilot_manager::signals::Signals::install()?,
        loop_time: Duration::ZERO,
    };
    for topic in ["carParams", "pandaStates", "deviceState"] {
        upstream.send(topic, &packet(topic, true))?;
    }
    let input = runtime.poll()?;
    assert!(input.started && input.ignition && !input.not_car);
    runtime.ensure_running(input.started, input.not_car, &[])?;
    let states = runtime.states()?;
    assert!(states[0].running && states[0].pid > 0 && states[0].should_be_running);
    let pid = states[0].pid;
    runtime.publish(&states)?;
    observer.update(Duration::from_secs(1))?;
    let event::ManagerState(message) = observer.state.topic("managerState")?.event()?.which()?
    else {
        panic!("wrong union")
    };
    let message = message?;
    assert!(!message.get_reboot_required());
    assert_eq!(message.get_processes()?.get(0).get_pid(), pid);
    runtime.watchdog()?;
    assert!(std::fs::read_to_string(output.join("watchdog"))?.parse::<f64>()? > 0.0);
    for topic in ["pandaStates", "deviceState"] {
        upstream.send(topic, &packet(topic, false))?;
    }
    let input = runtime.poll()?;
    assert!(
        !input.started && !input.ignition,
        "unknown panda ignition must be ignored"
    );
    runtime.ensure_running(false, false, &[])?;
    runtime.stop(false)?;
    runtime.stop(true)?;
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    runtime.params.put_bool("DoShutdown", true)?;
    upstream.send("deviceState", &packet("deviceState", false))?;
    let external_params = openpilot_params::Params::open(&output.join("params"), "d")?;
    lifecycle::run(&external_params, &mut runtime, &Environment::default())?;
    assert!(runtime.boundary.exited);
    assert!(external_params.get_bool("IsOffroad")?);
    runtime.boundary.exited = false;
    let signal = std::process::Command::new("/bin/kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()?;
    assert!(signal.success());
    assert!(matches!(
        lifecycle::run(&external_params, &mut runtime, &Environment::default()),
        Err(Error::Interrupted)
    ));
    assert!(!runtime.boundary.exited);
    // Requested unported entry fails without starting a placeholder or Python.
    runtime.processes.entries[0].process = None;
    assert!(matches!(
        runtime.ensure_running(true, false, &[]),
        Err(Error::Unavailable(_))
    ));
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"native_ipc":true,"manager_state_pid":pid,"owned_child_reaped":true,"watchdog":true,"unknown_panda_ignored":true,"shutdown_boundary":true,"missing_daemon_rejected":true,"sigterm_cleanup_without_hardware_exit":true}),
        )?,
    )?;
    println!(
        "PASS native manager IPC, owned child, watchdog, shutdown and missing-daemon boundary"
    );
    Ok(())
}
