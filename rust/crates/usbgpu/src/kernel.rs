use crate::{amd_metadata::Catalog, elf::Image, Error};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Descriptor {
    pub group_segment_size: u32,
    pub private_segment_size: u32,
    pub kernargs_segment_size: u32,
    pub wave32: bool,
    pub resources: [u32; 3],
    pub descriptor_offset: u64,
    pub entry_offset: u64,
    pub dispatch_pointer: bool,
    pub private_segment_sgpr: bool,
    pub argument_allocation_size: usize,
}
pub struct Kernel {
    pub image: Vec<u8>,
    pub descriptor: Descriptor,
}
impl Kernel {
    pub fn load(catalog: &Catalog, elf: &[u8], gfx_major: u8, lds_kib: u32) -> Result<Self, Error> {
        let mut image = Image::parse(elf)?;
        image.relocate_amd()?;
        let rodata = image
            .sections
            .iter()
            .find(|s| s.name == ".rodata")
            .ok_or(Error::Contract(".rodata section not found"))?
            .address;
        let bytes = image
            .bytes
            .get(
                usize::try_from(rodata)
                    .map_err(|_| Error::Contract("kernel descriptor offset overflow"))?..,
            )
            .ok_or(Error::Contract("kernel descriptor outside image"))?;
        let record = "llvm_amdhsa_kernel_descriptor_t";
        if bytes.len() < catalog.layout(record)?.size {
            return Err(Error::Contract("truncated AMD kernel descriptor"));
        }
        let get = |name| catalog.read(record, &[name], bytes);
        let group = get("group_segment_fixed_size")? as u32;
        let private = get("private_segment_fixed_size")? as u32;
        let kernargs = get("kernarg_size")? as u32;
        let lds = group.div_ceil(512) & 0x1ff;
        if u64::from(lds) > u64::from(lds_kib) * 2 {
            return Err(Error::Contract(
                "Too many resources requested: group_segment_size",
            ));
        }
        let entry = rodata
            .checked_add_signed(catalog.read_signed(
                record,
                &["kernel_code_entry_byte_offset"],
                bytes,
            )?)
            .ok_or(Error::Contract("AMD kernel entry offset overflow"))?;
        if entry >= image.bytes.len() as u64 {
            return Err(Error::Contract("AMD kernel entry outside image"));
        }
        let properties = get("kernel_code_properties")?;
        let dispatch = properties
            & catalog.constant("hsa", "AMD_KERNEL_CODE_PROPERTIES_ENABLE_SGPR_DISPATCH_PTR")?
            != 0;
        let private_sgpr = properties
            & catalog.constant(
                "hsa",
                "AMD_KERNEL_CODE_PROPERTIES_ENABLE_SGPR_PRIVATE_SEGMENT_BUFFER",
            )?
            != 0;
        let descriptor = Descriptor {
            group_segment_size: group,
            private_segment_size: private,
            kernargs_segment_size: kernargs,
            wave32: properties & 0x400 != 0,
            resources: [
                get("compute_pgm_rsrc1")? as u32 | if gfx_major == 11 { 1 << 20 } else { 0 },
                get("compute_pgm_rsrc2")? as u32 | (lds << 15),
                get("compute_pgm_rsrc3")? as u32,
            ],
            descriptor_offset: rodata,
            entry_offset: entry,
            dispatch_pointer: dispatch,
            private_segment_sgpr: private_sgpr,
            argument_allocation_size: kernargs as usize
                + if dispatch {
                    catalog.layout("struct_hsa_kernel_dispatch_packet_s")?.size
                } else {
                    0
                },
        };
        Ok(Self {
            image: image.bytes,
            descriptor,
        })
    }
}
impl Descriptor {
    pub fn dispatch_packet(
        &self,
        catalog: &Catalog,
        program_base: u64,
        arguments: u64,
        global: [u32; 3],
        local: [u32; 3],
        aql: bool,
    ) -> Result<Vec<u8>, Error> {
        let record = "struct_hsa_kernel_dispatch_packet_s";
        let mut bytes = vec![0; catalog.layout(record)?.size];
        let mut set = |field: &str, value| catalog.write(record, &[field], &mut bytes, value);
        if aql {
            let constant = |name| catalog.constant("hsa", name);
            let header = (1 << constant("HSA_PACKET_HEADER_BARRIER")?)
                | (constant("HSA_FENCE_SCOPE_SYSTEM")?
                    << constant("HSA_PACKET_HEADER_SCACQUIRE_FENCE_SCOPE")?)
                | (constant("HSA_FENCE_SCOPE_SYSTEM")?
                    << constant("HSA_PACKET_HEADER_SCRELEASE_FENCE_SCOPE")?)
                | (constant("HSA_PACKET_TYPE_KERNEL_DISPATCH")?
                    << constant("HSA_PACKET_HEADER_TYPE")?);
            set("header", header)?;
            set(
                "setup",
                3 << constant("HSA_KERNEL_DISPATCH_PACKET_SETUP_DIMENSIONS")?,
            )?;
            set(
                "kernel_object",
                program_base
                    .checked_add(self.descriptor_offset)
                    .ok_or(Error::Contract("kernel descriptor address overflow"))?,
            )?;
        }
        for (index, axis) in ["x", "y", "z"].into_iter().enumerate() {
            set(&format!("workgroup_size_{axis}"), u64::from(local[index]))?;
            set(
                &format!("grid_size_{axis}"),
                u64::from(global[index]) * u64::from(local[index]),
            )?;
        }
        set("group_segment_size", u64::from(self.group_segment_size))?;
        set("private_segment_size", u64::from(self.private_segment_size))?;
        set("kernarg_address", arguments)?;
        Ok(bytes)
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ScratchPlan {
    pub private_bytes: u32,
    pub bytes_per_xcc: u64,
    pub total_bytes: u64,
    pub tmpring: u32,
}
impl ScratchPlan {
    pub fn new(
        catalog: &Catalog,
        gfx_major: u8,
        private_bytes: u32,
        cu_count: u32,
        slots_per_cu: u32,
        shader_engines: u32,
        xccs: u8,
    ) -> Result<Self, Error> {
        if private_bytes == 0
            || cu_count == 0
            || slots_per_cu == 0
            || shader_engines == 0
            || xccs == 0
        {
            return Err(Error::Contract("invalid AMD scratch geometry"));
        }
        let alignment = if gfx_major == 9 { 1024 } else { 256 };
        let per_thread = u64::from(private_bytes).div_ceil(alignment / 64) * (alignment / 64);
        let bytes_per_xcc = per_thread
            .checked_mul(64 * u64::from(slots_per_cu) * u64::from(cu_count))
            .ok_or(Error::Contract("AMD scratch size overflow"))?;
        let total_bytes = bytes_per_xcc
            .checked_mul(u64::from(xccs))
            .ok_or(Error::Contract("AMD scratch total size overflow"))?;
        let wave_scratch = (64 * per_thread).div_ceil(alignment);
        let max_waves = u64::from(cu_count) * u64::from(slots_per_cu) * u64::from(xccs);
        let waves = (bytes_per_xcc / (wave_scratch * alignment))
            / if gfx_major == 9 {
                1
            } else {
                u64::from(shader_engines)
            };
        let name = if gfx_major == 9 {
            "union_COMPUTE_TMPRING_SIZE_bitfields".to_owned()
        } else {
            format!("union_COMPUTE_TMPRING_SIZE_GFX{gfx_major}_bitfields")
        };
        let mut bytes = [0; 4];
        catalog.write(&name, &["WAVES"], &mut bytes, waves.min(max_waves))?;
        catalog.write(&name, &["WAVESIZE"], &mut bytes, wave_scratch)?;
        Ok(Self {
            private_bytes,
            bytes_per_xcc,
            total_bytes,
            tmpring: u32::from_le_bytes(bytes),
        })
    }
    pub fn aql_descriptor(
        &self,
        catalog: &Catalog,
        gfx_major: u8,
        address: u64,
        descriptor: &mut [u8],
    ) -> Result<(), Error> {
        let record = "struct_amd_queue_s";
        catalog.write(
            record,
            &["scratch_backing_memory_location"],
            descriptor,
            address,
        )?;
        catalog.write(
            record,
            &["scratch_wave64_lane_byte_size"],
            descriptor,
            u64::from(self.private_bytes),
        )?;
        catalog.write(
            record,
            &["compute_tmpring_size"],
            descriptor,
            u64::from(self.tmpring),
        )?;
        let word1 = if gfx_major == 9 {
            "union_SQ_BUF_RSRC_WORD1_bitfields"
        } else {
            "union_SQ_BUF_RSRC_WORD1_GFX11_bitfields"
        };
        let word3 = if gfx_major == 9 {
            "union_SQ_BUF_RSRC_WORD3_bitfields".to_owned()
        } else {
            format!("union_SQ_BUF_RSRC_WORD3_GFX{gfx_major}_bitfields")
        };
        let mut w1 = [0; 4];
        catalog.write(word1, &["BASE_ADDRESS_HI"], &mut w1, address >> 32)?;
        catalog.write(word1, &["SWIZZLE_ENABLE"], &mut w1, 1)?;
        let mut w3 = [0; 4];
        for (name, constant) in [
            ("DST_SEL_X", "SQ_SEL_X"),
            ("DST_SEL_Y", "SQ_SEL_Y"),
            ("DST_SEL_Z", "SQ_SEL_Z"),
            ("DST_SEL_W", "SQ_SEL_W"),
            ("TYPE", "SQ_RSRC_BUF"),
        ] {
            catalog.write(&word3, &[name], &mut w3, catalog.constant("hsa", constant)?)?;
        }
        catalog.write(&word3, &["ADD_TID_ENABLE"], &mut w3, 1)?;
        if gfx_major == 9 {
            for (name, value) in [
                (
                    "NUM_FORMAT",
                    catalog.constant("hsa", "BUF_NUM_FORMAT_UINT")?,
                ),
                (
                    "DATA_FORMAT",
                    catalog.constant("hsa", "BUF_DATA_FORMAT_32")?,
                ),
                ("ELEMENT_SIZE", 1),
                ("INDEX_STRIDE", 3),
            ] {
                catalog.write(&word3, &[name], &mut w3, value)?;
            }
        } else {
            catalog.write(
                &word3,
                &["FORMAT"],
                &mut w3,
                catalog.constant("hsa", "BUF_FORMAT_32_UINT")?,
            )?;
            catalog.write(&word3, &["OOB_SELECT"], &mut w3, 2)?;
        }
        let (_, offset) = catalog.field(record, &["scratch_resource_descriptor"])?;
        let words = [
            address as u32,
            u32::from_le_bytes(w1),
            self.bytes_per_xcc as u32,
            u32::from_le_bytes(w3),
        ];
        descriptor
            .get_mut(offset..offset + 16)
            .ok_or(Error::Contract("truncated AQL queue descriptor"))?
            .copy_from_slice(
                &words
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
        Ok(())
    }
}
