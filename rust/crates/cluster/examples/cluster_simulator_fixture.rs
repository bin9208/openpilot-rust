use openpilot_cluster::models::SimulatorInput;
use openpilot_cluster::simulator::{ClusterSimulator, RandomInputSource};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;

#[derive(Deserialize)]
struct Step {
    dt: f64,
    command: SimulatorInput,
}
#[derive(Deserialize)]
struct Request {
    sequences: Vec<Vec<Step>>,
    random_steps: Vec<f64>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let sequences: Vec<Vec<Value>> = request
        .sequences
        .into_iter()
        .map(|steps| {
            let mut simulator = ClusterSimulator::default();
            steps
                .into_iter()
                .map(|step| {
                    let ui = simulator.update(&step.command, step.dt);
                    json!({"ui":ui,"internal":simulator})
                })
                .collect()
        })
        .collect();
    let mut source = RandomInputSource::default();
    let mut simulator = ClusterSimulator::default();
    let random: Vec<_> = request
        .random_steps
        .into_iter()
        .map(|dt| {
            let command = source.update(dt);
            let ui = simulator.update(&command, dt);
            json!({"command":command,"input_internal":source,"ui":ui,"internal":simulator})
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&json!({"sequences":sequences,"random":random}))?
    );
    Ok(())
}
