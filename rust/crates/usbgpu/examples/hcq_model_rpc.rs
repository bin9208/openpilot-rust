use openpilot_usbgpu::{
    amd_bus::Bus,
    asic::{Asic, BootOptions},
    firmware::FirmwareSource,
    hcq_gpu::{HcqBus, HcqGpu},
    hcq_model::Model,
    hcq_vm::{Function, Memory},
    runtime_bus::CpuLocation,
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    Error,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
include!("support/amd_rpc.rs");
struct Raw<'a>(&'a mut Rpc);
impl Transport for Raw<'_> {
    fn describe(&self) -> Result<Description, Error> {
        Err(Error::Contract("not used by HCQ fixture"))
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Err(Error::Contract("not used by HCQ fixture"))
    }
    fn streams(&mut self, _: &[u8], _: u32) -> Result<i32, Error> {
        Err(Error::Contract("not used by HCQ fixture"))
    }
    fn batch(&mut self, _: &mut [Transfer]) -> Result<(), Error> {
        Err(Error::Contract("not used by HCQ fixture"))
    }
    fn error_text(&self, code: i32) -> String {
        format!("owned libusb failure {code}")
    }
    fn control(&mut self, c: Control, bytes: &mut [u8]) -> Result<i32, Error> {
        let result=self.0.call(json!({"op":"usb_control","type":c.kind,"request":c.request,"value":c.value,"index":c.index,"timeout":c.timeout_ms,"data":bytes}))?;
        if c.kind & 0x80 != 0 {
            let data: Vec<u8> = serde_json::from_value(result["data"].clone())?;
            bytes.copy_from_slice(&data);
        }
        Ok(serde_json::from_value(result["code"].clone())?)
    }
    fn bulk(&mut self, endpoint: u8, bytes: &mut [u8], timeout: u32) -> Result<BulkResult, Error> {
        let result = self
            .0
            .call(json!({"op":"usb_bulk","endpoint":endpoint,"timeout":timeout,"data":bytes}))?;
        if endpoint & 0x80 != 0 {
            let data: Vec<u8> = serde_json::from_value(result["data"].clone())?;
            bytes.copy_from_slice(&data);
        }
        Ok(BulkResult {
            code: serde_json::from_value(result["code"].clone())?,
            actual: serde_json::from_value(result["actual"].clone())?,
        })
    }
}
impl HcqBus for Rpc {
    fn pci_address(&self, location: CpuLocation) -> Result<u64, Error> {
        match location {
            CpuLocation::Vram(offset) => Ok(0x1_0000_0000 + offset),
            CpuLocation::Controller(_) => Err(Error::Contract("controller pointer in HCQ fixture")),
        }
    }
    fn doorbell_address(&self, index: u32) -> Result<u64, Error> {
        Ok(0xcafe0000 + u64::from(index) * 8)
    }
    fn bus_lock(&self) -> Option<openpilot_usbgpu::bus_lock::BusLock> {
        None
    }
    fn transfer(
        &mut self,
        function: Function,
        args: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        openpilot_usbgpu::hcq_gpu::transfer(&mut Raw(self), function, args, memory)
    }
}
fn run() -> Result<Value, Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(Error::Contract(
            "expected firmware, descriptor, model and output path",
        ));
    }
    if (std::env::var_os("USBGPU_INSPECT_KERNELS").is_some()
        || std::env::var_os("USBGPU_SOURCE_ORACLE").is_some())
        && !cfg!(feature = "fixture-inspection")
    {
        return Err(Error::Contract("fixture-inspection feature is required"));
    }
    let mut firmware = Source(PathBuf::from(&args[0]));
    let bus = Rpc {
        input: io::BufReader::new(io::stdin()),
        clock: Cell::new(0),
        custom: true,
    };
    let gpu = HcqGpu::new(
        Asic::boot(
            bus,
            &mut firmware,
            BootOptions {
                disable_gmmu: true,
                ..BootOptions::default()
            },
        )?,
        Arc::new(AtomicBool::new(false)),
    )?;
    eprintln!("ASIC ready; loading pinned model");
    let mut model = Model::load(
        &std::fs::read(&args[1])?,
        std::path::Path::new(&args[2]),
        gpu,
    )?;
    eprintln!("model linked; uploading frame inputs");
    model.write_input(
        "new_img",
        &(0..393216).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
    )?;
    model.write_input(
        "desire",
        &[1f32, 0., 0., 0., 0., 0., 0., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )?;
    model.write_input(
        "traffic_convention",
        &[1f32, 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )?;
    model.write_input(
        "action_t",
        &[0.05f32, 0.05]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )?;
    eprintln!("dispatching pinned model");
    #[cfg(feature = "fixture-inspection")]
    if std::env::var_os("USBGPU_INSPECT_KERNELS").is_some() {
        println!(
            "{}",
            json!({"op":"inspect_bindings", "snapshot":model.fixture_snapshot()?})
        );
        io::stdout().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if serde_json::from_str::<Value>(&response)?["value"] != true {
            return Err(Error::Contract("fixture inspection handshake failed"));
        }
    }
    model.run()?;
    let mut output = vec![0; model.output_size("outputs")?];
    model.read_output("outputs", &mut output)?;
    std::fs::write(&args[3], &output)?;
    #[cfg(feature = "fixture-inspection")]
    if std::env::var_os("USBGPU_SOURCE_ORACLE").is_some() {
        println!(
            "{}",
            json!({"op":"source_oracle", "snapshot":model.fixture_snapshot()?})
        );
        io::stdout().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        let value: Value = serde_json::from_str(&response)?;
        if value["value"]["passed"] != true {
            return Err(Error::Contract("source model oracle failed"));
        }
    }
    let finite = output
        .chunks_exact(4)
        .filter(|value| f32::from_le_bytes((*value).try_into().unwrap()).is_finite())
        .count();
    Ok(
        json!({"output_bytes":output.len(),"finite_f32":finite,"sha256":format!("{:x}",Sha256::digest(&output))}),
    )
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
