use super::{Failure, Pull};
use crate::{auto_update::short_error, git_state::Time, Error, Value};
use std::{fs::File, future::Future, pin::Pin, sync::Arc};
use tokio::sync::watch;

type Alert = dyn Fn(bool, &Value) -> Result<(), Error> + Send + Sync;
type Notify =
    dyn Fn(Notification) -> Pin<Box<dyn Future<Output = Result<(), Failure>> + Send>> + Send + Sync;

pub struct Notification {
    pub old_head: String,
    pub lock: Arc<File>,
    pub stopped: watch::Receiver<bool>,
}

#[derive(Clone)]
pub struct Effects {
    pub clock: Arc<dyn Fn() -> Time + Send + Sync>,
    pub alert: Arc<Alert>,
    pub notify: Arc<Notify>,
}

pub(super) struct ErrorEvent {
    pub detail: String,
    pub blocked: bool,
    pub fields: Value,
}

impl Pull {
    pub(super) fn event(&self, status: &str, fields: &Value) -> Result<bool, Failure> {
        Ok(self
            .store
            .write_event(&Value::text(status), fields, &(self.effects.clock)())?
            .truth())
    }

    pub(super) fn alert(&self, show: bool, detail: &Value) {
        if let Err(error) = (self.effects.alert)(show, detail) {
            println!("[auto_update] offroad alert error: {error}");
        }
    }

    pub(super) fn error(&self, code: &str, event: ErrorEvent) -> Result<(), Failure> {
        let error = short_error(
            &Value::text(&event.detail),
            &code.chars().map(u32::from).collect::<Vec<_>>(),
        )?;
        let mut fields = vec![
            (
                "error_code".chars().map(u32::from).collect(),
                Value::text(code),
            ),
            ("error".chars().map(u32::from).collect(), error.clone()),
        ];
        // Preserve the original event field order, replacing the attempt's
        // empty error placeholders instead of writing duplicate JSON keys.
        for (key, value) in crate::json_fields::fields(&event.fields)? {
            if !key.iter().copied().eq("error_code".chars().map(u32::from))
                && !key.iter().copied().eq("error".chars().map(u32::from))
            {
                fields.push((key.clone(), value.clone()));
            }
        }
        let status = if event.blocked {
            "reboot_blocked"
        } else {
            "error"
        };
        self.event(status, &Value::Object(fields))?;
        self.alert(true, &error);
        println!(
            "[auto_update] {status} code={code}: {}",
            error.string().map_err(Error::from)?
        );
        Ok(())
    }

    pub(super) fn pull_time(&self) -> Result<(), Failure> {
        self.store
            .write_pull_time(&Value::Null, &(self.effects.clock)())?;
        Ok(())
    }
}
