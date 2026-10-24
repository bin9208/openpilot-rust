//! Original AArch64/no-INTERP/no-NEEDED contract, without an external readelf.
use crate::seal::Result;
fn number(bytes: &[u8], at: usize, size: usize, big: bool) -> Result<u64> {
    let data = bytes
        .get(at..at.checked_add(size).ok_or("ELF offset overflow")?)
        .ok_or("truncated ELF field")?;
    Ok(if big {
        data.iter().fold(0, |n, b| (n << 8) | u64::from(*b))
    } else {
        data.iter().rev().fold(0, |n, b| (n << 8) | u64::from(*b))
    })
}
pub fn inspect_static(bytes: &[u8]) -> Result<()> {
    if bytes.get(..4) != Some(b"\x7fELF") {
        return Err("candidate must be a static AArch64 ELF".into());
    }
    let class = *bytes.get(4).ok_or("missing ELF class")?;
    let big = match bytes.get(5) {
        Some(1) => false,
        Some(2) => true,
        _ => return Err("invalid ELF endianness".into()),
    };
    if number(bytes, 18, 2, big)? != 183 {
        return Err("candidate must be a static AArch64 ELF".into());
    }
    let (at, width, entry, count, minimum) = match class {
        1 => (28, 4, 42, 44, 32),
        2 => (32, 8, 54, 56, 56),
        _ => return Err("invalid ELF class".into()),
    };
    let offset = usize::try_from(number(bytes, at, width, big)?)?;
    let stride = usize::try_from(number(bytes, entry, 2, big)?)?;
    let count = usize::try_from(number(bytes, count, 2, big)?)?;
    if count > 0 && stride < minimum {
        return Err("invalid ELF program header size".into());
    }
    for i in 0..count {
        let at = offset
            .checked_add(i.checked_mul(stride).ok_or("ELF table overflow")?)
            .ok_or("ELF table overflow")?;
        let header = bytes
            .get(at..at.checked_add(minimum).ok_or("ELF header overflow")?)
            .ok_or("truncated ELF program headers")?;
        let kind = number(header, 0, 4, big)?;
        if kind == 3 {
            return Err("candidate must be static: PT_INTERP".into());
        }
        if kind == 2 {
            let (loc, len) = if class == 1 {
                (number(header, 4, 4, big)?, number(header, 16, 4, big)?)
            } else {
                (number(header, 8, 8, big)?, number(header, 32, 8, big)?)
            };
            let loc = usize::try_from(loc)?;
            let len = usize::try_from(len)?;
            let data = bytes
                .get(loc..loc.checked_add(len).ok_or("ELF dynamic overflow")?)
                .ok_or("truncated ELF dynamic segment")?;
            for dynamic in data.chunks_exact(width * 2) {
                let tag = number(dynamic, 0, width, big)?;
                if tag == 0 {
                    break;
                }
                if tag == 1 {
                    return Err("candidate must be static: DT_NEEDED".into());
                }
            }
        }
    }
    Ok(())
}
