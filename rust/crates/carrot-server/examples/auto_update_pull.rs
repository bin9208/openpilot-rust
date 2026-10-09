use openpilot_carrot_server::{
    auto_update_pull::{Attempt, Effects, Failure, Pull},
    git_state::{Store, Time},
    git_status::Repository,
    Error, Value,
};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Input {
    repository: PathBuf,
    launcher: PathBuf,
    state: PathBuf,
    lock: PathBuf,
    target: String,
    #[serde(default)]
    cancel_after_ms: Option<u64>,
    #[serde(default)]
    cancel_ready: Option<PathBuf>,
    #[serde(default)]
    alert_failure: bool,
    #[serde(default)]
    notify_failure: bool,
}

fn repository_held(path: &Path) -> Result<bool, Error> {
    let file = File::open(path)?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(true),
        Ok(()) => Ok(false),
        Err(error) => Err(std::io::Error::from(error).into()),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let input: Input = serde_json::from_str(&raw)?;
    let alerts = Arc::new(Mutex::new(Vec::<Value>::new()));
    let notifications = Arc::new(Mutex::new(Vec::<Value>::new()));
    let effect_locks = Arc::new(Mutex::new(Vec::<Value>::new()));
    let alert_sink = Arc::clone(&alerts);
    let notify_sink = Arc::clone(&notifications);
    let alert_checks = Arc::clone(&effect_locks);
    let notify_checks = Arc::clone(&effect_locks);
    let alert_lock = input.lock.clone();
    let notify_lock = input.lock.clone();
    let store = Arc::new(Store::new(input.state));
    let effects = Effects {
        clock: Arc::new(|| Time {
            seconds: Value::Float(1700000000.75),
            nanoseconds: Value::integer(1700000000750000000_u64),
        }),
        alert: Arc::new(move |show, detail| {
            alert_checks
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?
                .push(Value::Bool(repository_held(&alert_lock)?));
            alert_sink
                .lock()
                .map_err(|error| Error::Source(error.to_string()))?
                .push(Value::object([
                    ("show", Value::Bool(show)),
                    ("detail", if show { detail.clone() } else { Value::Null }),
                ]));
            if input.alert_failure {
                Err(Error::Source("owned alert failure".into()))
            } else {
                Ok(())
            }
        }),
        notify: Arc::new(move |notification| {
            let old_head = notification.old_head;
            let sink = Arc::clone(&notify_sink);
            let checks = Arc::clone(&notify_checks);
            let lock = notify_lock.clone();
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                checks
                    .lock()
                    .map_err(|error| Error::Source(error.to_string()))?
                    .push(Value::Bool(repository_held(&lock)?));
                sink.lock()
                    .map_err(|error| Error::Source(error.to_string()))?
                    .push(Value::text(&old_head));
                if input.notify_failure {
                    Err(Error::Source("owned notify failure".into()).into())
                } else {
                    Ok(())
                }
            })
        }),
    };
    let pull = Pull::new(
        Repository {
            directory: input.repository,
            launcher: input.launcher,
            lock: input.lock.clone(),
        },
        Arc::clone(&store),
        effects,
    );
    let lock = Arc::new(
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(input.lock)?,
    );
    lock.lock()?;
    let (cancel, stopped) = tokio::sync::watch::channel(false);
    let timer = input.cancel_after_ms.map(|millis| {
        let cancel = cancel.clone();
        let ready = input.cancel_ready;
        tokio::spawn(async move {
            if let Some(ready) = ready {
                let started = Instant::now();
                while !ready.exists() {
                    if started.elapsed() >= Duration::from_secs(2) {
                        cancel.send_replace(true);
                        return Err(Error::Source(
                            "owned reset descendant readiness missing".into(),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            tokio::time::sleep(Duration::from_millis(millis)).await;
            cancel.send_replace(true);
            Ok::<_, Error>(())
        })
    });
    let output = match pull
        .run(Attempt {
            target: &input.target,
            lock,
            stopped,
        })
        .await
    {
        Ok((pulled, updated, head)) => Value::object([(
            "result",
            Value::Array(vec![
                Value::Bool(pulled),
                Value::Bool(updated),
                Value::text(&head),
            ]),
        )]),
        Err(Failure::Busy(message)) => Value::object([
            ("exception", Value::text("RepoBusyError")),
            ("message", Value::text(&message)),
        ]),
        Err(Failure::Command(openpilot_carrot_server::git_status::Failure::Cancelled)) => {
            Value::object([
                ("exception", Value::text("CancelledError")),
                ("message", Value::text("")),
            ])
        }
        Err(error) => Value::object([
            ("exception", Value::text("NativeError")),
            ("message", Value::text(&error.to_string())),
        ]),
    };
    if let Some(timer) = timer {
        timer.await??;
    }
    drop(cancel);
    println!(
        "{}",
        Value::object([
            ("output", output),
            ("state", store.read()),
            (
                "effect_locks",
                Value::Array(
                    effect_locks
                        .lock()
                        .map_err(|error| error.to_string())?
                        .clone()
                )
            ),
            (
                "alerts",
                Value::Array(alerts.lock().map_err(|error| error.to_string())?.clone())
            ),
            (
                "notifications",
                Value::Array(
                    notifications
                        .lock()
                        .map_err(|error| error.to_string())?
                        .clone()
                )
            ),
        ])
        .encode()?
    );
    Ok(())
}
