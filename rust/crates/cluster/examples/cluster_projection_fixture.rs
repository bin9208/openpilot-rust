use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::live::Projection;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Step {
    messages: Vec<PathBuf>,
    now: f64,
    onroad: Option<bool>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input] = args.as_slice() else {
        return Err("cluster_projection_fixture INPUT".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut projection = Projection::default();
    let mut car_bytes = Vec::new();
    let mut output = Vec::new();
    for step in steps {
        for path in step.messages {
            let bytes = std::fs::read(path)?;
            let message = serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
            let root = message.get_root::<event::Reader<'_>>()?;
            projection.apply(root)?;
            if matches!(root.which()?, event::CarState(_)) {
                car_bytes = bytes;
            }
        }
        if let Some(onroad) = step.onroad {
            projection.set_onroad(onroad);
        }
        let message = serialize::read_message(&mut &car_bytes[..], ReaderOptions::new())?;
        let event::CarState(car) = message.get_root::<event::Reader<'_>>()?.which()? else {
            return Err("fixture carState missing".into());
        };
        output.push(projection.car_frame(car?.into(), step.now));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
