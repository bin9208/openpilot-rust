use crate::{amd_metadata::Catalog, discovery::Discovery, Error};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ops::Range, sync::Arc};
pub trait FirmwareSource {
    fn load(&mut self, name: &str, expected_sha256: &str) -> Result<Vec<u8>, Error>;
}
#[derive(Clone)]
pub struct Segment {
    pub kinds: Vec<u32>,
    blob: Arc<[u8]>,
    range: Range<usize>,
}
impl Segment {
    pub fn data(&self) -> &[u8] {
        &self.blob[self.range.clone()]
    }
}
pub struct Firmware {
    pub sos: BTreeMap<u32, Segment>,
    pub descriptors: Vec<Segment>,
    pub smu: Option<Segment>,
    pub ucode_start: BTreeMap<String, u64>,
}
struct Blob {
    data: Arc<[u8]>,
    header: String,
}
impl Blob {
    fn get(&self, catalog: &Catalog, path: &[&str]) -> Result<u64, Error> {
        catalog.read(&self.header, path, &self.data)
    }
    fn segment(&self, offset: u64, size: u64, kinds: Vec<u32>) -> Result<Segment, Error> {
        let start = usize::try_from(offset)
            .map_err(|_| Error::Contract("firmware offset exceeds host size"))?;
        let size = usize::try_from(size)
            .map_err(|_| Error::Contract("firmware size exceeds host size"))?;
        let end = start
            .checked_add(size)
            .ok_or(Error::Contract("firmware segment range overflow"))?;
        if end > self.data.len() {
            return Err(Error::Contract("firmware segment exceeds verified blob"));
        }
        Ok(Segment {
            kinds,
            blob: Arc::clone(&self.data),
            range: start..end,
        })
    }
}
fn load(
    catalog: &Catalog,
    source: &mut impl FirmwareSource,
    name: &str,
    header: &str,
    versioned: bool,
) -> Result<Blob, Error> {
    let expected = catalog
        .firmware_hashes
        .get(name)
        .ok_or_else(|| Error::Protocol(format!("unknown AMD firmware {name}")))?;
    let data = source.load(name, expected)?;
    let actual = format!("{:x}", Sha256::digest(&data));
    if &actual != expected {
        return Err(Error::Protocol(format!(
            "firmware hash mismatch for {name}: expected {expected}, got {actual}"
        )));
    }
    let header = if versioned {
        let major = catalog.read(
            "struct_common_firmware_header",
            &["header_version_major"],
            &data,
        )?;
        let minor = catalog.read(
            "struct_common_firmware_header",
            &["header_version_minor"],
            &data,
        )?;
        format!("{header}_v{major}_{minor}")
    } else {
        header.to_owned()
    };
    if data.len() < catalog.layout(&header)?.size {
        return Err(Error::Contract("truncated firmware header"));
    }
    Ok(Blob {
        data: data.into(),
        header,
    })
}
fn version(discovery: &Discovery, hardware: u32) -> Result<String, Error> {
    let v = discovery.version(hardware)?;
    Ok(format!("{}_{}_{}", v[0], v[1], v[2]))
}
fn kind(catalog: &Catalog, name: &str) -> Result<u32, Error> {
    u32::try_from(catalog.constant("am", name)?)
        .map_err(|_| Error::Contract("firmware kind overflow"))
}
impl Firmware {
    pub fn load(
        catalog: &Catalog,
        discovery: &Discovery,
        source: &mut impl FirmwareSource,
    ) -> Result<Self, Error> {
        let mut result = Self {
            sos: BTreeMap::new(),
            descriptors: Vec::new(),
            smu: None,
            ucode_start: BTreeMap::new(),
        };
        let gfx = discovery.gc_version()?;
        let blob = load(
            catalog,
            source,
            &format!("psp_{}_sos.bin", version(discovery, 15)?),
            "struct_psp_firmware_header",
            true,
        )?;
        let count = blob.get(catalog, &["psp_fw_bin_count"])? as usize;
        let (_, offset) = catalog.field(&blob.header, &["psp_fw_bin"])?;
        let stride = catalog.layout("struct_psp_fw_bin_desc")?.size;
        let base = blob.get(catalog, &["header", "ucode_array_offset_bytes"])?;
        for index in 0..count {
            let at = offset
                .checked_add(
                    index
                        .checked_mul(stride)
                        .ok_or(Error::Contract("firmware descriptor count overflow"))?,
                )
                .ok_or(Error::Contract("firmware descriptor offset overflow"))?;
            let descriptor = blob
                .data
                .get(at..)
                .ok_or(Error::Contract("truncated PSP descriptor"))?;
            let firmware_kind =
                catalog.read("struct_psp_fw_bin_desc", &["fw_type"], descriptor)? as u32;
            let offset = catalog
                .read("struct_psp_fw_bin_desc", &["offset_bytes"], descriptor)?
                .checked_add(base)
                .ok_or(Error::Contract("PSP offset overflow"))?;
            let size = catalog.read("struct_psp_fw_bin_desc", &["size_bytes"], descriptor)?;
            result.sos.insert(
                firmware_kind,
                blob.segment(offset, size, vec![firmware_kind])?,
            );
        }
        if discovery.version(16)? != [13, 0, 12] {
            let blob = load(
                catalog,
                source,
                &format!("smu_{}.bin", version(discovery, 16)?),
                "struct_smc_firmware_header",
                true,
            )?;
            if gfx >= [11, 0, 0] {
                result.smu = Some(blob.segment(
                    blob.get(catalog, &["v1_0", "header", "ucode_array_offset_bytes"])?,
                    blob.get(catalog, &["v1_0", "header", "ucode_size_bytes"])?,
                    vec![kind(catalog, "GFX_FW_TYPE_SMU")?],
                )?);
            } else {
                let count = blob.get(catalog, &["pptable_count"])? as usize;
                let offset = blob.get(catalog, &["pptable_entry_offset"])? as usize;
                let stride = catalog.layout("struct_smc_soft_pptable_entry")?.size;
                for index in 0..count {
                    let descriptor = blob
                        .data
                        .get(offset + index * stride..)
                        .ok_or(Error::Contract("truncated SMU table"))?;
                    if catalog.read("struct_smc_soft_pptable_entry", &["id"], descriptor)?
                        == 0x50325358
                    {
                        result.descriptors.push(blob.segment(
                            catalog.read(
                                "struct_smc_soft_pptable_entry",
                                &["ppt_offset_bytes"],
                                descriptor,
                            )?,
                            catalog.read(
                                "struct_smc_soft_pptable_entry",
                                &["ppt_size_bytes"],
                                descriptor,
                            )?,
                            vec![kind(catalog, "GFX_FW_TYPE_P2S_TABLE")?],
                        )?);
                    }
                }
            }
        }
        let blob = load(
            catalog,
            source,
            &format!("sdma_{}.bin", version(discovery, 3)?),
            "struct_sdma_firmware_header",
            true,
        )?;
        let header_version = blob.get(catalog, &["header", "header_version_major"])?;
        let code_offset = blob.get(catalog, &["header", "ucode_array_offset_bytes"])?;
        match header_version {
            1 => result.descriptors.push(
                blob.segment(
                    code_offset,
                    blob.get(catalog, &["header", "ucode_size_bytes"])?,
                    (0..4)
                        .map(|index| kind(catalog, &format!("GFX_FW_TYPE_SDMA{index}")))
                        .collect::<Result<Vec<_>, _>>()?,
                )?,
            ),
            2 => {
                result.descriptors.push(blob.segment(
                    blob.get(catalog, &["ctl_ucode_offset"])?,
                    blob.get(catalog, &["ctl_ucode_size_bytes"])?,
                    vec![kind(catalog, "GFX_FW_TYPE_SDMA_UCODE_TH1")?],
                )?);
                result.descriptors.push(blob.segment(
                    code_offset,
                    blob.get(catalog, &["ctx_ucode_size_bytes"])?,
                    vec![kind(catalog, "GFX_FW_TYPE_SDMA_UCODE_TH0")?],
                )?);
            }
            _ => result.descriptors.push(blob.segment(
                code_offset,
                blob.get(catalog, &["ucode_size_bytes"])?,
                vec![kind(catalog, "GFX_FW_TYPE_SDMA_UCODE_TH0")?],
            )?),
        }
        let parts: &[&str] = if gfx >= [12, 0, 0] {
            &["PFP", "ME", "MEC"]
        } else {
            &["MEC"]
        };
        for &part in parts {
            let blob = load(
                catalog,
                source,
                &format!("gc_{}_{}.bin", version(discovery, 1)?, part.to_lowercase()),
                "struct_gfx_firmware_header",
                true,
            )?;
            let offset = blob.get(catalog, &["header", "ucode_array_offset_bytes"])?;
            if blob.get(catalog, &["header", "header_version_major"])? == 1 {
                let jump_bytes = blob.get(catalog, &["jt_size"])? * 4;
                let code_size = blob
                    .get(catalog, &["header", "ucode_size_bytes"])?
                    .checked_sub(jump_bytes)
                    .ok_or(Error::Contract("firmware jump table exceeds code"))?;
                result.descriptors.push(blob.segment(
                    offset,
                    code_size,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_CP_{part}"))?],
                )?);
                result.descriptors.push(blob.segment(
                    offset + blob.get(catalog, &["jt_offset"])? * 4,
                    jump_bytes,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_CP_{part}_ME1"))?],
                )?);
            } else {
                result.descriptors.push(blob.segment(
                    offset,
                    blob.get(catalog, &["ucode_size_bytes"])?,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_RS64_{part}"))?],
                )?);
                result.descriptors.push(blob.segment(
                    blob.get(catalog, &["data_offset_bytes"])?,
                    blob.get(catalog, &["data_size_bytes"])?,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_RS64_{part}_P0_STACK"))?],
                )?);
                result.ucode_start.insert(
                    part.to_owned(),
                    blob.get(catalog, &["ucode_start_addr_lo"])?
                        | (blob.get(catalog, &["ucode_start_addr_hi"])? << 32),
                );
            }
        }
        if gfx >= [11, 0, 0] {
            let blob = load(
                catalog,
                source,
                &format!("gc_{}_imu.bin", version(discovery, 1)?),
                "struct_imu_firmware_header_v1_0",
                false,
            )?;
            let offset = blob.get(catalog, &["header", "ucode_array_offset_bytes"])?;
            let instruction_size = blob.get(catalog, &["imu_iram_ucode_size_bytes"])?;
            result.descriptors.push(blob.segment(
                offset,
                instruction_size,
                vec![kind(catalog, "GFX_FW_TYPE_IMU_I")?],
            )?);
            result.descriptors.push(blob.segment(
                offset + instruction_size,
                blob.get(catalog, &["imu_dram_ucode_size_bytes"])?,
                vec![kind(catalog, "GFX_FW_TYPE_IMU_D")?],
            )?);
        }
        let mut blob = load(
            catalog,
            source,
            &format!("gc_{}_rlc.bin", version(discovery, 1)?),
            "struct_rlc_firmware_header_v2_0",
            false,
        )?;
        let minor = blob.get(catalog, &["header", "header_version_minor"])?;
        if minor == 1 {
            blob.header = "struct_rlc_firmware_header_v2_1".into();
            for (name, field) in [
                ("LIST_SRM_CNTL", "list_cntl"),
                ("LIST_GPM_MEM", "list_gpm"),
                ("LIST_SRM_MEM", "list_srm"),
            ] {
                result.descriptors.push(blob.segment(
                    blob.get(catalog, &[&format!("save_restore_{field}_offset_bytes")])?,
                    blob.get(catalog, &[&format!("save_restore_{field}_size_bytes")])?,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_RLC_RESTORE_{name}"))?],
                )?);
            }
        }
        if minor >= 2 {
            blob.header = "struct_rlc_firmware_header_v2_2".into();
            for (name, field) in [("IRAM", "iram"), ("DRAM_BOOT", "dram")] {
                result.descriptors.push(blob.segment(
                    blob.get(catalog, &[&format!("rlc_{field}_ucode_offset_bytes")])?,
                    blob.get(catalog, &[&format!("rlc_{field}_ucode_size_bytes")])?,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_RLC_{name}"))?],
                )?);
            }
        }
        if minor == 3 {
            blob.header = "struct_rlc_firmware_header_v2_3".into();
            for name in ["P", "V"] {
                result.descriptors.push(blob.segment(
                    blob.get(
                        catalog,
                        &[&format!("rlc{}_ucode_offset_bytes", name.to_lowercase())],
                    )?,
                    blob.get(
                        catalog,
                        &[&format!("rlc{}_ucode_size_bytes", name.to_lowercase())],
                    )?,
                    vec![kind(catalog, &format!("GFX_FW_TYPE_RLC_{name}"))?],
                )?);
            }
        }
        blob.header = "struct_rlc_firmware_header_v2_0".into();
        result.descriptors.push(blob.segment(
            blob.get(catalog, &["header", "ucode_array_offset_bytes"])?,
            blob.get(catalog, &["header", "ucode_size_bytes"])?,
            vec![kind(catalog, "GFX_FW_TYPE_RLC_G")?],
        )?);
        Ok(result)
    }
}
