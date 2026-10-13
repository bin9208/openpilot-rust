use openpilot_cluster::live::Input;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
struct Case {
    started: f64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    messages: Vec<PathBuf>,
    #[serde(default)]
    params: BTreeMap<String, Option<String>>,
    #[serde(default)]
    panels: [bool; 3],
    #[serde(default)]
    hud: i64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, path] = args.as_slice() else {
        return Err("cluster_input_fixture CASE".into());
    };
    let case: Case = serde_json::from_slice(&std::fs::read(path)?)?;
    let directory = tempfile::tempdir()?;
    let params = openpilot_params::Params::open(directory.path(), "cluster")?;
    let mut input = Input::isolated(false, 0, case.started)?;
    input.attach_params(params.clone(), None);
    let mut output = Vec::new();
    for step in case.steps {
        for (key, value) in step.params {
            if let Some(value) = value {
                params.put(&key, value.as_bytes())?;
            } else {
                params.remove(&key)?;
            }
        }
        input.set_debug_panels_enabled(step.panels[0], step.panels[1], step.panels[2]);
        input.set_hud_debug_mode(step.hud);
        let messages = step
            .messages
            .iter()
            .map(std::fs::read)
            .collect::<Result<Vec<_>, _>>()?;
        input.subscriber.state.update(step.now, &messages)?;
        output.push(input.project_at(step.now)?);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
