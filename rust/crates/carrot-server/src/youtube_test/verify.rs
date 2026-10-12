use super::{config::Config, control, output, report, status, storage};
use crate::Error;
use std::time::Duration;

pub(super) async fn run(config: &Config) -> Result<i32, Error> {
    if status::get(config)?.get("runner_alive").truth() {
        eprintln!("[youtube-test] refused: stop the active test before one-shot verification");
        return Ok(1);
    }
    let mut started = false;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let checked: Result<i32, Error> = async {
        let result = control::start(config, false).await?;
        started = status::get(config)?.get("runner_alive").truth();
        if result != 0 { return Ok(1); }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(75);
        let mut stable = None;
        let mut result = 1;
        let mut current = status::get(config)?;
        while tokio::time::Instant::now() < deadline {
            current = status::get(config)?;
            if storage::text(current.get("status")) == "error" || !current.get("runner_alive").truth() { break; }
            if report::diagnose(&current)?.get("healthy").truth() {
                let since = stable.get_or_insert_with(tokio::time::Instant::now);
                if since.elapsed() >= Duration::from_secs(10) { result = 0; break; }
            } else { stable = None; }
            tokio::select! {
                _ = interrupt.recv() => { eprintln!("[youtube-test] verification interrupted"); return Ok(130); }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
        output::print(config, Some(current))?;
        if result == 0 { println!("[youtube-test] verification passed (10s stable)"); }
        else { eprintln!("[youtube-test] verification failed; attach {}", config.paths.report.display()); }
        Ok(result)
    }.await;
    if started {
        control::stop(config).await?;
    }
    checked
}
