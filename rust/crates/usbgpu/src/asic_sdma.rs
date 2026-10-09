use crate::{
    amd_bus::Bus,
    asic::{Asic, SdmaState},
    Error,
};
use std::time::Duration;
#[derive(Clone, Copy)]
pub struct CopyRing {
    pub address: u64,
    pub size: u64,
    pub read_pointer: u64,
    pub write_pointer: u64,
    pub index: u8,
}
impl<B: Bus> Asic<B> {
    pub(crate) fn init_sdma_software(&mut self) -> Result<(), Error> {
        self.sdma = Some(SdmaState {
            name: if self.hw.version(3)? < [7, 0, 0] {
                "F32"
            } else {
                "MCU"
            },
            rings: Vec::new(),
        });
        Ok(())
    }
    pub(crate) fn init_sdma(&mut self) -> Result<(), Error> {
        let version = self.hw.version(3)?;
        let name = self
            .sdma
            .as_ref()
            .ok_or(Error::Contract("SDMA software not initialized"))?
            .name;
        for index in 0..if version < [5, 0, 0] { 16 } else { 1 } {
            let (pipe, instance) = if version < [5, 0, 0] {
                (String::new(), index)
            } else {
                (index.to_string(), 0)
            };
            if version >= [6, 0, 0] {
                self.hw.update(
                    &format!("regSDMA{pipe}_WATCHDOG_CNTL"),
                    instance,
                    &[("queue_hang_count", 100)],
                )?;
                self.hw.update(
                    &format!("regSDMA{pipe}_UTCL1_CNTL"),
                    instance,
                    &[("resp_mode", 3), ("redo_delay", 9)],
                )?;
                let mut page = vec![("rd_l2_policy", 2), ("wr_l2_policy", 3)];
                if name == "F32" {
                    page.push(("llc_noalloc", 1));
                }
                self.hw
                    .update(&format!("regSDMA{pipe}_UTCL1_PAGE"), instance, &page)?;
                self.hw.update(
                    &format!("regSDMA{pipe}_{name}_CNTL"),
                    instance,
                    &[
                        ("halt", 0),
                        (if name == "F32" { "th1_reset" } else { "reset" }, 0),
                    ],
                )?;
            }
            let mut fields = vec![("trap_enable", 1)];
            if version <= [5, 2, 0] {
                fields.push(("utc_l1_enable", 1));
            }
            self.hw
                .update(&format!("regSDMA{pipe}_CNTL"), instance, &fields)?;
        }
        let base = self.hw.constant("AMDGPU_NAVI10_DOORBELL_sDMA_ENGINE0")?;
        if [[7, 9, 0], [7, 9, 1]].contains(&self.hw.version(14)?) {
            for aid in 0..4 {
                for (device, (port, awid, offset, address)) in
                    [(1, 14, 14, 1), (2, 8, 8, 2), (5, 9, 9, 8), (6, 10, 10, 9)]
                        .into_iter()
                        .enumerate()
                {
                    let entry = device as u32 + 1 + 4 * u32::from(aid);
                    self.hw.write(
                        &format!("regDOORBELL0_CTRL_ENTRY_{entry}"),
                        0,
                        0,
                        &[
                            (&format!("bif_doorbell{entry}_range_size_entry"), 20),
                            (
                                &format!("bif_doorbell{entry}_range_offset_entry"),
                                (base + (entry - 1) * 10) * 2,
                            ),
                        ],
                    )?;
                    self.enable_doorbell(port, awid, address, offset, 4, aid)?;
                }
            }
        } else {
            self.enable_doorbell(2, 14, 3, base * 2, 4, 0)?;
        }
        Ok(())
    }
    pub(crate) fn stop_sdma(&mut self) -> Result<(), Error> {
        let rings = self
            .sdma
            .as_ref()
            .ok_or(Error::Contract("SDMA software not initialized"))?
            .rings
            .clone();
        for (register, instance) in rings {
            self.hw.update(
                &format!("{register}_RB_CNTL"),
                instance,
                &[("rb_enable", 0)],
            )?;
            self.hw.update(
                &format!("{register}_IB_CNTL"),
                instance,
                &[("ib_enable", 0)],
            )?;
            self.hw
                .update(&format!("{register}_DOORBELL"), instance, &[("enable", 0)])?;
            self.hw.update(
                &format!("{register}_DOORBELL_OFFSET"),
                instance,
                &[("offset", 0)],
            )?;
        }
        if self.hw.version(3)? >= [6, 0, 0] {
            self.hw
                .write("regGRBM_SOFT_RESET", 0, 0, &[("soft_reset_sdma0", 1)])?;
            self.hw.bus.sleep(Duration::from_millis(10));
            self.hw.write("regGRBM_SOFT_RESET", 0, 0, &[])?;
        }
        Ok(())
    }
    pub fn setup_copy_ring(&mut self, ring: CopyRing) -> Result<u32, Error> {
        let version = self.hw.version(3)?;
        if version >= [5, 0, 0] && ring.index > 0 {
            return Err(Error::Contract("SDMA queue is not available"));
        }
        if ring.size < 4 {
            return Err(Error::Contract("SDMA ring is too small"));
        }
        let pipe = ring.index / 4;
        let queue = ring.index % 4;
        let (register, instance) = if version[..2] == [4, 4] {
            ("regSDMA_GFX".to_owned(), pipe + queue * 4)
        } else {
            (format!("regSDMA{pipe}_QUEUE{queue}"), 0)
        };
        let doorbell = self.hw.constant("AMDGPU_NAVI10_DOORBELL_sDMA_ENGINE0")?
            + u32::from(pipe + queue * 4) * 10;
        let state = self
            .sdma
            .as_mut()
            .ok_or(Error::Contract("SDMA state missing"))?;
        state.rings.push((register.clone(), instance));
        let name = state.name;
        self.hw
            .write(&format!("{register}_MINOR_PTR_UPDATE"), instance, 1, &[])?;
        self.hw
            .pair(&format!("{register}_RB_RPTR"), "", "_HI", 0, instance)?;
        self.hw
            .pair(&format!("{register}_RB_WPTR"), "", "_HI", 0, instance)?;
        self.hw.pair(
            &format!("{register}_RB_BASE"),
            "",
            "_HI",
            ring.address >> 8,
            instance,
        )?;
        self.hw.pair(
            &format!("{register}_RB_RPTR_ADDR"),
            "_LO",
            "_HI",
            ring.read_pointer,
            instance,
        )?;
        self.hw.pair(
            &format!("{register}_RB_WPTR_POLL_ADDR"),
            "_LO",
            "_HI",
            ring.write_pointer,
            instance,
        )?;
        self.hw.update(
            &format!("{register}_DOORBELL_OFFSET"),
            instance,
            &[("offset", doorbell * 2)],
        )?;
        self.hw
            .update(&format!("{register}_DOORBELL"), instance, &[("enable", 1)])?;
        self.hw
            .write(&format!("{register}_MINOR_PTR_UPDATE"), instance, 0, &[])?;
        let mut fields = vec![
            ("rb_vmid".to_owned(), 0),
            ("rptr_writeback_enable".to_owned(), 1),
            ("rptr_writeback_timer".to_owned(), 4),
            ("rb_enable".to_owned(), 1),
            ("rb_priv".to_owned(), 1),
            ("rb_size".to_owned(), 63 - (ring.size / 4).leading_zeros()),
        ];
        if version[..2] != [4, 4] {
            fields.push((format!("{}_wptr_poll_enable", name.to_lowercase()), 1));
        }
        self.hw.write(
            &format!("{register}_RB_CNTL"),
            instance,
            0,
            &fields
                .iter()
                .map(|(name, value)| (name.as_str(), *value))
                .collect::<Vec<_>>(),
        )?;
        self.hw.update(
            &format!("{register}_IB_CNTL"),
            instance,
            &[("ib_enable", 1)],
        )?;
        Ok(doorbell)
    }
}
