//! Pinned Git transaction from services/auto_update.py; alert/notify recipients are supplied by the caller.
mod events;
pub mod runtime;
mod update;

use crate::{git_state::Store, git_status, Error, Value};
use events::ErrorEvent;
pub use events::{Effects, Notification};
use std::{fs::File, sync::Arc, time::Duration};
use tokio::sync::watch;
pub use update::{Policy, Update, UpdateFailure};

const INFO_TIMEOUT: Duration = Duration::from_secs(10);
const STAGE_TIMEOUT: Duration = Duration::from_secs(900);
#[derive(Clone)]
pub struct Pull {
    service: Arc<git_status::Service>,
    store: Arc<Store>,
    effects: Effects,
}

/// The caller keeps this already-held repository lock across the whole transaction.
pub struct Attempt<'a> {
    pub target: &'a str,
    pub lock: Arc<File>,
    pub stopped: watch::Receiver<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("{0}")]
    Busy(String),
    #[error(transparent)]
    Command(#[from] git_status::Failure),
    #[error(transparent)]
    State(#[from] Error),
}

fn head_prefix(head: &str) -> String {
    head.chars().take(12).collect()
}

impl Pull {
    pub fn new(repository: git_status::Repository, store: Arc<Store>, effects: Effects) -> Self {
        Self::with_service(
            git_status::Service::with_clock(repository, || 0.),
            store,
            effects,
        )
    }

    pub fn with_service(
        service: Arc<git_status::Service>,
        store: Arc<Store>,
        effects: Effects,
    ) -> Self {
        Self {
            service,
            store,
            effects,
        }
    }

    pub async fn run(&self, attempt: Attempt<'_>) -> Result<(bool, bool, String), Failure> {
        let unchanged = || (false, false, String::new());
        let target = attempt.target;
        let fail = |detail: &str, fields: Value| ErrorEvent {
            detail: detail.into(),
            blocked: false,
            fields,
        };
        if target.is_empty() {
            self.error(
                "upstream_read_failed",
                fail("No verified update target", Value::object([])),
            )?;
            return Ok(unchanged());
        }
        let previous = self.store.auto_update();
        if previous.get("reboot_requested_head").text_eq(target) {
            if !previous.get("status").text_eq("reboot_blocked")
                || !previous.get("target_head").text_eq(target)
            {
                self.error(
                    "duplicate_reboot_blocked",
                    ErrorEvent {
                        detail: format!(
                            "Automatic reboot for {} was already requested",
                            head_prefix(target)
                        ),
                        blocked: true,
                        fields: Value::object([
                            ("target_head", Value::text(target)),
                            ("reboot_requested_head", Value::text(target)),
                        ]),
                    },
                )?;
            }
            return Ok(unchanged());
        }
        let notification = Notification {
            old_head: String::new(),
            lock: Arc::clone(&attempt.lock),
            stopped: attempt.stopped.clone(),
        };
        let commands = self.service.locked_commands(attempt.lock, attempt.stopped);
        let (code, output) = commands.run(&["rev-parse", "HEAD"], INFO_TIMEOUT).await?;
        let old_head = if code == 0 { output.trim() } else { "" };
        if old_head.is_empty() {
            self.error(
                "head_read_failed",
                fail(
                    "Unable to read the current Git HEAD",
                    Value::object([("target_head", Value::text(target))]),
                ),
            )?;
            return Ok(unchanged());
        }
        let attempted_at =
            Value::integer((self.effects.clock)().seconds.int().map_err(Error::from)?);
        let mut fields = Value::object([
            ("attempted_at", attempted_at),
            ("old_head", Value::text(old_head)),
            ("new_head", Value::text("")),
            ("target_head", Value::text(target)),
            ("reset_rc", Value::Null),
            ("pull_rc", Value::Null),
            ("error_code", Value::text("")),
            ("error", Value::text("")),
        ]);
        if !self.event("pulling", &fields)? {
            self.error(
                "state_write_failed",
                fail("Unable to save automatic-update attempt", Value::object([])),
            )?;
            return Ok(unchanged());
        }
        // Prepare the exact target and native package together. The active
        // source and installed package remain the old pair until normal boot.
        let (stage_rc, stage_out) = if old_head == target {
            (0, String::new())
        } else {
            commands.stage_update(target, STAGE_TIMEOUT).await?
        };
        crate::json_fields::set(&mut fields, "stage_rc", Value::integer(stage_rc))?;
        crate::json_fields::set(&mut fields, "pull_rc", Value::integer(stage_rc))?;
        if stage_rc != 0 {
            if stage_out.contains("index.lock") && stage_out.contains("File exists") {
                return Err(Failure::Busy(stage_out));
            }
            self.error("runtime_package_failed", fail(&stage_out, fields))?;
            return Ok(unchanged());
        }
        let (code, output) = commands.run(&["rev-parse", "HEAD"], INFO_TIMEOUT).await?;
        if code != 0 || output.trim() != old_head {
            self.error(
                "active_source_changed",
                fail(
                    "Active Git HEAD changed during staged update preparation",
                    fields,
                ),
            )?;
            return Ok(unchanged());
        }
        let new_head = target;
        crate::json_fields::set(&mut fields, "new_head", Value::text(new_head))?;
        for name in ["error_code", "error"] {
            let Value::Object(items) = &mut fields else {
                return Err(Error::Source("invalid pull event".into()).into());
            };
            items.retain(|(key, _)| !key.iter().copied().eq(name.chars().map(u32::from)));
        }
        if new_head == old_head {
            self.error(
                "head_unchanged",
                fail("Selected source is already active", fields),
            )?;
            return Ok((true, false, new_head.into()));
        }
        if previous.get("reboot_requested_head").text_eq(new_head) {
            self.pull_time()?;
            crate::json_fields::set(&mut fields, "reboot_requested_head", Value::text(new_head))?;
            self.error(
                "duplicate_reboot_blocked",
                ErrorEvent {
                    detail: format!(
                        "Automatic reboot for {} was already requested",
                        head_prefix(new_head)
                    ),
                    blocked: true,
                    fields,
                },
            )?;
            return Ok((true, false, new_head.into()));
        }
        let mut updated = fields.clone();
        crate::json_fields::set(&mut updated, "error_code", Value::text(""))?;
        crate::json_fields::set(&mut updated, "error", Value::text(""))?;
        if !self.event("updated", &updated)? {
            self.error(
                "state_write_failed",
                fail("Unable to save the verified automatic update", fields),
            )?;
            return Ok((true, false, new_head.into()));
        }
        self.alert(false, &Value::text(""));
        if let Err(_error) = self.pull_time() {}
        if let Err(error) = (self.effects.notify)(Notification {
            old_head: old_head.into(),
            ..notification
        })
        .await
        {
            if matches!(error, Failure::Command(git_status::Failure::Cancelled)) {
                return Err(error);
            }
            println!("[auto_update] notify skipped: {error}");
        }
        Ok((true, true, new_head.into()))
    }
}
