#[path = "support/driver.rs"]
mod driver;
#[path = "support/protocol.rs"]
mod protocol;

use openpilot_process_supervision::{Signal, StopOptions};
use std::{
    io::{BufRead, Write},
    process::ExitCode,
    time::Instant,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock().lines();
    let config = serde_json::from_str(&input.next().ok_or("missing configuration")??)?;
    let mut processes = driver::processes(config)?;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for line in input {
            let request: protocol::Request = serde_json::from_str(&line?)?;
            let action = request.action;
            let exit = matches!(action, protocol::Action::Exit);
            let start = Instant::now();
            if request.acknowledge {
                println!("{{\"acknowledged\":true}}");
                std::io::stdout().flush()?;
            }
            let result = driver::action(&mut processes, action);
            let elapsed = start.elapsed().as_secs_f64();
            let (value, error) = match result {
                Ok(value) => (value, serde_json::Value::Null),
                Err(error) => (serde_json::Value::Null, driver::error(&error)),
            };
            println!(
                "{}",
                serde_json::json!({"result": value, "error": error, "elapsed": elapsed, "snapshots": driver::snapshots(&mut processes)?})
            );
            std::io::stdout().flush()?;
            if exit {
                break;
            }
        }
        Ok(())
    })();
    for process in &mut processes {
        process.stop(StopOptions {
            signal: Some(Signal::KILL),
            ..StopOptions::default()
        })?;
    }
    result
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("process trace: {error}");
            ExitCode::FAILURE
        }
    }
}
