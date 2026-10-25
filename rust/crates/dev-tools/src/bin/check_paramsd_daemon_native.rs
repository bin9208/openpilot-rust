#[path = "../paramsd_daemon_check.rs"]
mod controller;

use controller::Options;
use std::path::PathBuf;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut values = std::collections::BTreeMap::new();
    while let Some(flag) = arguments.next() {
        let flag = flag.into_string().map_err(|_| "non-UTF-8 option")?;
        if !matches!(
            flag.as_str(),
            "--daemon" | "--library" | "--continuous-input" | "--restart-input" | "--evidence"
        ) {
            return Err(format!("unknown option: {flag}").into());
        }
        let value = arguments.next().ok_or("option value missing")?;
        if values.insert(flag, PathBuf::from(value)).is_some() {
            return Err("duplicate option".into());
        }
    }
    let mut take = |flag: &str| {
        values
            .remove(flag)
            .ok_or_else(|| format!("{flag} required"))
    };
    let options = Options {
        daemon: take("--daemon")?,
        library: take("--library")?,
        continuous_input: take("--continuous-input")?,
        restart_input: take("--restart-input")?,
        evidence: take("--evidence")?,
    };
    controller::run(&options)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd native controller: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
