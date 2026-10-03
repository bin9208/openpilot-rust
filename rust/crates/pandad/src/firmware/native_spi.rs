use super::spi::{Error, Io, Mode};
use openpilot_panda_spi::linux_io::OptionKind;
use openpilot_panda_spi_linux::FirmwareKernel;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

struct Device {
    kernel: FirmwareKernel,
    speed: u32,
    bits: u8,
}
#[derive(Clone)]
pub struct Pool {
    path: Rc<str>,
    devices: Rc<RefCell<BTreeMap<u32, Rc<RefCell<Device>>>>>,
}
type Logger = Box<dyn FnMut(String, Option<&Error>) -> Result<(), Error>>;
pub struct NativeIo {
    device: Rc<RefCell<Device>>,
    logger: Logger,
}
fn failure(error: std::io::Error) -> Error {
    Error::Io(error.to_string())
}

impl Pool {
    pub fn system() -> Self {
        Self::at_path("/dev/spidev0.0")
    }
    pub fn at_path(path: &str) -> Self {
        Self {
            path: path.into(),
            devices: Rc::default(),
        }
    }
    pub fn open(
        &self,
        speed: u32,
        logger: impl FnMut(String, Option<&Error>) -> Result<(), Error> + 'static,
    ) -> Result<Option<NativeIo>, Error> {
        if speed > 50_000_000 {
            return Err(Error::Invalid("SPI speed exceeds 50 MHz"));
        }
        let kernel = FirmwareKernel::at_path(&self.path);
        if !kernel.exists() {
            return Ok(None);
        }
        let cached = self.devices.borrow().get(&speed).cloned();
        let device = if let Some(cached) = cached {
            cached
        } else {
            let device = Rc::new(RefCell::new(Device {
                kernel,
                speed: 0,
                bits: 0,
            }));
            self.devices.borrow_mut().insert(speed, device.clone());
            {
                let mut state = device.borrow_mut();
                state.kernel.open().map_err(failure)?;
                state
                    .kernel
                    .read_option(OptionKind::Mode)
                    .map_err(failure)?;
                state.bits = state
                    .kernel
                    .read_option(OptionKind::Bits)
                    .map_err(failure)? as u8;
                state.speed = state
                    .kernel
                    .read_option(OptionKind::Speed)
                    .map_err(failure)?;
                if state.speed != speed {
                    state.kernel.set_speed(speed).map_err(failure)?;
                    state.speed = speed;
                }
            }
            device
        };
        Ok(Some(NativeIo {
            device,
            logger: Box::new(logger),
        }))
    }
}

impl NativeIo {
    fn flock(&mut self, exclusive: bool) -> Result<(), Error> {
        loop {
            match self.device.borrow_mut().kernel.flock(exclusive) {
                Err(error) if error.raw_os_error() == Some(4) => (),
                result => return result.map_err(failure),
            }
        }
    }
}

impl Io for NativeIo {
    fn lock(&mut self) -> Result<(), Error> {
        match self.flock(true) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.flock(false)?;
                Err(error)
            }
        }
    }
    fn unlock(&mut self) -> Result<(), Error> {
        self.flock(false)
    }
    fn transfer(&mut self, _: Mode, data: &[u8]) -> Result<Vec<u8>, Error> {
        if data.is_empty() || data.len() > 4096 {
            return Err(Error::Invalid(
                "spidev transfer length must be 1 through 4096",
            ));
        }
        let mut reply = vec![0; data.len()];
        let mut state = self.device.borrow_mut();
        let (speed, bits) = (state.speed, state.bits);
        state
            .kernel
            .transfer(data, &mut reply, speed, bits)
            .map_err(failure)?;
        Ok(reply)
    }
    fn read(&mut self, length: usize) -> Result<Vec<u8>, Error> {
        let mut data = vec![0; length.clamp(1, 4096)];
        let count = self
            .device
            .borrow_mut()
            .kernel
            .read(&mut data)
            .map_err(failure)?;
        if count != data.len() {
            return Err(Error::Io("short spidev read".into()));
        }
        Ok(data)
    }
    fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        if data.is_empty() || data.len() > 4096 {
            return Err(Error::Invalid("spidev write length must be 1 through 4096"));
        }
        let count = self
            .device
            .borrow_mut()
            .kernel
            .write(data)
            .map_err(failure)?;
        if count != data.len() {
            return Err(Error::Io("short spidev write".into()));
        }
        Ok(())
    }
    fn now(&mut self) -> Result<f64, Error> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(if now.tv_nsec == 0 {
            now.tv_sec as f64
        } else {
            (i128::from(now.tv_sec) * 1_000_000_000 + i128::from(now.tv_nsec)) as f64
                / 1_000_000_000.0
        })
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| Error::Invalid("invalid SPI sleep duration"))?;
        if duration.is_zero() {
            FirmwareKernel::sleep_zero();
        } else {
            std::thread::sleep(duration);
        }
        Ok(())
    }
    fn log(&mut self, text: String, exception: Option<&Error>) -> Result<(), Error> {
        (self.logger)(text, exception)
    }
    fn kernel(
        &mut self,
        endpoint: u8,
        data: &[u8],
        maximum: usize,
        disconnect: bool,
    ) -> Result<Vec<u8>, Error> {
        let mut reply = vec![0; maximum];
        let count = self
            .device
            .borrow_mut()
            .kernel
            .kernel_transfer(endpoint, data, &mut reply, disconnect)
            .map_err(|error| Error::Protocol(format!("kernel SPI ioctl failed: {error}")))?;
        if count > reply.len() {
            return Err(Error::Protocol(
                "kernel SPI response exceeds supplied buffer".into(),
            ));
        }
        reply.truncate(count);
        Ok(reply)
    }
}
