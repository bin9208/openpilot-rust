//! Composite cable lifetime and strict source transport selection.
pub mod config;

mod gadget;
mod network;
mod platform;
mod process;

/// Separate privileged helper; invoked only by an already authorized offroad
/// gadget owner, with explicit iOS helper mode. No shell or Python is executed.
pub fn helper(action: &str) -> Result<(), crate::Error> {
    use crate::Error;
    if !rustix::process::geteuid().is_root() {
        return Err(Error::Contract("mobile helper requires root"));
    }
    if std::env::var("JETLINK_TRANSPORT").as_deref() != Ok("ios") {
        return Err(Error::Contract(
            "mobile helper requires explicit ios transport",
        ));
    }
    if !matches!(action, "gadget" | "net" | "net-down" | "--teardown") {
        return Err(Error::Contract("unknown mobile helper action"));
    }
    let paths = platform::Paths::device();
    let _lock = paths.locked(0)?;
    match action {
        "gadget" => {
            paths.detached()?;
            crate::gadget::provision_device()?;
            gadget::composite(&paths)
        }
        "net" => network::up(&paths, &platform::Linux),
        "net-down" => network::down(&paths, &platform::Linux),
        "--teardown" => {
            paths.detached()?;
            network::down(&paths, &platform::Linux)?;
            gadget::detach(&paths)
        }
        _ => unreachable!(),
    }
}
/// Run the scoped helper with a total15s bound. Ownership records remain when
/// cleanup fails, so a later session cannot silently claim a foreign resource.
pub fn root(action: &str) -> Result<(), crate::Error> {
    use std::{
        io::{Read, Seek, SeekFrom},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let executable = std::env::current_exe()?;
    let mut command = if rustix::process::geteuid().is_root() {
        let mut command = Command::new(&executable);
        command.env("JETLINK_TRANSPORT", "ios");
        command
    } else {
        let mut command = Command::new("sudo");
        command
            .args(["-n", "env", "JETLINK_TRANSPORT=ios"])
            .arg(&executable);
        command
    };
    let mut output = tempfile::tempfile()?;
    let mut child = command
        .args(["--mobile-root", action])
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output.try_clone()?)
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= until {
            child.kill()?;
            child.wait()?;
            return Err(crate::Error::Deadline);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    output.take(1 << 20).read_to_end(&mut bytes)?;
    if !status.success() {
        return Err(crate::Error::Native(format!(
            "mobile helper {action}: {}",
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(1024)
                .collect::<String>()
        )));
    }
    Ok(())
}

pub mod backend;
pub mod cable;
