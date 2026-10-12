use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    gpu::{Gpu, Options},
    gpu_memory::BufferOptions,
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::Contract(
            "expected firmware directory and affine ELF",
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
    let program = gpu.load_program(&std::fs::read(&args[1])?)?;
    let input = gpu.allocate(128, BufferOptions::default())?;
    let output = gpu.allocate(128, BufferOptions::default())?;
    let bytes = (0..32)
        .flat_map(|i| (i as f32 - 16.).to_le_bytes())
        .collect::<Vec<_>>();
    gpu.upload(input, &bytes)?;
    gpu.execute(&program, &[output, input], &[], [1, 1, 1], [32, 1, 1])?;
    let result = gpu.download(output, 128)?;
    let result = json!({"output":result,"timeline":gpu.next_timeline});
    gpu.release_program(program)?;
    gpu.heap.asic.finish()?;
    Ok(result)
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
