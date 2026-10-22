//! Strict stateless CPython3.12.15 Big5/CP950/HKSCS source-byte decoding.
//! Immutable PSF table provenance and license accompany big5-tables.bin.
use super::{Diagnostic, Phase};

const TABLES: &[u8; 393216] = include_bytes!("big5-tables.bin");
const BLOCK_BYTES: usize = 128 * 256 * 4;

fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(Phase::Decode, message)
}

fn mapped(block: usize, first: u8, second: u8) -> Option<char> {
    // Only high input bytes reach this function. Absent cells contain u32::MAX.
    let at = block * BLOCK_BYTES + ((usize::from(first) - 128) * 256 + usize::from(second)) * 4;
    let scalar = u32::from_le_bytes(TABLES[at..at + 4].try_into().expect("fixed mapping cell"));
    char::from_u32(scalar)
}

pub(super) fn decode(raw: &[u8], name: &str) -> Result<String, Diagnostic> {
    let mut out = String::with_capacity(raw.len());
    let mut at = 0;
    while let Some(&first) = raw.get(at) {
        if first < 0x80 {
            out.push(char::from(first));
            at += 1;
            continue;
        }
        let &second = raw
            .get(at + 1)
            .ok_or_else(|| invalid("incomplete Big5-family multibyte sequence"))?;
        let scalar = match name {
            "big5" => mapped(0, first, second),
            "cp950" => mapped(1, first, second).or_else(|| mapped(0, first, second)),
            "big5hkscs" => {
                // CPython replaces this range, including holes, rather than
                // falling back to the original Big5 map after an HKSCS miss.
                let base = if !(0xc6..=0xc8).contains(&first) || (first == 0xc6 && second < 0xa1) {
                    mapped(0, first, second)
                } else {
                    None
                };
                base.or_else(|| mapped(2, first, second))
            }
            _ => {
                return Err(Diagnostic::new(
                    Phase::Capability,
                    "unregistered Big5-family decoder",
                ));
            }
        };
        if let Some(scalar) = scalar {
            out.push(scalar);
        } else if name == "big5hkscs" {
            // These four sequences deliberately produce two scalars, without
            // normalization or replacement by a precomposed character.
            let (body, modifier) = match [first, second] {
                [0x88, 0x62] => ('\u{00ca}', '\u{0304}'),
                [0x88, 0x64] => ('\u{00ca}', '\u{030c}'),
                [0x88, 0xa3] => ('\u{00ea}', '\u{0304}'),
                [0x88, 0xa5] => ('\u{00ea}', '\u{030c}'),
                _ => return Err(invalid("unmapped strict Big5HKSCS sequence")),
            };
            out.push(body);
            out.push(modifier);
        } else {
            return Err(invalid("unmapped strict Big5-family sequence"));
        }
        at += 2;
    }
    Ok(out)
}
