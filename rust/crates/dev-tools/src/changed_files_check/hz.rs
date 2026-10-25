//! CPython 3.12.15 strict HZ decoder (PSF license/provenance beside the table).
//! The mapping is implementation data, never the differential's expected oracle.
use super::{Diagnostic, Phase};
const GB2312: &[u8; 17672] = include_bytes!("hz-gb2312.bin");
fn invalid() -> Diagnostic {
    Diagnostic::new(Phase::Decode, "undefined or truncated strict HZ sequence")
}
pub(super) fn decode(raw: &[u8]) -> Result<String, Diagnostic> {
    let mut out = String::new();
    let mut gb = false;
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first == b'~' {
            let next = *raw.get(i + 1).ok_or_else(invalid)?;
            match (gb, next) {
                (false, b'~') => out.push('~'),
                (false, b'{') => gb = true,
                (false, b'\n') => {}
                (true, b'}') => gb = false,
                _ => return Err(invalid()),
            }
            i += 2;
        } else if first >= 0x80 {
            return Err(invalid());
        } else if !gb {
            out.push(char::from(first));
            i += 1;
        } else {
            let second = *raw.get(i + 1).ok_or_else(invalid)?;
            if !(0x21..=0x7e).contains(&first) || !(0x21..=0x7e).contains(&second) {
                return Err(invalid());
            }
            let at = (usize::from(first - 0x21) * 94 + usize::from(second - 0x21)) * 2;
            let cp = u16::from_le_bytes([GB2312[at], GB2312[at + 1]]);
            if cp == u16::MAX {
                return Err(invalid());
            }
            out.push(char::from_u32(u32::from(cp)).ok_or_else(invalid)?);
            i += 2;
        }
    }
    // Pinned CPython does not require a final '~}' after complete GB pairs.
    Ok(out)
}
