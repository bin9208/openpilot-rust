mod capture;
mod clock;
mod failure;
mod model;
mod paths;
mod power;
mod services;
mod system;
use paths::acquire;
pub use paths::Paths;
use services::{background, model_update, recovery};

use crate::{handoff, overlay, package::Manifest, watchdog::Server, Error};
use std::{
    env,
    fs::{self, File},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

fn pause(duration: Duration, stopped: &AtomicBool) {
    let until = Instant::now() + duration;
    while !stopped.load(Ordering::Relaxed) && Instant::now() < until {
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait(stopped: &AtomicBool) {
    while !stopped.load(Ordering::Relaxed) {
        pause(Duration::from_secs(1), stopped);
    }
}

fn error_screen(paths: &Paths, message: &str, stopped: &AtomicBool) {
    if let Err(error) = failure::run(paths, message, stopped) {
        eprintln!("startup recovery unavailable: {error}");
    }
}

fn verify_package(repository: &std::path::Path) -> Result<(), Error> {
    verified_package_identity(repository).map(|_| ())
}

fn verified_package_identity(repository: &std::path::Path) -> Result<String, Error> {
    let (manifest, digest) = Manifest::read_with_digest(repository)?;
    manifest.verify_identity(repository)?;
    manifest.verify(
        repository,
        &[
            "rust/bin/openpilot-launch",
            "rust/bin/openpilot-runtime-install",
            "rust/bin/openpilot-manager",
            "rust/bin/openpilot-process-child",
            "rust/bin/openpilot-reboot",
            "rust/bin/openpilot-startup-recovery",
            "rust/bin/openpilot-carrot-recovery",
            "rust/bin/openpilot-carrot-web-watchdog",
            "rust/bin/openpilot-carrot-server",
            "rust/bin/openpilot-agnos",
            "rust/bin/openpilot-agnos-updater",
            "rust/bin/openpilot-text-window",
            "rust/bin/openpilot-usbgpu-model",
        ],
    )?;
    Ok(digest)
}

fn ready(
    paths: &Paths,
    lock: &File,
    stopped: &AtomicBool,
    signals: &mut signal_hook::iterator::Signals,
) -> Result<(), Error> {
    if stopped.load(Ordering::Relaxed) {
        return Ok(());
    }
    let package_digest = verified_package_identity(&paths.repository)?;
    if env::var("CARROT_WEB_EXTERNAL").is_ok_and(|value| value == "1") {
        background(
            paths,
            "openpilot-carrot-web-watchdog",
            vec![paths.repository.clone().into_os_string()],
            "carrot_server.log",
        )?;
    }
    let startup_failed = model::prepare(paths, stopped)?;
    if stopped.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut model = Command::new(paths.binary("openpilot-usbgpu-model"));
    model.env_remove("CARROT_BIG_MODEL_STARTUP_FAILED");
    if startup_failed {
        model.env("CARROT_BIG_MODEL_STARTUP_FAILED", "1");
    }
    model
        .args(["--boot", "--root"])
        .arg(&paths.repository)
        .arg("--devices")
        .arg(paths.system.join("sys/bus/usb/devices"))
        .arg("--identity-root")
        .arg(&paths.system)
        .current_dir(&paths.repository);
    if !Server(model.spawn()?).finish(stopped)?.success() && !stopped.load(Ordering::Relaxed) {
        return Err(Error::Configuration("native model readiness failed".into()));
    }
    if stopped.load(Ordering::Relaxed) {
        return Ok(());
    }
    let capture = File::create(paths.temporary.join("launch_log"))?;
    let _ = Command::new("tmux")
        .args(["capture-pane", "-pq", "-S-1500"])
        .stdout(Stdio::from(capture))
        .status();
    model_update(paths)?;
    let mut manager = if paths.system.join("AGNOS").is_file() {
        let mut command = Command::new("taskset");
        command
            .args(["-c", "0-5"])
            .arg(paths.binary("openpilot-manager"));
        command
    } else {
        Command::new(paths.binary("openpilot-manager"))
    };
    Manifest::bind_descendant(&mut manager, &paths.repository, &package_digest)?;
    manager
        .env_remove("CARROT_BIG_MODEL_STARTUP_FAILED")
        .arg("--root")
        .arg(&paths.repository)
        .current_dir(&paths.repository);
    if startup_failed {
        manager.env("CARROT_BIG_MODEL_STARTUP_FAILED", "1");
    }
    let capture = capture::Capture::start(
        &mut manager,
        &paths.temporary.join("carrot_startup_failure.log"),
    )?;
    let result = handoff::run_locked_with_signals(manager, lock, stopped, signals);
    capture.finish()?;
    let status = result?;
    if !status.success() && !stopped.load(Ordering::Relaxed) {
        return Err(Error::Configuration(format!(
            "native manager failed: {status}"
        )));
    }
    println!("native manager exited: {status}");
    Ok(())
}

pub fn main() -> Result<(), Error> {
    if env::args_os().skip(1).collect::<Vec<_>>() == ["--jetlink-boot-power"] {
        power::command()?;
        return Ok(());
    }
    let mut paths = Paths::default();
    let mut verification_only = false;
    let mut arguments = env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--verify-package") => verification_only = true,
            Some("--paths") => {
                let file = arguments
                    .next()
                    .ok_or_else(|| Error::Configuration("missing paths file".into()))?;
                paths = serde_json::from_reader(File::open(file)?)?;
            }
            Some("--root") => {
                paths.repository = arguments
                    .next()
                    .ok_or_else(|| Error::Configuration("missing repository".into()))?
                    .into()
            }
            Some("--help" | "-h") => {
                println!("usage: openpilot-launch [--root REPOSITORY] [--paths PATHS_JSON] [--verify-package]");
                return Ok(());
            }
            _ => return Err(Error::Configuration("unexpected launcher argument".into())),
        }
    }
    paths.repository = paths.repository.canonicalize()?;
    if verification_only {
        return verify_package(&paths.repository);
    }
    paths.environment()?;
    power::prepare(&paths);
    fs::create_dir_all(&paths.temporary)?;
    let stopped = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&stopped))?;
    }
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;
    let Some(lock) = acquire(&paths, &stopped)? else {
        if !stopped.load(Ordering::Relaxed) {
            recovery(&paths);
            wait(&stopped);
        }
        return Ok(());
    };
    system::cleanup_lfs(&paths)?;
    if overlay::activate(&paths.repository, &paths.data.join("safe_staging"))?
        == overlay::Outcome::Activated
    {
        let temporary = tempfile::NamedTempFile::new_in(&paths.temporary)?;
        serde_json::to_writer(temporary.as_file(), &paths)?;
        let mut command = Command::new(paths.binary("openpilot-launch"));
        command
            .arg("--paths")
            .arg(temporary.path())
            .env_remove("AGNOS_VERSION")
            .current_dir(&paths.repository);
        handoff::run_locked_with_signals(command, &lock, &stopped, &mut signals)?;
        return Ok(());
    }
    system::enable_recovery_ssh(&paths)?;
    recovery(&paths);
    let result = system::agnos(&paths, &stopped)
        .and_then(|()| {
            if stopped.load(Ordering::Relaxed) {
                Ok(())
            } else {
                clock::prepare(&paths)
            }
        })
        .and_then(|()| ready(&paths, &lock, &stopped, &mut signals));
    lock.unlock()?;
    if let Err(error) = result {
        error_screen(
            &paths,
            &format!("Native startup failed.\n\n{error}\n\nRecovery: port 6999"),
            &stopped,
        );
    }
    wait(&stopped);
    Ok(())
}
