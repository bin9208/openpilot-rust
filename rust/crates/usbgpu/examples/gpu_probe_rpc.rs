use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    gpu::{Gpu, Options},
    probe, Error,
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
    if args.len() != 3 {
        return Err(Error::Contract(
            "expected firmware, probe descriptor, output",
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
    let output = probe::run(&mut gpu, &std::fs::read(&args[1])?, 42)?;
    std::fs::write(&args[2], &output)?;
    Ok(json!({"bytes":output.len(),"sha256":format!("{:x}",Sha256::digest(&output))}))
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
