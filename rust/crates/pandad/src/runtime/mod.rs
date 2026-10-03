mod can_loop;
mod effects;
mod logs;
mod main_loop;
mod platform;
mod spi_diagnostics;

use crate::{
    can, can_io,
    device::Device,
    native_transport::{self, Factory, Handle},
    safety,
};
use logs::Logs;
use openpilot_logging::record::Level;
use platform::Hardware;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

type Panda = Device<Handle>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Native(#[from] native_transport::Error),
    #[error(transparent)]
    Device(#[from] crate::device::Error<native_transport::Error>),
    #[error(transparent)]
    CanReceive(#[from] can_io::ReceiveError<native_transport::Error>),
    #[error(transparent)]
    CanSend(#[from] can::EncodeError<native_transport::Error>),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Message(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Publisher(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    Peripheral(#[from] crate::peripheral::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("Panda {0} thread panicked")]
    WorkerPanic(&'static str),
    #[error("{0}")]
    Contract(&'static str),
}

impl From<safety::Error<Error>> for Error {
    fn from(error: safety::Error<Error>) -> Self {
        match error {
            safety::Error::Effect(error) => error,
            safety::Error::Cereal(error) => Self::Cereal(error),
        }
    }
}

struct Shared {
    pandas: Vec<Arc<Panda>>,
    factory: Factory,
    hardware: Hardware,
    logs: Logs,
    stop: Arc<AtomicBool>,
    onroad: AtomicBool,
    no_fan_control: bool,
    spoofing_started: bool,
    fake_send: bool,
    maxout: bool,
}

impl Shared {
    fn connected(&self) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            return false;
        }
        for panda in &self.pandas {
            if !panda.transport().connected() {
                self.stop.store(true, Ordering::Relaxed);
                return false;
            }
        }
        true
    }
}

struct StopOnDrop<'a>(&'a AtomicBool);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn worker(
    shared: &Shared,
    function: impl FnOnce(&Shared) -> Result<(), Error>,
) -> Result<(), Error> {
    let _stop = StopOnDrop(&shared.stop);
    function(shared)
}

fn run_connected(shared: &Shared, frames: Option<u64>) -> Result<(), Error> {
    std::thread::scope(|scope| {
        let send = scope.spawn(|| worker(shared, can_loop::send));
        let receive = scope.spawn(|| worker(shared, can_loop::receive));
        let diagnostics = scope.spawn(|| worker(shared, spi_diagnostics::run));
        let result = worker(shared, |shared| main_loop::run(shared, frames));
        let receive = receive
            .join()
            .map_err(|_| Error::WorkerPanic("CAN receive"));
        let send = send.join().map_err(|_| Error::WorkerPanic("CAN send"));
        let diagnostics = diagnostics
            .join()
            .map_err(|_| Error::WorkerPanic("SPI diagnostics"));
        result?;
        receive??;
        send??;
        diagnostics??;
        Ok(())
    })
}

pub fn run(mut serials: Vec<Vec<u8>>, frames: Option<u64>) -> Result<(), Error> {
    let hardware = Hardware::detect()?;
    let logs = Logs::new(&hardware.name)?;
    logs.write(Level::Warning, "starting pandad");
    let result = (|| {
        if hardware.board {
            platform::realtime(54)?;
            platform::core_three()?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
        let factory = Factory::system(logs.transport(), logs.usb())?;
        if serials.is_empty() {
            serials = factory.list(false)?;
            if serials.is_empty() {
                logs.write(Level::Warning, "no pandas found, exiting");
                return Ok(());
            }
        }
        logs.write(
            Level::Warning,
            format!(
                "connecting to pandas: {}",
                serials
                    .iter()
                    .map(|serial| String::from_utf8_lossy(serial))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        let loopback = std::env::var_os("BOARDD_LOOPBACK").is_some();
        let skip_firmware = std::env::var_os("BOARDD_SKIP_FW_CHECK").is_some();
        let mut pandas = Vec::with_capacity(serials.len());
        for (index, serial) in serials.iter().enumerate() {
            while !stop.load(Ordering::Relaxed) {
                let panda = match factory
                    .connect(serial)
                    .map_err(Error::from)
                    .and_then(|handle| {
                        Device::connect(handle, (index as u32).wrapping_mul(4)).map_err(Error::from)
                    }) {
                    Ok(panda) => panda,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                };
                panda.configure(loopback, skip_firmware, |path| {
                    std::fs::read(path).unwrap_or_default()
                })?;
                pandas.push(Arc::new(panda));
                break;
            }
        }
        if !stop.load(Ordering::Relaxed) {
            logs.write(Level::Warning, "connected to all pandas");
            let shared = Shared {
                pandas,
                factory,
                hardware,
                logs: logs.clone(),
                stop,
                onroad: AtomicBool::new(false),
                no_fan_control: std::env::var_os("NO_FAN_CONTROL").is_some(),
                spoofing_started: std::env::var_os("STARTED").is_some(),
                fake_send: std::env::var_os("FAKESEND").is_some(),
                maxout: std::env::var_os("PANDAD_MAXOUT").is_some(),
            };
            run_connected(&shared, frames)?;
        }
        Ok(())
    })();
    logs.close()?;
    result
}
