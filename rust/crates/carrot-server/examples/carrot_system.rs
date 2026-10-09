use openpilot_carrot_server::{
    params::Backend,
    system::{
        defaults,
        network::Network,
        time_command::{Failure, Invocation},
        time_sync::{Request, TimeSync},
    },
    Error, Value,
};
use serde::Deserialize;
use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[path = "carrot_system/actions.rs"]
mod action_fixture;
#[path = "carrot_system/server.rs"]
mod server;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Time,
    Network,
    Select,
    Action,
    Calibration,
    Defaults,
    Server,
}

#[derive(Deserialize)]
struct Input {
    mode: Mode,
    root: PathBuf,
    #[serde(default)]
    now: i64,
    #[serde(default)]
    fail: Option<usize>,
    #[serde(default)]
    spawn_fail: Option<usize>,
    #[serde(default = "default_code")]
    code: i32,
    #[serde(default)]
    steps: Vec<serde_json::Value>,
}
fn default_code() -> i32 {
    1
}

#[derive(Deserialize)]
struct Selection {
    name: String,
    definition: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Refresh,
    Snapshot,
}

#[derive(Deserialize)]
struct Step {
    operation: Operation,
    #[serde(default)]
    values: serde_json::Map<String, serde_json::Value>,
}

fn time(input: &Input, body: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&commands);
    let root = input.root.clone();
    let fail = input.fail;
    let spawn_fail = input.spawn_fail;
    let code = input.code;
    let now = input.now;
    let sync = TimeSync {
        localtime: root.join("localtime"),
        zoneinfo: root.join("zones"),
        now: Arc::new(move || now),
        command: Arc::new(move |command| {
            let value = match command {
                Invocation::Argv(args) => {
                    Value::Array(args.iter().map(|arg| Value::text(arg)).collect())
                }
                Invocation::Shell(text) => Value::text(text),
            };
            let mut traces = recorded
                .lock()
                .map_err(|_| Error::Source("trace lock".into()))?;
            traces.push(value);
            if fail == Some(traces.len()) {
                return Err(Failure::Exit(format!(
                    "Command '{}' returned non-zero exit status {code}.",
                    command.repr()?
                )));
            }
            if spawn_fail == Some(traces.len()) {
                return Err(
                    Error::Source("[Errno 2] No such file or directory: 'sudo'".into()).into(),
                );
            }
            if let Invocation::Argv(args) = command {
                match args.get(1).map(String::as_str) {
                    Some("rm") => fs::remove_file(root.join("localtime")).map_err(Error::from)?,
                    Some("ln") => std::os::unix::fs::symlink(&args[3], root.join("localtime"))
                        .map_err(Error::from)?,
                    Some(_) | None => {
                        return Err(Error::Source("unexpected owned command".into()).into())
                    }
                }
            }
            Ok(())
        }),
    };
    let value = match Request::parse(body).and_then(|request| sync.sync(&request)) {
        Ok(value) => value,
        Err(error) => Value::object([("exception", Value::text(&error.to_string()))]),
    };
    let commands = commands.lock().map_err(|_| "trace lock")?.clone();
    Ok(Value::object([
        ("value", value),
        ("commands", Value::Array(commands)),
    ]))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if matches!(input.mode, Mode::Server) {
        return server::run(serde_json::from_str(&line)?).await;
    }
    let result = match input.mode {
        Mode::Action | Mode::Calibration | Mode::Defaults => {
            action_fixture::run(serde_json::from_str(&line)?)?
        }
        Mode::Server => return Err("server fixture dispatched twice".into()),
        Mode::Time => time(&input, Value::parse(&line)?.get("body"))?,
        Mode::Network => {
            let network = Network::new(input.root.join("nmcli"));
            let mut params = Backend::memory(input.root.clone());
            let mut values = Vec::new();
            for step in input.steps {
                let step: Step = serde_json::from_value(step)?;
                for (key, value) in step.values {
                    params.put(&key, &Value::parse(&value.to_string())?, None)?;
                }
                values.push(match step.operation {
                    Operation::Refresh => network.refresh(&params)?,
                    Operation::Snapshot => network.snapshot(&params)?,
                });
            }
            Value::object([("values", Value::Array(values))])
        }
        Mode::Select => {
            let values = input
                .steps
                .into_iter()
                .map(|step| {
                    let row: Selection = serde_json::from_value(step)?;
                    Ok(Value::Bool(defaults::selected(
                        &row.name,
                        &Value::parse(&row.definition.to_string())?,
                    )))
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            Value::object([("values", Value::Array(values))])
        }
    };
    println!("{}", result.encode()?);
    Ok(())
}
