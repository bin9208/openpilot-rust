#[path = "auto_update_attempt/fixture.rs"]
mod fixture;
use fixture::{effects, held, Input};
use openpilot_carrot_server::{
    auto_update_pull::{Policy, Pull, Update, UpdateFailure},
    git_state::Store,
    git_status::{Repository, Service},
    repo_update::{Inspection, Recovery},
    Error, Value,
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let input: Input = serde_json::from_str(&raw)?;
    let repository = || Repository {
        directory: input.repository.clone(),
        launcher: input.launcher.clone(),
        lock: input.lock.clone(),
    };
    let service = Service::with_clock(repository(), || 1000.);
    let recovery = Recovery::with_inspection(
        repository(),
        Inspection {
            proc_root: input.proc_root.clone(),
            ..Inspection::default()
        },
    );
    let store = Arc::new(Store::new(input.state.clone()));
    let records = Arc::new(Mutex::new(Vec::new()));
    let now = Arc::new(Mutex::new(0.));
    let readiness = Arc::new(Mutex::new(VecDeque::new()));
    let ready_calls = Arc::new(Mutex::new(0_usize));
    let sampled_now = Arc::clone(&now);
    let sampled_ready = Arc::clone(&readiness);
    let calls = Arc::clone(&ready_calls);
    let pull = Pull::with_service(
        Arc::clone(&service),
        Arc::clone(&store),
        effects(&input.lock, Arc::clone(&records)),
    );
    let mut update = Update::new(
        pull,
        recovery,
        Policy {
            monotonic: Arc::new(move || {
                sampled_now.lock().map(|now| *now).unwrap_or(f64::INFINITY)
            }),
            ready: Box::new(move || match (calls.lock(), sampled_ready.lock()) {
                (Ok(mut calls), Ok(mut ready)) => {
                    *calls = calls.saturating_add(1);
                    ready.pop_front().unwrap_or(true)
                }
                (Err(_), _) | (_, Err(_)) => false,
            }),
        },
    );
    let mut results = Vec::new();
    for step in input.steps {
        std::fs::write(&input.phase, "fixture")?;
        if step.restore {
            let result = std::process::Command::new("/usr/bin/git")
                .args(["reset", "--hard", &input.base])
                .current_dir(&input.repository)
                .output()?;
            if !result.status.success() {
                return Err(Error::Source(String::from_utf8_lossy(&result.stderr).into()).into());
            }
        }
        if step.warm {
            service.get(true).await?;
        }
        std::fs::write(&input.phase, "attempt")?;
        *now.lock().map_err(|error| error.to_string())? = step.now;
        *readiness.lock().map_err(|error| error.to_string())? = step.ready.into();
        *ready_calls.lock().map_err(|error| error.to_string())? = 0;
        let contender: Option<File> = if input.busy {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&input.lock)?;
            file.lock()?;
            Some(file)
        } else {
            None
        };
        if input.index_lock {
            std::fs::write(
                input.repository.join(".git/index.lock"),
                "owned current index lock\n",
            )?;
        }
        let (cancel, stopped) = tokio::sync::watch::channel(false);
        let timer = input.cancel.then(|| {
            let cancel = cancel.clone();
            let ready = input.cancel_ready.clone();
            let release = input.cancel_release.clone();
            let lock = input.lock.clone();
            let records = Arc::clone(&records);
            tokio::spawn(async move {
                let started = Instant::now();
                while !ready.exists() {
                    if started.elapsed() > Duration::from_secs(2) {
                        cancel.send_replace(true);
                        return Err(Error::Source(
                            "owned configuration readiness missing".into(),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                cancel.send_replace(true);
                tokio::task::yield_now().await;
                records
                    .lock()
                    .map_err(|error| Error::Source(error.to_string()))?
                    .push(Value::object([(
                        "cancel_lock_held",
                        Value::Bool(held(&lock)?),
                    )]));
                std::fs::write(release, "release owned configuration child")?;
                Ok(())
            })
        });
        let output = match update.run(stopped).await {
            Ok((pulled, updated, head)) => Value::object([(
                "result",
                Value::Array(vec![
                    Value::Bool(pulled),
                    Value::Bool(updated),
                    Value::text(&head),
                ]),
            )]),
            Err(UpdateFailure::Status(openpilot_carrot_server::git_status::Failure::Cancelled))
            | Err(UpdateFailure::Recovery(
                openpilot_carrot_server::repo_update::Failure::Cancelled,
            )) => Value::object([
                ("exception", Value::text("CancelledError")),
                ("message", Value::text("")),
            ]),
            Err(error) => Value::object([
                ("exception", Value::text("NativeError")),
                ("message", Value::text(&error.to_string())),
            ]),
        };
        if let Some(timer) = timer {
            timer.await??;
        }
        drop(cancel);
        drop(contender);
        std::fs::write(&input.phase, "probe")?;
        let cache = service.get(false).await?;
        results.push(Value::object([
            ("output", output),
            (
                "ready_calls",
                Value::integer(*ready_calls.lock().map_err(|error| error.to_string())?),
            ),
            ("state", store.read()),
            (
                "cache_head",
                Value::text(cache.head.as_deref().unwrap_or("")),
            ),
        ]));
    }
    println!(
        "{}",
        Value::object([
            ("steps", Value::Array(results)),
            (
                "effects",
                Value::Array(records.lock().map_err(|error| error.to_string())?.clone())
            ),
        ])
        .encode()?
    );
    Ok(())
}
