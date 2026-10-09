//! Source Tools merged session pipe for jobs; separate strict streams for synchronous subprocess.run.
use super::{group, jobs::Store};
use crate::Error;
use std::{fs::File, os::unix::process::ExitStatusExt, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("timeout")]
    Timeout,
    #[error("Tools operation cancelled")]
    Cancelled,
    #[error(transparent)]
    Boundary(#[from] Error),
}
#[derive(Clone)]
pub struct Runner {
    pub repository: PathBuf,
    pub launcher: PathBuf,
    pub lock: Option<Arc<File>>,
    pub stopped: watch::Receiver<bool>,
}
pub struct Completed {
    pub code: i32,
    pub output: String,
    pub streams: Option<(String, String)>,
}
pub struct Command<'a> {
    pub argv: &'a [String],
    pub cwd: Option<&'a std::path::Path>,
    pub timeout: Option<Duration>,
}
impl Runner {
    pub(crate) async fn raw_capture(
        &self,
        command: Command<'_>,
    ) -> Result<std::process::Output, Failure> {
        super::sync_capture::run(self, command).await
    }
    pub async fn job(
        &self,
        command: Command<'_>,
        log: Option<(Arc<Store>, String)>,
    ) -> Result<Completed, Failure> {
        group::run(self, command, log).await
    }
    pub async fn sync(&self, command: Command<'_>) -> Result<Completed, Failure> {
        let output = super::sync_capture::run(self, command).await?;
        let code = code(output.status);
        let stdout = strict(&output.stdout)?;
        let stderr = strict(&output.stderr)?;
        let output = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{stdout}\n{stderr}")
        };
        Ok(Completed {
            code,
            output: output.trim_matches(super::text::whitespace).into(),
            streams: Some((stdout, stderr)),
        })
    }
}
fn strict(bytes: &[u8]) -> Result<String, Failure> {
    let text = crate::request_text::decode(bytes, "utf-8")?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}
pub(super) fn code(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1))
}
pub(super) fn failure(
    error: openpilot_process_supervision::Error,
    cwd: &std::path::Path,
    program: Option<&str>,
) -> Failure {
    let error = match error {
        openpilot_process_supervision::Error::Io(error) => {
            let path = if cwd.is_dir()
                && rustix::fs::accessat(
                    rustix::fs::CWD,
                    cwd,
                    rustix::fs::Access::EXEC_OK,
                    rustix::fs::AtFlags::EACCESS,
                )
                .is_ok()
            {
                std::path::Path::new(program.unwrap_or(""))
            } else {
                cwd
            };
            crate::state::io_error(error, path)
        }
        openpilot_process_supervision::Error::Nul(_) => Error::Source("embedded null byte".into()),
        error => Error::Source(error.to_string()),
    };
    Failure::Boundary(error)
}
