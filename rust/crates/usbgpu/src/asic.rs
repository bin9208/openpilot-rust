use crate::{
    allocator::Tlsf,
    amd_bus::{Bus, Hardware},
    amd_metadata::Catalog,
    firmware::{Firmware, FirmwareSource},
    memory::Memory,
    page_table::PtePolicy,
    Error,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};
#[derive(Clone, Copy, Default)]
pub struct BootOptions {
    pub reset: bool,
    pub power_limit: Option<f64>,
    pub disable_gmmu: bool,
}
pub(crate) struct IhState {
    pub rings: [(u64, u64); 2],
    pub size: u64,
}
pub(crate) struct PspState {
    pub prefix: String,
    pub message_region: u64,
    pub message_address: u64,
    pub message_size: usize,
    pub command: u64,
    pub fence: u64,
    pub ring: u64,
    pub ring_size: u64,
    pub tmr: u64,
    pub tmr_size: u64,
    pub boot_tmr: bool,
    pub autoload_tmr: bool,
}
pub(crate) struct GfxState {
    pub mqds: [u64; 2],
    pub xccs: u8,
}
pub(crate) struct SdmaState {
    pub name: &'static str,
    pub rings: Vec<(String, u8)>,
}
pub(crate) struct SmuState {
    pub module: String,
    pub driver_table: u64,
    pub clocks: BTreeMap<u32, Vec<u32>>,
}
pub struct Asic<B: Bus> {
    pub hw: Hardware<B>,
    pub memory: Memory,
    pub firmware: Firmware,
    pub partial_boot: bool,
    pub error_state: bool,
    pub(crate) ih: Option<IhState>,
    pub(crate) psp: Option<PspState>,
    pub(crate) gfx: Option<GfxState>,
    pub(crate) sdma: Option<SdmaState>,
    pub(crate) smu: Option<SmuState>,
    pub(crate) booted: bool,
}
impl<B: Bus> Asic<B> {
    pub const VERSION: u32 = 0xa0000008;
    fn software(
        bus: B,
        source: &mut impl FirmwareSource,
        options: BootOptions,
    ) -> Result<Self, Error> {
        let catalog = Arc::new(Catalog::bundled()?);
        let mut hw = Hardware::discover(bus, catalog)?;
        let gfx = hw.gfx()?;
        let usable = hw
            .vram_size
            .checked_sub(hw.discovery.reserved_vram()?)
            .ok_or(Error::Contract("GPU VRAM smaller than reserved region"))?;
        static VIRTUAL: OnceLock<Arc<Mutex<Tlsf>>> = OnceLock::new();
        let virtual_allocator = Arc::clone(VIRTUAL.get_or_init(|| {
            Arc::new(Mutex::new(
                Tlsf::new(1 << 44, 0x200000000000).expect("fixed AMD virtual aperture"),
            ))
        }));
        let reserve = !hw.large_bar;
        let policy = PtePolicy {
            gfx_major: gfx[0],
            uncached_type: hw.soc_constant("MTYPE_UC")? as u8,
            address_mask: (1
                << if gfx[0] == 9 && [4, 5].contains(&gfx[1]) {
                    48
                } else {
                    44
                })
                - 1,
            physical_base: 0,
        };
        let mut memory = Memory::new(
            &mut hw,
            usable,
            32 << 20,
            reserve,
            policy,
            virtual_allocator,
            false,
        )?;
        memory.gmmu = !options.disable_gmmu;
        let firmware = Firmware::load(&hw.catalog, &hw.discovery, source)?;
        let mut result = Self {
            hw,
            memory,
            firmware,
            partial_boot: false,
            error_state: false,
            ih: None,
            psp: None,
            gfx: None,
            sdma: None,
            smu: None,
            booted: false,
        };
        result.init_gmc_software()?;
        result.init_ih_software()?;
        result.init_psp_software()?;
        result.init_smu_software()?;
        result.init_gfx_software()?;
        result.init_sdma_software()?;
        Ok(result)
    }
}

impl<B: Bus> Asic<B> {
    pub fn boot(
        bus: B,
        source: &mut impl FirmwareSource,
        options: BootOptions,
    ) -> Result<Self, Error> {
        let mut result = Self::software(bus, source, options)?;
        result.partial_boot =
            result.hw.read("regSCRATCH_REG7", 0)? == Self::VERSION && !options.reset;
        if result.partial_boot
            && (result.hw.read("regSCRATCH_REG6", 0)? != 0
                || result.hw.read(&result.fault_register("GC")?, 0)? != 0)
        {
            result.partial_boot = false;
        }
        if !result.partial_boot {
            if result.psp_alive()? && result.smu_alive()? {
                let command = result.hw.bus.read_config(4, 2)? & !4;
                result.hw.bus.write_config(4, 2, command)?;
                result.hw.bus.read_config(4, 2)?;
                if result.hw.gmc()?.xgmi_segment_size != 0 {
                    return Err(Error::Contract(
                        "malformed hive state requires coordinated reset",
                    ));
                }
                result.smu_reset()?;
            }
            let command = result.hw.bus.read_config(4, 2)? | 4;
            result.hw.bus.write_config(4, 2, command)?;
            result.hw.bus.read_config(4, 2)?;
            result.init_soc()?;
            let hubs = result.hw.gmc()?.vmhubs;
            result.init_hub("MM", hubs)?;
            result.init_ih()?;
            result.init_psp()?;
            result.init_smu()?;
        }
        result.memory.is_booting = false;
        result.init_gfx()?;
        result.init_sdma()?;
        if let Some(power) = options.power_limit.filter(|v| *v > 0.) {
            result.set_power_limit(power)?;
            result.set_clocks(None)?;
        } else {
            result.set_clocks(Some(-1))?;
        }
        result.soc_clockgating()?;
        result.gfx_clockgating()?;
        result.hw.write("regSCRATCH_REG7", 0, Self::VERSION, &[])?;
        result.hw.write("regSCRATCH_REG6", 0, 1, &[])?;
        result.booted = true;
        Ok(result)
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.booted {
            return Ok(());
        }
        self.stop_sdma()?;
        self.dequeue_compute()?;
        self.set_clocks(Some(0))?;
        self.handle_interrupts()?;
        self.hw
            .write("regSCRATCH_REG6", 0, u32::from(self.error_state), &[])?;
        self.booted = false;
        Ok(())
    }
    pub fn recover(&mut self, force: bool) -> Result<bool, Error> {
        if !force && !self.error_state {
            return Ok(false);
        }
        self.handle_interrupts()?;
        self.reset_mec()?;
        self.error_state = false;
        Ok(true)
    }
}
impl<B: Bus> Drop for Asic<B> {
    fn drop(&mut self) {
        if self.booted {
            if let Err(error) = self.finish() {
                eprintln!("usbgpu AMD finalization failed: {error}");
            }
        }
    }
}
