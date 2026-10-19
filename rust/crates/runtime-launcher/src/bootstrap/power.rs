//! zoompilot/jetlink cmd_boot: cut USB input current before manager/model work.
use super::Paths;
use openpilot_runtime_core::jetlink_power::{self, Action};
use std::{
    env, fs, io,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn command() -> io::Result<()> {
    if !rustix::process::geteuid().is_root() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Jetlink boot power helper requires root",
        ));
    }
    if Path::new("/AGNOS").is_file()
        && jetlink_power::boot_wanted(fs::read("/data/params/d/JetlinkMode").ok().as_deref())
    {
        jetlink_power::apply(Action::Cut)?;
    }
    Ok(())
}

pub(super) fn prepare(paths: &Paths) {
    if !paths.system.join("AGNOS").is_file()
        || !jetlink_power::boot_wanted(
            fs::read(paths.data.join("params/d/JetlinkMode"))
                .ok()
                .as_deref(),
        )
    {
        return;
    }
    if let Err(error) = run() {
        eprintln!("Jetlink early USB input-current policy: {error}");
    }
}

fn run() -> io::Result<()> {
    let mut command = if rustix::process::geteuid().is_root() {
        Command::new(env::current_exe()?)
    } else {
        let mut command = Command::new("sudo");
        command.arg("-n").arg(env::current_exe()?);
        command
    };
    let mut child = command
        .arg("--jetlink-boot-power")
        .stdin(Stdio::null())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("boot USB input-current helper failed"))
            };
        }
        if Instant::now() >= until {
            child.kill()?;
            child.wait()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "boot USB input-current helper exceeded2s",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}
