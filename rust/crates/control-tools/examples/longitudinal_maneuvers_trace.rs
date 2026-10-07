use openpilot_control_tools::{
    longitudinal_maneuvers::{Controller, Input},
    longitudinal_maneuvers_wire,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
struct Request {
    stopping_speed: f64,
    inputs: Vec<Input>,
}
#[derive(Serialize)]
struct Snapshot {
    active: bool,
    finished: bool,
    run_completed: bool,
    action_index: usize,
    action_frames: u32,
    ready_count: u64,
    repeated: u32,
}
#[derive(Serialize)]
struct Row {
    packets: [Vec<u8>; 3],
    acceleration_bits: u64,
    selected: Option<usize>,
    state: Option<Snapshot>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let request: Request = serde_json::from_str(&text)?;
    let mut controller = Controller::new(request.stopping_speed)?;
    let mut rows = Vec::with_capacity(request.inputs.len());
    for input in request.inputs {
        let command = controller.step(&input)?;
        let packets = longitudinal_maneuvers_wire::encode(&command, input.valid, || Ok(1))?;
        let state = command.state.map(|state| Snapshot {
            active: state.active,
            finished: state.finished,
            run_completed: state.run_completed,
            action_index: state.action_index,
            action_frames: state.action_frames,
            ready_count: state.ready_count,
            repeated: state.repeated,
        });
        rows.push(Row {
            packets,
            acceleration_bits: command.acceleration.to_bits(),
            selected: command.selected,
            state,
        });
    }
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &rows)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
