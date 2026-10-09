use super::{text, Failure, Repository};
use crate::{Error, Value};
use openpilot_process_supervision::{CaptureError, CapturedCommand};
use std::{os::unix::process::ExitStatusExt, path::Path, time::Duration};

#[derive(Clone, Copy)]
pub(super) enum Deadline {
    Float(u64),
    Integer(u64),
}

impl Deadline {
    fn seconds(self) -> u64 {
        match self {
            Self::Float(seconds) | Self::Integer(seconds) => seconds,
        }
    }

    fn label(self) -> String {
        match self {
            Self::Float(seconds) => format!("{seconds}.0"),
            Self::Integer(seconds) => seconds.to_string(),
        }
    }
}

pub(super) struct Options {
    pub allowed: &'static [i32],
    pub deadline: Deadline,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            allowed: &[0],
            deadline: Deadline::Float(15),
        }
    }
}

pub(super) struct Completed {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn io_failure(error: std::io::Error, filename: Option<&Path>) -> Failure {
    match filename {
        Some(path) => Failure::Caught(crate::state::io_error(error, path).to_string()),
        None => {
            let message = error.to_string();
            let message = message.split(" (os error").next().unwrap_or(&message);
            Failure::Caught(format!(
                "[Errno {}] {message}",
                error.raw_os_error().unwrap_or(0)
            ))
        }
    }
}

pub(super) fn run(
    repository: &Repository<'_>,
    args: &[&str],
    deadline: Deadline,
) -> Result<Completed, Failure> {
    let request = CapturedCommand {
        launcher: repository.launcher.into(),
        cwd: repository.directory.into(),
        argv: std::iter::once("git")
            .chain(args.iter().copied())
            .map(Into::into)
            .collect(),
    };
    let output = request
        .capture(Duration::from_secs(deadline.seconds()), repository.lock)
        .map_err(|error| match error {
            CaptureError::Timeout { .. } => {
                let command = Value::Array(
                    std::iter::once("git")
                        .chain(args.iter().copied())
                        .map(Value::text)
                        .collect(),
                );
                match command.repr() {
                    Ok(command) => Failure::Caught(format!(
                        "Command '{command}' timed out after {} seconds",
                        deadline.label()
                    )),
                    Err(error) => Failure::Boundary(error.into()),
                }
            }
            CaptureError::Io(error) => io_failure(error, None),
            CaptureError::Launch(openpilot_process_supervision::Error::Io(error)) => {
                let path = if repository.directory.is_dir()
                    && rustix::fs::accessat(
                        rustix::fs::CWD,
                        repository.directory,
                        rustix::fs::Access::EXEC_OK,
                        rustix::fs::AtFlags::EACCESS,
                    )
                    .is_ok()
                {
                    Path::new("git")
                } else {
                    repository.directory
                };
                io_failure(error, Some(path))
            }
            CaptureError::Launch(openpilot_process_supervision::Error::Nul(_)) => {
                Failure::Boundary(Error::Json(openpilot_carrot_navi::Error::value(
                    "embedded null byte",
                )))
            }
            CaptureError::Launch(error) => Failure::Boundary(Error::Source(error.to_string())),
        })?;
    Ok(Completed {
        code: output
            .status
            .code()
            .unwrap_or_else(|| -output.status.signal().unwrap_or(1)),
        stdout: text::decode(&output.stdout),
        stderr: text::decode(&output.stderr),
    })
}

pub(super) fn git(
    repository: &Repository<'_>,
    args: &[&str],
    options: Options,
) -> Result<String, Failure> {
    let result = run(repository, args, options.deadline)?;
    let output = [&result.stdout, &result.stderr]
        .into_iter()
        .filter(|text| !text.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    if !options.allowed.contains(&result.code) {
        return Err(Failure::Caught(if output.is_empty() {
            format!(
                "git {} failed ({})",
                args.first().copied().unwrap_or(""),
                result.code
            )
        } else {
            output
        }));
    }
    Ok(output)
}
