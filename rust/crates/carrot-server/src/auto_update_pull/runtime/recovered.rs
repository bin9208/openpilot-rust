use super::super::{Failure, Notification, Pull, INFO_TIMEOUT};
use crate::Value;
use openpilot_process_supervision::{CaptureError, CapturedCommand};
use std::{os::fd::AsFd, path::Path};

impl Pull {
    pub async fn clear_recovered_git_ref_error(
        &self,
        context: Notification,
    ) -> Result<(), Failure> {
        let state = self.store.auto_update();
        let detail = state.get("error");
        let error = if detail.truth() {
            detail.string().map_err(crate::Error::from)?
        } else {
            String::new()
        };
        let code = state.get("error_code");
        let reference = code.text_eq("pull_failed")
            && (error.contains("couldn't find remote ref")
                || error.contains("Cannot fast-forward to multiple branches"));
        let lock = ["reset_failed", "pull_failed", "git_busy"]
            .into_iter()
            .any(|name| code.text_eq(name))
            && error.contains("index.lock")
            && error.contains("File exists");
        if lock {
            let repository = self.service.repository();
            let directory = repository.directory.clone();
            let launcher = repository.launcher.clone();
            let clean = tokio::task::spawn_blocking(move || -> Result<bool, crate::Error> {
                for args in [
                    vec!["git", "rev-parse", "--verify", "HEAD^{commit}"],
                    vec![
                        "git",
                        "--no-optional-locks",
                        "diff",
                        "--quiet",
                        "HEAD",
                        "--",
                    ],
                ] {
                    let command = CapturedCommand {
                        launcher: launcher.clone(),
                        cwd: directory.clone(),
                        argv: args.iter().map(|arg| (*arg).into()).collect(),
                    };
                    let output = command
                        .capture(INFO_TIMEOUT, Some(context.lock.as_fd()))
                        .map_err(|error| match error {
                            CaptureError::Timeout { .. } => match Value::Array(
                                args.iter().map(|arg| Value::text(arg)).collect(),
                            )
                            .repr()
                            {
                                Ok(command) => crate::Error::Source(format!(
                                    "Command '{command}' timed out after 10.0 seconds"
                                )),
                                Err(error) => error.into(),
                            },
                            CaptureError::Io(error) => error.into(),
                            CaptureError::Launch(openpilot_process_supervision::Error::Io(
                                error,
                            )) => {
                                let path = if directory.is_dir()
                                    && rustix::fs::accessat(
                                        rustix::fs::CWD,
                                        &directory,
                                        rustix::fs::Access::EXEC_OK,
                                        rustix::fs::AtFlags::EACCESS,
                                    )
                                    .is_ok()
                                {
                                    Path::new("git")
                                } else {
                                    &directory
                                };
                                crate::state::io_error(error, path)
                            }
                            CaptureError::Launch(error) => crate::Error::Source(error.to_string()),
                        })?;
                    if !output.status.success() {
                        return Ok(false);
                    }
                }
                Ok(true)
            })
            .await
            .map_err(crate::git_status::Failure::from)??;
            if !clean {
                return Ok(());
            }
        }
        if ["error", "waiting"]
            .into_iter()
            .any(|name| state.get("status").text_eq(name))
            && (reference || lock)
            && self.event(
                "idle",
                &Value::object([("error_code", Value::text("")), ("error", Value::text(""))]),
            )?
        {
            self.alert(false, &Value::Null);
        }
        Ok(())
    }
}
