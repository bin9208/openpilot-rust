use crate::{
    amd_bus::Bus,
    asic::{Asic, IhState},
    Error,
};
use std::collections::BTreeMap;
impl<B: Bus> Asic<B> {
    pub(crate) fn init_ih_software(&mut self) -> Result<(), Error> {
        let size = 256 << 10;
        let mut rings = [(0, 0); 2];
        for ring in &mut rings {
            ring.0 = self
                .memory
                .palloc(&mut self.hw, size, 4096, false, true, false)?;
            ring.1 = self
                .memory
                .palloc(&mut self.hw, 4096, 4096, false, true, false)?;
        }
        self.ih = Some(IhState { rings, size });
        Ok(())
    }
    pub(crate) fn init_ih(&mut self) -> Result<(), Error> {
        let state = self
            .ih
            .as_ref()
            .ok_or(Error::Contract("IH software not initialized"))?;
        let (rings, size) = (state.rings, state.size);
        for (index, (ring, pointer)) in rings.into_iter().enumerate() {
            let suffix = if index == 0 { "" } else { "_RING1" };
            self.hw.pair(
                "regIH_RB_BASE",
                suffix,
                &format!("_HI{suffix}"),
                self.hw.paddr_to_mc(ring)? >> 8,
                0,
            )?;
            let mut fields = vec![
                ("mc_space", 4),
                ("wptr_overflow_clear", 1),
                ("rb_size", 64 - (size / 4 - 1).leading_zeros()),
                ("mc_snoop", 1),
                ("mc_ro", 0),
                ("mc_vmid", 0),
            ];
            if index == 0 {
                fields.extend([("wptr_overflow_enable", 1), ("rptr_rearm", 1)]);
            } else {
                fields.push(("rb_full_drain_enable", 1));
            }
            self.hw
                .write(&format!("regIH_RB_CNTL{suffix}"), 0, 0, &fields)?;
            if index == 0 {
                self.hw.pair(
                    "regIH_RB_WPTR_ADDR",
                    "_LO",
                    "_HI",
                    self.hw.paddr_to_mc(pointer)?,
                    0,
                )?;
            }
            self.hw
                .write(&format!("regIH_RB_WPTR{suffix}"), 0, 0, &[])?;
            self.hw
                .write(&format!("regIH_RB_RPTR{suffix}"), 0, 0, &[])?;
            self.hw.write(
                &format!("regIH_DOORBELL_RPTR{suffix}"),
                0,
                0,
                &[("enable", 0)],
            )?;
        }
        if self.hw.version(23)? != [4, 4, 2] {
            self.hw.update(
                "regIH_STORM_CLIENT_LIST_CNTL",
                0,
                &[("client18_is_storm_client", 1)],
            )?;
            self.hw
                .update("regIH_INT_FLOOD_CNTL", 0, &[("flood_cntl_enable", 1)])?;
            self.hw.update("regIH_MSI_STORM_CTRL", 0, &[("delay", 3)])?;
        }
        self.hw
            .update("regIH_RB_CNTL", 0, &[("rb_enable", 1), ("enable_intr", 1)])?;
        self.hw
            .update("regIH_RB_CNTL_RING1", 0, &[("rb_enable", 1)])?;
        Ok(())
    }
    fn interrupt_source(&self, client: u32, source: u32) -> Result<String, Error> {
        let newer = self.hw.gfx()?[0] >= 11;
        let graphics = if newer {
            vec![
                self.hw.constant("SOC21_IH_CLIENTID_GRBM_CP")?,
                self.hw.constant("SOC21_IH_CLIENTID_GFX")?,
            ]
        } else {
            let mut values = vec![self.hw.constant("SOC15_IH_CLIENTID_GRBM_CP")?];
            for i in 0..4 {
                values.push(self.hw.constant(&format!("SOC15_IH_CLIENTID_SE{i}SH"))?);
            }
            values
        };
        let prefix = if graphics.contains(&client) {
            Some(format!("GFX_{}", self.hw.gfx()?[0]))
        } else if !newer
            && (0..8).any(|index| {
                self.hw
                    .constant(&format!("SOC15_IH_CLIENTID_SDMA{index}"))
                    .ok()
                    == Some(client)
            })
        {
            Some(format!("SDMA0_{}", self.hw.version(3)?[0]))
        } else {
            None
        };
        let Some(prefix) = prefix else {
            return Ok(String::new());
        };
        let mut result = String::new();
        for (name, value) in &self.hw.catalog.constants["am"] {
            if name.starts_with(&prefix) && value.as_u64() == Some(u64::from(source)) {
                if let Some((_, suffix)) = name.split_once("__SRCID__") {
                    result = suffix.to_owned();
                }
            }
        }
        Ok(result)
    }
    pub(crate) fn drain_interrupts(&mut self) -> Result<(), Error> {
        let size = self
            .ih
            .as_ref()
            .ok_or(Error::Contract("IH software not initialized"))?
            .size;
        let value = self.hw.read("regIH_RB_WPTR", 0)?;
        let offset = self.hw.reg("regIH_RB_WPTR")?.decode("offset", value)?;
        let overflow = self.hw.reg("regIH_RB_WPTR")?.decode("rb_overflow", value)?;
        self.hw.write(
            "regIH_RB_RPTR",
            0,
            (u64::from(offset) % (size / 4)) as u32,
            &[],
        )?;
        if overflow != 0 {
            self.hw.update("regIH_RB_WPTR", 0, &[("rb_overflow", 0)])?;
            self.hw
                .update("regIH_RB_CNTL", 0, &[("wptr_overflow_clear", 1)])?;
            self.hw
                .update("regIH_RB_CNTL", 0, &[("wptr_overflow_clear", 0)])?;
        }
        Ok(())
    }
    pub fn handle_interrupts(&mut self) -> Result<(), Error> {
        let state = self
            .ih
            .as_ref()
            .ok_or(Error::Contract("IH software not initialized"))?;
        let (ring, size) = (state.rings[0].0, state.size);
        let pointer = self.hw.read("regIH_RB_WPTR", 0)?;
        let write = self.hw.reg("regIH_RB_WPTR")?.decode("offset", pointer)?;
        let mut read = self.hw.read("regIH_RB_RPTR", 0)?;
        if u64::from(write) >= size / 4
            || u64::from(read) >= size / 4
            || write % 8 != 0
            || read % 8 != 0
        {
            return Err(Error::Contract("IH ring pointer out of range or unaligned"));
        }
        while read != write {
            let mut entry = [0u32; 8];
            for (index, value) in entry.iter_mut().enumerate() {
                *value = self.hw.bus.read_vram_scalar(
                    ring + ((u64::from(read) + index as u64) % (size / 4)) * 4,
                    4,
                )? as u32;
            }
            read = ((u64::from(read) + 8) % (size / 4)) as u32;
            let client = entry[0] & 255;
            let source = (entry[0] >> 8) & 255;
            let name = self.interrupt_source(client, source)?;
            if name == "SDMA_TRAP" || name == "CP_EOP_INTR" {
                continue;
            }
            eprintln!("am: IH ({read:#x}/{write:#x}) client={client} src={name}({source}) ring={} vmid={}({}) pasid={} node={} ctx=[{:#x}, {:#x}, {:#x}, {:#x}]",(entry[0]>>16)&255,(entry[0]>>24)&15,(entry[0]>>31)&1,entry[3]&0xffff,(entry[3]>>16)&255,entry[4],entry[5],entry[6],entry[7]);
            if name == "SQ_INTERRUPT_ID" {
                let newer = self.hw.gfx()?[0] >= 11;
                let encoding = if newer {
                    (entry[5] >> 6) & 3
                } else {
                    (entry[4] >> 26) & 3
                };
                let error = if newer {
                    (entry[4] >> 21) & 15
                } else {
                    (((entry[4] & 0xfff)
                        | ((entry[4] >> 16) & 0xf000)
                        | ((entry[5] << 16) & 0xff0000))
                        >> 20)
                        & 15
                };
                if encoding > 2 {
                    return Err(Error::Contract("unknown SQ interrupt encoding"));
                }
                eprintln!("am: sq_intr encoding={encoding} error_type={error}");
                self.error_state |= encoding == 2;
            } else if name == "UTCL2_FAULT"
                || (self.hw.gfx()?[0] == 9
                    && client == self.hw.constant("SOC15_IH_CLIENTID_UTCL2")?)
            {
                let register = self.fault_register("GC")?;
                let value = self.hw.read(&register, 0)?;
                let fields = self
                    .hw
                    .reg(&register)?
                    .fields
                    .keys()
                    .map(|field| Ok((field.clone(), self.hw.reg(&register)?.decode(field, value)?)))
                    .collect::<Result<BTreeMap<_, _>, Error>>()?;
                let upper = self.hw.read("regGCVM_L2_PROTECTION_FAULT_ADDR_HI32", 0)?;
                let lower = self.hw.read("regGCVM_L2_PROTECTION_FAULT_ADDR_LO32", 0)?;
                eprintln!(
                    "am: GCVM_L2_PROTECTION_FAULT_STATUS: {fields:?} {:#x}",
                    ((u64::from(upper) << 32) | u64::from(lower)) << 12
                );
                self.hw.update(
                    "regGCVM_L2_PROTECTION_FAULT_CNTL",
                    0,
                    &[("clear_protection_fault_status_addr", 1)],
                )?;
                self.error_state = true;
            } else {
                self.error_state = true;
            }
        }
        self.drain_interrupts()?;
        let value = self.hw.read("regBIF_BX0_BIF_DOORBELL_INT_CNTL", 0)?;
        let register = self.hw.reg("regBIF_BX0_BIF_DOORBELL_INT_CNTL")?;
        let athub = register.decode("ras_athub_err_event_interrupt_status", value)?;
        let controller = register.decode("ras_cntlr_interrupt_status", value)?;
        if athub != 0 || controller != 0 {
            eprintln!("am: fatal hardware error detected RAS_ATHUB_ERR_EVENT={athub} RAS_CNTLR={controller}");
            let mut banks = self.aca_banks(true)?;
            banks.extend(self.aca_banks(false)?);
            for registers in banks {
                eprintln!(
                    "am: ACA uncorrectable={} hwid={:#x} mcatype={:#x} regs={registers:x?}",
                    (registers[1] >> 61) & 1 != 0 && (registers[1] >> 57) & 1 != 0,
                    (registers[5] >> 32) & 0xfff,
                    (registers[5] >> 48) & 0xffff
                );
            }
            self.hw.write(
                "regBIF_BX0_BIF_DOORBELL_INT_CNTL",
                0,
                0,
                &[
                    ("ras_cntlr_interrupt_clear", controller),
                    ("ras_athub_err_event_interrupt_clear", athub),
                ],
            )?;
            self.error_state = true;
        }
        Ok(())
    }
}
