//! Active terminal youtube-test support, from services/youtube_test.py.
mod cleanup;
mod config;
mod control;
mod output;
mod poll;
mod process;
pub mod report;
mod runner;
pub mod status;
mod storage;
mod verify;
use crate::Error;
pub use config::{CommandSpec, Config, Paths};

pub fn help(args: &[String]) -> bool {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.to_lowercase().as_str(), "help" | "-h" | "--help"))
    {
        println!("Usage: carrot youtube-test [verify|start|status|logs|stop] [--lines N]\n  verify  Run one complete test, save a compact report, then stop\n  start   Start an offroad YouTube camera test\n  status  Show encoder and upload diagnostics\n  logs    Show recent test process logs\n  stop    Stop the test and restore its live toggle");
        return true;
    }
    false
}
pub async fn run_command(config: &Config, args: &[String]) -> Result<i32, Error> {
    if help(args) {
        return Ok(0);
    }
    let mut action = None;
    let mut lines = 80_i64;
    let mut quality = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        if name == "--lines" {
            let Some(value) = inline
                .or_else(|| args.next().map(String::as_str))
                .and_then(|value| value.parse().ok())
            else {
                return Ok(2);
            };
            lines = value;
        } else if name == "--quality" {
            let Some(value) = inline
                .or_else(|| args.next().map(String::as_str))
                .and_then(|value| value.parse::<i32>().ok())
                .filter(|value| (0..=3).contains(value))
            else {
                return Ok(2);
            };
            quality = Some(value);
        } else if action.is_none() && !arg.starts_with('-') {
            action = Some(arg.as_str());
        } else {
            return Ok(2);
        }
    }
    match action.unwrap_or("start") {
        "_run" if quality.is_some() => runner::run(config, quality.unwrap_or(0)).await,
        "verify" if quality.is_none() => verify::run(config).await,
        "start" if quality.is_none() => control::start(config, true).await,
        "stop" if quality.is_none() => control::stop(config).await,
        "status" if quality.is_none() => output::print(config, None),
        "logs" if quality.is_none() => {
            println!("[youtube-test] log={}", config.paths.log.display());
            for line in storage::tail(config, usize::try_from(lines.clamp(1, 500)).unwrap_or(80)) {
                println!("{line}");
            }
            Ok(0)
        }
        _ => Ok(2),
    }
}
