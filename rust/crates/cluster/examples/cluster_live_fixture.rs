use openpilot_cluster::{live::Input, models::ClusterAlert};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Case {
    include_can: bool,
    started: f64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    messages: Vec<PathBuf>,
    onroad: bool,
    current: Option<ClusterAlert>,
    frames: u64,
    radar: usize,
    detected: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, case_file] = args.as_slice() else {
        return Err("cluster_live_fixture CASE".into());
    };
    let case: Case = serde_json::from_slice(&std::fs::read(case_file)?)?;
    let mut input = Input::isolated(case.include_can, 0, case.started)?;
    let mut output = Vec::new();
    for step in case.steps {
        let messages = step
            .messages
            .iter()
            .map(std::fs::read)
            .collect::<Result<Vec<_>, _>>()?;
        input.subscriber.state.update(step.now, &messages)?;
        input.frames = step.frames;
        let onroad = input.onroad_state()?;
        let brightness = input.brightness()?;
        let alert = input.alert(step.current, step.onroad, step.now)?;
        let status = input.status_text(step.now, step.radar, step.detected)?;
        output.push(serde_json::json!({"onroad":onroad,"brightness":brightness,"alert":alert,"status":status}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
