use crate::{
    amd_metadata::{Catalog, Register},
    Error,
};
use std::collections::BTreeMap;

pub fn words64(value: u64) -> [u32; 2] {
    [value as u32, (value >> 32) as u32]
}
pub fn packet3(op: u32, count: usize) -> u32 {
    (3 << 30) | ((op & 255) << 8) | (((count as u32) & 0x3fff) << 16)
}

pub struct Dispatch<'a> {
    pub descriptor: &'a crate::kernel::Descriptor,
    pub program_base: u64,
    pub arguments: u64,
    pub global: [u32; 3],
    pub local: [u32; 3],
    pub scratch_address: u64,
    pub scratch_bytes: u64,
    pub tmpring: u32,
    pub waves_per_shader: u32,
}

pub struct ComputePackets<'a> {
    pub words: Vec<u32>,
    pub catalog: &'a Catalog,
    pub registers: &'a BTreeMap<String, Register>,
    pub gfx_major: u8,
    pub xccs: u8,
}
#[derive(Clone, Copy)]
pub struct CachePolicy {
    pub gli: u32,
    pub glm: u32,
    pub glk: u32,
    pub glv: u32,
    pub gl1: u32,
    pub gl2: u32,
}
impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            gli: 1,
            glm: 1,
            glk: 1,
            glv: 1,
            gl1: 1,
            gl2: 1,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Release {
    pub address: u64,
    pub value: u64,
    pub data_select: u32,
    pub interrupt_select: u32,
    pub context: u32,
    pub flush: bool,
}
impl<'a> ComputePackets<'a> {
    pub fn new(
        catalog: &'a Catalog,
        registers: &'a BTreeMap<String, Register>,
        gfx_major: u8,
        xccs: u8,
    ) -> Self {
        Self {
            words: Vec::new(),
            catalog,
            registers,
            gfx_major,
            xccs,
        }
    }
    fn module(&self) -> &'static str {
        if self.gfx_major == 9 {
            "pm4_soc15"
        } else {
            "pm4_nv"
        }
    }
    fn constant(&self, name: &str) -> Result<u32, Error> {
        Ok(self.catalog.constant(self.module(), name)? as u32)
    }
    fn field(&self, name: &str, value: u32) -> Result<u32, Error> {
        self.catalog.encode_macro(self.module(), name, value)
    }
    pub fn packet(&mut self, name: &str, values: &[u32]) -> Result<(), Error> {
        if values.is_empty() || values.len() > 0x4000 {
            return Err(Error::Contract("invalid PM4 packet length"));
        }
        self.words
            .push(packet3(self.constant(name)?, values.len() - 1));
        self.words.extend_from_slice(values);
        Ok(())
    }
    pub fn register(&self, name: &str) -> Result<&Register, Error> {
        self.registers
            .get(name)
            .or_else(|| self.registers.get(&name.replace("reg", "mm")))
            .ok_or_else(|| Error::Protocol(format!("unknown queue register {name}")))
    }
    pub fn write_register(&mut self, name: &str, values: &[u32]) -> Result<(), Error> {
        let address = self.register(name)?.address(0)?;
        let sh = u64::from(self.constant("PACKET3_SET_SH_REG_START")?);
        let uconfig = u64::from(self.constant("PACKET3_SET_UCONFIG_REG_START")?);
        let (op, base) =
            if (sh..u64::from(self.constant("PACKET3_SET_SH_REG_END")?)).contains(&address) {
                ("PACKET3_SET_SH_REG", sh)
            } else if (uconfig..uconfig + 65535).contains(&address) {
                ("PACKET3_SET_UCONFIG_REG", uconfig)
            } else {
                return Err(Error::Contract("register is not writable through PM4"));
            };
        let mut args = vec![(address - base) as u32];
        args.extend_from_slice(values);
        self.packet(op, &args)
    }
    pub fn write_fields(&mut self, name: &str, values: &[(&str, u32)]) -> Result<(), Error> {
        self.write_register(name, &[self.register(name)?.encode(values)?])
    }
    pub fn wait(
        &mut self,
        address: u64,
        value: u32,
        mask: u32,
        operation: u32,
    ) -> Result<(), Error> {
        self.wait_register_memory(Some(address), 0, 0, value, mask, operation)
    }
    pub fn wait_register_memory(
        &mut self,
        memory: Option<u64>,
        register: u32,
        done: u32,
        value: u32,
        mask: u32,
        operation: u32,
    ) -> Result<(), Error> {
        let flags = self.field("WAIT_REG_MEM_MEM_SPACE", u32::from(memory.is_some()))?
            | self.field(
                "WAIT_REG_MEM_OPERATION",
                u32::from(memory.is_none() && done > 0),
            )?
            | self.field("WAIT_REG_MEM_FUNCTION", operation)?
            | self.field("WAIT_REG_MEM_ENGINE", 0)?;
        let pair = memory.map(words64).unwrap_or([register, done]);
        self.packet(
            "PACKET3_WAIT_REG_MEM",
            &[flags, pair[0], pair[1], value, mask, 4],
        )
    }
    pub fn acquire(&mut self, address: u64, size: u64, cache: CachePolicy) -> Result<(), Error> {
        let mut flags = 0;
        if self.gfx_major == 9 {
            for (name, value) in [
                ("SH_ICACHE_ACTION_ENA", cache.gli),
                ("SH_KCACHE_ACTION_ENA", cache.glk),
                ("TC_ACTION_ENA", cache.gl2),
                ("TCL1_ACTION_ENA", cache.gl1),
                ("TC_WB_ACTION_ENA", cache.gl2),
            ] {
                flags |= self.field(&format!("PACKET3_ACQUIRE_MEM_CP_COHER_CNTL_{name}"), value)?;
            }
            let sz = words64(size);
            let addr = words64(address);
            self.packet(
                "PACKET3_ACQUIRE_MEM",
                &[flags, sz[0], sz[1], addr[0], addr[1], 10],
            )
        } else {
            for (name, value) in [
                ("GLI_INV", cache.gli),
                ("GLM_INV", cache.glm),
                ("GLM_WB", cache.glm),
                ("GLK_INV", cache.glk),
                ("GLK_WB", cache.glk),
                ("GLV_INV", cache.glv),
                ("GL1_INV", cache.gl1),
                ("GL2_INV", cache.gl2),
                ("GL2_WB", cache.gl2),
            ] {
                flags |= self.field(&format!("PACKET3_ACQUIRE_MEM_GCR_CNTL_{name}"), value)?;
            }
            let sz = words64(size);
            let addr = words64(address);
            self.packet(
                "PACKET3_ACQUIRE_MEM",
                &[0, sz[0], sz[1], addr[0], addr[1], 0, flags],
            )
        }
    }
    pub fn release(&mut self, release: Release) -> Result<(), Error> {
        let mut cache = 0;
        let (event, select, context) = if self.gfx_major == 9 {
            if release.flush {
                cache =
                    self.constant("EOP_TC_WB_ACTION_EN")? | self.constant("EOP_TC_NC_ACTION_EN")?;
            }
            (
                self.field("EVENT_TYPE", self.constant("CACHE_FLUSH_AND_INV_TS_EVENT")?)?
                    | self.field(
                        "EVENT_INDEX",
                        self.constant("event_index__mec_release_mem__end_of_pipe")?,
                    )?,
                self.field("DATA_SEL", release.data_select)?
                    | self.field("INT_SEL", release.interrupt_select)?,
                0,
            )
        } else {
            if release.flush {
                for flag in [
                    "GLV_INV", "GL1_INV", "GL2_INV", "GLM_WB", "GLM_INV", "GL2_WB", "SEQ",
                ] {
                    cache |= self.constant(&format!("PACKET3_RELEASE_MEM_GCR_{flag}"))?;
                }
            }
            (
                self.field(
                    "PACKET3_RELEASE_MEM_EVENT_TYPE",
                    self.constant("CACHE_FLUSH_AND_INV_TS_EVENT")?,
                )? | self.field(
                    "PACKET3_RELEASE_MEM_EVENT_INDEX",
                    self.constant("event_index__mec_release_mem__end_of_pipe")?,
                )?,
                self.field("PACKET3_RELEASE_MEM_DATA_SEL", release.data_select)?
                    | self.field("PACKET3_RELEASE_MEM_INT_SEL", release.interrupt_select)?
                    | self.field("PACKET3_RELEASE_MEM_DST_SEL", 0)?,
                release.context,
            )
        };
        let address = words64(release.address);
        let value = words64(release.value);
        self.packet(
            "PACKET3_RELEASE_MEM",
            &[
                event | cache,
                select,
                address[0],
                address[1],
                value[0],
                value[1],
                context,
            ],
        )
    }
    pub fn predicated(
        &mut self,
        mask: u32,
        operation: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.xccs == 1 {
            return operation(self);
        }
        self.packet("PACKET3_PRED_EXEC", &[mask << 24])?;
        let start = self.words.len();
        operation(self)?;
        self.words[start - 1] |= (self.words.len() - start) as u32;
        Ok(())
    }
    pub fn signal(&mut self, address: u64, value: u32) -> Result<(), Error> {
        self.predicated(1, |queue| {
            queue.release(Release {
                address,
                value: u64::from(value),
                data_select: queue.constant("data_sel__mec_release_mem__send_32_bit_low")?,
                interrupt_select: queue
                    .constant("int_sel__mec_release_mem__send_interrupt_after_write_confirm")?,
                context: 0,
                flush: true,
            })
        })
    }
    pub fn timestamp(&mut self, address: u64) -> Result<(), Error> {
        self.predicated(1, |queue| {
            queue.release(Release {
                address: 0,
                value: 0,
                data_select: 0,
                interrupt_select: 2,
                context: 0,
                flush: false,
            })?;
            queue.release(Release {
                address,
                value: 0,
                data_select: queue.constant("data_sel__mec_release_mem__send_gpu_clock_counter")?,
                interrupt_select: queue.constant("int_sel__mec_release_mem__none")?,
                context: 0,
                flush: false,
            })?;
            queue.acquire(0, u64::MAX, CachePolicy::default())
        })
    }
    pub fn write(&mut self, address: u64, value: u64, wide: bool) -> Result<(), Error> {
        self.release(Release {
            address,
            value,
            data_select: self.constant(if wide {
                "data_sel__mec_release_mem__send_64_bit_data"
            } else {
                "data_sel__mec_release_mem__send_32_bit_low"
            })?,
            interrupt_select: self.constant("int_sel__mec_release_mem__none")?,
            context: 0,
            flush: false,
        })
    }
    pub fn dispatch(&mut self, launch: Dispatch<'_>) -> Result<(), Error> {
        let descriptor = launch.descriptor;
        self.acquire(
            0,
            u64::MAX,
            CachePolicy {
                gli: 0,
                gl2: 0,
                ..CachePolicy::default()
            },
        )?;
        let mut user = Vec::new();
        if descriptor.private_segment_sgpr {
            if self.xccs != 1 {
                return Err(Error::Contract(
                    "private segment SGPR is unsupported with multiple XCCs",
                ));
            }
            let pair = words64(launch.scratch_address);
            user.extend_from_slice(&[pair[0], pair[1] | 1 << 31, u32::MAX, 0x20c14000]);
        }
        if descriptor.dispatch_pointer {
            user.extend_from_slice(&words64(
                launch
                    .arguments
                    .checked_add(u64::from(descriptor.kernargs_segment_size))
                    .ok_or(Error::Contract("dispatch packet address overflow"))?,
            ));
        }
        user.extend_from_slice(&words64(launch.arguments));
        let entry = launch
            .program_base
            .checked_add(descriptor.entry_offset)
            .ok_or(Error::Contract("GPU program address overflow"))?;
        self.write_register("regCOMPUTE_PGM_LO", &words64(entry >> 8))?;
        self.write_register("regCOMPUTE_PGM_RSRC1", &descriptor.resources[..2])?;
        self.write_register("regCOMPUTE_PGM_RSRC3", &descriptor.resources[2..])?;
        self.write_register("regCOMPUTE_TMPRING_SIZE", &[launch.tmpring])?;
        if self.xccs == 0 || self.xccs > 32 {
            return Err(Error::Contract("invalid compute XCC count"));
        }
        for xcc in 0..self.xccs {
            let base = launch
                .scratch_address
                .checked_add(launch.scratch_bytes / u64::from(self.xccs) * u64::from(xcc))
                .ok_or(Error::Contract("GPU scratch address overflow"))?;
            self.predicated(1 << xcc, |queue| {
                queue.write_register("regCOMPUTE_DISPATCH_SCRATCH_BASE_LO", &words64(base >> 8))
            })?;
        }
        self.write_register("regCOMPUTE_RESTART_X", &[0, 0, 0])?;
        self.write_register("regCOMPUTE_USER_DATA_0", &user)?;
        self.write_fields(
            "regCOMPUTE_RESOURCE_LIMITS",
            &[("waves_per_sh", launch.waves_per_shader)],
        )?;
        self.write_register(
            "regCOMPUTE_START_X",
            &[
                0,
                0,
                0,
                launch.local[0],
                launch.local[1],
                launch.local[2],
                0,
                0,
            ],
        )?;
        let mut fields = vec![("force_start_at_000", 1), ("compute_shader_en", 1)];
        if self.gfx_major != 9 {
            fields.push(("cs_w32_en", u32::from(descriptor.wave32)));
        }
        let flags = self
            .register("regCOMPUTE_DISPATCH_INITIATOR")?
            .encode(&fields)?;
        self.packet(
            "PACKET3_DISPATCH_DIRECT",
            &[launch.global[0], launch.global[1], launch.global[2], flags],
        )?;
        let soc = match self.gfx_major {
            9 => "soc_9",
            11 => "soc_11",
            12 => "soc_12",
            _ => return Err(Error::Contract("unsupported compute architecture")),
        };
        self.packet(
            "PACKET3_EVENT_WRITE",
            &[self.field(
                "EVENT_TYPE",
                self.catalog.constant(soc, "CS_PARTIAL_FLUSH")? as u32,
            )? | self.field("EVENT_INDEX", 4)?],
        )
    }
    pub fn memory_barrier(&mut self, nbio_version: [u8; 3]) -> Result<(), Error> {
        let pf = if nbio_version[..2] == [7, 11] { 1 } else { 0 };
        let request = self
            .register(&format!("regBIF_BX_PF{pf}_GPU_HDP_FLUSH_REQ"))?
            .address(0)?;
        let done = self
            .register(&format!("regBIF_BX_PF{pf}_GPU_HDP_FLUSH_DONE"))?
            .address(0)?;
        self.wait_register_memory(None, request as u32, done as u32, u32::MAX, u32::MAX, 5)?;
        self.acquire(0, u64::MAX, CachePolicy::default())
    }
    pub fn indirect(&self, address: u64) -> Result<Vec<u32>, Error> {
        let pair = words64(address);
        Ok(vec![
            packet3(self.constant("PACKET3_INDIRECT_BUFFER")?, 2),
            pair[0],
            pair[1],
            self.words.len() as u32 | self.constant("INDIRECT_BUFFER_VALID")?,
        ])
    }
}

pub struct CopyPackets<'a> {
    pub words: Vec<u32>,
    pub command_sizes: Vec<usize>,
    pub catalog: &'a Catalog,
    pub module: &'static str,
    pub gfx_major: u8,
    pub max_copy_size: u64,
}
impl<'a> CopyPackets<'a> {
    pub fn new(catalog: &'a Catalog, sdma_major: u8, gfx_major: u8) -> Result<Self, Error> {
        let module = match sdma_major {
            4 => "sdma_4_0_0",
            5 => "sdma_5_0_0",
            6.. => "sdma_6_0_0",
            _ => return Err(Error::Contract("unsupported SDMA packet version")),
        };
        Ok(Self {
            words: Vec::new(),
            command_sizes: Vec::new(),
            catalog,
            module,
            gfx_major,
            max_copy_size: 0x40000000,
        })
    }
    fn constant(&self, name: &str) -> Result<u32, Error> {
        Ok(self.catalog.constant(self.module, name)? as u32)
    }
    fn field(&self, name: &str, value: u32) -> Result<u32, Error> {
        self.catalog.encode_macro(self.module, name, value)
    }
    fn append(&mut self, values: &[u32]) {
        self.words.extend_from_slice(values);
        self.command_sizes.push(values.len());
    }
    pub fn copy(&mut self, destination: u64, source: u64, size: u64) -> Result<(), Error> {
        if self.max_copy_size == 0 || self.max_copy_size > 0x40000000 {
            return Err(Error::Contract("invalid SDMA maximum copy size"));
        }
        let mut copied = 0;
        while copied < size {
            let count = (size - copied).min(self.max_copy_size);
            let src = words64(
                source
                    .checked_add(copied)
                    .ok_or(Error::Contract("SDMA source overflow"))?,
            );
            let dst = words64(
                destination
                    .checked_add(copied)
                    .ok_or(Error::Contract("SDMA destination overflow"))?,
            );
            self.append(&[
                self.constant("SDMA_OP_COPY")?
                    | self.field(
                        "SDMA_PKT_COPY_LINEAR_HEADER_SUB_OP",
                        self.constant("SDMA_SUBOP_COPY_LINEAR")?,
                    )?,
                self.field("SDMA_PKT_COPY_LINEAR_COUNT_COUNT", (count - 1) as u32)?,
                0,
                src[0],
                src[1],
                dst[0],
                dst[1],
            ]);
            copied += count;
        }
        Ok(())
    }
    pub fn signal(&mut self, address: u64, value: u32, owned: bool) -> Result<(), Error> {
        let flags = if self.gfx_major == 9 {
            0
        } else {
            self.field("SDMA_PKT_FENCE_HEADER_MTYPE", 3)?
        };
        let pair = words64(address);
        self.append(&[
            self.constant("SDMA_OP_FENCE")? | flags,
            pair[0],
            pair[1],
            value,
        ]);
        if owned {
            self.append(&[self.constant("SDMA_OP_TRAP")?, 0]);
        }
        Ok(())
    }
    pub fn wait(&mut self, address: u64, value: u32) -> Result<(), Error> {
        let pair = words64(address);
        self.append(&[
            self.constant("SDMA_OP_POLL_REGMEM")?
                | self.field("SDMA_PKT_POLL_REGMEM_HEADER_FUNC", 5)?
                | self.field("SDMA_PKT_POLL_REGMEM_HEADER_MEM_POLL", 1)?,
            pair[0],
            pair[1],
            value,
            u32::MAX,
            self.field("SDMA_PKT_POLL_REGMEM_DW5_INTERVAL", 4)?
                | self.field("SDMA_PKT_POLL_REGMEM_DW5_RETRY_COUNT", 0xfff)?,
        ]);
        Ok(())
    }
    pub fn timestamp(&mut self, address: u64) -> Result<(), Error> {
        let pair = words64(address);
        self.append(&[
            self.constant("SDMA_OP_TIMESTAMP")?
                | self.field(
                    "SDMA_PKT_TIMESTAMP_GET_HEADER_SUB_OP",
                    self.constant("SDMA_SUBOP_TIMESTAMP_GET_GLOBAL")?,
                )?,
            pair[0],
            pair[1],
        ]);
        Ok(())
    }
    pub fn write(&mut self, address: u64, value: u64, wide: bool) -> Result<(), Error> {
        let pair = words64(address);
        let mut values = vec![
            self.constant("SDMA_OP_WRITE")?,
            pair[0],
            pair[1],
            u32::from(wide),
            value as u32,
        ];
        if wide {
            values.push((value >> 32) as u32);
        }
        self.append(&values);
        Ok(())
    }
    pub fn indirect(&self, address: u64) -> Result<(Vec<u32>, Vec<u32>), Error> {
        let mut commands = self.words.clone();
        commands.resize(commands.len().div_ceil(8) * 8, 0);
        let pair = words64(address);
        let packet = vec![
            self.constant("SDMA_OP_INDIRECT")? | self.field("SDMA_PKT_INDIRECT_HEADER_VMID", 0)?,
            pair[0],
            pair[1],
            commands.len() as u32,
            0,
            0,
        ];
        Ok((commands, packet))
    }
}
