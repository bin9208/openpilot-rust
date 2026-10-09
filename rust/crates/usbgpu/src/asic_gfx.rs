use crate::{
    amd_bus::Bus,
    asic::{Asic, GfxState},
    Error,
};
use std::time::Duration;
#[derive(Clone, Copy)]
pub struct ComputeRing {
    pub address: u64,
    pub size: u64,
    pub read_pointer: u64,
    pub write_pointer: u64,
    pub eop: u64,
    pub eop_size: u64,
    pub index: u8,
    pub aql: bool,
}
impl<B: Bus> Asic<B> {
    pub(crate) fn init_gfx_software(&mut self) -> Result<(), Error> {
        let xccs = self.hw.gmc()?.xccs;
        let size = 4096 * u64::from(xccs);
        let first = self
            .memory
            .palloc(&mut self.hw, size, 4096, false, true, false)?;
        let second = self
            .memory
            .palloc(&mut self.hw, size, 4096, false, true, false)?;
        self.gfx = Some(GfxState {
            mqds: [first, second],
            xccs,
        });
        Ok(())
    }
    fn gfx_instances(&self) -> Result<u8, Error> {
        Ok(self
            .gfx
            .as_ref()
            .ok_or(Error::Contract("GFX software not initialized"))?
            .xccs)
    }
    fn grbm(
        &mut self,
        me: u32,
        pipe: u32,
        queue: u32,
        vmid: u32,
        instance: u8,
    ) -> Result<(), Error> {
        self.hw.write(
            "regGRBM_GFX_CNTL",
            instance,
            0,
            &[
                ("meid", me),
                ("pipeid", pipe),
                ("vmid", vmid),
                ("queueid", queue),
            ],
        )
    }
    fn configure_engine(
        &mut self,
        engine: &str,
        control: &str,
        program: &str,
        me: u32,
        instance: u8,
    ) -> Result<(), Error> {
        let address = *self
            .firmware
            .ucode_start
            .get(engine)
            .ok_or(Error::Contract("MEC firmware entry address missing"))?;
        self.grbm(me, 0, 0, 0, instance)?;
        self.hw.pair(
            &format!("regCP_{program}_PRGRM_CNTR_START"),
            "",
            "_HI",
            address >> 2,
            instance,
        )?;
        self.grbm(0, 0, 0, 0, instance)?;
        let field = format!("{}_pipe0_reset", engine.to_lowercase());
        self.hw
            .update(&format!("regCP_{control}_CNTL"), instance, &[(&field, 1)])?;
        self.hw
            .update(&format!("regCP_{control}_CNTL"), instance, &[(&field, 0)])
    }
    fn configure_mec(&mut self) -> Result<(), Error> {
        for instance in 0..self.gfx_instances()? {
            if self.hw.gfx()? < [10, 0, 0] {
                self.hw.update(
                    "regCP_MEC_CNTL",
                    instance,
                    &[
                        ("mec_invalidate_icache", 1),
                        ("mec_me1_pipe0_reset", 1),
                        ("mec_me2_pipe0_reset", 1),
                        ("mec_me1_halt", 1),
                        ("mec_me2_halt", 1),
                    ],
                )?;
            }
            if self.hw.gfx()? >= [12, 0, 0] {
                self.configure_engine("PFP", "ME", "PFP", 0, instance)?;
                self.configure_engine("ME", "ME", "ME", 0, instance)?;
            }
            if self.hw.gfx()? >= [10, 0, 0] {
                self.configure_engine("MEC", "MEC_RS64", "MEC_RS64", 1, instance)?;
            }
        }
        Ok(())
    }
    fn enable_mec(&mut self) -> Result<(), Error> {
        for instance in 0..self.gfx_instances()? {
            if self.hw.gfx()? >= [10, 0, 0] {
                self.hw.update(
                    "regCP_MEC_RS64_CNTL",
                    instance,
                    &[
                        ("mec_pipe0_reset", 0),
                        ("mec_pipe0_active", 1),
                        ("mec_halt", 0),
                    ],
                )?;
            } else {
                self.hw.write("regCP_MEC_CNTL", instance, 0, &[])?;
            }
        }
        self.hw.bus.sleep(Duration::from_millis(50));
        Ok(())
    }
    pub(crate) fn dequeue_compute(&mut self) -> Result<(), Error> {
        for queue in 0..2 {
            for instance in 0..self.gfx_instances()? {
                self.grbm(1, 0, queue, 0, instance)?;
                if self.hw.read("regCP_HQD_ACTIVE", instance)? & 1 != 0 {
                    self.hw
                        .write("regCP_HQD_DEQUEUE_REQUEST", instance, 2, &[])?;
                    self.hw
                        .write("regSPI_COMPUTE_QUEUE_RESET", instance, 1, &[])?;
                    if !self.error_state {
                        self.hw.wait_register(
                            "regCP_HQD_ACTIVE",
                            instance,
                            1,
                            0,
                            "HQD dequeue timeout",
                        )?;
                    }
                }
            }
        }
        self.grbm(0, 0, 0, 0, 0)
    }
    pub(crate) fn reset_mec(&mut self) -> Result<(), Error> {
        self.dequeue_compute()?;
        if self.hw.gfx()? < [12, 0, 0] {
            for instance in 0..self.gfx_instances()? {
                self.hw.write(
                    "regGRBM_SOFT_RESET",
                    instance,
                    0,
                    &[("soft_reset_cp", 1), ("soft_reset_cpc", 1)],
                )?;
            }
            self.hw.bus.sleep(Duration::from_millis(50));
            for instance in 0..self.gfx_instances()? {
                self.hw.write("regGRBM_SOFT_RESET", instance, 0, &[])?;
            }
        }
        self.configure_mec()?;
        self.enable_mec()
    }
    pub(crate) fn init_gfx(&mut self) -> Result<(), Error> {
        self.hw.wait(10000, 1, "RLC autoload timeout", |hw| {
            Ok(u64::from(
                hw.read("regCP_STAT", 0)? == 0
                    || hw.field("regRLC_RLCS_BOOTLOAD_STATUS", 0, "bootload_complete")? == 0,
            ))
        })?;
        let xccs = self.gfx_instances()?;
        self.init_hub("GC", xccs)?;
        if self.partial_boot {
            return self.reset_mec();
        }
        self.configure_mec()?;
        for instance in 0..xccs {
            let value = self.hw.read("regTCP_CNTL", 0)? | 0x20000000;
            self.hw.write("regTCP_CNTL", instance, value, &[])?;
        }
        for instance in 0..xccs {
            self.hw.write("regRLC_CNTL", instance, 1, &[])?;
        }
        for instance in 0..xccs {
            self.hw.update(
                "regRLC_SRM_CNTL",
                instance,
                &[("srm_enable", 1), ("auto_incr_addr", 1)],
            )?;
        }
        for instance in 0..xccs {
            self.hw.write("regRLC_SPM_MC_CNTL", instance, 15, &[])?;
        }
        if self.hw.version(14)?[..2] != [7, 9] {
            self.enable_doorbell(0, 3, 3, 0, 0, 0)?;
            self.enable_doorbell(3, 6, 3, 0, 0, 0)?;
        }
        for instance in 0..xccs {
            if [[9, 4, 3], [9, 5, 0]].contains(&self.hw.gfx()?) {
                self.hw
                    .write("regGB_ADDR_CONFIG", instance, 0x2a114042, &[])?;
                self.hw
                    .update("regTCP_UTCL1_CNTL2", instance, &[("spare", 1)])?;
            }
            self.hw
                .update("regGRBM_CNTL", instance, &[("read_timeout", 255)])?;
            for vmid in 0..16 {
                self.grbm(0, 0, 0, vmid, instance)?;
                let mut fields = vec![
                    (
                        "address_mode",
                        self.hw.soc_constant("SH_MEM_ADDRESS_MODE_64")?,
                    ),
                    (
                        "alignment_mode",
                        self.hw.soc_constant("SH_MEM_ALIGNMENT_MODE_UNALIGNED")?,
                    ),
                ];
                if self.hw.gfx()?[0] >= 10 {
                    fields.push(("initial_inst_prefetch", 3));
                } else {
                    fields.push(("retry_disable", 1));
                }
                if self.hw.gfx()?[..2] == [9, 4] {
                    fields.push(("f8_mode", 1));
                }
                self.hw.write("regSH_MEM_CONFIG", instance, 0, &fields)?;
                self.hw.write(
                    "regSH_MEM_BASES",
                    instance,
                    0,
                    &[("shared_base", 1), ("private_base", 2)],
                )?;
            }
            self.grbm(0, 0, 0, 0, instance)?;
            self.hw.write(
                "regCP_MEC_DOORBELL_RANGE_LOWER",
                instance,
                0x100 * u32::from(instance),
                &[],
            )?;
            self.hw.write(
                "regCP_MEC_DOORBELL_RANGE_UPPER",
                instance,
                0x100 * u32::from(instance) + 0xf8,
                &[],
            )?;
        }
        self.enable_mec()?;
        if xccs > 1 {
            self.psp_partition(1)?;
        }
        Ok(())
    }
    pub(crate) fn gfx_clockgating(&mut self) -> Result<(), Error> {
        if self.hw.registers.contains_key("regMM_ATC_L2_MISC_CG") {
            self.hw.write(
                "regMM_ATC_L2_MISC_CG",
                0,
                0,
                &[("enable", 1), ("mem_ls_enable", 1)],
            )?;
        }
        for instance in 0..self.gfx_instances()? {
            self.hw.write(
                "regRLC_SAFE_MODE",
                instance,
                0,
                &[("message", 1), ("cmd", 1)],
            )?;
            self.hw
                .wait_register("regRLC_SAFE_MODE", instance, 1, 0, "RLC safe mode timeout")?;
            self.hw.update(
                "regRLC_CGCG_CGLS_CTRL",
                instance,
                &[
                    ("cgcg_gfx_idle_threshold", 0x36),
                    ("cgcg_en", 1),
                    ("cgls_rep_compansat_delay", 15),
                    ("cgls_en", 1),
                ],
            )?;
            self.hw.update(
                "regCP_RB_WPTR_POLL_CNTL",
                instance,
                &[("poll_frequency", 0x100), ("idle_poll_count", 0x90)],
            )?;
            self.hw.update(
                "regCP_INT_CNTL",
                instance,
                &[
                    ("cntx_busy_int_enable", 1),
                    ("cntx_empty_int_enable", 1),
                    ("cmp_busy_int_enable", 1),
                ],
            )?;
            if self.hw.gfx()? >= [10, 0, 0] {
                self.hw.update(
                    "regSDMA0_RLC_CGCG_CTRL",
                    instance,
                    &[("cgcg_int_enable", 1)],
                )?;
                self.hw.update(
                    "regSDMA1_RLC_CGCG_CTRL",
                    instance,
                    &[("cgcg_int_enable", 1)],
                )?;
            }
            let mut fields = vec![
                ("gfxip_fgcg_override", 0),
                ("grbm_cgtt_sclk_override", 0),
                ("rlc_cgtt_sclk_override", 0),
                ("gfxip_mgcg_override", 0),
                ("gfxip_cgls_override", 0),
                ("gfxip_cgcg_override", 0),
            ];
            if self.hw.gfx()?[0] == 9 {
                fields.extend([("gfxip_mgls_override", 0), ("gfxip_rep_fgcg_override", 0)]);
            }
            if self.hw.gfx()?[0] >= 11 {
                fields.extend([
                    ("perfmon_clock_state", 1),
                    ("gfxip_repeater_fgcg_override", 0),
                ]);
            }
            self.hw
                .update("regRLC_CGTT_MGCG_OVERRIDE", instance, &fields)?;
            self.hw.write(
                "regRLC_SAFE_MODE",
                instance,
                0,
                &[("message", 0), ("cmd", 1)],
            )?;
        }
        Ok(())
    }
    pub fn setup_compute_ring(&mut self, ring: ComputeRing) -> Result<u32, Error> {
        let pipe = u32::from(ring.index / 4);
        let queue = usize::from(ring.index % 4);
        let doorbell = self.hw.constant("AMDGPU_NAVI10_DOORBELL_MEC_RING0")?;
        let state = self
            .gfx
            .as_ref()
            .ok_or(Error::Contract("GFX state missing"))?;
        let mqd = *state
            .mqds
            .get(queue)
            .ok_or(Error::Contract("compute queue exceeds allocated MQDs"))?;
        let xccs = state.xccs;
        let major = self.hw.gfx()?[0];
        let layout = format!(
            "struct_v{major}{}_mqd",
            if major >= 10 { "_compute" } else { "" }
        );
        if ring.size < 8 || ring.eop_size < 8 {
            return Err(Error::Contract("compute ring/eop is too small"));
        }
        for instance in 0..if ring.aql { xccs } else { 1 } {
            self.grbm(1, pipe, queue as u32, 0, instance)?;
            let physical = mqd + 4096 * u64::from(instance);
            let address = self.hw.paddr_to_mc(physical)?;
            let mut buffer = vec![0; self.hw.catalog.layout(&layout)?.size];
            let mut fields = vec![
                ("header", 0xc0310800),
                ("cp_mqd_base_addr_lo", address as u32),
                ("cp_mqd_base_addr_hi", (address >> 32) as u32),
                ("cp_hqd_pipe_priority", 2),
                ("cp_hqd_queue_priority", 15),
                ("cp_hqd_quantum", 0x111),
                (
                    "cp_hqd_persistent_state",
                    self.hw
                        .reg("regCP_HQD_PERSISTENT_STATE")?
                        .encode(&[("preload_size", 0x55), ("preload_req", 1)])?,
                ),
                ("cp_hqd_pq_base_lo", (ring.address >> 8) as u32),
                ("cp_hqd_pq_base_hi", (ring.address >> 40) as u32),
                ("cp_hqd_pq_rptr_report_addr_lo", ring.read_pointer as u32),
                (
                    "cp_hqd_pq_rptr_report_addr_hi",
                    (ring.read_pointer >> 32) as u32,
                ),
                ("cp_hqd_pq_wptr_poll_addr_lo", ring.write_pointer as u32),
                (
                    "cp_hqd_pq_wptr_poll_addr_hi",
                    (ring.write_pointer >> 32) as u32,
                ),
                (
                    "cp_hqd_pq_doorbell_control",
                    self.hw
                        .reg("regCP_HQD_PQ_DOORBELL_CONTROL")?
                        .encode(&[("doorbell_offset", doorbell * 2), ("doorbell_en", 1)])?,
                ),
                (
                    "cp_hqd_ib_control",
                    self.hw
                        .reg("regCP_HQD_IB_CONTROL")?
                        .encode(&[("min_ib_avail_size", 3)])?,
                ),
                ("cp_hqd_hq_status0", 0x20004000),
                (
                    "cp_mqd_control",
                    self.hw
                        .reg("regCP_MQD_CONTROL")?
                        .encode(&[("priv_state", 1)])?,
                ),
                ("cp_hqd_vmid", 0),
                ("cp_hqd_aql_control", u32::from(ring.aql)),
                ("cp_hqd_eop_base_addr_lo", (ring.eop >> 8) as u32),
                ("cp_hqd_eop_base_addr_hi", (ring.eop >> 40) as u32),
                (
                    "cp_hqd_eop_control",
                    self.hw
                        .reg("regCP_HQD_EOP_CONTROL")?
                        .encode(&[("eop_size", 62 - (ring.eop_size / 4).leading_zeros())])?,
                ),
            ];
            let mut pq = vec![
                ("rptr_block_size", 5),
                ("unord_dispatch", 0),
                ("queue_size", 62 - (ring.size / 4).leading_zeros()),
            ];
            if ring.aql {
                pq.extend([
                    ("queue_full_en", 1),
                    ("slot_based_wptr", 2),
                    ("no_update_rptr", u32::from(instance != 0 || xccs == 1)),
                ]);
            }
            fields.push((
                "cp_hqd_pq_control",
                self.hw.reg("regCP_HQD_PQ_CONTROL")?.encode(&pq)?,
            ));
            if ring.aql && xccs > 1 {
                fields.extend([
                    ("compute_tg_chunk_size", 1),
                    ("compute_current_logic_xcc_id", u32::from(instance)),
                    ("cp_mqd_stride_size", 4096),
                ]);
            }
            for (name, value) in fields {
                self.hw
                    .catalog
                    .write(&layout, &[name], &mut buffer, u64::from(value))?;
            }
            for engine in 0..if major >= 10 { 8 } else { 4 } {
                self.hw.catalog.write(
                    &layout,
                    &[&format!("compute_static_thread_mgmt_se{engine}")],
                    &mut buffer,
                    u64::from(u32::MAX),
                )?;
            }
            self.hw.bus.write_vram(physical, &buffer)?;
            let start = self.hw.reg("regCP_MQD_BASE_ADDR")?.address(instance)?;
            let end = self.hw.reg("regCP_HQD_PQ_WPTR_HI")?.address(instance)?;
            for (index, register) in (start..=end).enumerate() {
                let offset = (0x80 + index) * 4;
                let data = buffer
                    .get(offset..offset + 4)
                    .ok_or(Error::Contract("MQD register span exceeds structure"))?;
                self.hw
                    .raw_write(register, u32::from_le_bytes(data.try_into().unwrap()))?;
            }
            self.hw.write("regCP_HQD_ACTIVE", instance, 1, &[])?;
            self.hw.flush_hdp()?;
            self.grbm(0, 0, 0, 0, instance)?;
        }
        Ok(doorbell)
    }
}
