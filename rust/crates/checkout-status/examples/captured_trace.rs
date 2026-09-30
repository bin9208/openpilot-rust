use openpilot_process_supervision::CapturedCommand;
use serde::Deserialize;
use std::{
    io::Read, os::unix::process::ExitStatusExt, path::PathBuf, process::ExitCode, time::Instant,
};

#[derive(Deserialize)]
struct Config {
    launcher: PathBuf,
    cwd: PathBuf,
    argv: Vec<String>,
    #[serde(default)]
    inherit: bool,
    #[serde(default)]
    environment: Vec<(String, String)>,
    report: Option<PathBuf>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let config: Config = serde_json::from_str(&input)?;
    let parent_environment_before = std::env::var("PARAMS_COPY_PATH").ok();
    let start = Instant::now();
    let command = CapturedCommand {
        launcher: config.launcher,
        cwd: config.cwd,
        argv: config.argv.into_iter().map(Into::into).collect(),
    };
    let child = if config.inherit {
        if config.environment.is_empty() {
            command.spawn_inherited()
        } else {
            let environment = config
                .environment
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect::<Vec<_>>();
            command.spawn_inherited_with_env(&environment)
        }
    } else {
        command.spawn()
    };
    let spawn_elapsed = start.elapsed().as_secs_f64();
    let mut response = match child {
        Ok(child) => {
            let output = child.process.wait_with_output()?;
            serde_json::json!({"kind": "child_exit", "code": output.status.code(), "signal": output.status.signal(),
                "stdout": output.stdout, "stderr": output.stderr, "spawn_elapsed": spawn_elapsed,
                "total_elapsed": start.elapsed().as_secs_f64()})
        }
        Err(error) => {
            serde_json::json!({"kind": "spawn_error", "message": error.to_string(), "errno": match &error { openpilot_process_supervision::Error::Io(inner) => inner.raw_os_error(), _ => None }, "spawn_elapsed": spawn_elapsed})
        }
    };
    response["parent_environment_before"] = serde_json::json!(parent_environment_before);
    response["parent_environment_after"] =
        serde_json::json!(std::env::var("PARAMS_COPY_PATH").ok());
    if let Some(path) = config.report {
        std::fs::write(path, serde_json::to_vec(&response)?)?;
    } else {
        println!("{response}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("captured trace: {error}");
            ExitCode::FAILURE
        }
    }
}
