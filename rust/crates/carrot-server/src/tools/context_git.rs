use super::{
    context::Context,
    runner::{Completed, Failure},
    text,
};
use crate::{Error, Value};
use std::{os::fd::AsFd, sync::Arc};

impl Context {
    pub async fn repair(
        &self,
        remote: Option<String>,
        upstream: bool,
    ) -> Result<Completed, Failure> {
        let config = Arc::clone(&self.config);
        let lock = self.runner.lock.clone();
        let output = tokio::task::spawn_blocking(move || {
            crate::git_config::repair_git_config(
                &crate::git_config::Repository {
                    directory: &config.paths.repository,
                    launcher: &config.paths.launcher,
                    lock: lock.as_ref().map(|file| file.as_fd()),
                },
                remote.as_deref(),
                upstream,
            )
        })
        .await
        .map_err(|error| Error::Source(error.to_string()))??;
        self.clear_cache()?;
        if self.streaming() {
            self.append(&(output.1.clone() + "\n"))?;
        }
        Ok(Completed {
            streams: None,
            code: output.0,
            output: output.1,
        })
    }
    pub async fn prepare_pull(&self) -> Result<(Completed, String), Failure> {
        let config = Arc::clone(&self.config);
        let lock = self.runner.lock.clone();
        let (code, output, target) = tokio::task::spawn_blocking(move || {
            crate::git_config::prepare_git_pull(&crate::git_config::Repository {
                directory: &config.paths.repository,
                launcher: &config.paths.launcher,
                lock: lock.as_ref().map(|file| file.as_fd()),
            })
        })
        .await
        .map_err(|error| Error::Source(error.to_string()))??;
        if self.streaming() {
            self.append(&(output.clone() + "\n"))?;
        }
        Ok((
            Completed {
                code,
                output,
                streams: None,
            },
            target,
        ))
    }
    pub fn pull_time(&self, output: &str) -> Result<(), Failure> {
        if crate::git_state::did_pull_update(&Value::text(output))? {
            let store = crate::git_state::Store::new(
                self.config
                    .paths
                    .history
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .into(),
            );
            store.write_pull_time(
                &Value::Null,
                &crate::git_state::Time {
                    seconds: Value::Float(text::time()),
                    nanoseconds: Value::integer(0),
                },
            )?;
        }
        Ok(())
    }
}
