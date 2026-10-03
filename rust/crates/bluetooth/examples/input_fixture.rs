use openpilot_bluetooth::{decode_events, enumerate, Input, InputBatch};
use serde::Deserialize;
use std::{
    error::Error,
    io::{self, BufRead, Write},
    os::fd::{AsFd, AsRawFd},
    path::Path,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("decode") => {
            for line in io::stdin().lock().lines() {
                let bytes: Vec<u8> = serde_json::from_str(&line?)?;
                let result = match decode_events(&bytes) {
                    Ok(events) => serde_json::json!({"events":events}),
                    Err(error) => serde_json::json!({"error":error.to_string()}),
                };
                println!("{result}");
            }
        }
        Some("enumerate") => {
            let sysfs = args.get(2).ok_or("sysfs required")?;
            let dev = args.get(3).ok_or("dev required")?;
            println!(
                "{}",
                serde_json::to_string(&enumerate(Path::new(sysfs), Path::new(dev)))?
            );
        }
        Some("open") => {
            let path = args.get(2).ok_or("path required")?;
            let before = std::fs::read_dir("/proc/self/fd")?.count();
            let mut input = match Input::open(Path::new(path)) {
                Ok(input) => input,
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::json!({"error":error.to_string(), "fd_delta":std::fs::read_dir("/proc/self/fd")?.count().checked_sub(before)})
                    );
                    return Ok(());
                }
            };
            println!(
                "{}",
                serde_json::json!({"opened":true,"fd":input.as_fd().as_raw_fd()})
            );
            io::stdout().flush()?;
            for line in io::stdin().lock().lines() {
                let Command::Read = serde_json::from_str(&line?)?;
                let result = match input.read() {
                    Ok(InputBatch::Pending) => serde_json::json!({"pending":true}),
                    Ok(InputBatch::Events(events)) => serde_json::json!({"events":events}),
                    Err(error) => serde_json::json!({"error":error.to_string()}),
                };
                println!("{result}");
                io::stdout().flush()?;
            }
            drop(input);
            println!(
                "{}",
                serde_json::json!({"closed":true,"fd_delta":std::fs::read_dir("/proc/self/fd")?.count().checked_sub(before)})
            );
        }
        _ => return Err("expected decode, enumerate or open".into()),
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Command {
    Read,
}
