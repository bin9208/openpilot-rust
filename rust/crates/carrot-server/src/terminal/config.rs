//! Native provider paths for services/tmux.py and terminal_commands/bridge.py.
use crate::{tools::shell::quote, Error};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub launcher: PathBuf,
    pub cli: PathBuf,
    pub cli_cwd: PathBuf,
    pub start_dir: PathBuf,
    pub motd_dir: PathBuf,
    pub motd_cache: PathBuf,
    pub tmux_log: PathBuf,
    pub web_session: String,
    pub capture_lines: usize,
}
impl Config {
    pub fn original() -> Result<Self, Error> {
        let binary = std::env::current_exe()?;
        let directory = binary
            .parent()
            .ok_or_else(|| Error::Source("native executable directory missing".into()))?;
        Ok(Self {
            launcher: directory.join("openpilot-process-child"),
            cli: directory.join("openpilot-carrot-command"),
            cli_cwd: crate::config::runtime_repository()?.join("openpilot"),
            start_dir: "/data/openpilot".into(),
            motd_dir: "/etc/update-motd.d".into(),
            motd_cache: "/run/motd.dynamic".into(),
            tmux_log: "/data/media/tmux.log".into(),
            web_session: std::env::var("CARROT_TMUX_WEB_SESSION")
                .unwrap_or_else(|_| "carrot-terminal".into()),
            capture_lines: 160,
        })
    }
    pub fn shell_function(&self) -> String {
        format!(
            "carrot() {{ {} \"$@\"; }}; export -f carrot",
            quote(&self.cli.to_string_lossy())
        )
    }
    pub fn bootstrap(&self) -> String {
        format!(
            "( run-parts {} 2>/dev/null || cat {} 2>/dev/null ); cd {} 2>/dev/null; {}; exec bash -il",
            quote(&self.motd_dir.to_string_lossy()),
            quote(&self.motd_cache.to_string_lossy()),
            quote(&self.start_dir.to_string_lossy()),
            self.shell_function(),
        )
    }
    pub fn shell() -> OsString {
        std::env::var_os("SHELL")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/bin/bash".into())
    }
    pub fn shell_environment() -> Vec<(OsString, OsString)> {
        [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]
            .into_iter()
            .filter(|(name, _)| std::env::var_os(name).is_none())
            .map(|(name, value)| (name.into(), value.into()))
            .collect()
    }
}
