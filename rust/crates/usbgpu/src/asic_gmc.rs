use crate::{
    amd_bus::{Bus, GmcState},
    asic::Asic,
    Error,
};
impl<B: Bus> Asic<B> {
    pub(crate) fn init_gmc_software(&mut self) -> Result<(), Error> {
        let vmhubs = u8::try_from(
            self.hw
                .discovery
                .bases
                .get(&12)
                .ok_or(Error::Contract("MMHUB instances missing"))?
                .len(),
        )
        .map_err(|_| Error::Contract("too many MMHUBs"))?;
        let xccs = u8::try_from(
            self.hw
                .discovery
                .bases
                .get(&1)
                .ok_or(Error::Contract("GC instances missing"))?
                .len(),
        )
        .map_err(|_| Error::Contract("too many XCCs"))?;
        let physical_id = if self.hw.registers.contains_key("regMMMC_VM_XGMI_LFB_CNTL") {
            u64::from(
                self.hw
                    .field("regMMMC_VM_XGMI_LFB_CNTL", 0, "pf_lfb_region")?,
            )
        } else {
            0
        };
        let xgmi_segment_size = if self.hw.registers.contains_key("regMMMC_VM_XGMI_LFB_SIZE") {
            u64::from(
                self.hw
                    .field("regMMMC_VM_XGMI_LFB_SIZE", 0, "pf_lfb_size")?,
            ) << 24
        } else {
            0
        };
        let physical_base = physical_id * xgmi_segment_size;
        let framebuffer_base =
            u64::from(self.hw.read("regMMMC_VM_FB_LOCATION_BASE", 0)? & 0xffffff) << 24;
        let framebuffer_end =
            u64::from(self.hw.read("regMMMC_VM_FB_LOCATION_TOP", 0)? & 0xffffff) << 24;
        let memory_base = framebuffer_base + physical_base;
        let virtual_base = self.memory.virtual_base;
        let virtual_end = (virtual_base + (1 << 48) - 1).min(0x7fffffffffff);
        let scratch = self
            .memory
            .palloc(&mut self.hw, 4096, 4096, false, true, false)?
            + physical_base;
        let dummy = self
            .memory
            .palloc(&mut self.hw, 4096, 4096, false, true, false)?
            + physical_base;
        self.memory.policy.physical_base = physical_base;
        self.hw.gmc = Some(GmcState {
            physical_base,
            framebuffer_base,
            framebuffer_end,
            memory_base,
            virtual_base,
            virtual_end,
            scratch,
            dummy,
            mm_ready: true,
            gc_ready: false,
            vmhubs,
            xccs,
            xgmi_segment_size,
        });
        Ok(())
    }
    pub(crate) fn fault_register(&self, hub: &str) -> Result<String, Error> {
        Ok(format!(
            "reg{hub}VM_L2_PROTECTION_FAULT_STATUS{}",
            if self.hw.gfx()? >= [12, 0, 0] {
                "_LO32"
            } else {
                ""
            }
        ))
    }
    pub(crate) fn init_hub(&mut self, hub: &str, instances: u8) -> Result<(), Error> {
        let state = self.hw.gmc()?.clone();
        let older = self.hw.gfx()? < [10, 0, 0];
        for instance in 0..instances {
            for (suffix, value) in [("BASE", 0), ("BOT", 0xffffff), ("TOP", 0)] {
                self.hw
                    .write(&format!("reg{hub}MC_VM_AGP_{suffix}"), instance, value, &[])?;
            }
            self.hw.write(
                &format!("reg{hub}MC_VM_SYSTEM_APERTURE_LOW_ADDR"),
                instance,
                (state.framebuffer_base >> 18) as u32,
                &[],
            )?;
            self.hw.write(
                &format!("reg{hub}MC_VM_SYSTEM_APERTURE_HIGH_ADDR"),
                instance,
                (state.framebuffer_end >> 18) as u32,
                &[],
            )?;
            self.hw.pair(
                &format!("reg{hub}MC_VM_SYSTEM_APERTURE_DEFAULT_ADDR"),
                "_LSB",
                "_MSB",
                state.scratch >> 12,
                instance,
            )?;
            self.hw.pair(
                &format!("reg{hub}VM_L2_PROTECTION_FAULT_DEFAULT_ADDR"),
                "_LO32",
                "_HI32",
                state.dummy >> 12,
                instance,
            )?;
            self.hw.update(
                &format!("reg{hub}VM_L2_PROTECTION_FAULT_CNTL2"),
                instance,
                &[("active_page_migration_pte_read_retry", 1)],
            )?;
            self.hw.update(
                &format!("reg{hub}MC_VM_MX_L1_TLB_CNTL"),
                instance,
                &[
                    ("enable_l1_tlb", 1),
                    ("system_access_mode", 3),
                    ("enable_advanced_driver_model", 1),
                    ("system_aperture_unmapped_access", 0),
                    ("mtype", self.hw.soc_constant("MTYPE_UC")?),
                ],
            )?;
            self.hw.update(
                &format!("reg{hub}VM_L2_CNTL"),
                instance,
                &[
                    ("enable_l2_cache", 1),
                    ("enable_default_page_out_to_system_memory", 1),
                    ("l2_pde0_cache_tag_generation_mode", 0),
                    ("pde_fault_classification", 0),
                    ("context1_identity_access_mode", 1),
                    ("identity_mode_fragment_size", 0),
                    ("enable_l2_fragment_processing", u32::from(older)),
                ],
            )?;
            self.hw.update(
                &format!("reg{hub}VM_L2_CNTL2"),
                instance,
                &[("invalidate_all_l1_tlbs", 1), ("invalidate_l2_cache", 1)],
            )?;
            self.hw.write(
                &format!("reg{hub}VM_L2_CNTL3"),
                instance,
                0,
                &[
                    ("l2_cache_4k_associativity", 1),
                    ("l2_cache_bigk_associativity", 1),
                    ("bank_select", if older { 12 } else { 9 }),
                    ("l2_cache_bigk_fragment_size", if older { 9 } else { 6 }),
                ],
            )?;
            self.hw.write(
                &format!("reg{hub}VM_L2_CNTL4"),
                instance,
                0,
                &[("l2_cache_4k_partition_count", 1)],
            )?;
            if !older {
                self.hw.write(
                    &format!("reg{hub}VM_L2_CNTL5"),
                    instance,
                    0,
                    &[("walker_priority_client_id", 0x1ff)],
                )?;
            }
            self.hw.pair(
                &format!("reg{hub}VM_CONTEXT0_PAGE_TABLE_START_ADDR"),
                "_LO32",
                "_HI32",
                state.virtual_base >> 12,
                instance,
            )?;
            self.hw.pair(
                &format!("reg{hub}VM_CONTEXT0_PAGE_TABLE_END_ADDR"),
                "_LO32",
                "_HI32",
                state.virtual_end >> 12,
                instance,
            )?;
            self.hw.pair(
                &format!("reg{hub}VM_CONTEXT0_PAGE_TABLE_BASE_ADDR"),
                "_LO32",
                "_HI32",
                self.hw.paddr_to_physical(self.memory.root_address())? | 1,
                instance,
            )?;
            let mut flags = vec![
                ("enable_context".to_owned(), 1),
                ("page_table_depth".to_owned(), if older { 2 } else { 3 }),
                (
                    "page_table_block_size".to_owned(),
                    if older { 9 } else { 0 },
                ),
            ];
            for fault in [
                "pde0",
                "dummy_page",
                "range",
                "valid",
                "read",
                "write",
                "execute",
            ] {
                flags.push((format!("{fault}_protection_fault_enable_interrupt"), 1));
                flags.push((format!("{fault}_protection_fault_enable_default"), 1));
            }
            self.hw.write(
                &format!("reg{hub}VM_CONTEXT0_CNTL"),
                instance,
                0x1800000,
                &flags
                    .iter()
                    .map(|(name, value)| (name.as_str(), *value))
                    .collect::<Vec<_>>(),
            )?;
            for (name, value) in [
                ("APERTURE_LOW_ADDR", 0xfffffffff),
                ("APERTURE_HIGH_ADDR", 0),
            ] {
                self.hw.pair(
                    &format!("reg{hub}VM_L2_CONTEXT1_IDENTITY_{name}"),
                    "_LO32",
                    "_HI32",
                    value,
                    instance,
                )?;
            }
            self.hw.pair(
                &format!("reg{hub}VM_L2_CONTEXT_IDENTITY_PHYSICAL_OFFSET"),
                "_LO32",
                "_HI32",
                0,
                instance,
            )?;
            for engine in 0..18 {
                self.hw.pair(
                    &format!("reg{hub}VM_INVALIDATE_ENG{engine}_ADDR_RANGE"),
                    "_LO32",
                    "_HI32",
                    0x1fffffffff,
                    instance,
                )?;
            }
        }
        let gmc = self
            .hw
            .gmc
            .as_mut()
            .ok_or(Error::Contract("GMC state missing"))?;
        if hub == "MM" {
            gmc.mm_ready = true;
        } else {
            gmc.gc_ready = true;
        }
        Ok(())
    }
    pub(crate) fn init_soc(&mut self) -> Result<(), Error> {
        if [[7, 9, 0], [7, 9, 1]].contains(&self.hw.version(14)?) {
            self.hw.write("regXCC_DOORBELL_FENCE", 0, 0, &[])?;
            let address = self.hw.reg("regXCC_DOORBELL_FENCE")?.address(0)?;
            let value = self
                .hw
                .reg("regXCC_DOORBELL_FENCE")?
                .encode(&[("shub_slv_mode", 1)])?;
            for aid in 1..self.hw.gmc()?.vmhubs {
                self.hw.indirect_pcie_write(address, value, aid)?;
            }
            self.hw
                .write("regBIFC_GFX_INT_MONITOR_MASK", 0, 0x7ff, &[])?;
            self.hw
                .write("regBIFC_DOORBELL_ACCESS_EN_PF", 0, 0xfffff, &[])?;
        } else {
            self.hw.update(
                "regRCC_DEV0_EPF2_STRAP2",
                0,
                &[("strap_no_soft_reset_dev0_f2", 0)],
            )?;
        }
        self.hw
            .write("regRCC_DEV0_EPF0_RCC_DOORBELL_APER_EN", 0, 1, &[])
    }
    pub(crate) fn soc_clockgating(&mut self) -> Result<(), Error> {
        if self.hw.version(2)? >= [5, 2, 1] {
            self.hw.update(
                "regHDP_MEM_POWER_CTRL",
                0,
                &[
                    ("atomic_mem_power_ctrl_en", 1),
                    ("atomic_mem_power_ds_en", 1),
                ],
            )?;
        }
        Ok(())
    }
    pub(crate) fn enable_doorbell(
        &mut self,
        port: u8,
        awid: u32,
        upper_address: u32,
        offset: u32,
        size: u32,
        aid: u8,
    ) -> Result<(), Error> {
        let name = format!(
            "{}_DOORBELL_ENTRY_{port}_CTRL",
            if self.hw.gfx()? >= [12, 0, 0] {
                "regGDC_S2A0_S2A"
            } else {
                "regS2A"
            }
        );
        let fields = [
            (format!("s2a_doorbell_port{port}_enable"), 1),
            (format!("s2a_doorbell_port{port}_awid"), awid),
            (format!("s2a_doorbell_port{port}_range_size"), size),
            (
                format!("s2a_doorbell_port{port}_awaddr_31_28_value"),
                upper_address,
            ),
            (format!("s2a_doorbell_port{port}_range_offset"), offset),
        ];
        let register = self.hw.reg(&name)?;
        let value = register.encode(
            &fields
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect::<Vec<_>>(),
        )?;
        let address = register.address(0)?;
        if [[7, 9, 0], [7, 9, 1]].contains(&self.hw.version(14)?) {
            self.hw.indirect_pcie_write(address, value, aid)
        } else {
            self.hw.write(&name, 0, value, &[])
        }
    }
}
