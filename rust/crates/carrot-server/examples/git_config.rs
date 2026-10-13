use openpilot_carrot_server::git_config::{prepare_git_pull, repair_git_config, Repository};
use serde::Deserialize;
use serde_json::json;
use std::{fs::OpenOptions, io::Read, os::fd::AsFd, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Repair,
    Prepare,
}

#[derive(Deserialize)]
struct Input {
    operation: Operation,
    repo: PathBuf,
    launcher: PathBuf,
    remote: Option<String>,
    #[serde(default = "enabled")]
    repair_upstream: bool,
    lock: Option<PathBuf>,
}

const fn enabled() -> bool {
    true
}

fn run(input: &Input) -> Result<serde_json::Value, openpilot_carrot_server::Error> {
    let lock = input
        .lock
        .as_ref()
        .map(|path| {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path)?;
            file.lock()?;
            Ok::<_, std::io::Error>(file)
        })
        .transpose()?;
    let repository = Repository {
        directory: &input.repo,
        launcher: &input.launcher,
        lock: lock.as_ref().map(AsFd::as_fd),
    };
    match input.operation {
        Operation::Repair => Ok(json!({"result": repair_git_config(
            &repository, input.remote.as_deref(), input.repair_upstream
        )?})),
        Operation::Prepare => Ok(json!({"result": prepare_git_pull(&repository)?})),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let input = serde_json::from_str(&raw)?;
    let output = match run(&input) {
        Ok(output) => output,
        Err(openpilot_carrot_server::Error::Json(error)) => {
            json!({"exception": error.kind, "message": error.to_string()})
        }
        Err(error) => json!({"exception": "NativeError", "message": error.to_string()}),
    };
    println!("{output}");
    Ok(())
}
