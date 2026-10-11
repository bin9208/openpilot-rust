use crate::{
    amd_metadata::{Catalog, Register},
    clock::Clock,
    device::{PciDevice, View},
    discovery::Discovery,
    memory::MemoryIo,
    transport::Transport,
    Error,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
pub trait Bus {
    fn mmio_words(&self) -> u64;
    fn vram_bar_size(&self) -> u64;
    fn read_register(&mut self, index: u64) -> Result<u32, Error>;
    fn write_register(&mut self, index: u64, value: u32) -> Result<(), Error>;
    fn read_vram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error>;
    fn write_vram(&mut self, address: u64, data: &[u8]) -> Result<(), Error>;
    fn read_vram_scalar(&mut self, address: u64, size: u8) -> Result<u64, Error>;
    fn write_vram_scalar(&mut self, address: u64, size: u8, value: u64) -> Result<(), Error>;
    fn write_doorbell(&mut self, index: u64, value: u64) -> Result<(), Error>;
    fn read_config(&mut self, offset: u16, size: u8) -> Result<u32, Error>;
    fn write_config(&mut self, offset: u16, size: u8, value: u32) -> Result<(), Error>;
    fn alloc_sram(&mut self, size: usize) -> Result<(u64, u64), Error>;
    fn write_sram(&mut self, address: u64, data: &[u8]) -> Result<(), Error>;
    fn read_sram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error>;
    fn now(&self) -> Duration;
    fn sleep(&mut self, duration: Duration);
}
pub struct NativeBus<T, C> {
    pub device: PciDevice<T, C>,
    mmio: View,
    vram: View,
    doorbell: View,
    sram: BTreeMap<u64, View>,
}
impl<T: Transport, C: Clock> NativeBus<T, C> {
    pub fn new(device: PciDevice<T, C>) -> Result<Self, Error> {
        let mmio = device.map_bar(5, 0, None, 4)?;
        let vram = device.map_bar(0, 0, None, 1)?;
        let doorbell = device.map_bar(2, 0, None, 8)?;
        Ok(Self {
            device,
            mmio,
            vram,
            doorbell,
            sram: BTreeMap::new(),
        })
    }
    fn vram_scalar(&self, address: u64, size: u8) -> Result<View, Error> {
        self.vram.subview(
            usize::try_from(address).map_err(|_| Error::Contract("VRAM offset overflow"))?,
            Some(usize::from(size)),
            Some(size),
        )
    }
}
impl<T: Transport, C: Clock> Bus for NativeBus<T, C> {
    fn mmio_words(&self) -> u64 {
        self.mmio.bytes as u64 / 4
    }
    fn vram_bar_size(&self) -> u64 {
        self.vram.bytes as u64
    }
    fn read_register(&mut self, index: u64) -> Result<u32, Error> {
        Ok(self.device.read_element(
            self.mmio,
            usize::try_from(index).map_err(|_| Error::Contract("register index overflow"))?,
        )? as u32)
    }
    fn write_register(&mut self, index: u64, value: u32) -> Result<(), Error> {
        self.device.write_element(
            self.mmio,
            usize::try_from(index).map_err(|_| Error::Contract("register index overflow"))?,
            u64::from(value),
        )
    }
    fn read_vram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        self.device.read_bytes(
            self.vram,
            usize::try_from(address).map_err(|_| Error::Contract("VRAM offset overflow"))?,
            size,
        )
    }
    fn write_vram(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        self.device.write_bytes(
            self.vram,
            usize::try_from(address).map_err(|_| Error::Contract("VRAM offset overflow"))?,
            data,
        )
    }
    fn read_vram_scalar(&mut self, address: u64, size: u8) -> Result<u64, Error> {
        let view = self.vram_scalar(address, size)?;
        self.device.read_element(view, 0)
    }
    fn write_vram_scalar(&mut self, address: u64, size: u8, value: u64) -> Result<(), Error> {
        let view = self.vram_scalar(address, size)?;
        self.device.write_element(view, 0, value)
    }
    fn write_doorbell(&mut self, index: u64, value: u64) -> Result<(), Error> {
        self.device.write_element(
            self.doorbell,
            usize::try_from(index).map_err(|_| Error::Contract("doorbell index overflow"))?,
            value,
        )
    }
    fn read_config(&mut self, offset: u16, size: u8) -> Result<u32, Error> {
        self.device.read_config(offset, size)
    }
    fn write_config(&mut self, offset: u16, size: u8, value: u32) -> Result<(), Error> {
        self.device.write_config(offset, size, value)
    }
    fn alloc_sram(&mut self, size: usize) -> Result<(u64, u64), Error> {
        let (view, physical) = self.device.alloc_sysmem(size)?;
        self.sram.insert(view.address, view);
        Ok((view.address, physical))
    }
    fn write_sram(&mut self, address: u64, data: &[u8]) -> Result<(), Error> {
        let view = self
            .sram
            .get(&address)
            .copied()
            .ok_or(Error::Contract("SRAM region is not allocated"))?;
        self.device.write_bytes(view, 0, data)
    }
    fn read_sram(&mut self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        let view = self
            .sram
            .get(&address)
            .copied()
            .ok_or(Error::Contract("SRAM region is not allocated"))?;
        self.device.read_bytes(view, 0, size)
    }
    fn now(&self) -> Duration {
        self.device.controller.now()
    }
    fn sleep(&mut self, duration: Duration) {
        self.device.controller.sleep(duration);
    }
}
#[derive(Clone, Debug)]
pub struct GmcState {
    pub physical_base: u64,
    pub framebuffer_base: u64,
    pub framebuffer_end: u64,
    pub memory_base: u64,
    pub virtual_base: u64,
    pub virtual_end: u64,
    pub scratch: u64,
    pub dummy: u64,
    pub mm_ready: bool,
    pub gc_ready: bool,
    pub vmhubs: u8,
    pub xccs: u8,
    pub xgmi_segment_size: u64,
}
pub struct Hardware<B> {
    pub bus: B,
    pub catalog: Arc<Catalog>,
    pub discovery: Discovery,
    pub registers: BTreeMap<String, Register>,
    pub vram_size: u64,
    pub large_bar: bool,
    pub gmc: Option<GmcState>,
}
impl<B: Bus> Hardware<B> {
    pub fn discover(mut bus: B, catalog: Arc<Catalog>) -> Result<Self, Error> {
        let vram_size = u64::from(bus.read_register(0xde3)?) << 20;
        let large_bar = bus.vram_bar_size() >= vram_size;
        let offset = vram_size
            .checked_sub(64 << 10)
            .ok_or(Error::Contract("GPU VRAM smaller than discovery region"))?;
        let bytes = if large_bar {
            bus.read_vram(offset, 10 << 10)?
        } else {
            let mut bytes = Vec::with_capacity(10 << 10);
            for address in (offset..offset + (10 << 10)).step_by(4) {
                bus.write_register(6, (address >> 31) as u32)?;
                bus.write_register(0, ((address & 0x7fffffff) | 0x80000000) as u32)?;
                bytes.extend_from_slice(&bus.read_register(1)?.to_le_bytes());
            }
            bytes
        };
        let discovery = Discovery::parse(&catalog, &bytes)?;
        let registers = discovery.registers(&catalog)?;
        Ok(Self {
            bus,
            catalog,
            discovery,
            registers,
            vram_size,
            large_bar,
            gmc: None,
        })
    }
    pub fn version(&self, hardware: u32) -> Result<[u8; 3], Error> {
        self.discovery.version(hardware)
    }
    pub fn gfx(&self) -> Result<[u8; 3], Error> {
        self.discovery.gc_version()
    }
    pub fn reg(&self, name: &str) -> Result<&Register, Error> {
        self.registers
            .get(name)
            .or_else(|| self.registers.get(&name.replace("reg", "mm")))
            .ok_or_else(|| Error::Protocol(format!("AMD register not found: {name}")))
    }
    pub fn raw_read(&mut self, index: u64) -> Result<u32, Error> {
        if index < self.bus.mmio_words() {
            return self.bus.read_register(index);
        }
        let selector = self.reg("regBIF_BX_PF0_RSMU_INDEX")?.address(0)?;
        let data = self.reg("regBIF_BX_PF0_RSMU_DATA")?.address(0)?;
        self.bus.write_register(
            selector,
            u32::try_from(
                index
                    .checked_mul(4)
                    .ok_or(Error::Contract("indirect register address overflow"))?,
            )
            .map_err(|_| Error::Contract("indirect register address exceeds dword"))?,
        )?;
        self.bus.read_register(data)
    }
    pub fn raw_write(&mut self, index: u64, value: u32) -> Result<(), Error> {
        if index < self.bus.mmio_words() {
            return self.bus.write_register(index, value);
        }
        let selector = self.reg("regBIF_BX_PF0_RSMU_INDEX")?.address(0)?;
        let data = self.reg("regBIF_BX_PF0_RSMU_DATA")?.address(0)?;
        self.bus.write_register(
            selector,
            u32::try_from(
                index
                    .checked_mul(4)
                    .ok_or(Error::Contract("indirect register address overflow"))?,
            )
            .map_err(|_| Error::Contract("indirect register address exceeds dword"))?,
        )?;
        self.bus.write_register(data, value)
    }
    pub fn read(&mut self, name: &str, instance: u8) -> Result<u32, Error> {
        let address = self.reg(name)?.address(instance)?;
        self.raw_read(address)
    }
    pub fn field(&mut self, name: &str, instance: u8, field: &str) -> Result<u32, Error> {
        let value = self.read(name, instance)?;
        self.reg(name)?.decode(field, value)
    }
    pub fn write(
        &mut self,
        name: &str,
        instance: u8,
        value: u32,
        fields: &[(&str, u32)],
    ) -> Result<(), Error> {
        let register = self.reg(name)?;
        let address = register.address(instance)?;
        let value = value | register.encode(fields)?;
        self.raw_write(address, value)
    }
    pub fn update(
        &mut self,
        name: &str,
        instance: u8,
        fields: &[(&str, u32)],
    ) -> Result<(), Error> {
        let mask = self
            .reg(name)?
            .field_mask(&fields.iter().map(|(name, _)| *name).collect::<Vec<_>>())?;
        let old = self.read(name, instance)?;
        self.write(name, instance, old & !mask, fields)
    }
    pub fn pair(
        &mut self,
        base: &str,
        low: &str,
        high: &str,
        value: u64,
        instance: u8,
    ) -> Result<(), Error> {
        self.write(&format!("{base}{low}"), instance, value as u32, &[])?;
        self.write(
            &format!("{base}{high}"),
            instance,
            (value >> 32) as u32,
            &[],
        )
    }
    pub fn constant(&self, name: &str) -> Result<u32, Error> {
        u32::try_from(self.catalog.constant("am", name)?)
            .map_err(|_| Error::Contract("AMD constant exceeds dword"))
    }
    pub fn soc_constant(&self, name: &str) -> Result<u32, Error> {
        let major = self.gfx()?[0];
        let soc = if major < 11 { 9 } else { major };
        u32::try_from(self.catalog.constant(&format!("soc_{soc}"), name)?)
            .map_err(|_| Error::Contract("AMD SOC constant exceeds dword"))
    }
    pub fn wait(
        &mut self,
        timeout_ms: u64,
        expected: u64,
        operation: &str,
        mut read: impl FnMut(&mut Self) -> Result<u64, Error>,
    ) -> Result<u64, Error> {
        let start = self.bus.now().as_millis();
        let mut last = None;
        while self.bus.now().as_millis().saturating_sub(start) < u128::from(timeout_ms) {
            let value = read(self)?;
            if value == expected {
                return Ok(value);
            }
            last = Some(value);
        }
        Err(Error::Timeout {
            operation: operation.to_owned(),
            milliseconds: timeout_ms,
            last,
            expected,
        })
    }
    pub fn wait_register(
        &mut self,
        name: &str,
        instance: u8,
        mask: u32,
        expected: u32,
        operation: &str,
    ) -> Result<(), Error> {
        self.wait(10000, u64::from(expected), operation, |hw| {
            Ok(u64::from(hw.read(name, instance)? & mask))
        })?;
        Ok(())
    }
    pub fn indirect_pcie_write(&mut self, index: u64, value: u32, aid: u8) -> Result<(), Error> {
        let address = index
            .checked_mul(4)
            .ok_or(Error::Contract("PCIe indirect address overflow"))?
            | if aid > 0 {
                (u64::from(aid & 3) << 32) | (1 << 34)
            } else {
                0
            };
        self.write("regBIF_BX0_PCIE_INDEX2", 0, address as u32, &[])?;
        if address >> 32 != 0 {
            self.write(
                "regBIF_BX0_PCIE_INDEX2_HI",
                0,
                ((address >> 32) & 255) as u32,
                &[],
            )?;
        }
        self.write("regBIF_BX0_PCIE_DATA2", 0, value, &[])?;
        if address >> 32 != 0 {
            self.write("regBIF_BX0_PCIE_INDEX2_HI", 0, 0, &[])?;
        }
        Ok(())
    }
    pub fn gmc(&self) -> Result<&GmcState, Error> {
        self.gmc
            .as_ref()
            .ok_or(Error::Contract("GMC software state not initialized"))
    }
    pub fn paddr_to_mc(&self, address: u64) -> Result<u64, Error> {
        self.gmc()?
            .memory_base
            .checked_add(address)
            .ok_or(Error::Contract("GPU MC address overflow"))
    }
    pub fn paddr_to_physical(&self, address: u64) -> Result<u64, Error> {
        self.gmc()?
            .physical_base
            .checked_add(address)
            .ok_or(Error::Contract("GPU physical address overflow"))
    }
    pub fn flush_hdp(&mut self) -> Result<(), Error> {
        let address = u64::from(self.read("regBIF_BX0_REMAP_HDP_MEM_FLUSH_CNTL", 0)?) / 4;
        self.raw_write(address, 0)
    }
    pub fn flush_tlb(&mut self, hub: &str, vmid: u8, flush_type: u32) -> Result<(), Error> {
        self.flush_hdp()?;
        let state = self.gmc()?;
        if !(if hub == "MM" {
            state.mm_ready
        } else {
            state.gc_ready
        }) {
            return Ok(());
        }
        let count = if hub == "MM" {
            state.vmhubs
        } else {
            state.xccs
        };
        for instance in 0..count {
            if hub == "MM" {
                self.wait_register(
                    "regMMVM_INVALIDATE_ENG17_SEM",
                    instance,
                    1,
                    1,
                    "mm flush_tlb timeout",
                )?;
            }
            self.write(
                &format!("reg{hub}VM_INVALIDATE_ENG17_REQ"),
                instance,
                0,
                &[
                    ("flush_type", flush_type),
                    ("per_vmid_invalidate_req", 1 << vmid),
                    ("invalidate_l2_ptes", 1),
                    ("invalidate_l2_pde0", 1),
                    ("invalidate_l2_pde1", 1),
                    ("invalidate_l2_pde2", 1),
                    ("invalidate_l1_ptes", 1),
                    ("clear_protection_fault_status_addr", 0),
                ],
            )?;
            self.wait_register(
                &format!("reg{hub}VM_INVALIDATE_ENG17_ACK"),
                instance,
                1 << vmid,
                1 << vmid,
                "flush_tlb timeout",
            )?;
            if hub == "MM" {
                self.write("regMMVM_INVALIDATE_ENG17_SEM", instance, 0, &[])?;
            }
            if self.gfx()? >= [11, 0, 0] && hub == "MM" {
                self.update(
                    "regMMVM_L2_BANK_SELECT_RESERVED_CID2",
                    instance,
                    &[("reserved_cache_private_invalidation", 1)],
                )?;
                self.read("regMMVM_L2_BANK_SELECT_RESERVED_CID2", instance)?;
            }
        }
        Ok(())
    }
}
impl<B: Bus> MemoryIo for Hardware<B> {
    fn read_entry(&mut self, address: u64) -> Result<u64, Error> {
        self.bus.read_vram_scalar(address, 8)
    }
    fn write_entry(&mut self, address: u64, value: u64) -> Result<(), Error> {
        self.bus.write_vram_scalar(address, 8, value)
    }
    fn zero(&mut self, address: u64, size: u64) -> Result<(), Error> {
        let size = usize::try_from(size)
            .map_err(|_| Error::Contract("GPU zero size exceeds host space"))?;
        self.bus.write_vram(address, &vec![0; size])
    }
    fn mappings_changed(&mut self) -> Result<(), Error> {
        self.flush_tlb("GC", 0, 0)?;
        self.flush_tlb("MM", 0, 0)
    }
}
