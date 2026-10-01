use crate::{bridge::ffi, loops::Poll, Bus, Error};
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::Path,
};
pub struct LinuxBus {
    file: File,
}
impl LinuxBus {
    pub fn open(path: &Path) -> Result<Self, Error> {
        Ok(Self {
            file: OpenOptions::new().read(true).write(true).open(path)?,
        })
    }
    pub fn read_byte(&mut self, address: u16, reg: u8, force: bool) -> Result<u8, Error> {
        Ok(ffi::read_byte(self.file.as_raw_fd(), address, reg, force)?)
    }
    pub fn write_byte(
        &mut self,
        address: u16,
        reg: u8,
        value: u8,
        force: bool,
    ) -> Result<(), Error> {
        Ok(ffi::write_byte(
            self.file.as_raw_fd(),
            address,
            reg,
            value,
            force,
        )?)
    }
    pub fn read_block(
        &mut self,
        address: u16,
        reg: u8,
        length: usize,
        force: bool,
    ) -> Result<Vec<u8>, Error> {
        let data = ffi::read_block(self.file.as_raw_fd(), address, reg, length, force)?;
        Ok(data
            .as_ref()
            .ok_or(Error::Contract("null SMBus result"))?
            .as_slice()
            .to_vec())
    }
}
impl Bus for LinuxBus {
    fn read(&mut self, register: u8, length: usize) -> Result<Vec<u8>, Error> {
        self.read_block(0x6a, register, length, false)
    }
    fn write(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.write_byte(0x6a, register, value, false)
    }
}
pub struct Gpio {
    device: cxx::UniquePtr<ffi::Gpio>,
}
impl Gpio {
    pub fn open(path: &Path, label: &str, pin: u32) -> Result<Self, Error> {
        let path = path
            .to_str()
            .ok_or(Error::Contract("GPIO path is not UTF-8"))?;
        cxx::let_cxx_string!(device_path = path);
        cxx::let_cxx_string!(consumer = label);
        let device = ffi::open_gpio(&device_path, &consumer, pin)?;
        if device.is_null() {
            return Err(Error::Contract("null GPIO result"));
        }
        Ok(Self { device })
    }
    pub fn poll(&mut self, timeout_ms: i32) -> Result<Poll, Error> {
        let flags = self.device.pin_mut().poll_event(timeout_ms)?;
        if flags == 0 {
            return Ok(Poll::Timeout);
        }
        if flags & 3 == 0 {
            return Ok(Poll::Other);
        }
        let data = self.device.pin_mut().read_events()?;
        Ok(Poll::Data(
            data.as_ref()
                .ok_or(Error::Contract("null GPIO data"))?
                .as_slice()
                .to_vec(),
        ))
    }
}
pub fn realtime(pc: bool) -> Result<(), Error> {
    Ok(ffi::realtime(pc)?)
}
