use crate::{
    amd_bus::{Bus, NativeBus},
    clock::Clock,
    controller::Controller,
    device::View,
    transport::Transport,
    Error,
};
pub trait RuntimeBus: Bus {
    fn custom_bridge(&self) -> bool;
    fn cache_doorbells(&mut self) -> Result<(), Error>;
    fn cache_vram(&mut self, offset: u64, size: u64) -> Result<(), Error>;
    fn read_controller(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error>;
    fn write_controller(&mut self, address: u64, data: &[u8]) -> Result<(), Error>;
    fn arm_staging_read(&mut self, size: usize) -> Result<(), Error>;
    fn memory_barrier(&mut self) -> Result<(), Error> {
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
impl<T: Transport, C: Clock> RuntimeBus for NativeBus<T, C> {
    fn custom_bridge(&self) -> bool {
        matches!(self.device.controller, Controller::Custom(_))
    }
    fn cache_doorbells(&mut self) -> Result<(), Error> {
        let bar = self
            .device
            .bars
            .get(&2)
            .ok_or(Error::Contract("GPU doorbell BAR missing"))?;
        self.device.controller.cache_range(bar.address, bar.size);
        Ok(())
    }
    fn cache_vram(&mut self, offset: u64, size: u64) -> Result<(), Error> {
        let bar = self
            .device
            .bars
            .get(&0)
            .ok_or(Error::Contract("GPU VRAM BAR missing"))?;
        if offset.checked_add(size).is_none_or(|end| end > bar.size) {
            return Err(Error::Contract("GPU cache range outside BAR"));
        }
        self.device
            .controller
            .cache_range(bar.address + offset, size);
        Ok(())
    }
    fn read_controller(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        self.device.read_bytes(
            View {
                address,
                bytes: size,
                element_size: 1,
                pci_memory: false,
            },
            0,
            size,
        )
    }
    fn write_controller(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        self.device.write_bytes(
            View {
                address,
                bytes: data.len(),
                element_size: 1,
                pci_memory: false,
            },
            0,
            data,
        )
    }
    fn arm_staging_read(&mut self, size: usize) -> Result<(), Error> {
        match &mut self.device.controller {
            Controller::Custom(controller) => controller.scsi_read_arm(size),
            Controller::Stock(_) => Err(Error::Contract("bulk read arm requires custom bridge")),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum CpuLocation {
    Vram(u64),
    Controller(u64),
}
impl CpuLocation {
    pub fn offset(self, offset: u64) -> Result<Self, Error> {
        Ok(match self {
            Self::Vram(address) => Self::Vram(
                address
                    .checked_add(offset)
                    .ok_or(Error::Contract("VRAM CPU address overflow"))?,
            ),
            Self::Controller(address) => Self::Controller(
                address
                    .checked_add(offset)
                    .ok_or(Error::Contract("controller CPU address overflow"))?,
            ),
        })
    }
    pub fn read(self, bus: &mut impl RuntimeBus, size: usize) -> Result<Vec<u8>, Error> {
        match self {
            Self::Vram(address) => bus.read_vram(address, size),
            Self::Controller(address) => bus.read_controller(address, size),
        }
    }
    pub fn write(self, bus: &mut impl RuntimeBus, data: &[u8]) -> Result<(), Error> {
        match self {
            Self::Vram(address) => bus.write_vram(address, data),
            Self::Controller(address) => bus.write_controller(address, data),
        }
    }
    pub fn read_scalar(self, bus: &mut impl RuntimeBus, size: u8) -> Result<u64, Error> {
        if let Self::Vram(address) = self {
            return bus.read_vram_scalar(address, size);
        }
        let data = self.read(bus, usize::from(size))?;
        if data.len() != usize::from(size) || size > 8 {
            return Err(Error::Contract("invalid CPU scalar response"));
        }
        let mut bytes = [0; 8];
        bytes[..data.len()].copy_from_slice(&data);
        Ok(u64::from_le_bytes(bytes))
    }
    pub fn write_scalar(
        self,
        bus: &mut impl RuntimeBus,
        size: u8,
        value: u64,
    ) -> Result<(), Error> {
        if ![1, 2, 4, 8].contains(&size) {
            return Err(Error::Contract("invalid CPU scalar width"));
        }
        match self {
            Self::Vram(address) => bus.write_vram_scalar(address, size, value),
            Self::Controller(_) => self.write(bus, &value.to_le_bytes()[..usize::from(size)]),
        }
    }
}
pub struct NativeRingIo<'a, B> {
    pub bus: &'a mut B,
    pub ring: CpuLocation,
    pub pointer: CpuLocation,
    pub doorbell: u64,
}
impl<B: RuntimeBus> crate::queue::RingIo for NativeRingIo<'_, B> {
    fn write_word(&mut self, offset: usize, value: u32) -> Result<(), Error> {
        self.ring
            .offset(offset as u64)?
            .write_scalar(self.bus, 4, u64::from(value))
    }
    fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.ring.offset(offset as u64)?.write(self.bus, bytes)
    }
    fn write_pointer(&mut self, value: u64) -> Result<(), Error> {
        self.pointer.write_scalar(self.bus, 8, value)
    }
    fn memory_barrier(&mut self) -> Result<(), Error> {
        self.bus.memory_barrier()
    }
    fn doorbell(&mut self, value: u64) -> Result<(), Error> {
        self.bus.write_doorbell(self.doorbell, value)
    }
}
