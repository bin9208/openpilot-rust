//! Whole-buffer strict ISO-2022-JP-2, ported from CPython 3.12.15 (PSF-2.0).
//! Source/data pins and reused Japanese mapping provenance: jp2-provenance.json.
//! This module does not change the independently accepted Japanese variants.
use super::{Diagnostic, Phase};

type Result<T> = std::result::Result<T, Diagnostic>;
const JP: &[u8; 247408] = include_bytes!("japanese-variant-tables.bin");
const OTHER: &[u8; 35344] = include_bytes!("jp2-tables.bin");

fn invalid() -> Diagnostic {
    Diagnostic::new(
        Phase::Decode,
        "undefined or truncated strict ISO-2022-JP-2 sequence",
    )
}
fn cell(first: u8, second: u8) -> Option<usize> {
    if !(0x21..=0x7e).contains(&first) || !(0x21..=0x7e).contains(&second) {
        return None;
    }
    Some(usize::from(first - 0x21) * 94 + usize::from(second - 0x21))
}
fn japanese(index: usize, first: u8, second: u8) -> Option<u32> {
    let at = (index * 94 * 94 + cell(first, second)?) * 4;
    let value = u32::from_le_bytes(JP[at..at + 4].try_into().expect("fixed Japanese cell"));
    (value != u32::MAX).then_some(value)
}
fn other(index: usize, first: u8, second: u8) -> Option<u32> {
    let at = (index * 94 * 94 + cell(first, second)?) * 2;
    let value = u16::from_le_bytes(OTHER[at..at + 2].try_into().expect("fixed JP2 cell"));
    (value != u16::MAX).then_some(u32::from(value))
}
fn push(out: &mut String, value: u32) -> Result<()> {
    out.push(char::from_u32(value).ok_or_else(invalid)?);
    Ok(())
}
fn escape_end(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte == b'@'
}
fn escape(raw: &[u8]) -> Result<(usize, usize, u8)> {
    // CPython scans at most 16 bytes and skips &@ while finding the end.
    // Its six-byte prefix branch only checks the final ESC $ B bytes.
    let mut i = 1;
    let mut length = 0;
    while i < 16 {
        let byte = *raw.get(i).ok_or_else(invalid)?;
        if escape_end(byte) {
            length = i + 1;
            break;
        } else if byte == b'&' && raw.get(i + 1) == Some(&b'@') {
            i += 2;
        }
        i += 1;
    }
    let (group, mark) = match length {
        3 if raw[1] == b'$' => (0, raw[2] | 0x80),
        3 if matches!(raw[1], b'(' | b')' | b'.') => (
            match raw[1] {
                b'(' => 0,
                b')' => 1,
                _ => 2,
            },
            raw[2],
        ),
        4 if raw[1] == b'$' && matches!(raw[2], b'(' | b')') => {
            (usize::from(raw[2] == b')'), raw[3] | 0x80)
        }
        6 if raw[3..6] == [0x1b, b'$', b'B'] => (0, 0xc2),
        _ => return Err(invalid()),
    };
    // Like CPython, validate the registry mark, not its nominal plane. G1 is
    // stored but never invoked: JP2's NO_SHIFT emits SI/SO as ordinary C0.
    if !matches!(mark, b'B' | b'J' | b'A' | b'F' | 0xc0..=0xc4) {
        return Err(invalid());
    }
    Ok((length, group, mark))
}
fn greek(byte: u8) -> Option<u32> {
    // ISO8859_7_DECODE in _codecs_iso2022.c is an older, explicit repertoire;
    // substituting the modern ISO-8859-7 codec changes its undefined positions.
    let c = byte ^ 0x80;
    if c < 0xa0 || (c < 0xc0 && (0x288f3bc9u32 & (1u32 << (c - 0xa0))) != 0) {
        Some(u32::from(c))
    } else if (0xb4..=0xfe).contains(&c)
        && (c >= 0xd4 || (0xbffffd77u32 & (1u32 << (c - 0xb4))) != 0)
    {
        Some(0x02d0 + u32::from(c))
    } else {
        match c {
            0xa1 => Some(0x2018),
            0xa2 => Some(0x2019),
            0xaf => Some(0x2015),
            _ => None,
        }
    }
}
fn single_shift(group: u8, byte: u8) -> Option<u32> {
    match group {
        b'A' if byte < 0x80 => Some(u32::from(byte) + 0x80),
        b'F' => greek(byte),
        b'B' if byte < 0x80 => Some(u32::from(byte)),
        // Other admitted G2 registry marks hit CPython MBERR_INTERNAL, which
        // is a rejection for this Result-based whole-buffer admission API.
        _ => None,
    }
}
fn g0(raw: &[u8], mark: u8) -> Option<(usize, u32)> {
    let first = raw[0];
    if mark == b'J' {
        return Some((
            1,
            match first {
                b'\\' => 0xa5,
                b'~' => 0x203e,
                _ => u32::from(first),
            },
        ));
    }
    // ISO8859 G0 entries use CPython's dummy_decoder (width1, unmappable).
    if matches!(mark, b'A' | b'F') {
        return None;
    }
    let second = *raw.get(1)?;
    let value = match mark {
        0xc0 | 0xc2 if (first, second) == (0x21, 0x40) => Some(0xff3c),
        0xc0 | 0xc2 => japanese(0, first, second),
        0xc4 => japanese(1, first, second),
        0xc1 => other(0, first, second),
        0xc3 => other(1, first, second),
        _ => None,
    }?;
    Some((2, value))
}
pub(super) fn decode(raw: &[u8]) -> Result<String> {
    // Every call initializes G0/G1/G2 to ASCII. No end-of-buffer ASCII reset
    // is required. LF preserves designations (including G2) under NO_SHIFT.
    let mut groups = [b'B'; 3];
    let mut throughout = false;
    let mut out = String::new();
    let mut i = 0;
    while let Some(&byte) = raw.get(i) {
        if throughout {
            // Unknown ESC sequences pass Latin1 through until uppercase/@;
            // even a trailing incomplete sequence succeeds in this state.
            out.push(char::from(byte));
            throughout = !escape_end(byte);
            i += 1;
        } else if byte == 0x1b {
            let next = *raw.get(i + 1).ok_or_else(invalid)?;
            if matches!(next, b'(' | b')' | b'.' | b'$' | b'&') {
                let (length, group, mark) = escape(&raw[i..])?;
                groups[group] = mark;
                i += length;
            } else if next == b'N' {
                let payload = *raw.get(i + 2).ok_or_else(invalid)?;
                push(
                    &mut out,
                    single_shift(groups[2], payload).ok_or_else(invalid)?,
                )?;
                i += 3;
            } else {
                out.push('\x1b');
                throughout = true;
                i += 1;
            }
        } else if byte >= 0x80 {
            return Err(invalid());
        } else if byte < 0x20 || groups[0] == b'B' {
            out.push(char::from(byte));
            i += 1;
        } else {
            let (width, value) = g0(&raw[i..], groups[0]).ok_or_else(invalid)?;
            push(&mut out, value)?;
            i += width;
        }
    }
    Ok(out)
}
