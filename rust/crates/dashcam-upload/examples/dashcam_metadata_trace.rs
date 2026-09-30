use openpilot_dashcam_upload::{metadata, worker};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Decode {
        value: String,
        key: String,
    },
    Concurrency {
        value: Option<String>,
    },
    Serial {
        cmdline: String,
    },
    Git {
        repo: PathBuf,
        args: Vec<String>,
        default: String,
    },
    Target {
        path: PathBuf,
        environment: openpilot_web_upload::Environment,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let requests: Vec<Request> = serde_json::from_str(&input)?;
    let output: Vec<Value> = requests
        .into_iter()
        .map(|request| match request {
            Request::Decode { value, key } => json!(metadata::decode_obfuscated(&value, &key)),
            Request::Concurrency { value } => json!(worker::concurrency(value.as_deref())),
            Request::Serial { cmdline } => json!(metadata::serial_from_cmdline(&cmdline)),
            Request::Git {
                repo,
                args,
                default,
            } => json!(metadata::git_text(
                &repo,
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                &default
            )),
            Request::Target { path, environment } => {
                match metadata::target_settings(&path, &environment) {
                    Ok(value) => json!({"value":value}),
                    Err(error) => json!({"error":error.to_string()}),
                }
            }
        })
        .collect();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
