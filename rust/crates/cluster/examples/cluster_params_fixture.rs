use openpilot_cluster::{
    usb::{ParamsRuntime, Runtime},
    Error,
};
use openpilot_params::Params;
use serde_json::{json, Value};
use std::io::{self, Read};

#[derive(serde::Deserialize)]
struct Case {
    root: std::path::PathBuf,
    prefix: String,
    actions: Vec<Action>,
}
#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Flags,
    Connected { value: bool },
    Milliseconds,
    RemoveNamespace,
    ReplaceRootFile,
}

fn main() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let params = Params::open(&case.root, &case.prefix)?;
    let mut runtime = ParamsRuntime::at(&case.root, &case.prefix);
    let mut observations = Vec::new();
    for action in case.actions {
        observations.push(match action {
            Action::Flags => json!([runtime.gpu_active(), runtime.reset_protected()]),
            Action::Connected { value } => {
                runtime.connected(value);
                json!(params.get_bool("ClusterHudConnected")?)
            }
            Action::Milliseconds => json!(runtime.milliseconds()?),
            Action::RemoveNamespace => {
                std::fs::remove_dir_all(case.root.join(&case.prefix))?;
                Value::Null
            }
            Action::ReplaceRootFile => {
                std::fs::remove_dir_all(&case.root)?;
                std::fs::write(&case.root, b"ordinary file")?;
                Value::Null
            }
        });
    }
    println!("{}", serde_json::to_string(&Value::Array(observations))?);
    Ok(())
}
