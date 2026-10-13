use std::{ffi::OsStr, io, os::unix::process::CommandExt, process::Command};

#[must_use]
pub fn select(mut available: impl FnMut(&'static str) -> bool) -> Option<&'static str> {
    ["C.UTF-8", "C"]
        .into_iter()
        .find(|candidate| available(candidate))
}

/// # Errors
/// Returns executable discovery or replacement failure before runtime workers start.
pub fn configure() -> io::Result<()> {
    let Some(candidate) =
        select(|candidate| openpilot_startup_ui::try_process_locale(candidate).unwrap_or(false))
    else {
        return Ok(());
    };
    let keys = ["LC_ALL", "LC_CTYPE", "LANG"];
    if keys
        .iter()
        .all(|key| std::env::var_os(key).as_deref() == Some(OsStr::new(candidate)))
    {
        return Ok(());
    }
    let mut command = Command::new(std::env::current_exe()?);
    command.args(std::env::args_os().skip(1));
    for key in keys {
        command.env(key, candidate);
    }
    Err(command.exec())
}
