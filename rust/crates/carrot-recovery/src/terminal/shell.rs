use crate::{config::Config, Error};
use openpilot_process_supervision::CapturedCommand;
use std::{ffi::OsString, path::PathBuf, time::Duration};

fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "_@%+=:,./-".contains(ch))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}
pub(super) fn cwd(config: &Config) -> PathBuf {
    let original = config.repository.join("openpilot");
    if original.is_dir() {
        original
    } else {
        config.repository.clone()
    }
}
pub(super) fn command(config: &Config) -> Result<String, Error> {
    let bootstrap = "( run-parts /etc/update-motd.d 2>/dev/null || cat /run/motd.dynamic 2>/dev/null ); cd /data/openpilot 2>/dev/null; exec bash -il";
    let user = std::env::var_os("USER")
        .filter(|user| !user.is_empty())
        .or_else(|| std::env::var_os("USERNAME").filter(|user| !user.is_empty()));
    if user.is_some_and(|user| user == "comma") {
        return Ok(bootstrap.into());
    }
    if rustix::process::geteuid().is_root() {
        return Ok(format!("exec su - comma -c {}", quote(bootstrap)));
    }
    let sudo = std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let path = directory.join("sudo");
            path.is_file()
                && rustix::fs::accessat(
                    rustix::fs::CWD,
                    path,
                    rustix::fs::Access::EXEC_OK,
                    rustix::fs::AtFlags::empty(),
                )
                .is_ok()
        })
    });
    // Like the source, unavailable/denied/timed-out sudo falls back to this user.
    if sudo
        && (CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: cwd(config),
            argv: ["sudo", "-n", "-u", "comma", "true"]
                .map(OsString::from)
                .into(),
        })
        .capture(Duration::from_secs(2), None)
        .is_ok_and(|result| result.status.success())
    {
        return Ok(format!(
            "exec sudo -n -u comma -i bash -lc {}",
            quote(bootstrap)
        ));
    }
    Ok(bootstrap.into())
}
pub(super) fn environment() -> Vec<(OsString, OsString)> {
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]
        .into_iter()
        .filter(|(name, _)| std::env::var_os(name).is_none())
        .map(|(name, value)| (name.into(), value.into()))
        .collect()
}
