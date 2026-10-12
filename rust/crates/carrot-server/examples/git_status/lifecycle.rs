use openpilot_carrot_server::git_status::Service;
use serde_json::{json, Value};
use std::{io::Read, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;

pub async fn queued_poll_cancel(
    service: &Arc<Service>,
    notice: PathBuf,
) -> Result<Value, Box<dyn std::error::Error>> {
    let work = Arc::clone(service);
    let api = tokio::spawn(async move { work.get(true).await });
    tokio::task::spawn_blocking(move || {
        let mut byte = [0];
        std::fs::File::open(notice)?.read_exact(&mut byte)
    })
    .await??;
    let (sender, mut stopped) = watch::channel(false);
    let work = Arc::clone(service);
    let mut poll = tokio::spawn(async move {
        work.run_loop_with_timing(&mut stopped, Duration::from_millis(60), Duration::ZERO)
            .await
    });
    tokio::task::yield_now().await;
    sender.send_replace(true);
    let poll_stopped = match tokio::time::timeout(Duration::from_millis(500), &mut poll).await {
        Ok(result) => {
            result??;
            true
        }
        Err(_) => false,
    };
    let api_running = !api.is_finished();
    api.abort();
    match api.await {
        Err(error) if error.is_cancelled() => {}
        _ => return Err("owned API request did not cancel".into()),
    }
    service.wait_idle().await;
    if !poll_stopped {
        poll.await??;
    }
    Ok(json!({"stopped_while_api_running":poll_stopped && api_running}))
}
