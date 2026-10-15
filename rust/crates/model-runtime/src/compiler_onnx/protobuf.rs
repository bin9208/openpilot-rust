//! Bounded ONNX protobuf wire reader; no generated Python or model execution.
#[derive(Clone, Copy, Debug)]
pub enum Wire<'a> {
    Integer(u64),
    Fixed32(u32),
    Fixed64(u64),
    Bytes(&'a [u8]),
}
impl<'a> Wire<'a> {
    pub fn integer(self) -> Result<u64, String> {
        match self {
            Self::Integer(v) => Ok(v),
            _ => Err("expected protobuf integer".into()),
        }
    }
    pub fn bytes(self) -> Result<&'a [u8], String> {
        match self {
            Self::Bytes(v) => Ok(v),
            _ => Err("expected protobuf bytes".into()),
        }
    }
    pub fn string(self) -> Result<String, String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| "invalid ONNX UTF8".into())
    }
}
pub fn varint(bytes: &[u8], index: &mut usize) -> Result<u64, String> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*index).ok_or("truncated protobuf varint")?;
        *index += 1;
        if shift == 63 && byte > 1 {
            return Err("protobuf varint overflow".into());
        }
        value |= ((byte & 127) as u64) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    Err("protobuf varint overflow".into())
}
pub fn fields(bytes: &[u8]) -> Result<Vec<(u32, Wire<'_>)>, String> {
    let mut index = 0;
    let mut output = Vec::new();
    while index < bytes.len() {
        let tag = varint(bytes, &mut index)?;
        let id = u32::try_from(tag >> 3).map_err(|_| "protobuf field overflow")?;
        if id == 0 || id >= (1 << 29) {
            return Err("invalid protobuf field".into());
        }
        let wire = match tag & 7 {
            0 => Wire::Integer(varint(bytes, &mut index)?),
            width @ (1 | 2 | 5) => {
                let length = match width {
                    1 => 8,
                    5 => 4,
                    _ => usize::try_from(varint(bytes, &mut index)?)
                        .map_err(|_| "protobuf size overflow")?,
                };
                let end = index.checked_add(length).ok_or("protobuf size overflow")?;
                let value = bytes.get(index..end).ok_or("truncated protobuf field")?;
                index = end;
                match width {
                    1 => Wire::Fixed64(u64::from_le_bytes(value.try_into().unwrap())),
                    5 => Wire::Fixed32(u32::from_le_bytes(value.try_into().unwrap())),
                    _ => Wire::Bytes(value),
                }
            }
            _ => return Err("unsupported protobuf wire type".into()),
        };
        if output.len() >= 1_000_000 {
            return Err("excessive protobuf fields".into());
        }
        output.push((id, wire));
    }
    Ok(output)
}
pub fn integers(wire: Wire<'_>) -> Result<Vec<i64>, String> {
    match wire {
        Wire::Integer(v) => Ok(vec![v as i64]),
        Wire::Bytes(bytes) => {
            let mut i = 0;
            let mut values = Vec::new();
            while i < bytes.len() {
                values.push(varint(bytes, &mut i)? as i64);
            }
            Ok(values)
        }
        _ => Err("expected packed protobuf integers".into()),
    }
}
