use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    gpu::{Gpu, Options},
    gpu_memory::BufferOptions,
    warp::Warp,
    Error,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Duration,
};
include!("support/amd_rpc.rs");
fn run() -> Result<Value, Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(Error::Contract(
            "expected firmware, warp descriptor, frames, transforms, output",
        ));
    }
    let mut source = Source(PathBuf::from(&args[0]));
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: true,
    };
    let mut gpu = Gpu::new(
        Asic::boot(bus, &mut source, BootOptions::default())?,
        Options::default(),
    )?;
    let output = gpu.allocate(393216, BufferOptions::default())?;
    let warp = Warp::load(&std::fs::read(&args[1])?, &mut gpu, output)?;
    warp.run(
        &mut gpu,
        &std::fs::read(&args[2])?,
        &std::fs::read(&args[3])?,
    )?;
    let bytes = gpu.download(output, 393216)?;
    std::fs::write(&args[4], &bytes)?;
    Ok(json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}))
}
fn main() {
    match run() {
        Ok(done) => println!("{}", json!({"done":done})),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
