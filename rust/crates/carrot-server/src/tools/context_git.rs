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
    pub fn record_staged_update(&self) -> Result<(), Failure> {
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
        Ok(())
    }
}

impl Context {
    pub async fn native_origin(&self) -> Result<Completed, Failure> {
        let origin = self
            .git(&["config", "--get", "remote.origin.url"], 15, false)
            .await?;
        if origin.code != 0
            || !matches!(
                origin.output.trim(),
                "https://github.com/bin9208/openpilot-rust.git"
                    | "https://github.com/bin9208/openpilot-rust"
                    | "git@github.com:bin9208/openpilot-rust.git"
                    | "ssh://git@github.com/bin9208/openpilot-rust.git"
            )
        {
            self.append("Native runtime origin must be bin9208/openpilot-rust\n")?;
            return Ok(Completed {
                code: 1,
                output: "Native runtime origin must be bin9208/openpilot-rust".into(),
                streams: None,
            });
        }
        Ok(Completed {
            code: 0,
            output: String::new(),
            streams: None,
        })
    }
    pub async fn native_fetch(&self) -> Result<Completed, Failure> {
        let origin = self.native_origin().await?;
        if origin.code != 0 {
            return Ok(origin);
        }
        self.git(&["fetch", "origin", "--prune"], 300, true).await
    }
    pub async fn stage_update(
        &self,
        target: &str,
        branch: Option<&str>,
        detach: bool,
        ff_only: bool,
    ) -> Result<Completed, Failure> {
        self.progress("prepare verified native source and package", 1, 1)?;
        let root = &self.config.paths.repository;
        let mut args = vec![
            root.join("rust/bin/openpilot-runtime-install")
                .to_string_lossy()
                .into_owned(),
            "--stage-update".into(),
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--target".into(),
            target.into(),
        ];
        if let Some(branch) = branch {
            args.extend(["--branch".into(), branch.into()]);
        }
        if detach {
            args.push("--detach".into());
        }
        if ff_only {
            args.push("--ff-only".into());
        }
        let output = self.bounded(&args, 900).await?;
        self.append(&output.output)?;
        Ok(output)
    }
}
