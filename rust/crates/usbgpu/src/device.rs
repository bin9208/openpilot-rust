use crate::{
    clock::Clock,
    controller::Controller,
    pci::{self, Bar, Config},
    transport::Transport,
    usb3::Usb3,
    Error,
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub address: u64,
    pub bytes: usize,
    pub element_size: u8,
    pub pci_memory: bool,
}
impl View {
    pub fn subview(
        self,
        offset: usize,
        size: Option<usize>,
        element_size: Option<u8>,
    ) -> Result<Self, Error> {
        let available = self
            .bytes
            .checked_sub(offset)
            .ok_or(Error::Contract("MMIO view offset out of range"))?;
        let size = size.filter(|&v| v != 0).unwrap_or(available);
        if size > available {
            return Err(Error::Contract("MMIO subview out of range"));
        }
        let element_size = element_size.unwrap_or(self.element_size);
        if ![1, 2, 4, 8].contains(&element_size) {
            return Err(Error::Contract("invalid MMIO element size"));
        }
        Ok(Self {
            address: self
                .address
                .checked_add(offset as u64)
                .ok_or(Error::Contract("MMIO address overflow"))?,
            bytes: size,
            element_size,
            pci_memory: self.pci_memory,
        })
    }
    fn checked_address(self, offset: usize, length: usize) -> Result<u64, Error> {
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.bytes)
        {
            return Err(Error::Contract("MMIO access out of range"));
        }
        self.address
            .checked_add(offset as u64)
            .ok_or(Error::Contract("MMIO access address overflow"))
    }
}
pub struct PciDevice<T, C> {
    pub controller: Controller<T, C>,
    pub bars: BTreeMap<u8, Bar>,
    sram_used: usize,
    _device_lock: File,
}
pub(crate) fn acquire_lock(lock_path: &Path) -> Result<File, Error> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o666)
        .open(lock_path)
    {
        Ok(file) => {
            file.set_permissions(std::fs::Permissions::from_mode(0o666))?;
            file
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            OpenOptions::new().read(true).write(true).open(lock_path)?
        }
        Err(error) => return Err(error.into()),
    };
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(std::io::Error::from)?;
    Ok(file)
}
impl<T: Transport, C: Clock> PciDevice<T, C> {
    pub fn new(usb: Usb3<T, C>, lock_path: &Path) -> Result<Self, Error> {
        Self::new_locked(usb, acquire_lock(lock_path)?)
    }
    pub(crate) fn new_locked(usb: Usb3<T, C>, file: File) -> Result<Self, Error> {
        let mut controller = Controller::new(usb)?;
        let bars = pci::setup_bars(&mut controller, 4, 0x10000000, 32 << 30)?;
        Ok(Self {
            controller,
            bars,
            sram_used: 0,
            _device_lock: file,
        })
    }
    pub fn map_bar(
        &self,
        index: u8,
        offset: usize,
        size: Option<usize>,
        element_size: u8,
    ) -> Result<View, Error> {
        let bar = self
            .bars
            .get(&index)
            .ok_or(Error::Contract("GPU BAR is missing"))?;
        let full = View {
            address: bar.address,
            bytes: usize::try_from(bar.size)
                .map_err(|_| Error::Contract("GPU BAR exceeds host address size"))?,
            element_size,
            pci_memory: true,
        };
        full.subview(offset, size, Some(element_size))
    }
    pub fn alloc_sysmem(&mut self, size: usize) -> Result<(View, u64), Error> {
        let offset = self.sram_used;
        let end = offset
            .checked_add(size)
            .ok_or(Error::Contract("SRAM allocation size overflow"))?;
        if end > 0x80000 {
            return Err(Error::Allocation(size as u64));
        }
        self.sram_used = end;
        Ok((
            View {
                address: 0xf000 + offset as u64,
                bytes: size,
                element_size: 1,
                pci_memory: false,
            },
            0x200000 + offset as u64,
        ))
    }
    pub fn read_config(&mut self, offset: u16, size: u8) -> Result<u32, Error> {
        self.controller.read_config(4, offset, size)
    }
    pub fn write_config(&mut self, offset: u16, size: u8, value: u32) -> Result<(), Error> {
        self.controller.write_config(4, offset, size, value)
    }
    pub fn read_bytes(
        &mut self,
        view: View,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, Error> {
        let address = view.checked_address(offset, length)?;
        if view.pci_memory {
            return self.controller.read_memory(address, length);
        }
        if view.address == 0xf000 {
            if let Controller::Custom(controller) = &mut self.controller {
                return controller.scsi_read(length);
            }
        }
        self.controller.read(address as u32, length)
    }
    pub fn write_bytes(&mut self, view: View, offset: usize, data: &[u8]) -> Result<(), Error> {
        let address = view.checked_address(offset, data.len())?;
        if !view.pci_memory {
            if view.address == 0xf000 {
                return self.controller.scsi_write(data);
            }
            let cache = (0xa800..=0xb000).contains(&view.address);
            return self.controller.write(address as u32, data, !cache);
        }
        let scaled = data
            .len()
            .checked_mul(usize::from(view.element_size))
            .ok_or(Error::Contract("MMIO transfer size overflow"))?;
        let size = if scaled.is_multiple_of(4) {
            4
        } else if scaled.is_multiple_of(2) {
            2
        } else {
            1
        };
        let words = data
            .chunks(size)
            .map(|part| {
                let mut bytes = [0; 4];
                bytes[..part.len()].copy_from_slice(part);
                u32::from_le_bytes(bytes)
            })
            .collect::<Vec<_>>();
        self.controller.memory_write(address, &words, size as u8)
    }
    pub fn read_element(&mut self, view: View, index: usize) -> Result<u64, Error> {
        let offset = index
            .checked_mul(usize::from(view.element_size))
            .ok_or(Error::Contract("MMIO index overflow"))?;
        let address = view.checked_address(offset, usize::from(view.element_size))?;
        if view.pci_memory {
            return self.controller.read_scalar(address, view.element_size);
        }
        let data = self.read_bytes(view, offset, usize::from(view.element_size))?;
        if data.len() != usize::from(view.element_size) {
            return Err(Error::Contract("short MMIO scalar read"));
        }
        let mut bytes = [0; 8];
        bytes[..data.len()].copy_from_slice(&data);
        Ok(u64::from_le_bytes(bytes))
    }
    pub fn write_element(&mut self, view: View, index: usize, value: u64) -> Result<(), Error> {
        let offset = index
            .checked_mul(usize::from(view.element_size))
            .ok_or(Error::Contract("MMIO index overflow"))?;
        let address = view.checked_address(offset, usize::from(view.element_size))?;
        if view.pci_memory {
            return self
                .controller
                .write_scalar(address, view.element_size, value);
        }
        self.write_bytes(
            view,
            offset,
            &value.to_le_bytes()[..usize::from(view.element_size)],
        )
    }
}
