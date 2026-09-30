use openpilot_checkout_status::{read_checkout_commit, UpdateStatus};
use serde::Deserialize;
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    process::ExitCode,
    time::Instant,
};

#[derive(Deserialize)]
struct Config {
    repo: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Read,
    Capture,
    Update { now_bits: u64 },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let Config { repo } = serde_json::from_str(&lines.next().ok_or("missing configuration")??)?;
    let mut status = None;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for line in lines {
        let request: Request = serde_json::from_str(&line?)?;
        let start = Instant::now();
        let mut commit = None;
        let mut returned = None;
        match request {
            Request::Read => commit = read_checkout_commit(&repo),
            Request::Capture => status = Some(UpdateStatus::new(&repo)),
            Request::Update { now_bits } => {
                returned = Some(
                    status
                        .as_mut()
                        .ok_or("capture status first")?
                        .update(f64::from_bits(now_bits)),
                );
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "{}",
            serde_json::json!({
                "commit": commit,
                "running_commit": status.as_ref().and_then(UpdateStatus::running_commit),
                "reboot_required": status.as_ref().is_some_and(UpdateStatus::reboot_required),
                "returned": returned,
                "elapsed": elapsed,
            })
        );
        std::io::stdout().flush()?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("checkout trace: {error}");
            ExitCode::FAILURE
        }
    }
}
