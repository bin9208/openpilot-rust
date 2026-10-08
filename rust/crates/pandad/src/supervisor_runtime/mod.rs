pub mod child;
mod logs;
pub use logs::record;

use crate::{
    firmware::{
        client::{Client, Environment},
        native_environment::{stderr_logger, NativeEnvironment},
        native_spi::Pool,
    },
    supervisor::{Backend, Fault, Log, Supervisor},
};
use openpilot_hardware_control::{HardwareControl, LinuxPlatform, ProcessCommands};
use openpilot_hardware_info::{HardwareInfo, HardwarePaths, Tici};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
};
use openpilot_params::Params;
use openpilot_process_supervision::CapturedCommand;
use std::path::{Path, PathBuf};

pub struct Config {
    pub root: PathBuf,
    pub basedir: PathBuf,
    pub firmware: PathBuf,
    pub child: PathBuf,
    pub launcher: PathBuf,
    pub cycles: Option<u64>,
}

fn fault(error: impl std::fmt::Display) -> Fault {
    Fault::Other(error.to_string())
}
fn params_write(result: Result<(), openpilot_params::Error>) -> Result<(), Fault> {
    match result {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
        Err(error) => Err(fault(error)),
    }
}

pub struct NativeBackend {
    environment: NativeEnvironment,
    params: Params,
    hardware: HardwareControl,
    platform: LinuxPlatform,
    logger: Logger,
    children: child::Children,
    command: CapturedCommand,
}
impl NativeBackend {
    pub fn new(config: &Config) -> Result<Self, Fault> {
        let hardware = if config.root.join("TICI").is_file() {
            let board = Tici::with_paths(HardwarePaths::under(&config.root));
            HardwareControl::board(&board.get_device_type().map_err(fault)?)
        } else {
            HardwareControl::pc()
        };
        let params = if config.root == Path::new("/") {
            Params::for_runtime()
        } else {
            Params::open(&config.root.join("data/params"), "d")
        }
        .map_err(fault)?;
        Ok(Self {
            environment: NativeEnvironment::new(
                openpilot_panda_usb::Api::system().map_err(fault)?,
                Pool::at_path(
                    config
                        .root
                        .join("dev/spidev0.0")
                        .to_str()
                        .ok_or_else(|| fault("SPI path is not UTF-8"))?,
                ),
                config.firmware.clone(),
                stderr_logger()?,
            ),
            params,
            hardware,
            platform: LinuxPlatform::new(
                &config.root,
                ProcessCommands {
                    launcher: config.launcher.clone(),
                },
            ),
            logger: Factory::for_runtime().map_err(fault)?.logger(),
            children: child::Children::new().map_err(fault)?,
            command: CapturedCommand {
                launcher: config.launcher.clone(),
                cwd: config.basedir.join("openpilot/selfdrive/pandad"),
                argv: vec![config.child.clone().into_os_string()],
            },
        })
    }
    fn flush_interrupts(&mut self) -> Result<(), Fault> {
        for _ in 0..self.children.take_interrupts().map_err(fault)? {
            self.emit(Log::Info("Caught signal 2, exiting".into()))?;
        }
        Ok(())
    }
    fn emit(&mut self, entry: Log) -> Result<(), Fault> {
        self.logger
            .emit(log_site!(), record(entry).map_err(fault)?)
            .map_err(fault)?;
        Ok(())
    }
}

impl Backend for NativeBackend {
    type Device = Client<NativeEnvironment>;
    fn log(&mut self, entry: Log) -> Result<(), Fault> {
        self.flush_interrupts()?;
        self.emit(entry)
    }
    fn remove_signatures(&mut self) -> Result<(), Fault> {
        params_write(self.params.remove("PandaSignatures"))
    }
    fn reset_internal(&mut self) -> Result<(), Fault> {
        self.hardware
            .reset_internal_panda(&mut self.platform)
            .map_err(fault)
    }
    fn recover_internal(&mut self) -> Result<(), Fault> {
        self.hardware
            .recover_internal_panda(&mut self.platform)
            .map_err(fault)
    }
    fn sleep(&mut self, seconds: u64) -> Result<(), Fault> {
        self.environment.sleep(seconds as f64)
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        self.environment.dfu_list()
    }
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault> {
        self.environment.dfu_recover(serial)
    }
    fn panda_list(&mut self) -> Result<Vec<String>, Fault> {
        self.environment.panda_list()
    }
    fn connect(&mut self, serial: &str) -> Result<Self::Device, Fault> {
        Client::open(self.environment.clone(), serial.into())
    }
    fn expected_signature(&mut self, panda: &mut Self::Device) -> Result<Vec<u8>, Fault> {
        panda.expected_signature()
    }
    fn has_internal(&mut self) -> Result<bool, Fault> {
        Ok(self.hardware.has_internal_panda())
    }
    fn put_signatures(&mut self, signatures: &[u8]) -> Result<(), Fault> {
        params_write(self.params.put("PandaSignatures", signatures))
    }
    fn put_bool(&mut self, key: &str) -> Result<(), Fault> {
        params_write(self.params.put_bool(key, true))
    }
    fn run_child(&mut self, serials: &[String]) -> Result<(), Fault> {
        self.flush_interrupts()?;
        self.command.argv.truncate(1);
        self.command.argv.extend(serials.iter().map(Into::into));
        let logger = &mut self.logger;
        self.children
            .run(&self.command, || {
                logger.emit(
                    log_site!(),
                    record(Log::Info("Caught signal 2, exiting".into()))?,
                )?;
                Ok(())
            })
            .map_err(fault)?;
        self.flush_interrupts()
    }
}

pub fn run(config: Config) -> Result<(), Fault> {
    let mut backend = NativeBackend::new(&config)?;
    let mut supervisor = Supervisor::default();
    let mut cycles = 0;
    while !backend.children.requested() && config.cycles.is_none_or(|limit| cycles < limit) {
        let result = supervisor.step(&mut backend);
        backend.flush_interrupts()?;
        result?;
        cycles += 1;
    }
    backend.flush_interrupts()?;
    backend.logger.close();
    Ok(())
}
