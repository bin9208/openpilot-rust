use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    gpu::{Gpu, Options},
    model_graph::Graph,
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Duration,
};
include!("support/amd_rpc.rs");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    entrypoint: String,
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
}
fn run() -> Result<Value, Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(Error::Contract("expected firmware, bundle and sequence"));
    }
    let mut firmware = Source(PathBuf::from(&args[0]));
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: true,
    };
    let mut gpu = Gpu::new(
        Asic::boot(bus, &mut firmware, BootOptions::default())?,
        Options::default(),
    )?;
    let mut graph = Graph::load(&PathBuf::from(&args[1]), &mut gpu)?;
    let sequence = PathBuf::from(&args[2]);
    let directory = sequence
        .parent()
        .ok_or(Error::Contract("sequence directory missing"))?;
    let frames: Vec<Frame> = serde_json::from_slice(&std::fs::read(&sequence)?)?;
    for frame in &frames {
        for (name, path) in &frame.inputs {
            graph.write(&mut gpu, name, &std::fs::read(directory.join(path))?)?;
        }
        graph.run(&mut gpu, &frame.entrypoint)?;
        for (name, path) in &frame.outputs {
            std::fs::write(directory.join(path), graph.read(&mut gpu, name)?)?;
        }
    }
    gpu.heap.asic.finish()?;
    Ok(json!({"calls": frames.len()}))
}
fn main() {
    match run() {
        Ok(value) => println!("{}", json!({"done":value})),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
