//! Complete immutable original-provider ShiftJIS/CP932 byte maps.
//! Original run39814172666; PSF codec provenance/seals recorded separately.
//! Lead-byte boundaries follow pinned Modules/cjkcodecs/_codecs_jp.c.
use super::{Diagnostic, Phase};
fn invalid() -> Diagnostic {
    Diagnostic::new(
        Phase::Decode,
        "undefined or truncated strict Japanese codec byte sequence",
    )
}
fn point(bytes: &[u8]) -> Result<char, Diagnostic> {
    let value = u32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?);
    char::from_u32(value).ok_or_else(invalid)
}
pub(super) fn decode(raw: &[u8], cp932: bool) -> Result<String, Diagnostic> {
    let map: &[u8] = if cp932 {
        include_bytes!("cp932-original-map.bin")
    } else {
        include_bytes!("shift_jis-original-map.bin")
    };
    let mut output = String::new();
    let mut cursor = 0;
    while let Some(&first) = raw.get(cursor) {
        if !(0x81..=0x9f).contains(&first) && !(0xe0..=0xfc).contains(&first) {
            let at = usize::from(first) * 4;
            output.push(point(&map[at..at + 4])?);
            cursor += 1;
            continue;
        }
        let second = *raw.get(cursor + 1).ok_or_else(invalid)?;
        let code = u16::from_be_bytes([first, second]);
        let mut low = 0;
        let mut high = (map.len() - 1024) / 6;
        let mut found = None;
        while low < high {
            let mid = low + (high - low) / 2;
            let at = 1024 + mid * 6;
            let key = u16::from_le_bytes([map[at], map[at + 1]]);
            match key.cmp(&code) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    found = Some(point(&map[at + 2..at + 6])?);
                    break;
                }
            }
        }
        output.push(found.ok_or_else(invalid)?);
        cursor += 2;
    }
    Ok(output)
}
