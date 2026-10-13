use openpilot_cluster::h264::diagnostics::{nal_stats, Diagnostics};
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    packets: Vec<Vec<u8>>,
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    interval: f64,
    started: f64,
    backend: String,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
struct Operation {
    at: f64,
    #[serde(flatten)]
    action: Action,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Unit {
        source: String,
        packet: Vec<u8>,
        chunks: Vec<usize>,
        keyframe: bool,
    },
    Queue {
        depth: usize,
    },
    Drop {
        units: usize,
        chunks: usize,
    },
    Send {
        source: String,
        bytes: usize,
        ms: f64,
    },
    Report {
        force: bool,
    },
}
fn scenario(scenario: Scenario) -> Vec<Option<String>> {
    let mut diagnostics = Diagnostics::new(scenario.interval, scenario.started);
    scenario
        .operations
        .into_iter()
        .map(|operation| match operation.action {
            Action::Unit {
                source,
                packet,
                chunks,
                keyframe,
            } => {
                diagnostics.record_unit(&source, &packet, &chunks, keyframe);
                None
            }
            Action::Queue { depth } => {
                diagnostics.record_queue_depth(depth);
                None
            }
            Action::Drop { units, chunks } => {
                diagnostics.record_drop(units, chunks);
                None
            }
            Action::Send { source, bytes, ms } => {
                diagnostics.record_send(&source, bytes, ms);
                diagnostics.report(operation.at, &scenario.backend, false)
            }
            Action::Report { force } => diagnostics.report(operation.at, &scenario.backend, force),
        })
        .collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin().lock())?;
    let packets: Vec<_> = input
        .packets
        .iter()
        .map(|packet| nal_stats(packet))
        .collect();
    let scenarios: Vec<_> = input.scenarios.into_iter().map(scenario).collect();
    println!(
        "{}",
        serde_json::json!({"packets":packets,"scenarios":scenarios})
    );
    Ok(())
}
