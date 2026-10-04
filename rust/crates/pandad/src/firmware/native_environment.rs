use super::{
    client::{Connection, Environment, Handle, Level},
    dfu_serial,
    dfu_spi::DfuSpi,
    dfu_usb::DfuUsb,
    native_spi::{NativeIo, Pool},
    spi::{Error as SpiError, PandaSpi},
    usb::{self, UsbHandle},
    Mcu, Request, Transport,
};
use crate::supervisor::Fault;
use openpilot_logging::record::Level as LogLevel;
use openpilot_panda_usb::Api;
use std::{
    cell::RefCell,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

pub type LogSink = dyn FnMut(LogLevel, String, Option<String>) -> Result<(), Fault>;
type Log = Rc<RefCell<LogSink>>;

pub enum Device {
    Usb(UsbHandle),
    Spi(PandaSpi<NativeIo>),
}

impl Transport for Device {
    type Error = Fault;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Fault> {
        match self {
            Self::Usb(handle) => handle.control_read(request, length),
            Self::Spi(handle) => handle.control_read(request, length),
        }
    }
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Fault> {
        match self {
            Self::Usb(handle) => handle.control_write(request, data),
            Self::Spi(handle) => handle.control_write(request, data),
        }
    }
    fn bulk_write(&mut self, endpoint: u8, data: &[u8], timeout: u32) -> Result<(), Fault> {
        match self {
            Self::Usb(handle) => handle.bulk_write(endpoint, data, timeout),
            Self::Spi(handle) => handle.bulk_write(endpoint, data, timeout),
        }
    }
}
impl Handle for Device {
    fn close(&mut self) -> Result<(), Fault> {
        match self {
            Self::Usb(handle) => handle.close(),
            Self::Spi(handle) => handle.close(),
        }
    }
}

#[derive(Clone)]
pub struct NativeEnvironment {
    api: Arc<Api>,
    spi: Pool,
    firmware: PathBuf,
    log: Log,
    kernel_spi: bool,
}

fn fault(error: impl std::fmt::Display) -> Fault {
    Fault::Other(error.to_string())
}

fn usb_log(log: &Log, entry: usb::Log) -> Result<(), Fault> {
    let (level, text, exception) = match entry {
        usb::Log::Opening { serial, product } => (
            LogLevel::Debug,
            format!("opening device {serial} {product:#x}"),
            None,
        ),
        usb::Log::InvalidSerial(serial) => (
            LogLevel::Warning,
            format!("found device with panda descriptors but invalid serial: {serial}"),
            None,
        ),
        usb::Log::Exception(text, error) => (LogLevel::Error, text.into(), Some(error.to_string())),
    };
    log.borrow_mut()(level, text, exception)
}

pub fn stderr_logger() -> Result<Box<LogSink>, Fault> {
    let configured = std::env::var("LOGLEVEL")
        .unwrap_or_else(|_| "INFO".into())
        .to_uppercase();
    let threshold = match configured.as_str() {
        "NOTSET" => LogLevel::Warning,
        "DEBUG" => LogLevel::Debug,
        "INFO" => LogLevel::Info,
        "WARN" | "WARNING" => LogLevel::Warning,
        "ERROR" => LogLevel::Error,
        "FATAL" | "CRITICAL" => LogLevel::Critical,
        _ => return Err(fault(format!("Unknown level: '{configured}'"))),
    };
    Ok(Box::new(move |level, text, exception| {
        if level >= threshold {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "{text}");
            if let Some(exception) = exception {
                let _ = writeln!(stderr, "{exception}");
            }
        }
        Ok(())
    }))
}

impl NativeEnvironment {
    pub fn new(
        api: Arc<Api>,
        spi: Pool,
        firmware: PathBuf,
        log: impl FnMut(LogLevel, String, Option<String>) -> Result<(), Fault> + 'static,
    ) -> Self {
        Self {
            api,
            spi,
            firmware,
            log: Rc::new(RefCell::new(log)),
            kernel_spi: std::env::var_os("KERN").is_some(),
        }
    }

    fn io(&self, speed: u32) -> Result<Option<NativeIo>, SpiError> {
        let log = self.log.clone();
        self.spi.open(speed, move |text, exception| {
            log.borrow_mut()(LogLevel::Debug, text, exception.map(ToString::to_string))
                .map_err(|error| SpiError::Io(error.to_string()))
        })
    }

    fn spi_panda(
        &self,
        serial: Option<&str>,
        ignore_version: bool,
    ) -> Result<Option<Connection<Device>>, Fault> {
        let Some(io) = self.io(50_000_000).map_err(SpiError::into_fault)? else {
            return Ok(None);
        };
        let mut handle = PandaSpi::new(io, self.kernel_spi);
        let Some(identity) = handle
            .identify(serial, ignore_version)
            .map_err(SpiError::into_fault)?
        else {
            return Ok(None);
        };
        Ok(Some(Connection {
            handle: Device::Spi(handle),
            serial: identity.serial,
            bootstub: identity.bootstub,
            bcd: None,
            spi: true,
        }))
    }

    pub fn panda_list(&mut self) -> Result<Vec<String>, Fault> {
        let log = self.log.clone();
        let mut serials = usb::list(self.api.clone(), |entry| usb_log(&log, entry))?;
        if let Some(connection) = self.spi_panda(None, true)? {
            serials.push(connection.serial);
        }
        let mut seen = std::collections::HashSet::new();
        serials.retain(|serial| seen.insert(serial.clone()));
        Ok(serials)
    }

    fn dfu_spi(&self, serial: Option<&str>) -> Result<Option<DfuSpi<NativeIo>>, Fault> {
        let operation: Result<Option<DfuSpi<NativeIo>>, SpiError> = (|| {
            let Some(io) = self.io(1_000_000)? else {
                return Ok(None);
            };
            let mut handle = DfuSpi::probe(io)?;
            let uid = handle.uid()?;
            let candidate =
                dfu_serial(&uid, handle.mcu).map_err(|error| SpiError::Io(error.to_string()))?;
            if serial.is_some_and(|serial| candidate.as_deref() != Some(serial)) {
                return Ok(None);
            }
            Ok(Some(handle))
        })();
        match operation {
            Err(error) if error.retryable() => Ok(None),
            Err(error) => Err(error.into_fault()),
            Ok(value) => Ok(value),
        }
    }

    fn recover_usb(&mut self, handle: &mut UsbHandle, mcu: Mcu) -> Result<(), Fault> {
        let code = self.file_read(&self.firmware.join(mcu.bootstub_filename()), None)?;
        DfuUsb {
            transport: handle,
            mcu,
        }
        .recover_with_progress(&code, |text| {
            writeln!(std::io::stdout(), "{text}").map_err(fault)
        })
        .map_err(|error| match error {
            super::Error::Transport(error) => error,
            error => fault(error),
        })
    }
}

impl Environment for NativeEnvironment {
    type Device = Device;
    fn usb_connect(
        &mut self,
        serial: &str,
        claim: bool,
        no_error: bool,
    ) -> Result<Option<Connection<Device>>, Fault> {
        let log = self.log.clone();
        usb::connect(self.api.clone(), serial, claim, no_error, |entry| {
            usb_log(&log, entry)
        })
        .map(|value| {
            value.map(|connection| Connection {
                handle: Device::Usb(connection.handle),
                serial: connection.serial,
                bootstub: connection.bootstub,
                bcd: connection.bcd,
                spi: connection.spi,
            })
        })
    }
    fn spi_connect(&mut self, serial: &str) -> Result<Option<Connection<Device>>, Fault> {
        self.spi_panda(Some(serial), false)
    }
    fn firmware_dir(&self) -> &Path {
        &self.firmware
    }
    fn file_exists(&mut self, path: &Path) -> Result<bool, Fault> {
        Ok(path.is_file())
    }
    fn file_read(&mut self, path: &Path, tail: Option<usize>) -> Result<Vec<u8>, Fault> {
        let mut file = File::open(path).map_err(fault)?;
        if let Some(tail) = tail {
            let offset = i64::try_from(tail).map_err(fault)?;
            file.seek(SeekFrom::End(-offset)).map_err(fault)?;
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).map_err(fault)?;
        Ok(contents)
    }
    fn log(&mut self, level: Level, text: String) -> Result<(), Fault> {
        self.log.borrow_mut()(
            match level {
                Level::Debug => LogLevel::Debug,
                Level::Info => LogLevel::Info,
            },
            text,
            None,
        )
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Fault> {
        std::thread::sleep(Duration::try_from_secs_f64(seconds).map_err(fault)?);
        Ok(())
    }
    fn monotonic(&mut self) -> Result<f64, Fault> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(if now.tv_nsec == 0 {
            now.tv_sec as f64
        } else {
            (i128::from(now.tv_sec) * 1_000_000_000 + i128::from(now.tv_nsec)) as f64
                / 1_000_000_000.0
        })
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        let mut serials = usb::dfu_list(self.api.clone());
        if let Some(mut handle) = self.dfu_spi(None)? {
            match handle.uid() {
                Ok(uid) => serials.push(dfu_serial(&uid, handle.mcu).map_err(fault)?),
                Err(error) if error.retryable() => (),
                Err(error) => return Err(error.into_fault()),
            }
        }
        let mut seen = std::collections::HashSet::new();
        serials.retain(|serial| seen.insert(serial.clone()));
        Ok(serials)
    }
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault> {
        if let Some((mut handle, mcu)) = usb::dfu_connect(self.api.clone(), serial)? {
            return self.recover_usb(&mut handle, mcu);
        }
        let Some(mut handle) = self.dfu_spi(serial)? else {
            return Err(fault(format!(
                "failed to open DFU device {}",
                serial.unwrap_or("None")
            )));
        };
        let code = self.file_read(&self.firmware.join(handle.mcu.bootstub_filename()), None)?;
        handle.recover(&code).map_err(SpiError::into_fault)
    }
}
