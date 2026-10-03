use crate::{hardware, Error};
use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub struct Options {
    pub devices: PathBuf,
    pub probe: PathBuf,
    pub timeout: Duration,
    pub require_clean_link: bool,
}
impl Options {
    pub fn for_runtime() -> Result<Self, Error> {
        Ok(Self {
            devices: hardware::SYSFS.into(),
            probe: std::env::current_exe()?.with_file_name("openpilot-usbgpu-probe"),
            timeout: Duration::from_secs(15),
            require_clean_link: true,
        })
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Err(error) = self.0.kill() {
            eprintln!("usbgpu probe cleanup: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("usbgpu probe reap: {error}");
        }
    }
}
enum Probe {
    Passed,
    Failed(String),
    TimedOut,
}
fn read_output(file: &mut File) -> Result<String, Error> {
    file.rewind()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}
fn probe(path: &Path, timeout: Duration, cancelled: &AtomicBool) -> Result<Probe, Error> {
    let mut output = tempfile::tempfile()?;
    let mut error = tempfile::tempfile()?;
    let mut child = OwnedChild(
        Command::new(path)
            .env("DEV", "USB+AMD:LLVM")
            .env("GMMU", "0")
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(error.try_clone()?)
            .spawn()?,
    );
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        if let Some(status) = child.0.try_wait()? {
            let output = read_output(&mut output)?;
            let error = read_output(&mut error)?;
            return Ok(if status.success() {
                Probe::Passed
            } else {
                Probe::Failed(format!("{output}\n{error}").to_lowercase())
            });
        }
        if started.elapsed() >= timeout {
            return Ok(Probe::TimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn retry_delay(cancelled: &AtomicBool) -> Result<(), Error> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(1) {
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
pub fn run(options: &Options, cancelled: &AtomicBool) -> Result<Option<String>, Error> {
    let before = hardware::devices(&options.devices)?;
    let device = hardware::single(&before);
    if let Some(error) = hardware::connection_diagnostic(device) {
        return Ok(Some(error));
    }
    let errors = device
        .ok_or(Error::Contract("checked USB device disappeared"))?
        .link_error_count;
    for attempt in 0..2 {
        match probe(&options.probe, options.timeout, cancelled)? {
            Probe::Passed => break,
            Probe::TimedOut => return Ok(Some("GPU check timed out".into())),
            Probe::Failed(output) => {
                let pcie = output.contains("pcie link not up") || output.contains("read(0xb450");
                if pcie && attempt == 0 {
                    retry_delay(cancelled)?;
                    continue;
                }
                return Ok(Some(
                    if pcie {
                        "12V / PCIe not ready"
                    } else {
                        "GPU incompatible"
                    }
                    .into(),
                ));
            }
        }
    }
    let after = hardware::devices(&options.devices)?;
    let Some(device) = hardware::single(&after) else {
        return Ok(Some("USB disconnected during GPU check".into()));
    };
    Ok(
        (options.require_clean_link && device.link_error_count > errors)
            .then(|| "USB link errors".into()),
    )
}
