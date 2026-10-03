mod core_fixture;
use core_fixture::{Case, Driver, Io, Output, Tail};
use openpilot_card::core::Card;
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use std::{
    io::{self, Read},
    path::Path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    let root = Path::new(&path)
        .parent()
        .ok_or("missing output directory")?
        .join("params");
    let mut output = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let params = capnp::serialize::read_message(
            std::io::Cursor::new(&case.params),
            capnp::message::ReaderOptions::new(),
        )?;
        let mut message = capnp::message::Builder::new_default();
        message.set_root(params.get_root::<car_params::Reader>()?)?;
        let settings = Params::open(&root, &format!("case{index}"))?;
        let mut card = Card::new(message, settings, case.replay, case.has_controller)?;
        let first = case.steps.first().ok_or("missing steps")?.clone();
        let mut io = Io::new(first.clone(), Params::open(&root, &format!("case{index}"))?)?;
        let mut driver = Driver::new(first);
        let mut tail = Tail { calls: vec![] };
        let mut frames = Vec::new();
        for step in case.steps {
            for (key, value) in &step.settings {
                card.settings.put(key, value.as_bytes())?;
            }
            card.remaining = step.remaining;
            driver.step = step.clone();
            io.step = step;
            let error = card
                .step(&mut driver, &mut tail, &mut io)
                .err()
                .map(|error| error.to_string());
            io.output.calls.extend(std::mem::take(&mut driver.calls));
            io.output.calls.extend(std::mem::take(&mut tail.calls));
            io.output.initialized = card.initialized_previous;
            io.output.timeouts = card.can_timeouts;
            io.output.error = error;
            for key in [
                "ControlsReady",
                "OpenpilotEnabledToggle",
                "OnroadCycleRequested",
            ] {
                io.output
                    .settings
                    .insert(key.into(), card.settings.get_bool(key)?);
            }
            let stop = io.output.error.is_some();
            frames.push(std::mem::take(&mut io.output));
            if stop {
                break;
            }
        }
        output.push(frames);
    }
    std::fs::write(path, serde_json::to_vec::<Vec<Vec<Output>>>(&output)?)?;
    Ok(())
}
