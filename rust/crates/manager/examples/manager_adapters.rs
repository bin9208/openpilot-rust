//! Safe adapter composition: fake bootlog child, isolated Params and board/SDK seams.
#[path = "adapters/fixtures.rs"]
mod fixtures;
use openpilot_crash_reporting::{ParamsSource, Reporter, RuntimeInputs};
use openpilot_manager::{
    initialization::{initialize_main, InitPaths},
    lifecycle::ExitAction,
    native_boot::NativeBoot,
    native_exit::NativeExit,
    processes::Processes,
    runtime::ExitBoundary,
    startup::NativeStartup,
    Error,
};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let launcher = PathBuf::from(&args[2]);
    let params = openpilot_params::Params::open(&output.join("params"), "d")?;
    params.put("Version", b"before-manager-init")?;
    let loggerd = output.join("loggerd");
    std::fs::create_dir_all(&loggerd)?;
    let executable = loggerd.join("bootlog");
    std::fs::write(
        &executable,
        "#!/bin/sh\ncat \"$PARAMS_COPY_PATH/d/Version\" > snapshot-value\n",
    )?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))?;
    let lock_path = output.join("boot.lock");
    let lock = std::fs::File::create(&lock_path)?;
    lock.lock()?;
    let boot = NativeBoot {
        params_directory: output.join("params/d"),
        loggerd_directory: loggerd.clone(),
        launcher: launcher.clone(),
        lock_path: lock_path.clone(),
        lock: Some(lock),
        worker: None,
    };
    let logging =
        openpilot_logging::producer::Factory::new("inproc://manager-adapter-fixture".into())?;
    let mut reporter = Reporter {
        sdk: fixtures::Capture::default(),
        inputs: RuntimeInputs {
            base: output.clone(),
            params: ParamsSource::Isolated {
                root: output.join("params"),
                prefix: "d".into(),
            },
            pc: true,
            device_override: Some("fixture".into()),
        },
        logger: logging.logger(),
    };
    std::fs::write(
        output.join("build.json"),
        r#"{"channel":"fixture","openpilot":{"version":"after-manager-init","git_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","git_commit_date":"date","git_origin":"https://github.com/fixture/openpilot.git","build_style":"fixture"}}"#,
    )?;
    let identity = fixtures::Identity;
    let mut hardware = openpilot_registration::NativeHardware::new(&identity);
    let mut clock = openpilot_registration::SystemClock::default();
    let processes = Processes { entries: vec![] };
    let persist = output.join("persist");
    let mut startup = NativeStartup {
        source_root: &output,
        launcher: &launcher,
        params: &params,
        persist: &persist,
        api_host: "http://127.0.0.1:1",
        hardware: &mut hardware,
        device_type: "fixture",
        clock: &mut clock,
        spinner: None,
        reporter: &mut reporter,
        logging: &logging,
        processes: &processes,
        boot,
        update_status: None,
    };
    initialize_main(
        &params,
        &mut startup,
        &InitPaths {
            shm: &output.join("shm"),
            params: &output.join("params/d"),
        },
    )?;
    assert_eq!(
        startup.update_status.as_ref().unwrap().running_commit(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    startup.boot.worker.take().unwrap().join().unwrap()?;
    assert_eq!(
        std::fs::read(loggerd.join("snapshot-value"))?,
        b"before-manager-init"
    );
    assert!(startup.boot.lock.is_none());
    std::fs::File::open(lock_path)?.try_lock()?;
    assert_eq!(
        params.get("HardwareSerial")?.as_deref(),
        Some(b"fixture-serial".as_slice())
    );
    assert_eq!(
        params.get("DongleId")?.as_deref(),
        Some(b"UnregisteredDevice".as_slice())
    );
    assert_eq!(
        params.get("Version")?.as_deref(),
        Some(b"after-manager-init".as_slice())
    );
    assert_eq!(
        std::fs::read_to_string(output.join("params/d/SupportedCars"))?
            .lines()
            .count(),
        138
    );
    drop(startup);
    let mut exit = NativeExit {
        reporter,
        hardware: openpilot_hardware_control::HardwareControl::board("tizi"),
        platform: fixtures::Board::default(),
    };
    for action in [
        ExitAction::Uninstall,
        ExitAction::Reboot,
        ExitAction::Shutdown,
    ] {
        exit.exit(action)?;
    }
    assert_eq!(
        exit.platform.trace,
        [
            "touch /data/__system_reset__",
            "sync",
            "Output([\"sudo\", \"reboot\"])",
            "Output([\"sudo\", \"reboot\"])",
            "Shell(\"sudo poweroff\")"
        ]
    );
    exit.capture_exception(&Error::Contract("synthetic loop exception"))?;
    assert_eq!(
        exit.reporter.sdk.messages,
        ["manager contract: synthetic loop exception", "flush"]
    );
    assert_eq!(
        params.get("CarrotException")?.as_deref(),
        Some(b"exception".as_slice())
    );
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"snapshot_before_mutation":true,"owned_lock_released":true,"native_board_dispatch":exit.platform.trace,"native_reporter":exit.reporter.sdk.messages,"native_startup_registration":true,"supported_car_count":138,"external_device_actions":false}),
        )?,
    )?;
    println!(
        "PASS NativeBoot snapshot/lock, NativeExit board dispatch and native exception reporter"
    );
    Ok(())
}
