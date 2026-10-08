use super::Failure;
use crate::{git_status::Repository, Error};
use openpilot_process_supervision::{CaptureError, CapturedCommand};
use std::{fs::File, os::fd::AsFd, path::PathBuf, time::Duration};

fn text(bytes: &[u8]) -> Result<String, Failure> {
    Ok(crate::request_text::decode(bytes, "utf-8")
        .map_err(Failure::Decode)?
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .to_owned())
}

pub(super) fn index_path(repository: &Repository, lock: &File) -> Result<PathBuf, Failure> {
    let command = CapturedCommand {
        launcher: repository.launcher.clone(),
        cwd: repository.directory.clone(),
        argv: [
            "git",
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "index.lock",
        ]
        .map(Into::into)
        .into(),
    };
    let output = command
        .capture(Duration::from_secs(10), Some(lock.as_fd()))
        .map_err(|error| match error {
            CaptureError::Timeout { .. } => Failure::Timeout(
                "Command '['git', 'rev-parse', '--path-format=absolute', '--git-path', 'index.lock']' timed out after 10 seconds".into(),
            ),
            CaptureError::Io(error) => Failure::Runtime(error.into()),
            CaptureError::Launch(openpilot_process_supervision::Error::Io(error)) => {
                let path = if repository.directory.is_dir()
                    && rustix::fs::accessat(
                        rustix::fs::CWD,
                        &repository.directory,
                        rustix::fs::Access::EXEC_OK,
                        rustix::fs::AtFlags::EACCESS,
                    )
                    .is_ok()
                {
                    std::path::Path::new("git")
                } else {
                    &repository.directory
                };
                Failure::Runtime(crate::state::io_error(error, path))
            }
            CaptureError::Launch(error) => Failure::Runtime(Error::Source(error.to_string())),
        })?;
    let stdout = text(&output.stdout)?;
    let stderr = text(&output.stderr)?;
    if !output.status.success() {
        return Err(Error::Source(if stderr.is_empty() {
            "Unable to locate Git index lock".into()
        } else {
            stderr
        })
        .into());
    }
    Ok(repository.directory.join(stdout))
}
