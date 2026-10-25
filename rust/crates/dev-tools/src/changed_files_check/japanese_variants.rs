//! Strict Japanese variants ported from CPython 3.12.15 CJK codecs (PSF-2.0).
//! Pinned source/table hashes and byte layout: japanese-variant-provenance.json.
//! Source-derived implementation tables are never used as test expectations.
use super::{Diagnostic, Phase};

type Result<T> = std::result::Result<T, Diagnostic>;
const MAP: &[u8; 247408] = include_bytes!("japanese-variant-tables.bin");
fn invalid() -> Diagnostic {
    Diagnostic::new(
        Phase::Decode,
        "undefined or truncated strict Japanese variant sequence",
    )
}
fn table(index: usize, first: u8, second: u8) -> Option<u32> {
    if !(0x21..=0x7e).contains(&first) || !(0x21..=0x7e).contains(&second) {
        return None;
    }
    let at = (index * 94 * 94 + usize::from(first - 0x21) * 94 + usize::from(second - 0x21)) * 4;
    let value = u32::from_le_bytes(MAP[at..at + 4].try_into().expect("fixed table cell"));
    (value != u32::MAX).then_some(value)
}
fn push(out: &mut String, value: u32) -> Result<()> {
    // CPython stores a pair as two BMP values packed high16 then low16.
    if value >= 0x30000 {
        out.push(char::from_u32(value >> 16).ok_or_else(invalid)?);
        out.push(char::from_u32(value & 0xffff).ok_or_else(invalid)?);
    } else {
        out.push(char::from_u32(value).ok_or_else(invalid)?);
    }
    Ok(())
}
fn old_invalid(first: u8, second: u8) -> bool {
    matches!(
        (first, second),
        (0x2e, 0x21)
            | (0x2f, 0x7e)
            | (0x4f, 0x54)
            | (0x4f, 0x7e)
            | (0x74, 0x27)
            | (0x7e, 0x7a..=0x7e)
    )
}
fn plane1(first: u8, second: u8, old: bool) -> Option<u32> {
    if old && old_invalid(first, second) {
        return None;
    }
    table(0, first, second)
        .or_else(|| table(2, first, second))
        .or_else(|| table(4, first, second).map(|p| p | 0x20000))
        .or_else(|| table(6, first, second))
}
fn plane2(first: u8, second: u8, old: bool) -> Option<u32> {
    if old && (first, second) == (0x7d, 0x3b) {
        return Some(0x9b1d);
    }
    table(3, first, second).or_else(|| table(5, first, second).map(|p| p | 0x20000))
}
fn roman(byte: u8) -> u32 {
    match byte {
        b'\\' => 0xa5,
        b'~' => 0x203e,
        _ => u32::from(byte),
    }
}
pub(super) fn euc(raw: &[u8], variant: &str) -> Result<String> {
    let old = variant == "euc_jisx0213";
    let jp = variant == "euc_jp";
    let mut out = String::new();
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first < 0x80 {
            out.push(char::from(first));
            i += 1;
            continue;
        }
        let second = *raw.get(i + 1).ok_or_else(invalid)?;
        let (value, width) = if first == 0x8e {
            (
                (0xa1..=0xdf)
                    .contains(&second)
                    .then_some(0xfec0 + u32::from(second)),
                2,
            )
        } else if first == 0x8f {
            let third = *raw.get(i + 2).ok_or_else(invalid)?;
            let (row, col) = (second ^ 0x80, third ^ 0x80);
            (
                if jp {
                    table(1, row, col)
                } else {
                    plane2(row, col, old).or_else(|| table(1, row, col))
                },
                3,
            )
        } else {
            let (row, col) = (first ^ 0x80, second ^ 0x80);
            (
                if (row, col) == (0x21, 0x40) {
                    Some(0xff3c)
                } else if !jp && (row, col) == (0x22, 0x32) {
                    Some(0xff5e)
                } else if jp {
                    table(0, row, col)
                } else {
                    plane1(row, col, old)
                },
                2,
            )
        };
        push(&mut out, value.ok_or_else(invalid)?)?;
        i += width;
    }
    Ok(out)
}
pub(super) fn shift(raw: &[u8], old: bool) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first < 0x80 {
            push(&mut out, roman(first))?;
            i += 1;
        } else if (0xa1..=0xdf).contains(&first) {
            push(&mut out, 0xfec0 + u32::from(first))?;
            i += 1;
        } else if (0x81..=0x9f).contains(&first) || (0xe0..=0xfc).contains(&first) {
            let second = *raw.get(i + 1).ok_or_else(invalid)?;
            if !(0x40..=0xfc).contains(&second) || second == 0x7f {
                return Err(invalid());
            }
            let mut row = 2 * if first < 0xe0 {
                first - 0x81
            } else {
                first - 0xc1
            };
            let mut col = if second < 0x80 {
                second - 0x40
            } else {
                second - 0x41
            };
            if col >= 0x5e {
                row += 1;
                col -= 0x5e;
            }
            col += 0x21;
            let value = if row < 0x5e {
                plane1(row + 0x21, col, old)
            } else {
                row = if row >= 0x67 {
                    row + 7
                } else if row >= 0x63 || row == 0x5f {
                    row - 0x37
                } else {
                    row - 0x3d
                };
                plane2(row, col, old)
            };
            push(&mut out, value.ok_or_else(invalid)?)?;
            i += 2;
        } else {
            return Err(invalid());
        }
    }
    Ok(out)
}
fn designated(variant: &str, mark: u8) -> bool {
    match mark {
        b'B' | 0xc2 => true, // ASCII / JIS X 0208
        0xc0 | b'J' => matches!(variant, "iso2022_jp" | "iso2022_jp_1" | "iso2022_jp_ext"),
        0xc4 => matches!(variant, "iso2022_jp_1" | "iso2022_jp_ext"),
        b'I' => variant == "iso2022_jp_ext",
        0xcf => variant == "iso2022_jp_3",
        0xd1 => variant == "iso2022_jp_2004",
        0xd0 => matches!(variant, "iso2022_jp_3" | "iso2022_jp_2004"),
        _ => false,
    }
}
fn escape(raw: &[u8], variant: &str) -> Result<(usize, usize, u8)> {
    let mut i = 1;
    let mut length = 0;
    while i < 16 {
        let byte = *raw.get(i).ok_or_else(invalid)?;
        if byte.is_ascii_uppercase() || byte == b'@' {
            length = i + 1;
            break;
        } else if byte == b'&' && raw.get(i + 1) == Some(&b'@') {
            i += 2;
        }
        i += 1;
    }
    let (group, mark) = match length {
        3 if raw[1] == b'$' => (0, raw[2] | 0x80),
        3 if matches!(raw[1], b'(' | b')') => (usize::from(raw[1] == b')'), raw[2]),
        4 if raw[1] == b'$' && matches!(raw[2], b'(' | b')') => {
            (usize::from(raw[2] == b')'), raw[3] | 0x80)
        }
        6 if raw[3..6] == [0x1b, b'$', b'B'] => (0, 0xc2),
        _ => return Err(invalid()),
    };
    if !designated(variant, mark) {
        return Err(invalid());
    }
    Ok((length, group, mark))
}
fn iso_value(raw: &[u8], mark: u8) -> Option<(usize, u32)> {
    let first = raw[0];
    if mark == b'J' {
        return Some((1, roman(first)));
    }
    if mark == b'I' {
        return (0x21..=0x5f)
            .contains(&first)
            .then_some((1, 0xff40 + u32::from(first)));
    }
    let second = *raw.get(1)?;
    let value = match mark {
        0xc0 | 0xc2 => {
            if (first, second) == (0x21, 0x40) {
                Some(0xff3c)
            } else {
                table(0, first, second)
            }
        }
        0xc4 => table(1, first, second),
        0xcf | 0xd1 => {
            if (first, second) == (0x21, 0x40) {
                Some(0xff3c)
            } else {
                plane1(first, second, mark == 0xcf)
            }
        }
        // Pinned iso2022's old plane2 macro is followed by a separate `if`,
        // not `else if`: the table lookup overwrites the old-2000 substitution.
        0xd0 => plane2(first, second, false),
        _ => None,
    }?;
    Some((2, value))
}
pub(super) fn iso2022(raw: &[u8], variant: &str) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    let mut groups = [b'B'; 2];
    let mut throughout = false;
    while let Some(&byte) = raw.get(i) {
        if throughout {
            // CPython's unknown-escape pass-through deliberately accepts all
            // ISO-8859-1 bytes until an ASCII uppercase or '@' terminates it.
            out.push(char::from(byte));
            throughout = !(byte.is_ascii_uppercase() || byte == b'@');
            i += 1;
        } else if byte == 0x1b {
            let next = *raw.get(i + 1).ok_or_else(invalid)?;
            if matches!(next, b'(' | b')' | b'$' | b'.' | b'&') {
                let (length, group, mark) = escape(&raw[i..], variant)?;
                groups[group] = mark;
                i += length;
            } else {
                out.push('\x1b');
                throughout = true;
                i += 1;
            }
        } else if byte < 0x20 || (byte < 0x80 && groups[0] == b'B') {
            out.push(char::from(byte));
            i += 1;
        } else if byte >= 0x80 {
            return Err(invalid());
        } else {
            let (width, value) = iso_value(&raw[i..], groups[0]).ok_or_else(invalid)?;
            push(&mut out, value)?;
            i += width;
        }
    }
    Ok(out)
}
