use crate::{
    amd_bus::NativeBus,
    bus_lock::BusLock,
    clock::Clock,
    controller::Controller,
    hcq_vm::{Function, Memory},
    runtime_bus::{CpuLocation, RuntimeBus},
    transport::{Control, Transport},
    usb3::Usb3,
    Error,
};

pub trait HcqBus: RuntimeBus {
    fn pci_address(&self, location: CpuLocation) -> Result<u64, Error>;
    fn doorbell_address(&self, index: u32) -> Result<u64, Error>;
    fn bus_lock(&self) -> Option<BusLock>;
    fn transfer(
        &mut self,
        function: Function,
        arguments: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error>;
}
impl<T: Transport, C: Clock> NativeBus<T, C> {
    fn usb(&self) -> &Usb3<T, C> {
        match &self.device.controller {
            Controller::Custom(c) => &c.usb,
            Controller::Stock(c) => &c.usb,
        }
    }
    fn usb_mut(&mut self) -> &mut Usb3<T, C> {
        match &mut self.device.controller {
            Controller::Custom(c) => &mut c.usb,
            Controller::Stock(c) => &mut c.usb,
        }
    }
    fn bar_address(&self, index: u8, offset: u64) -> Result<u64, Error> {
        let bar = self
            .device
            .bars
            .get(&index)
            .ok_or(Error::Contract("HCQ PCI BAR missing"))?;
        if offset >= bar.size {
            return Err(Error::Contract("HCQ PCI BAR offset outside range"));
        }
        bar.address
            .checked_add(offset)
            .ok_or(Error::Contract("HCQ PCI address overflow"))
    }
}
impl<T: Transport, C: Clock> HcqBus for NativeBus<T, C> {
    fn pci_address(&self, location: CpuLocation) -> Result<u64, Error> {
        match location {
            CpuLocation::Vram(offset) => self.bar_address(0, offset),
            CpuLocation::Controller(_) => Err(Error::Contract(
                "HCQ PCI pointer cannot reference controller memory",
            )),
        }
    }
    fn doorbell_address(&self, index: u32) -> Result<u64, Error> {
        self.bar_address(2, u64::from(index) * 8)
    }
    fn bus_lock(&self) -> Option<BusLock> {
        Some(self.usb().lock.clone())
    }
    fn transfer(
        &mut self,
        function: Function,
        args: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        let lock = self.usb().lock.clone();
        let _guard = lock.enter()?;
        transfer(&mut self.usb_mut().transport, function, args, memory)
    }
}
pub fn transfer(
    transport: &mut impl Transport,
    function: Function,
    args: &[u64],
    memory: &mut Memory,
) -> Result<u64, Error> {
    let arity = match function {
        Function::Control => 8,
        Function::Bulk => 6,
    };
    if args.len() != arity || args[0] != 1 {
        return Err(Error::Contract("HCQ USB function signature or handle"));
    }
    let narrow = || Error::Contract("HCQ USB integer argument out of range");
    match function {
        Function::Control => {
            let size = u16::try_from(args[6]).map_err(|_| narrow())?;
            let code = transport.control(
                Control {
                    kind: u8::try_from(args[1]).map_err(|_| narrow())?,
                    request: u8::try_from(args[2]).map_err(|_| narrow())?,
                    value: u16::try_from(args[3]).map_err(|_| narrow())?,
                    index: u16::try_from(args[4]).map_err(|_| narrow())?,
                    timeout_ms: u32::try_from(args[7]).map_err(|_| narrow())?,
                },
                memory.read_mut(args[5], usize::from(size))?,
            )?;
            transport.checked(code, "HCQ USB control transfer failed")?;
            if code != i32::from(size) {
                return Err(Error::Protocol(format!(
                    "HCQ USB control short transfer: {code}/{size}"
                )));
            }
            Ok(u64::from(size))
        }
        Function::Bulk => {
            let size = i32::try_from(args[3]).map_err(|_| narrow())?;
            let result = transport.bulk(
                u8::try_from(args[1]).map_err(|_| narrow())?,
                memory.read_mut(args[2], usize::try_from(size).map_err(|_| narrow())?)?,
                u32::try_from(args[5]).map_err(|_| narrow())?,
            )?;
            if args[4] != 0 {
                memory.write(args[4], &result.actual.to_le_bytes())?;
            }
            transport.checked(result.code, "HCQ USB bulk transfer failed")?;
            if u64::from(result.actual) != args[3] {
                return Err(Error::Protocol(format!(
                    "HCQ USB bulk short transfer: {}/{}",
                    result.actual, args[3]
                )));
            }
            Ok(0)
        }
    }
}
