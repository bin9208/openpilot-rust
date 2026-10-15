//! The checkout lock has been released before entering this recovery loop.
use super::{pause, Paths};
use crate::{watchdog::Server, Error};
use std::{
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) fn run(paths: &Paths, reason: &str, stopped: &AtomicBool) -> Result<(), Error> {
    eprintln!("{reason}");
    let file = tempfile::NamedTempFile::new_in(&paths.temporary)?;
    serde_json::to_writer(file.as_file(), paths)?;
    while !stopped.load(Ordering::Relaxed) {
        for headless in [false, true] {
            if stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            let mut command = Command::new(paths.binary("openpilot-startup-recovery"));
            command
                .env("CARROT_STARTUP_RECOVERY", "1")
                .env_remove("CARROT_BOOT_LOCK_SOCKET")
                .env_remove("CARROT_BOOT_LOCK_FD")
                .arg("--paths")
                .arg(file.path())
                .arg("--reason")
                .arg(reason)
                .arg("--log")
                .arg(paths.temporary.join("carrot_startup_failure.log"))
                .current_dir(&paths.repository);
            if headless {
                command.arg("--headless");
            }
            match command.spawn() {
                Ok(child) => {
                    Server(child).finish(stopped)?;
                }
                Err(error) => eprintln!("startup recovery process: {error}"),
            }
            if !headless && !stopped.load(Ordering::Relaxed) {
                eprintln!("Recovery display stopped; checking for a fix without graphics. Web recovery remains on port 6999.");
            }
        }
        pause(Duration::from_secs(30), stopped);
    }
    Ok(())
}
