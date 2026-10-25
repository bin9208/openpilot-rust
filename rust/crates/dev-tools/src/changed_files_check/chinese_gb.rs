//! CPython 3.12.15 strict GB2312/GBK/GB18030 decoding.
//! PSF mapping provenance and license are beside these implementation tables.
use super::{Diagnostic, Phase};

// Reuse the byte-identical GB2312 mapping already admitted for HZ, without
// sharing HZ state or changing its decoder. Values are u16LE; 0xffff is absent.
const GB2312: &[u8; 17672] = include_bytes!("hz-gb2312.bin");
const GBK_EXT: &[u8; 131072] = include_bytes!("gb-gbkext.bin");
const GB18030_EXT: &[u8; 131072] = include_bytes!("gb-gb18030ext.bin");
// (Unicode first, Unicode last, four-byte pointer base), all u32LE.
// The terminal triple has first=last=0 and base=39420.
const BMP_RANGES: &[u8; 2484] = include_bytes!("gb-bmp-ranges.bin");

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Codec {
    Gb2312,
    Gbk,
    Gb18030,
}

fn invalid(offset: usize) -> Diagnostic {
    Diagnostic::new(
        Phase::Decode,
        format!("undefined or truncated strict Chinese GB sequence at byte {offset}"),
    )
}

fn value(table: &[u8], index: usize) -> Option<u32> {
    let at = index * 2;
    let cp = u16::from_le_bytes([table[at], table[at + 1]]);
    (cp != u16::MAX).then_some(u32::from(cp))
}

fn gb2312(first: u8, second: u8) -> Option<u32> {
    if !(0xa1..=0xfe).contains(&first) || !(0xa1..=0xfe).contains(&second) {
        return None;
    }
    value(
        GB2312,
        usize::from(first - 0xa1) * 94 + usize::from(second - 0xa1),
    )
}

fn gbk(first: u8, second: u8) -> Option<u32> {
    // GBK_DECODE overrides precede both maps in the pinned original decoder.
    match (first, second) {
        (0xa1, 0xa4) => Some(0x00b7),
        (0xa1, 0xaa) => Some(0x2014),
        (0xa8, 0x44) => Some(0x2015),
        _ => gb2312(first, second)
            .or_else(|| value(GBK_EXT, usize::from(first) * 256 + usize::from(second))),
    }
}

fn u32_at(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]])
}

fn four(first: u8, second: u8, third: u8, fourth: u8) -> Option<u32> {
    if !(0x81..=0xfe).contains(&first)
        || !(0x30..=0x39).contains(&second)
        || !(0x81..=0xfe).contains(&third)
        || !(0x30..=0x39).contains(&fourth)
    {
        return None;
    }
    let pointer = ((u32::from(first - 0x81) * 10 + u32::from(second - 0x30)) * 126
        + u32::from(third - 0x81))
        * 10
        + u32::from(fourth - 0x30);
    if pointer < 39420 {
        // Select the last base <= pointer, retaining the pinned original's
        // discontinuities (in particular skipping surrogate code points).
        let count = BMP_RANGES.len() / 12 - 1;
        let (mut low, mut high) = (0, count);
        while low < high {
            let mid = low + (high - low) / 2;
            if u32_at(BMP_RANGES, mid * 12 + 8) <= pointer {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let at = low.checked_sub(1)? * 12;
        let cp = u32_at(BMP_RANGES, at) + pointer - u32_at(BMP_RANGES, at + 8);
        (cp <= u32_at(BMP_RANGES, at + 4)).then_some(cp)
    } else if pointer >= 189000 {
        let cp = 0x10000 + pointer - 189000;
        (cp <= 0x10ffff).then_some(cp)
    } else {
        None
    }
}

pub(super) fn decode(raw: &[u8], codec: Codec) -> Result<String, Diagnostic> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(&first) = raw.get(i) {
        if first < 0x80 {
            out.push(char::from(first));
            i += 1;
            continue;
        }
        // No Euro shortcut for 0x80: CPython rejects it in all three codecs.
        let second = *raw.get(i + 1).ok_or_else(|| invalid(i))?;
        let (cp, width) = if codec == Codec::Gb18030 && (0x30..=0x39).contains(&second) {
            let third = *raw.get(i + 2).ok_or_else(|| invalid(i))?;
            let fourth = *raw.get(i + 3).ok_or_else(|| invalid(i))?;
            (four(first, second, third, fourth), 4)
        } else if codec == Codec::Gb2312 {
            (gb2312(first, second), 2)
        } else {
            let cp = gbk(first, second).or_else(|| {
                if codec == Codec::Gb18030 {
                    value(GB18030_EXT, usize::from(first) * 256 + usize::from(second))
                } else {
                    None
                }
            });
            (cp, 2)
        };
        out.push(cp.and_then(char::from_u32).ok_or_else(|| invalid(i))?);
        i += width;
    }
    Ok(out)
}
