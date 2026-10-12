use crate::Error;

pub struct AccessUnit {
    pub avcc: Vec<u8>,
    pub nal_types: Vec<u8>,
}
impl AccessUnit {
    pub fn is_idr(&self) -> bool {
        self.nal_types.contains(&5)
    }
}
fn error(message: &str) -> Error {
    Error::Source(message.into())
}
pub fn annexb(payload: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= payload.len() {
        let size = if payload.get(index..index + 4) == Some(&[0, 0, 0, 1]) {
            4
        } else if payload[index..index + 3] == [0, 0, 1] {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, size));
        index += size;
    }
    let mut units = Vec::new();
    for (position, &(offset, size)) in starts.iter().enumerate() {
        let start = offset + size;
        let mut end = starts
            .get(position + 1)
            .map_or(payload.len(), |next| next.0);
        while end > start && payload[end - 1] == 0 {
            end -= 1;
        }
        if end > start {
            units.push(&payload[start..end]);
        }
    }
    units
}
pub fn avcc(payload: &[u8], length_size: usize) -> Result<Vec<&[u8]>, Error> {
    if !matches!(length_size, 1 | 2 | 4) {
        return Err(error("unsupported H.264 AVCC length size"));
    }
    let mut units = Vec::new();
    let mut offset = 0;
    while offset < payload.len() {
        let Some(length) = payload.get(offset..offset + length_size) else {
            return Err(error("truncated H.264 AVCC length"));
        };
        let size = length
            .iter()
            .fold(0_usize, |size, &byte| size * 256 + usize::from(byte));
        offset += length_size;
        let Some(end) = offset.checked_add(size) else {
            return Err(error("invalid H.264 AVCC NAL size"));
        };
        let Some(unit) = payload.get(offset..end).filter(|_| size > 0) else {
            return Err(error("invalid H.264 AVCC NAL size"));
        };
        units.push(unit);
        offset = end;
    }
    if units.is_empty() {
        return Err(error("H.264 access unit has no NAL units"));
    }
    Ok(units)
}
pub fn units(payload: &[u8]) -> Result<Vec<&[u8]>, Error> {
    if payload.is_empty() {
        return Err(error("H.264 access unit is empty"));
    }
    let prefix = &payload[..payload.len().min(8)];
    if prefix.windows(3).any(|prefix| prefix == [0, 0, 1]) {
        let units = annexb(payload);
        if units.is_empty() {
            return Err(error("H.264 access unit has no NAL units"));
        }
        Ok(units)
    } else {
        avcc(payload, 4)
    }
}
pub fn normalize(payload: &[u8]) -> Result<AccessUnit, Error> {
    let mut avcc = Vec::new();
    let mut nal_types = Vec::new();
    for unit in units(payload)? {
        let size = u32::try_from(unit.len()).map_err(|_| error("int too big to convert"))?;
        avcc.extend_from_slice(&size.to_be_bytes());
        avcc.extend_from_slice(unit);
        nal_types.push(unit[0] & 31);
    }
    Ok(AccessUnit { avcc, nal_types })
}
fn validate_configuration(config: &[u8]) -> Result<(), Error> {
    if config.len() < 7 || config[0] != 1 {
        return Err(error("invalid H.264 AVC decoder configuration"));
    }
    if config[4] & 3 != 3 {
        return Err(error(
            "H.264 AVC configuration must use four-byte NAL lengths",
        ));
    }
    let mut offset = 6;
    let sps_count = config[5] & 31;
    if sps_count == 0 {
        return Err(error("H.264 codec header has no SPS"));
    }
    parameter_sets(config, &mut offset, sps_count, 7, 4, "SPS")?;
    let Some(&pps_count) = config.get(offset) else {
        return Err(error("H.264 codec header has no PPS"));
    };
    offset += 1;
    if pps_count == 0 {
        return Err(error("H.264 codec header has no PPS"));
    }
    parameter_sets(config, &mut offset, pps_count, 8, 1, "PPS")
}
fn parameter_sets(
    config: &[u8],
    offset: &mut usize,
    count: u8,
    kind: u8,
    minimum: usize,
    name: &str,
) -> Result<(), Error> {
    for _ in 0..count {
        let Some(length) = config.get(*offset..*offset + 2) else {
            return Err(error(&format!("truncated H.264 {name} length")));
        };
        let size = usize::from(u16::from_be_bytes([length[0], length[1]]));
        *offset += 2;
        if size < minimum || *offset + size > config.len() || config[*offset] & 31 != kind {
            return Err(error(&format!("invalid H.264 {name}")));
        }
        *offset += size;
    }
    Ok(())
}
pub fn configuration(header: &[u8]) -> Result<Vec<u8>, Error> {
    if header.first() == Some(&1) {
        validate_configuration(header)?;
        return Ok(header.to_vec());
    }
    let units = annexb(header);
    let sps: Vec<_> = units
        .iter()
        .copied()
        .filter(|unit| unit[0] & 31 == 7)
        .collect();
    let pps: Vec<_> = units
        .iter()
        .copied()
        .filter(|unit| unit[0] & 31 == 8)
        .collect();
    if sps.is_empty() || pps.is_empty() || sps[0].len() < 4 {
        return Err(error("H.264 codec header has no SPS/PPS"));
    }
    if sps.len() > 31 || pps.len() > 255 {
        return Err(error("H.264 codec header has too many parameter sets"));
    }
    let mut result = vec![
        1,
        sps[0][1],
        sps[0][2],
        sps[0][3],
        255,
        224 | u8::try_from(sps.len()).unwrap_or(0),
    ];
    for unit in sps {
        result.extend_from_slice(
            &u16::try_from(unit.len())
                .map_err(|_| error("int too big to convert"))?
                .to_be_bytes(),
        );
        result.extend_from_slice(unit);
    }
    result.push(u8::try_from(pps.len()).unwrap_or(0));
    for unit in pps {
        result.extend_from_slice(
            &u16::try_from(unit.len())
                .map_err(|_| error("int too big to convert"))?
                .to_be_bytes(),
        );
        result.extend_from_slice(unit);
    }
    validate_configuration(&result)?;
    Ok(result)
}
pub fn validate_start(header: &[u8], payload: &[u8]) -> Result<(), Error> {
    configuration(header)?;
    if !units(payload).is_ok_and(|units| units.iter().any(|unit| unit[0] & 31 == 5)) {
        return Err(error("first H.264 access unit is not an IDR frame"));
    }
    Ok(())
}
