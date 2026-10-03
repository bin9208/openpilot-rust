use crate::{
    can_io::BulkTransport,
    device::{Control, Transport},
};
use openpilot_panda_spi::{
    device::{Device as SpiDevice, Shared},
    linux_io::LinuxIo,
};
use openpilot_panda_spi_linux::NativeKernel;
use openpilot_panda_usb::{Api, Enumerator, Session};
use std::sync::{Arc, Mutex};

pub type Logger = Arc<dyn Fn(u8, &str) + Send + Sync>;
type Spi = SpiDevice<LinuxIo<NativeKernel>>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Usb(#[from] openpilot_panda_usb::Error),
    #[error(transparent)]
    Spi(#[from] openpilot_panda_spi::device::Error),
    #[error(transparent)]
    SpiSetup(#[from] openpilot_panda_spi::linux_io::SetupError),
    #[error("SPI serial is not UTF-8: {0}")]
    Serial(#[from] std::str::Utf8Error),
    #[error("Panda enumeration mutex poisoned")]
    EnumerationPoisoned,
}

pub enum Handle {
    Usb(Session),
    Spi(Box<Spi>),
}

impl Handle {
    pub fn serial(&self) -> &[u8] {
        match self {
            Self::Usb(device) => device.serial(),
            Self::Spi(device) => device.serial().as_bytes(),
        }
    }
    pub fn connected(&self) -> bool {
        match self {
            Self::Usb(device) => device.connected(),
            Self::Spi(device) => device.connected(),
        }
    }
}

impl Transport for Handle {
    type Error = Error;
    fn control_read(&self, command: Control, output: &mut [u8]) -> Result<i32, Error> {
        Ok(match self {
            Self::Usb(device) => device.control_read(
                command.request,
                command.value,
                command.index,
                output,
                command.timeout_ms,
            )?,
            Self::Spi(device) => device.control_read(
                command.request,
                command.value,
                command.index,
                output,
                command.timeout_ms,
            )?,
        })
    }
    fn control_write(&self, command: Control) -> Result<i32, Error> {
        Ok(match self {
            Self::Usb(device) => device.control_write(
                command.request,
                command.value,
                command.index,
                command.timeout_ms,
            )?,
            Self::Spi(device) => device.control_write(
                command.request,
                command.value,
                command.index,
                command.timeout_ms,
            )?,
        })
    }
}

impl BulkTransport for Handle {
    fn bulk_read(&self, endpoint: u8, output: &mut [u8], timeout_ms: u32) -> Result<i32, Error> {
        Ok(match self {
            Self::Usb(device) => device.bulk_read(endpoint, output, timeout_ms)?,
            Self::Spi(device) => device.bulk_read(endpoint, output, timeout_ms)?,
        })
    }
    fn bulk_write(&self, endpoint: u8, input: &[u8], timeout_ms: u32) -> Result<i32, Error> {
        Ok(match self {
            Self::Usb(device) => device.bulk_write(endpoint, &mut input.to_vec(), timeout_ms)?,
            Self::Spi(device) => device.bulk_write(endpoint, input, timeout_ms)?,
        })
    }
    fn comms_healthy(&self) -> bool {
        match self {
            Self::Usb(device) => device.healthy(),
            Self::Spi(device) => device.healthy(),
        }
    }
}

pub struct Factory {
    api: Arc<Api>,
    enumeration: Mutex<Option<Enumerator>>,
    usb_log: openpilot_panda_usb::Logger,
    log: Logger,
    spi: Arc<Shared>,
}

impl Factory {
    pub fn system(log: Logger, usb_log: openpilot_panda_usb::Logger) -> Result<Self, Error> {
        Ok(Self::with_api(Api::system()?, log, usb_log))
    }

    pub fn with_api(api: Arc<Api>, log: Logger, usb_log: openpilot_panda_usb::Logger) -> Self {
        Self {
            api,
            enumeration: Mutex::new(None),
            usb_log,
            log,
            spi: Shared::process(),
        }
    }

    pub fn spi_diagnostics(&self) -> Arc<Shared> {
        Arc::clone(&self.spi)
    }

    fn spi(&self, serial: &str) -> Result<Spi, Error> {
        let log = Arc::clone(&self.log);
        let io = LinuxIo::open(NativeKernel::system(move |level, text| log(level, &text)))?;
        Ok(SpiDevice::connect(io, Arc::clone(&self.spi), serial)?)
    }

    pub fn connect(&self, serial: &[u8]) -> Result<Handle, Error> {
        match Session::open(Arc::clone(&self.api), serial, Arc::clone(&self.usb_log)) {
            Ok(device) => {
                (self.log)(
                    30,
                    &format!("connected to {} over USB", String::from_utf8_lossy(serial)),
                );
                Ok(Handle::Usb(device))
            }
            Err(_) => {
                let device = self.spi(std::str::from_utf8(serial)?)?;
                (self.log)(
                    30,
                    &format!("connected to {} over SPI", String::from_utf8_lossy(serial)),
                );
                Ok(Handle::Spi(Box::new(device)))
            }
        }
    }

    pub fn list(&self, usb_only: bool) -> Result<Vec<Vec<u8>>, Error> {
        let mut enumeration = self
            .enumeration
            .lock()
            .map_err(|_| Error::EnumerationPoisoned)?;
        let mut serials = match enumeration.as_ref() {
            Some(enumerator) => enumerator.list()?,
            None => {
                let enumerator = Enumerator::new(Arc::clone(&self.api), Arc::clone(&self.usb_log))?;
                let result = enumerator.list();
                *enumeration = Some(enumerator);
                result?
            }
        };
        drop(enumeration);
        if !usb_only {
            if let Ok(device) = self.spi("") {
                let serial = device.serial().as_bytes();
                if !serials.iter().any(|existing| existing == serial) {
                    serials.push(serial.to_vec());
                }
            }
        }
        Ok(serials)
    }
}
