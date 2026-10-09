use super::{
    config::Config,
    job_updates::Progress,
    jobs::Store,
    runner::{self, Completed, Failure, Runner},
};
use crate::{json_fields::set, Error, Value};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, sync::Arc, time::Duration};

pub struct Reply {
    pub status: u16,
    pub value: Value,
}
impl Reply {
    pub(super) fn ok(value: Value) -> Self {
        Self { status: 200, value }
    }
    pub(super) fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            value: Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        }
    }
}
pub(super) struct Context {
    pub config: Arc<Config>,
    pub jobs: Arc<Store>,
    pub id: Option<String>,
    pub runner: Runner,
}
impl Context {
    pub fn streaming(&self) -> bool {
        self.id.is_some()
    }
    pub async fn lock(&mut self) -> Result<(), Reply> {
        let path = &self.config.paths.lock;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| {
                Self::lock_error(false, crate::state::io_error(error, path).to_string())
            })?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |_| {
                Self::lock_error(
                    true,
                    "Build or another Git operation is running; retry when it finishes.".into(),
                )
            },
        )?;
        let file = Arc::new(file);
        self.runner.lock = Some(Arc::clone(&file));
        let recovery = crate::repo_update::Recovery::new(crate::git_status::Repository {
            directory: self.config.paths.repository.clone(),
            lock: path.clone(),
            launcher: self.config.paths.launcher.clone(),
        });
        recovery
            .prepare(file, self.runner.stopped.clone())
            .await
            .map_err(|error| {
                Self::lock_error(
                    matches!(error, crate::repo_update::Failure::Busy(_)),
                    error.to_string(),
                )
            })?;
        Ok(())
    }
    fn lock_error(busy: bool, message: String) -> Reply {
        Reply {
            status: 409,
            value: Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&message)),
                (
                    "error_code",
                    Value::text(if busy {
                        "GIT_BUSY"
                    } else {
                        "GIT_PRECHECK_FAILED"
                    }),
                ),
            ]),
        }
    }
    pub fn progress(&self, message: &str, current: i64, total: i64) -> Result<(), Failure> {
        if let Some(id) = &self.id {
            self.jobs.progress(
                id,
                Progress {
                    message: Some(message),
                    current: Some(current),
                    total: Some(total),
                },
            )?;
        }
        Ok(())
    }
    pub fn append(&self, output: &str) -> Result<(), Failure> {
        if let Some(id) = &self.id {
            self.jobs.append(id, &Value::text(output))?;
        }
        Ok(())
    }
    pub async fn command(
        &self,
        args: &[String],
        timeout: u64,
        stream: bool,
    ) -> Result<Completed, Failure> {
        let command = runner::Command {
            argv: args,
            cwd: Some(&self.config.paths.repository),
            timeout: self.streaming().then(|| Duration::from_secs(timeout)),
        };
        if self.streaming() {
            self.runner
                .job(
                    command,
                    if stream {
                        self.id
                            .as_ref()
                            .map(|id| (Arc::clone(&self.jobs), id.clone()))
                    } else {
                        None
                    },
                )
                .await
        } else {
            self.runner.sync(command).await
        }
    }
    pub async fn bounded(&self, args: &[String], timeout: u64) -> Result<Completed, Failure> {
        let command = runner::Command {
            argv: args,
            cwd: Some(&self.config.paths.repository),
            timeout: Some(Duration::from_secs(timeout)),
        };
        if self.streaming() {
            self.runner.job(command, None).await
        } else {
            self.runner.sync(command).await
        }
    }
    pub async fn git(
        &self,
        args: &[&str],
        timeout: u64,
        stream: bool,
    ) -> Result<Completed, Failure> {
        self.command(
            &std::iter::once("git")
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            timeout,
            stream,
        )
        .await
    }
    pub fn result(
        &self,
        result: Completed,
        summary: Option<(&str, Value)>,
    ) -> Result<Reply, Failure> {
        let mut value = match &self.id {
            Some(id) => self.jobs.result(id, result.code)?,
            None => Value::object([
                ("ok", Value::Bool(result.code == 0)),
                ("rc", Value::integer(result.code)),
                ("out", Value::text(&result.output)),
            ]),
        };
        if let Some((key, variables)) = summary {
            set(&mut value, "summary_key", Value::text(key))?;
            if variables.truth() {
                set(&mut value, "summary_vars", variables)?;
            }
        }
        Ok(Reply::ok(value))
    }
    pub fn invalid(&self, message: &str, code: &str) -> Reply {
        let mut reply = Reply::error(400, message);
        if self.streaming() && !code.is_empty() {
            if let Err(error) = set(&mut reply.value, "error_code", Value::text(code)) {
                eprintln!("Tools validation: {error}");
            }
        }
        reply
    }
    pub fn clear_cache(&self) -> Result<(), Failure> {
        if let Some(status) = &self.config.git_status {
            status
                .clear_cache()
                .map_err(|error| Error::Source(error.to_string()))?;
        }
        Ok(())
    }
}
