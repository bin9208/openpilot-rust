use openpilot_carrot_server::{Error, git_status::Repository, repo_update};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

#[derive(Deserialize, Serialize)]
struct Input {
    repo: PathBuf,
    launcher: PathBuf,
    lock: PathBuf,
    proc_root: PathBuf,
    now: f64,
    action: String,
    cancel: bool,
    python: PathBuf,
    source: PathBuf,
}

fn held(path: &Path) -> Result<bool, Error> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(false),
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(true),
        Err(error) => Err(std::io::Error::from(error).into()),
    }
}

fn record(locks: &Mutex<Vec<bool>>, path: &Path) -> Result<(), Error> {
    locks
        .lock()
        .map_err(|_| Error::Source("owned inspection observations poisoned".into()))?
        .push(held(path)?);
    Ok(())
}

fn mutate(input: &Input) -> Result<(), Error> {
    let mut child = Command::new(&input.python)
        .arg(&input.source)
        .arg("--mutate")
        .stdin(Stdio::piped())
        .spawn()?;
    let Some(mut pipe) = child.stdin.take() else {
        child.kill()?;
        child.wait()?;
        return Err(Error::Source("owned mutation pipe missing".into()));
    };
    let written = serde_json::to_vec(input)
        .map_err(|error| Error::Source(error.to_string()))
        .and_then(|bytes| pipe.write_all(&bytes).map_err(Error::from));
    drop(pipe);
    let status = child.wait()?;
    written?;
    if !status.success() {
        return Err(Error::Source("owned inspection mutation failed".into()));
    }
    Ok(())
}

fn readiness(input: &Input) -> Result<(PathBuf, PathBuf), Error> {
    let directory = input
        .repo
        .parent()
        .ok_or_else(|| Error::Source("owned repo parent missing".into()))?;
    Ok((
        directory.join("pause-entered"),
        directory.join("pause-release"),
    ))
}

async fn run(input: Arc<Input>) -> Result<serde_json::Value, Error> {
    let lock = Arc::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&input.lock)?,
    );
    lock.lock()?;
    let locks = Arc::new(Mutex::new(Vec::new()));
    let observation = Arc::clone(&locks);
    let callback = Arc::clone(&input);
    let (ready, release) = readiness(&input)?;
    let pause_ready = ready.clone();
    let pause_release = release.clone();
    let recovery = repo_update::Recovery::with_inspection(
        Repository {
            directory: input.repo.clone(),
            launcher: input.launcher.clone(),
            lock: input.lock.clone(),
        },
        repo_update::Inspection {
            proc_root: input.proc_root.clone(),
            now: Arc::new({
                let now = input.now;
                move || now
            }),
            pause: Arc::new(move |duration| {
                if duration != Duration::from_millis(100) {
                    return Err(Error::Source("owned recheck duration changed".into()));
                }
                record(&observation, &callback.lock)?;
                fs::write(&pause_ready, "owned inspection recheck entered\n")?;
                if callback.cancel {
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while !pause_release.exists() {
                        if Instant::now() >= deadline {
                            return Err(Error::Source("owned recheck release missing".into()));
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                std::thread::sleep(duration);
                mutate(&callback)?;
                record(&observation, &callback.lock)
            }),
        },
    );
    let (sender, stopped) = watch::channel(false);
    let timer = input.cancel.then(|| {
        let sender = sender.clone();
        let input = Arc::clone(&input);
        let locks = Arc::clone(&locks);
        tokio::spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !ready.exists() {
                if Instant::now() >= deadline {
                    return Err(Error::Source("owned recheck readiness missing".into()));
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            sender.send_replace(true);
            tokio::time::sleep(Duration::from_millis(20)).await;
            record(&locks, &input.lock)?;
            fs::write(release, "release owned inspection after cancellation\n")?;
            Ok::<_, Error>(())
        })
    });
    let output = match recovery.prepare(Arc::clone(&lock), stopped).await {
        Ok(result) => json!({"result": result}),
        Err(repo_update::Failure::Busy(message)) => {
            json!({"exception": "RepoBusyError", "message": message})
        }
        Err(repo_update::Failure::Decode(error)) => {
            json!({"exception": "UnicodeDecodeError", "message": error.to_string()})
        }
        Err(repo_update::Failure::Timeout(message)) => {
            json!({"exception": "TimeoutExpired", "message": message})
        }
        Err(repo_update::Failure::Cancelled) => {
            json!({"exception": "CancelledError", "message": ""})
        }
        Err(error) => json!({"exception": "RuntimeError", "message": error.to_string()}),
    };
    if let Some(timer) = timer {
        timer
            .await
            .map_err(|error| Error::Source(error.to_string()))??;
    }
    drop(sender);
    drop(lock);
    let observations = locks
        .lock()
        .map_err(|_| Error::Source("owned inspection observations poisoned".into()))?;
    Ok(json!({"output": output, "locks": *observations}))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let input = Arc::new(serde_json::from_str(&raw)?);
    println!("{}", run(input).await?);
    Ok(())
}
