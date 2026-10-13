//! Readiness/cooldown and locked checkout revalidation from _attempt_update.
mod locked;
use super::{Failure, Pull};
use crate::{auto_update, git_status, repo_update, Error, Value};
use std::sync::Arc;
use tokio::sync::watch;

pub struct Policy {
    pub ready: Box<dyn Fn() -> bool>,
    pub monotonic: Arc<dyn Fn() -> f64 + Send + Sync>,
}

pub struct Update {
    pull: Pull,
    recovery: Arc<repo_update::Recovery>,
    policy: Policy,
    last_pull_at: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateFailure {
    #[error(transparent)]
    Pull(#[from] Failure),
    #[error(transparent)]
    Recovery(#[from] repo_update::Failure),
    #[error(transparent)]
    Status(#[from] git_status::Failure),
    #[error(transparent)]
    Runtime(#[from] Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl Update {
    /// Recovery and Pull use the same caller-selected repository and cooperative lock.
    pub fn new(pull: Pull, recovery: Arc<repo_update::Recovery>, policy: Policy) -> Self {
        Self {
            pull,
            recovery,
            policy,
            last_pull_at: f64::NEG_INFINITY,
        }
    }

    pub async fn run(
        &mut self,
        stopped: watch::Receiver<bool>,
    ) -> Result<(bool, bool, String), UpdateFailure> {
        let unchanged = || (false, false, String::new());
        if !(self.policy.ready)() {
            return Ok(unchanged());
        }
        let mut status_stop = stopped.clone();
        let status = tokio::select! {
            result = self.pull.service.get(false) => result?,
            () = crate::auto_update_runtime::stopping(&mut status_stop) => {
                self.pull.service.wait_idle().await;
                return Err(git_status::Failure::Cancelled.into());
            }
        };
        let value = Value::object([
            ("available", Value::Bool(status.available)),
            (
                "state",
                Value::text(match status.state {
                    git_status::State::Ok => "ok",
                    git_status::State::Error => "error",
                    git_status::State::Busy => "busy",
                    git_status::State::NoUpstream => "no_upstream",
                    git_status::State::FetchError => "fetch_error",
                }),
            ),
            ("behind", Value::integer(status.behind)),
            (
                "target_head",
                Value::text(status.target_head.as_deref().unwrap_or("")),
            ),
        ]);
        let (behind, _) = auto_update::verified_update_target(&value)?;
        if behind == num_bigint::BigInt::from(0)
            || (self.policy.monotonic)() - self.last_pull_at < auto_update::COOLDOWN
        {
            return Ok(unchanged());
        }
        let result = self.locked(&status, stopped).await;
        let result = match result {
            Err(UpdateFailure::Pull(Failure::Busy(message)))
            | Err(UpdateFailure::Recovery(repo_update::Failure::Busy(message))) => {
                self.waiting(&message).map(|()| unchanged())
            }
            result => result,
        };
        self.pull.service.clear_cache()?;
        result
    }

    fn waiting(&self, message: &str) -> Result<(), UpdateFailure> {
        if self
            .pull
            .store
            .auto_update()
            .get("status")
            .text_eq("pulling")
        {
            self.pull.event(
                "waiting",
                &Value::object([
                    ("error_code", Value::text("git_busy")),
                    ("error", Value::text(message)),
                ]),
            )?;
        }
        println!("[auto_update] waiting: {message}");
        Ok(())
    }
}
