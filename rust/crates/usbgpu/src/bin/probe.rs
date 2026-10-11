use openpilot_usbgpu::{
    gpu::{Gpu, Options},
    native_runtime::{self, FirmwareDirectory},
    probe, Error,
};
use std::{fs::File, io::Read, process::ExitCode};

fn run() -> Result<(), Error> {
    let directory = std::env::current_exe()?.with_file_name("usbgpu-assets");
    let mut source = FirmwareDirectory(directory.join("firmware"));
    let mut seed = [0; 4];
    File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let mut gpu = Gpu::new(native_runtime::open(&mut source)?, Options::default())?;
    let descriptor = directory.join(format!("probe-{}.json", probe::architecture(&gpu)));
    probe::run(
        &mut gpu,
        &std::fs::read(descriptor)?,
        u32::from_le_bytes(seed),
    )?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
