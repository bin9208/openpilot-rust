use openpilot_usbgpu::{
    worker::{self, Info, Metadata, Runtime},
    Error,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    process::ExitCode,
};

struct OwnedRuntime {
    runs: u32,
    log: File,
    nonfinite: bool,
    fail_after_one: bool,
    pause_after_one: bool,
}
impl Runtime for OwnedRuntime {
    fn run(&mut self, packed: &[u8], info: &Info, output: &mut [u8]) -> Result<(), Error> {
        self.runs += 1;
        let mut inputs = serde_json::Map::new();
        for name in info.layout.keys() {
            inputs.insert(
                name.clone(),
                json!(format!("{:x}", Sha256::digest(info.input(packed, name)?))),
            );
        }
        writeln!(
            self.log,
            "{}",
            json!({"run":self.runs,"inputs":inputs,
            "desire":info.input(packed,"desire")?.chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>()})
        )?;
        self.log.flush()?;
        if self.fail_after_one && self.runs > 1 {
            return Err(Error::Protocol("owned USB disconnection".into()));
        }
        if self.pause_after_one && self.runs > 1 {
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        output.fill(0);
        output[..4].copy_from_slice(
            &(if self.nonfinite {
                f32::NAN
            } else {
                self.runs as f32
            })
            .to_le_bytes(),
        );
        Ok(())
    }
}
fn run() -> Result<(), Error> {
    if !worker::watch_parent() {
        return Ok(());
    }
    if let Some(error) = std::env::var_os("USBGPU_WORKER_TEST_LOAD_ERROR") {
        return Err(Error::Protocol(error.to_string_lossy().into_owned()));
    }
    if let Some(path) = std::env::var_os("USBGPU_WORKER_TEST_RETRY_FILE") {
        let path = std::path::Path::new(&path);
        let attempt = std::fs::read_to_string(path)
            .unwrap_or_default()
            .parse::<u32>()
            .unwrap_or(0)
            + 1;
        std::fs::write(path, attempt.to_string())?;
        if attempt == 1 {
            return Err(Error::Protocol("pcie link not up".into()));
        }
    }
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if ![4, 5].contains(&args.len()) {
        return Err(Error::Contract(
            "expected metadata, shared file, width, height, log",
        ));
    }
    let metadata: Metadata = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let info = Info::new(
        metadata,
        [
            args[2].parse().map_err(|_| Error::Contract("width"))?,
            args[3].parse().map_err(|_| Error::Contract("height"))?,
        ],
    )?;
    let file = OpenOptions::new().read(true).write(true).open(&args[1])?;
    let log = args
        .get(4)
        .cloned()
        .or_else(|| std::env::var("USBGPU_WORKER_TEST_LOG").ok())
        .ok_or(Error::Contract("missing fixture log"))?;
    let mut runtime = OwnedRuntime {
        runs: 0,
        log: File::create(log)?,
        nonfinite: std::env::var_os("USBGPU_WORKER_TEST_NONFINITE").is_some(),
        fail_after_one: std::env::var_os("USBGPU_WORKER_TEST_DISCONNECT").is_some(),
        pause_after_one: std::env::var_os("USBGPU_WORKER_TEST_TIMEOUT").is_some(),
    };
    worker::serve(
        &mut runtime,
        &file,
        &info,
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
    )
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = worker::report_error(&error, &mut io::stdout().lock());
            ExitCode::FAILURE
        }
    }
}
