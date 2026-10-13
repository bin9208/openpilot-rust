use crate::{
    clock::Clock, custom_asm::CustomAsm, pci::Config, stock_asm::StockAsm, transport::Transport,
    usb3::Usb3, Error,
};
pub enum Controller<T, C> {
    Custom(Box<CustomAsm<T, C>>),
    Stock(Box<StockAsm<T, C>>),
}
impl<T: Transport, C: Clock> Controller<T, C> {
    pub fn new(usb: Usb3<T, C>) -> Result<Self, Error> {
        if usb.custom {
            Ok(Self::Custom(Box::new(CustomAsm::new(usb)?)))
        } else {
            Ok(Self::Stock(Box::new(StockAsm::new(usb)?)))
        }
    }
    pub fn request(
        &mut self,
        format: u8,
        address: u64,
        value: Option<u32>,
        size: u8,
    ) -> Result<Option<u32>, Error> {
        match self {
            Self::Custom(c) => c.request(format, address, value, size),
            Self::Stock(c) => c.request(format, address, value, size),
        }
    }
    pub fn read(&mut self, address: u32, length: usize) -> Result<Vec<u8>, Error> {
        match self {
            Self::Custom(c) => c.read(address as u16, length),
            Self::Stock(c) => c.read(address, length, 255),
        }
    }
    pub fn write(&mut self, address: u32, data: &[u8], ignore_cache: bool) -> Result<(), Error> {
        match self {
            Self::Custom(c) => c.write(address as u16, data),
            Self::Stock(c) => c.write(address, data, ignore_cache),
        }
    }
    pub fn cache_range(&mut self, address: u64, size: u64) {
        match self {
            Self::Custom(c) => c.cache_range(address, size),
            Self::Stock(c) => c.cache_range(address, size),
        }
    }
    pub fn memory_write(&mut self, address: u64, data: &[u32], size: u8) -> Result<(), Error> {
        match self {
            Self::Custom(c) => c.memory_write(address, data),
            Self::Stock(c) => c.memory_write(address, data, size),
        }
    }
    pub fn scsi_write(&mut self, data: &[u8]) -> Result<(), Error> {
        match self {
            Self::Custom(c) => c.scsi_write(data),
            Self::Stock(c) => c.scsi_write(data, 0),
        }
    }
    pub fn read_scalar(&mut self, address: u64, size: u8) -> Result<u64, Error> {
        if ![1, 2, 4, 8].contains(&size) {
            return Err(Error::Contract("invalid MMIO scalar size"));
        }
        let upper = if size == 8 {
            u64::from(
                self.request(0x20, address + 4, None, 4)?
                    .ok_or(Error::Contract("missing upper PCIe read result"))?,
            )
        } else {
            0
        };
        let lower = self
            .request(0x20, address, None, size.min(4))?
            .ok_or(Error::Contract("missing PCIe read result"))?;
        Ok((upper << 32) | u64::from(lower))
    }
    pub fn write_scalar(&mut self, address: u64, size: u8, value: u64) -> Result<(), Error> {
        if ![1, 2, 4, 8].contains(&size) {
            return Err(Error::Contract("invalid MMIO scalar size"));
        }
        if size == 8 {
            self.request(0x60, address + 4, Some((value >> 32) as u32), 4)?;
        }
        self.request(0x60, address, Some(value as u32), size.min(4))?;
        Ok(())
    }
    pub fn read_memory(&mut self, address: u64, length: usize) -> Result<Vec<u8>, Error> {
        if length >= 4 && length.is_multiple_of(4) {
            if let Self::Custom(c) = self {
                return c.memory_read(address, length);
            }
        }
        let size = if length.is_multiple_of(4) {
            4
        } else if length.is_multiple_of(2) {
            2
        } else {
            1
        };
        let mut result = Vec::with_capacity(length);
        for offset in (0..length).step_by(size) {
            let value = self.read_scalar(address + offset as u64, size as u8)?;
            result.extend_from_slice(&value.to_le_bytes()[..size]);
        }
        Ok(result)
    }
}
impl<T: Transport, C: Clock> Config for Controller<T, C> {
    fn read_config(&mut self, bus: u8, offset: u16, size: u8) -> Result<u32, Error> {
        if offset >= 4096 {
            return Err(Error::Contract("PCI config offset outside 4 KiB"));
        }
        self.request(
            4 | u8::from(bus > 0),
            (u64::from(bus) << 24) | u64::from(offset),
            None,
            size,
        )?
        .ok_or(Error::Contract("missing PCI config result"))
    }
    fn write_config(&mut self, bus: u8, offset: u16, size: u8, value: u32) -> Result<(), Error> {
        if offset >= 4096 {
            return Err(Error::Contract("PCI config offset outside 4 KiB"));
        }
        self.request(
            0x44 | u8::from(bus > 0),
            (u64::from(bus) << 24) | u64::from(offset),
            Some(value),
            size,
        )?;
        Ok(())
    }
}

impl<T: Transport, C: Clock> Controller<T, C> {
    pub fn now(&self) -> std::time::Duration {
        match self {
            Self::Custom(c) => c.usb.clock.now(),
            Self::Stock(c) => c.usb.clock.now(),
        }
    }
    pub fn sleep(&mut self, duration: std::time::Duration) {
        match self {
            Self::Custom(c) => c.usb.clock.sleep(duration),
            Self::Stock(c) => c.usb.clock.sleep(duration),
        }
    }
}
