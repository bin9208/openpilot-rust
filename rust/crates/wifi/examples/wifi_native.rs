use openpilot_wifi::{Command, Config, Event, Snapshot, WifiManager};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    address: String,
    launcher: PathBuf,
}
#[derive(Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "snake_case")]
enum Request {
    Snapshot,
    Command(Command),
    Stop,
}
#[derive(Serialize)]
struct Reply {
    snapshot: Snapshot,
    events: Vec<Event>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let start: Start =
        serde_json::from_str(&lines.next().ok_or("missing private bus configuration")??)?;
    if !start.address.starts_with("unix:") {
        return Err("owned Unix bus required".into());
    }
    let mut manager = WifiManager::start(Config {
        address: Some(start.address),
        launcher: start.launcher,
    })?;
    for line in lines {
        let request: Request = serde_json::from_str(&line?)?;
        match request {
            Request::Snapshot => {}
            Request::Command(command) => manager.send(command)?,
            Request::Stop => {
                manager.stop()?;
                println!("{{\"stopped\":true}}");
                return Ok(());
            }
        }
        println!(
            "{}",
            serde_json::to_string(&Reply {
                snapshot: manager.snapshot()?,
                events: manager.drain_events()?
            })?
        );
        io::stdout().flush()?;
    }
    manager.stop()?;
    Ok(())
}
