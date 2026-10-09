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
    let remaining = args.collect::<Vec<_>>();
    let power = remaining.iter().find_map(|value| value.parse().ok());
    let queues = remaining.iter().any(|value| value == "--queues");
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: true,
    };
    let mut asic = Asic::boot(
        bus,
        &mut source,
        BootOptions {
            power_limit: power,
            ..BootOptions::default()
        },
    )?;
    if queues {
        let ring = asic.memory.allocate(&mut asic.hw, 8192, 4096, true, true)?;
        let gart = asic.memory.allocate(&mut asic.hw, 4096, 4096, true, true)?;
        let eop = asic
            .memory
            .allocate(&mut asic.hw, 4096, 4096, false, false)?;
        asic.setup_compute_ring(openpilot_usbgpu::asic_gfx::ComputeRing {
            address: ring.address,
            size: ring.size,
            read_pointer: gart.address + 128,
            write_pointer: gart.address + 56,
            eop: eop.address,
            eop_size: eop.size,
            index: 0,
            aql: false,
        })?;
        let ring = asic.memory.allocate(&mut asic.hw, 4096, 4096, true, true)?;
        let gart = asic.memory.allocate(&mut asic.hw, 4096, 4096, true, true)?;
        asic.setup_copy_ring(openpilot_usbgpu::asic_sdma::CopyRing {
            address: ring.address,
            size: ring.size,
            read_pointer: gart.address + 128,
            write_pointer: gart.address + 56,
            index: 0,
        })?;
    }
    let mut result = json!({"partial_boot":asic.partial_boot,"error_state":asic.error_state,"vram_size":asic.hw.vram_size});
    asic.finish()?;
    result["error_state"] = json!(asic.error_state);
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
