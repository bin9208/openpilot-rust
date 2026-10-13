use crate::{config::Config, Error, Value};
use openpilot_process_supervision::{CaptureError, CapturedCommand};
use std::{
    os::{fd::BorrowedFd, unix::process::ExitStatusExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Completed {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
impl Completed {
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}
pub(super) fn cwd(config: &Config) -> PathBuf {
    let source = config.repository.join("openpilot");
    if source.is_dir() {
        source
    } else {
        config.repository.clone()
    }
}
pub(super) fn request(config: &Config, args: &[&str], cwd: &Path) -> CapturedCommand {
    CapturedCommand {
        launcher: config.launcher.clone(),
        cwd: cwd.into(),
        argv: args.iter().map(Into::into).collect(),
    }
}
pub(super) fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
pub(super) fn io_error(error: &std::io::Error, path: Option<&Path>) -> String {
    let message = error.to_string();
    let message = message.split(" (os error").next().unwrap_or(&message);
    let prefix = format!("[Errno {}] {message}", error.raw_os_error().unwrap_or(0));
    match path {
        Some(path) => {
            let name = path.to_string_lossy();
            let quote = if name.contains('\'') && !name.contains('"') {
                '"'
            } else {
                '\''
            };
            let escaped = name
                .replace('\\', "\\\\")
                .replace(quote, &format!("\\{quote}"));
            format!("{prefix}: {quote}{escaped}{quote}")
        }
        None => prefix,
    }
}
pub(super) fn launch_error(error: openpilot_process_supervision::Error, program: &str) -> String {
    match error {
        openpilot_process_supervision::Error::Io(error) => {
            io_error(&error, Some(Path::new(program)))
        }
        error => error.to_string(),
    }
}
pub(super) fn capture(
    request: &CapturedCommand,
    timeout: u64,
    lock: Option<BorrowedFd<'_>>,
) -> Result<Completed, Error> {
    let output = request
        .capture(Duration::from_secs(timeout), lock)
        .map_err(|error| match error {
            CaptureError::Timeout { .. } => {
                let argv = Value::Array(
                    request
                        .argv
                        .iter()
                        .map(|arg| Value::text(&arg.to_string_lossy()))
                        .collect(),
                );
                match argv.repr() {
                    Ok(argv) => Error::Source(format!(
                        "Command '{argv}' timed out after {timeout} seconds"
                    )),
                    Err(error) => error.into(),
                }
            }
            CaptureError::Io(error)
            | CaptureError::Launch(openpilot_process_supervision::Error::Io(error)) => {
                Error::Io(error)
            }
            CaptureError::Launch(error) => Error::Source(error.to_string()),
        })?;
    Ok(Completed {
        code: output
            .status
            .code()
            .unwrap_or_else(|| -output.status.signal().unwrap_or(1)),
        stdout: decode(&output.stdout),
        stderr: decode(&output.stderr),
    })
}
pub(super) fn run(
    config: &Config,
    args: &[&str],
    timeout: u64,
    lock: Option<BorrowedFd<'_>>,
) -> Result<Completed, Error> {
    capture(&request(config, args, &cwd(config)), timeout, lock)
}
