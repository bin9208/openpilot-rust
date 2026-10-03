use openpilot_selfdrived::state::{EventType, Flags, State, StateMachine};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
struct Input {
    state: Option<State>,
    timer: Option<u32>,
    events: Vec<EventType>,
}

#[derive(Serialize)]
struct Output<'a> {
    #[serde(flatten)]
    machine: &'a StateMachine,
    #[serde(flatten)]
    flags: Flags,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut machine = StateMachine::default();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        if let Some(state) = input.state {
            machine.state = state;
        }
        if let Some(timer) = input.timer {
            machine.soft_disable_timer = timer;
        }
        let flags = machine.update(&input.events);
        serde_json::to_writer(
            &mut output,
            &Output {
                machine: &machine,
                flags,
            },
        )?;
        writeln!(output)?;
    }
    Ok(())
}
