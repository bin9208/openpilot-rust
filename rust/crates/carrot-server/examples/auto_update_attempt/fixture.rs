use openpilot_carrot_server::{auto_update_pull::Effects, git_state::Time, Error, Value};
use serde::Deserialize;
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Deserialize)]
pub struct Step {
    pub now: f64,
    pub ready: Vec<bool>,
    pub restore: bool,
    pub warm: bool,
}

#[derive(Deserialize)]
pub struct Input {
    pub repository: PathBuf,
    pub launcher: PathBuf,
    pub state: PathBuf,
    pub lock: PathBuf,
    pub proc_root: PathBuf,
    pub phase: PathBuf,
    pub base: String,
    pub steps: Vec<Step>,
    pub busy: bool,
    pub index_lock: bool,
    pub cancel: bool,
    pub cancel_ready: PathBuf,
    pub cancel_release: PathBuf,
}

pub fn held(path: &Path) -> Result<bool, Error> {
    let file = File::open(path)?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(true),
        Ok(()) => Ok(false),
        Err(error) => Err(std::io::Error::from(error).into()),
    }
}

pub fn effects(lock: &Path, records: Arc<Mutex<Vec<Value>>>) -> Effects {
    let alert_records = Arc::clone(&records);
    let alert_lock = lock.to_path_buf();
    let notify_lock = lock.to_path_buf();
    Effects {
        clock: Arc::new(|| Time {
            seconds: Value::Float(1700000000.75),
            nanoseconds: Value::integer(1700000000750000000_u64),
        }),
        alert: Arc::new(move |show, detail| {
            alert_records
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?
                .push(Value::object([
                    ("show", Value::Bool(show)),
                    ("detail", if show { detail.clone() } else { Value::Null }),
                    ("lock_held", Value::Bool(held(&alert_lock)?)),
                ]));
            Ok(())
        }),
        notify: Arc::new(move |notification| {
            let head = notification.old_head;
            let records = Arc::clone(&records);
            let lock = notify_lock.clone();
            Box::pin(async move {
                tokio::task::yield_now().await;
                records
                    .lock()
                    .map_err(|error| Error::Source(error.to_string()))?
                    .push(Value::object([
                        ("notify", Value::text(&head)),
                        ("lock_held", Value::Bool(held(&lock)?)),
                    ]));
                Ok(())
            })
        }),
    }
}
