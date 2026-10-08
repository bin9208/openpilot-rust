use openpilot_carrot_server::git_status::{Repository, Service};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinHandle, JoinSet},
};
#[path = "git_status/lifecycle.rs"]
mod lifecycle;

#[derive(Deserialize)]
struct Config {
    repo: PathBuf,
    lock: PathBuf,
    launcher: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Step {
    Get { force: bool, now: f64 },
    Group { force: bool, now: f64, count: usize },
    Clear,
    Cancel { notice: PathBuf },
    CancelImmediate,
    QueuedPollCancel { notice: PathBuf },
    Poll { interval_ms: u64, initial_ms: u64 },
    StopPoll,
}

type PollTask = (
    watch::Sender<bool>,
    JoinHandle<Result<(), openpilot_carrot_server::git_status::Failure>>,
);

async fn stop_poll(poll: &mut Option<PollTask>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some((sender, task)) = poll.take() {
        sender.send_replace(true);
        task.await??;
    }
    Ok(())
}

async fn run(
    config: Config,
    mut input: mpsc::Receiver<Result<Step, serde_json::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let clock = Arc::new(AtomicU64::new(1000f64.to_bits()));
    let read_clock = Arc::clone(&clock);
    let service = Service::with_clock(
        Repository {
            directory: config.repo,
            lock: config.lock,
            launcher: config.launcher,
        },
        move || f64::from_bits(read_clock.load(Ordering::Relaxed)),
    );
    let mut poll = None;
    while let Some(step) = input.recv().await {
        let output = match step? {
            Step::Get { force, now } => {
                clock.store(now.to_bits(), Ordering::Relaxed);
                serde_json::to_value(service.get(force).await?)?
            }
            Step::Group { force, now, count } => {
                if count > 8 {
                    return Err("owned group exceeds eight requests".into());
                }
                clock.store(now.to_bits(), Ordering::Relaxed);
                let mut requests = JoinSet::new();
                for _ in 0..count {
                    let service = Arc::clone(&service);
                    requests.spawn(async move { service.get(force).await });
                }
                let mut states = Vec::new();
                while let Some(status) = requests.join_next().await {
                    states.push(status??);
                }
                serde_json::to_value(states)?
            }
            Step::Clear => {
                service.clear_cache()?;
                json!({"cleared":true})
            }
            Step::Cancel { notice } => {
                let work = Arc::clone(&service);
                let task = tokio::spawn(async move { work.get(true).await });
                tokio::task::spawn_blocking(move || {
                    let mut byte = [0];
                    std::fs::File::open(notice)?.read_exact(&mut byte)
                })
                .await??;
                task.abort();
                match task.await {
                    Err(error) if error.is_cancelled() => {}
                    _ => return Err("owned request did not cancel".into()),
                }
                service.wait_idle().await;
                json!({"cancelled":true})
            }
            Step::CancelImmediate => {
                let work = Arc::clone(&service);
                let task = tokio::spawn(async move { work.get(true).await });
                tokio::task::yield_now().await;
                task.abort();
                match task.await {
                    Err(error) if error.is_cancelled() => {}
                    _ => return Err("initial request did not cancel".into()),
                }
                service.wait_idle().await;
                json!({"cancelled":true})
            }
            Step::QueuedPollCancel { notice } => {
                lifecycle::queued_poll_cancel(&service, notice).await?
            }
            Step::Poll {
                interval_ms,
                initial_ms,
            } => {
                stop_poll(&mut poll).await?;
                let (sender, mut stopped) = watch::channel(false);
                let work = Arc::clone(&service);
                poll = Some((
                    sender,
                    tokio::spawn(async move {
                        work.run_loop_with_timing(
                            &mut stopped,
                            Duration::from_millis(interval_ms),
                            Duration::from_millis(initial_ms),
                        )
                        .await
                    }),
                ));
                json!({"polling":true})
            }
            Step::StopPoll => {
                stop_poll(&mut poll).await?;
                json!({"stopped":true})
            }
        };
        println!("{output}");
        std::io::stdout().flush()?;
    }
    stop_poll(&mut poll).await
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut first = String::new();
    std::io::stdin().read_line(&mut first)?;
    let config = serde_json::from_str(&first)?;
    let (sender, receiver) = mpsc::channel(8);
    let input = std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if sender.blocking_send(serde_json::from_str(&line)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("owned Git status input: {error}");
                    break;
                }
            }
        }
    });
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(config, receiver));
    input.join().map_err(|_| "owned input thread failed")?;
    result
}
