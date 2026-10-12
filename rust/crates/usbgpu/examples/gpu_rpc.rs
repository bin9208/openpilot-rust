use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    Error,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Duration,
};
include!("support/amd_rpc.rs");
fn run() -> Result<Value, Error> {
    let mut args = std::env::args().skip(1);
    let mut source = Source(args.next().unwrap().into());
    let args = args.collect::<Vec<_>>();
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: !args.iter().any(|s| s == "--stock"),
    };
    let asic = Asic::boot(bus, &mut source, BootOptions::default())?;
    let mut gpu = openpilot_usbgpu::gpu::Gpu::new(
        asic,
        openpilot_usbgpu::gpu::Options {
            aql: Some(args.iter().any(|s| s == "--aql")),
            disable_copy: args.iter().any(|s| s == "--no-copy"),
            ..Default::default()
        },
    )?;
    let result = json!({"properties":gpu.properties,"system_next":gpu.system.next,"staging":gpu.staging.address(),
        "completion":gpu.completion.address(),"timeline":gpu.timeline.address(),"next_timeline":gpu.next_timeline});
    gpu.heap.asic.finish()?;
    Ok(result)
}
fn main() {
    match run() {
        Ok(value) => println!("{}", json!({"done":value})),
        Err(error) => {
            println!("{}", json!({"done":{"error":error.to_string()}}));
            std::process::exit(1);
        }
    }
}
