use openpilot_crash_reporting::{sdk::NativeSdk, Reporter, RuntimeInputs};
use openpilot_hardware_control::{HardwareControl, LinuxPlatform, ProcessCommands};
use openpilot_hardware_info::paths::Paths;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_manager::{
    initialization::{self, InitPaths},
    installed,
    lifecycle::{self, Environment},
    native_boot::NativeBoot,
    native_exit::NativeExit,
    runtime::{self, NativeRuntime},
    startup::NativeStartup,
    Error,
};
use openpilot_manager_catalog::binding::Installation;
use openpilot_params::Params;
use openpilot_registration::Spinner;
use openpilot_startup_ui::{children::TextWindow, native_children::NativeLaunch};
use std::{os::unix::net::UnixStream, path::Path, time::Duration};

pub fn run(installation: &Installation) -> Result<(), Error> {
    std::env::set_current_dir(&installation.root)?;
    std::env::set_var("BASEDIR", &installation.root);
    std::env::set_var("OPENPILOT_BASEDIR", &installation.root);
    std::env::set_var("OPENPILOT_ROOT", &installation.root);
    if std::env::var_os("USBGPU_ASSETS_ROOT").is_none() {
        std::env::set_var(
            "USBGPU_ASSETS_ROOT",
            installation.binaries.join("usbgpu-assets"),
        );
    }
    let factory = Factory::for_runtime()?;
    factory.bind_global(
        [
            (
                "runtime_language".into(),
                openpilot_logging::Value::Text("rust".into()),
            ),
            (
                "runtime_implementation".into(),
                openpilot_logging::Value::Text("openpilot-manager".into()),
            ),
        ]
        .into_iter()
        .collect(),
    )?;
    let mut processes = installed::processes(installation, &factory)?;
    let params = Params::for_runtime()?;
    let paths = Paths::default();
    let identity = openpilot_hardware_info::for_runtime();
    let device = identity
        .get_device_type()
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
    let mut hardware = openpilot_registration::NativeHardware::new(identity.as_ref());
    let mut clock = openpilot_registration::SystemClock::default();
    let launcher = installation.binaries.join("openpilot-process-child");
    let windows = NativeLaunch {
        source_root: installation.root.clone(),
        binaries: installation.binaries.clone(),
        launcher: launcher.clone(),
    };
    let mut reporter = Reporter {
        sdk: NativeSdk::default(),
        inputs: RuntimeInputs::new(installation.root.clone()),
        logger: factory.logger(),
    };
    let lock = match std::env::var_os("CARROT_BOOT_LOCK_SOCKET") {
        Some(path) => {
            let mut connection = UnixStream::connect(path)?;
            connection.set_read_timeout(Some(Duration::from_secs(30)))?;
            Some(openpilot_process_supervision::receive_boot_lock(
                &mut connection,
            )?)
        }
        None => None,
    };
    std::env::remove_var("CARROT_BOOT_LOCK_SOCKET");
    let boot = NativeBoot {
        params_directory: params.directory().to_owned(),
        loggerd_directory: installation.binaries.clone(),
        executable: Some(installation.binaries.join("openpilot-bootlog")),
        launcher: launcher.clone(),
        lock_path: std::env::var_os("CARROT_REPO_LOCK_PATH")
            .map_or_else(|| "/tmp/carrot_repo_update.lock".into(), Into::into),
        lock,
        worker: None,
    };
    let persist = paths
        .persist_root()
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
    let mut spinner = crate::startup_display::Display::new(windows.clone());
    let api_host = crate::startup_api::host(std::env::var_os("API_HOST"))?;
    let mut startup = NativeStartup {
        source_root: &installation.root,
        launcher: &launcher,
        params: &params,
        persist: Path::new(&persist),
        api_host: &api_host,
        hardware: &mut hardware,
        device_type: &device,
        clock: &mut clock,
        spinner: Some(&mut spinner),
        reporter: &mut reporter,
        logging: &factory,
        processes: &processes,
        boot,
        update_status: None,
    };
    let initialized = initialization::initialize_main(
        &params,
        &mut startup,
        &InitPaths {
            shm: Path::new(paths.shm_path()),
            params: params.directory(),
        },
    );
    let update_status = startup.update_status.take();
    drop(startup);
    spinner.close()?;
    if let Err(error) = initialized {
        crate::startup_failure::save(&factory, &paths, &error)?;
        factory.logger().emit(
            log_site!(),
            Record::text(Level::Error, "Manager failed to start".into())
                .with_exception(error.to_string()),
        )?;
        if let Some(ui) = processes
            .entries
            .iter_mut()
            .find(|entry| entry.descriptor.name == "ui")
            .and_then(|entry| entry.process.as_mut())
        {
            ui.stop(openpilot_process_supervision::StopOptions::default())?;
        }
        let mut window = TextWindow::new(windows, &format!("Manager failed to start\n\n{error}"));
        window
            .wait_for_exit()
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        return Err(error);
    }
    let environment = Environment::capture();
    if environment.prepare_only {
        return Ok(());
    }
    let (subscriber, publisher) = runtime::subscriptions(false)?;
    let control = if device == "pc" {
        HardwareControl::pc()
    } else {
        HardwareControl::board(&device)
    };
    let boundary = NativeExit {
        reporter,
        hardware: control,
        platform: LinuxPlatform::new(
            Path::new("/"),
            ProcessCommands {
                launcher: launcher.clone(),
            },
        ),
    };
    let mut runtime = NativeRuntime {
        params: Params::for_runtime()?,
        processes,
        subscriber,
        publisher,
        update_status: update_status.ok_or(Error::Contract("startup checkout status missing"))?,
        watchdog_path: "/var/tmp/power_watchdog".into(),
        logger: factory.logger(),
        boundary,
        signals: openpilot_manager::signals::Signals::install()?,
        loop_time: Duration::ZERO,
    };
    lifecycle::run(&params, &mut runtime, &environment)
}
