//! Strict CPython3.12.15 Korean decoders; PSF provenance/license beside the map.
//! The native mapping/index data is never used as the expected test oracle.
use super::{korean_indices::*, Diagnostic, Phase};
type Result<T> = std::result::Result<T, Diagnostic>;
const KSX: &[u8; 17672] = include_bytes!("korean-ksx1001.bin");
fn invalid() -> Diagnostic {
    Diagnostic::new(
        Phase::Decode,
        "undefined or truncated strict Korean codec sequence",
    )
}
fn point(out: &mut String, cp: u32) -> Result<()> {
    out.push(char::from_u32(cp).ok_or_else(invalid)?);
    Ok(())
}
fn ksx(first: u8, second: u8) -> Result<u32> {
    if !(0x21..=0x7e).contains(&first) || !(0x21..=0x7e).contains(&second) {
        return Err(invalid());
    }
    let at = (usize::from(first - 0x21) * 94 + usize::from(second - 0x21)) * 2;
    let cp = u16::from_le_bytes([KSX[at], KSX[at + 1]]);
    if cp == u16::MAX {
        return Err(invalid());
    }
    Ok(u32::from(cp))
}
fn makeup(raw: &[u8]) -> Result<u32> {
    let raw = raw.get(..8).ok_or_else(invalid)?;
    if raw[2] != 0xa4 || raw[4] != 0xa4 || raw[6] != 0xa4 {
        return Err(invalid());
    }
    let cho = *CGK2U_CHOSEONG
        .get(usize::from(raw[3].wrapping_sub(0xa1)))
        .ok_or_else(invalid)?;
    let jung = raw[5].wrapping_sub(0xbf);
    let jong = if raw[7] == 0xd4 {
        0
    } else {
        *CGK2U_JONGSEONG
            .get(usize::from(raw[7].wrapping_sub(0xa1)))
            .ok_or_else(invalid)?
    };
    if cho == 127 || jung >= 21 || jong == 127 {
        return Err(invalid());
    }
    Ok(0xac00 + u32::from(cho) * 588 + u32::from(jung) * 28 + u32::from(jong))
}
pub(super) fn euc(raw: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first < 0x80 {
            out.push(char::from(first));
            i += 1;
            continue;
        }
        let second = *raw.get(i + 1).ok_or_else(invalid)?;
        let (cp, width) = if first == 0xa4 && second == 0xd4 {
            (makeup(&raw[i..])?, 8)
        } else {
            (ksx(first ^ 0x80, second ^ 0x80)?, 2)
        };
        point(&mut out, cp)?;
        i += width;
    }
    Ok(out)
}
fn johab_pair(first: u8, second: u8) -> Result<u32> {
    if first < 0xd8 {
        let cho = usize::from((first >> 2) & 31);
        let jung = usize::from(((u16::from(first) << 3) | (u16::from(second) >> 5)) & 31);
        let jong = usize::from(second & 31);
        let (a, b, c) = (
            JOHABIDX_CHOSEONG[cho],
            JOHABIDX_JUNGSEONG[jung],
            JOHABIDX_JONGSEONG[jong],
        );
        if [a, b, c].contains(&255) {
            return Err(invalid());
        }
        return match (a, b, c) {
            (253, 253, 253) => Ok(0x3000),
            (253, 253, _) => Ok(0x3100 | u32::from(JOHABJAMO_JONGSEONG[jong])),
            (253, _, 253) => Ok(0x3100 | u32::from(JOHABJAMO_JUNGSEONG[jung])),
            (253, _, _) => Err(invalid()),
            (_, 253, 253) => Ok(0x3100 | u32::from(JOHABJAMO_CHOSEONG[cho])),
            (_, 253, _) => Err(invalid()),
            _ => Ok(0xac00
                + u32::from(a) * 588
                + u32::from(b) * 28
                + u32::from(if c == 253 { 0 } else { c })),
        };
    }
    if first == 0xdf
        || first > 0xf9
        || second < 0x31
        || (0x80..0x91).contains(&second)
        || second & 0x7f == 0x7f
        || (first == 0xda && (0xa1..=0xd3).contains(&second))
    {
        return Err(invalid());
    }
    // CPython assigns arithmetic into unsigned char temporaries, including D8.
    let row = if first < 0xe0 {
        first.wrapping_sub(0xd9).wrapping_mul(2)
    } else {
        u8::try_from(u16::from(first) * 2 - 0x197).map_err(|_| invalid())?
    };
    let col = if second < 0x91 {
        second - 0x31
    } else {
        second - 0x43
    };
    ksx(
        row.wrapping_add(u8::from(col >= 0x5e)).wrapping_add(0x21),
        if col < 0x5e { col } else { col - 0x5e } + 0x21,
    )
}
pub(super) fn johab(raw: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first < 0x80 {
            out.push(char::from(first));
            i += 1;
        } else {
            let second = *raw.get(i + 1).ok_or_else(invalid)?;
            point(&mut out, johab_pair(first, second)?)?;
            i += 2;
        }
    }
    Ok(out)
}
fn designation(raw: &[u8]) -> Result<(usize, usize, u8)> {
    let mut length = 0;
    for i in 1..16 {
        let byte = *raw.get(i).ok_or_else(invalid)?;
        if byte.is_ascii_uppercase() || byte == b'@' {
            length = i + 1;
            break;
        }
    }
    let (group, mark) = match length {
        3 if raw[1] == b'$' => (0, raw[2] | 0x80),
        3 if matches!(raw[1], b'(' | b')') => (usize::from(raw[1] == b')'), raw[2]),
        4 if raw[1] == b'$' && matches!(raw[2], b'(' | b')') => {
            (usize::from(raw[2] == b')'), raw[3] | 0x80)
        }
        _ => return Err(invalid()),
    };
    if !matches!(mark, b'B' | 0xc3) {
        return Err(invalid());
    }
    Ok((length, group, mark))
}
pub(super) fn iso2022(raw: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    let mut groups = [b'B'; 2];
    let mut shifted = false;
    let mut throughout = false;
    while let Some(&byte) = raw.get(i) {
        if throughout {
            out.push(char::from(byte));
            i += 1;
            throughout = !(byte.is_ascii_uppercase() || byte == b'@');
            continue;
        }
        match byte {
            0x1b => {
                let next = *raw.get(i + 1).ok_or_else(invalid)?;
                if matches!(next, b'(' | b')' | b'$' | b'.' | b'&') {
                    let (length, group, mark) = designation(&raw[i..])?;
                    groups[group] = mark;
                    i += length;
                } else {
                    out.push('\x1b');
                    throughout = true;
                    i += 1;
                }
            }
            0x0e => {
                shifted = true;
                i += 1;
            }
            0x0f => {
                shifted = false;
                i += 1;
            }
            b'\n' => {
                shifted = false;
                out.push('\n');
                i += 1;
            }
            0x00..=0x1f => {
                out.push(char::from(byte));
                i += 1;
            }
            0x80..=0xff => return Err(invalid()),
            _ if groups[usize::from(shifted)] == b'B' => {
                out.push(char::from(byte));
                i += 1;
            }
            _ => {
                let next = *raw.get(i + 1).ok_or_else(invalid)?;
                point(&mut out, ksx(byte, next)?)?;
                i += 2;
            }
        }
    }
    Ok(out)
}
