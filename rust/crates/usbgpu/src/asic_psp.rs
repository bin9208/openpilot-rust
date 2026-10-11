use crate::{
    amd_bus::Bus,
    asic::{Asic, PspState},
    firmware::Segment,
    memory::Mapping,
    page_table::AddressSpace,
    Error,
};
use std::time::Duration;
impl<B: Bus> Asic<B> {
    pub(crate) fn init_psp_software(&mut self) -> Result<(), Error> {
        let version = self.hw.version(15)?;
        let prefix = if version < [14, 0, 0] {
            "regMP0_SMN_C2PMSG"
        } else {
            "regMPASP_SMN_C2PMSG"
        }
        .to_owned();
        let message_size = 512 << 10;
        let (message_region, physical) = self.hw.bus.alloc_sram(message_size)?;
        let message_address = self.memory.alloc_virtual(message_size as u64, 1 << 20)?;
        self.memory.map(
            &mut self.hw,
            Mapping {
                address: message_address,
                size: message_size as u64,
                physical: vec![(physical, message_size as u64)],
                space: AddressSpace::System,
                uncached: true,
                snooped: false,
            },
            true,
        )?;
        let command_size = u64::from(self.hw.constant("PSP_CMD_BUFFER_SIZE")?);
        let command = self
            .memory
            .palloc(&mut self.hw, command_size, 4096, false, true, false)?;
        let fence_size = u64::from(self.hw.constant("PSP_FENCE_BUFFER_SIZE")?);
        let fence = self
            .memory
            .palloc(&mut self.hw, fence_size, 4096, true, true, false)?;
        let ring_size = 0x10000;
        let ring = self
            .memory
            .palloc(&mut self.hw, ring_size, 4096, false, true, false)?;
        let boot_tmr = [[13, 0, 6], [13, 0, 14], [14, 0, 2], [14, 0, 3]].contains(&version);
        let autoload_tmr = ![[13, 0, 6], [13, 0, 14]].contains(&version);
        let tmr = if boot_tmr {
            0
        } else {
            let align = u64::from(self.hw.constant("PSP_TMR_ALIGNMENT")?);
            self.memory
                .palloc(&mut self.hw, 0x1300000, align, false, true, false)?
        };
        self.psp = Some(PspState {
            prefix,
            message_region,
            message_address,
            message_size,
            command,
            fence,
            ring,
            ring_size,
            tmr,
            tmr_size: 0,
            boot_tmr,
            autoload_tmr,
        });
        Ok(())
    }
    fn psp_state(&self) -> Result<&PspState, Error> {
        self.psp
            .as_ref()
            .ok_or(Error::Contract("PSP software not initialized"))
    }
    fn psp_register(&self, index: u8) -> Result<String, Error> {
        Ok(format!("{}_{index}", self.psp_state()?.prefix))
    }
    pub(crate) fn psp_alive(&mut self) -> Result<bool, Error> {
        let name = self.psp_register(81)?;
        Ok(self.hw.read(&name, 0)? != 0)
    }
    fn psp_wait_bootloader(&mut self) -> Result<(), Error> {
        let name = self.psp_register(35)?;
        self.hw
            .wait_register(&name, 0, 0x80000000, 0x80000000, "BL not ready")
    }
    fn psp_message_data(&mut self, data: &[u8]) -> Result<(), Error> {
        let state = self.psp_state()?;
        if data.len() > state.message_size {
            return Err(Error::Contract("PSP msg1 buffer too small"));
        }
        let region = state.message_region;
        let length = data
            .len()
            .checked_add(4)
            .and_then(|value| value.checked_next_multiple_of(16))
            .ok_or(Error::Contract("PSP msg1 padding overflow"))?;
        let mut padded = data.to_vec();
        padded.resize(length, 0);
        self.hw.bus.write_sram(region, &padded)?;
        self.hw.flush_hdp()
    }
    fn psp_load_component(&mut self, kind: u32, command: u32) -> Result<(), Error> {
        let Some(segment) = self.firmware.sos.get(&kind).cloned() else {
            return Ok(());
        };
        self.psp_wait_bootloader()?;
        self.psp_message_data(segment.data())?;
        let address = self.psp_state()?.message_address;
        self.hw
            .write(&self.psp_register(36)?, 0, (address >> 20) as u32, &[])?;
        self.hw.write(&self.psp_register(35)?, 0, command, &[])?;
        if command != self.hw.constant("PSP_BL__LOAD_SOSDRV")? {
            self.psp_wait_bootloader()?;
        }
        Ok(())
    }
    fn psp_create_ring(&mut self) -> Result<(), Error> {
        if self.hw.read(&self.psp_register(71)?, 0)? != 0 {
            self.hw.write(
                &self.psp_register(64)?,
                0,
                self.hw.constant("GFX_CTRL_CMD_ID_DESTROY_RINGS")?,
                &[],
            )?;
            self.hw.bus.sleep(Duration::from_millis(20));
        }
        self.hw.wait_register(
            &self.psp_register(64)?,
            0,
            0x80000000,
            0x80000000,
            "sOS not ready",
        )?;
        let state = self.psp_state()?;
        let prefix = state.prefix.clone();
        let address = self.hw.paddr_to_mc(state.ring)?;
        let size = state.ring_size;
        self.hw.pair(&prefix, "_69", "_70", address, 0)?;
        self.hw
            .write(&self.psp_register(71)?, 0, size as u32, &[])?;
        self.hw.write(
            &self.psp_register(64)?,
            0,
            self.hw.constant("PSP_RING_TYPE__KM")? << 16,
            &[],
        )?;
        self.hw.bus.sleep(Duration::from_millis(20));
        self.hw.wait_register(
            &self.psp_register(64)?,
            0,
            0x8000ffff,
            0x80000000,
            "sOS ring not created",
        )
    }
    fn psp_command(&self, kind: &str) -> Result<Vec<u8>, Error> {
        let mut bytes = vec![0; self.hw.catalog.layout("struct_psp_gfx_cmd_resp")?.size];
        self.hw.catalog.write(
            "struct_psp_gfx_cmd_resp",
            &["cmd_id"],
            &mut bytes,
            u64::from(self.hw.constant(kind)?),
        )?;
        Ok(bytes)
    }
    fn psp_set(&self, command: &mut [u8], path: &[&str], value: u64) -> Result<(), Error> {
        self.hw
            .catalog
            .write("struct_psp_gfx_cmd_resp", path, command, value)
    }
    fn psp_pair(
        &self,
        command: &mut [u8],
        group: &str,
        base: &str,
        value: u64,
    ) -> Result<(), Error> {
        self.psp_set(command, &["cmd", group, &format!("{base}_hi")], value >> 32)?;
        self.psp_set(
            command,
            &["cmd", group, &format!("{base}_lo")],
            value & 0xffffffff,
        )
    }
    fn psp_submit(&mut self, command: Vec<u8>) -> Result<Vec<u8>, Error> {
        let pointer = self.hw.read(&self.psp_register(67)?, 0)?;
        let state = self.psp_state()?;
        let (cmd, fence, ring) = (state.command, state.fence, state.ring);
        let command_address = self.hw.paddr_to_mc(cmd)?;
        let fence_address = self.hw.paddr_to_mc(fence)?;
        let mut frame = vec![0; self.hw.catalog.layout("struct_psp_gfx_rb_frame")?.size];
        for (name, value) in [
            ("fence_value", u64::from(pointer) + 1),
            ("cmd_buf_addr_lo", command_address & 0xffffffff),
            ("cmd_buf_addr_hi", command_address >> 32),
            ("fence_addr_lo", fence_address & 0xffffffff),
            ("fence_addr_hi", fence_address >> 32),
        ] {
            self.hw
                .catalog
                .write("struct_psp_gfx_rb_frame", &[name], &mut frame, value)?;
        }
        self.hw.bus.write_vram(cmd, &command)?;
        self.hw
            .bus
            .write_vram(ring + u64::from(pointer) * 4, &frame)?;
        self.hw.write(
            &self.psp_register(67)?,
            0,
            pointer + (frame.len() / 4) as u32,
            &[],
        )?;
        self.hw.wait(
            10000,
            u64::from(pointer) + 1,
            "sOS ring not responding",
            |hw| hw.bus.read_vram_scalar(fence, 4),
        )?;
        let response = self.hw.bus.read_vram(cmd, command.len())?;
        let status =
            self.hw
                .catalog
                .read("struct_psp_gfx_cmd_resp", &["resp", "status"], &response)?;
        if status != 0 {
            let kind = self
                .hw
                .catalog
                .read("struct_psp_gfx_cmd_resp", &["cmd_id"], &response)?;
            return Err(Error::Protocol(format!(
                "PSP command failed {kind} {status}"
            )));
        }
        Ok(response)
    }
    fn psp_load_firmware(&mut self, segment: &Segment) -> Result<(), Error> {
        self.psp_message_data(segment.data())?;
        let address = self.psp_state()?.message_address;
        for &kind in &segment.kinds {
            let mut command = self.psp_command("GFX_CMD_ID_LOAD_IP_FW")?;
            self.psp_pair(&mut command, "cmd_load_ip_fw", "fw_phy_addr", address)?;
            self.psp_set(
                &mut command,
                &["cmd", "cmd_load_ip_fw", "fw_size"],
                segment.data().len() as u64,
            )?;
            self.psp_set(
                &mut command,
                &["cmd", "cmd_load_ip_fw", "fw_type"],
                u64::from(kind),
            )?;
            self.psp_submit(command)?;
        }
        Ok(())
    }
    fn psp_load_tmr(&mut self) -> Result<(), Error> {
        let state = self.psp_state()?;
        let address = if state.tmr == 0 {
            0
        } else {
            self.hw.paddr_to_mc(state.tmr)?
        };
        let physical = if state.tmr == 0 {
            0
        } else {
            self.hw.paddr_to_physical(state.tmr)?
        };
        let size = if state.tmr == 0 { 0 } else { state.tmr_size };
        let mut command = self.psp_command("GFX_CMD_ID_SETUP_TMR")?;
        self.psp_pair(&mut command, "cmd_setup_tmr", "buf_phy_addr", address)?;
        self.psp_pair(&mut command, "cmd_setup_tmr", "system_phy_addr", physical)?;
        self.psp_set(
            &mut command,
            &["cmd", "cmd_setup_tmr", "bitfield", "virt_phy_addr"],
            1,
        )?;
        self.psp_set(&mut command, &["cmd", "cmd_setup_tmr", "buf_size"], size)?;
        self.psp_submit(command)?;
        Ok(())
    }
    pub(crate) fn psp_partition(&mut self, mode: u32) -> Result<(), Error> {
        let mut command = self.psp_command("GFX_CMD_ID_SRIOV_SPATIAL_PART")?;
        self.psp_set(
            &mut command,
            &["cmd", "cmd_spatial_part", "mode"],
            u64::from(mode),
        )?;
        self.psp_submit(command)?;
        Ok(())
    }
    pub(crate) fn init_psp(&mut self) -> Result<(), Error> {
        let spl = if self.hw.version(15)? >= [14, 0, 0] {
            "PSP_FW_TYPE_PSP_SPL"
        } else {
            "PSP_FW_TYPE_PSP_KDB"
        };
        if !self.psp_alive()? {
            for (kind, command) in [
                ("PSP_FW_TYPE_PSP_KDB", "PSP_BL__LOAD_KEY_DATABASE"),
                (spl, "PSP_BL__LOAD_TOS_SPL_TABLE"),
                ("PSP_FW_TYPE_PSP_SYS_DRV", "PSP_BL__LOAD_SYSDRV"),
                ("PSP_FW_TYPE_PSP_SOC_DRV", "PSP_BL__LOAD_SOCDRV"),
                ("PSP_FW_TYPE_PSP_INTF_DRV", "PSP_BL__LOAD_INTFDRV"),
                ("PSP_FW_TYPE_PSP_DBG_DRV", "PSP_BL__LOAD_DBGDRV"),
                ("PSP_FW_TYPE_PSP_RAS_DRV", "PSP_BL__LOAD_RASDRV"),
                ("PSP_FW_TYPE_PSP_SOS", "PSP_BL__LOAD_SOSDRV"),
            ] {
                self.psp_load_component(self.hw.constant(kind)?, self.hw.constant(command)?)?;
            }
            let name = self.psp_register(81)?;
            self.hw.wait(10000, 1, "sOS failed to start", |hw| {
                Ok(u64::from(hw.read(&name, 0)? != 0))
            })?;
        }
        self.psp_create_ring()?;
        if let Some(toc) = self
            .firmware
            .sos
            .get(&self.hw.constant("PSP_FW_TYPE_PSP_TOC")?)
            .cloned()
        {
            self.psp_message_data(toc.data())?;
            let mut command = self.psp_command("GFX_CMD_ID_LOAD_TOC")?;
            self.psp_pair(
                &mut command,
                "cmd_load_toc",
                "toc_phy_addr",
                self.psp_state()?.message_address,
            )?;
            self.psp_set(
                &mut command,
                &["cmd", "cmd_load_toc", "toc_size"],
                toc.data().len() as u64,
            )?;
            let response = self.psp_submit(command)?;
            let size = self.hw.catalog.read(
                "struct_psp_gfx_cmd_resp",
                &["resp", "tmr_size"],
                &response,
            )?;
            if size > 0x1300000 {
                return Err(Error::Contract("PSP TMR exceeds boot reserve"));
            }
            self.psp.as_mut().unwrap().tmr_size = size;
        }
        if let Some(smu) = self.firmware.smu.clone() {
            self.psp_load_firmware(&smu)?;
        }
        let state = self.psp_state()?;
        if !state.boot_tmr || !state.autoload_tmr {
            self.psp_load_tmr()?;
        }
        for segment in self.firmware.descriptors.clone() {
            self.psp_load_firmware(&segment)?;
        }
        if self.hw.gfx()? >= [11, 0, 0] {
            let command = self.psp_command("GFX_CMD_ID_AUTOLOAD_RLC")?;
            self.psp_submit(command)?;
        } else {
            let mut segment = self
                .firmware
                .sos
                .get(&self.hw.constant("PSP_FW_TYPE_PSP_RL")?)
                .cloned()
                .ok_or(Error::Contract("PSP register-list firmware missing"))?;
            segment.kinds = vec![self.hw.constant("GFX_FW_TYPE_REG_LIST")?];
            self.psp_load_firmware(&segment)?;
        }
        Ok(())
    }
}
