use crate::{Bus, Platform};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct LinuxPlatform {
    device: PathBuf,
}

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new(Path::new("/dev/i2c-0"))
    }
}

impl LinuxPlatform {
    pub fn new(device: &Path) -> Self {
        Self {
            device: device.into(),
        }
    }
}

pub struct LinuxBus(File);

// i2c-linux-sys 0.2.1 inverts its force flag: false selects I2C_SLAVE_FORCE.
// Keep this pinned workaround paired with the source/native ioctl oracle (#102).
// Python 3.12 fcntl returns EINTR; the amplifier retries the whole transaction.
impl Bus for LinuxBus {
    fn read_byte(&mut self, register: u8) -> io::Result<u8> {
        i2c_linux_sys::i2c_set_slave_address(self.0.as_raw_fd(), 0x10, false)?;
        i2c_linux_sys::i2c_smbus_read_byte_data(self.0.as_raw_fd(), register)
    }
    fn write_byte(&mut self, register: u8, value: u8) -> io::Result<()> {
        i2c_linux_sys::i2c_set_slave_address(self.0.as_raw_fd(), 0x10, false)?;
        i2c_linux_sys::i2c_smbus_write_byte_data(self.0.as_raw_fd(), register, value)
    }
    fn close(self) -> io::Result<()> {
        nix::unistd::close(self.0).map_err(Into::into)
    }
}

impl Platform for LinuxPlatform {
    type Bus = LinuxBus;
    fn open_bus(&mut self) -> io::Result<LinuxBus> {
        Ok(LinuxBus(
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.device)?,
        ))
    }
    fn sleep(&mut self, seconds: f64) -> io::Result<()> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        std::thread::sleep(duration);
        Ok(())
    }
    fn print(&mut self, text: &str) -> io::Result<()> {
        writeln!(io::stdout().lock(), "{text}")
    }
}
