//! CPython 3.12.15 `encodings.idna` strict bytes decoder, PSF license.
//!
//! Implements the byte-to-str boundary used by changed-files source decoding.
//! It deliberately retains CPython's no-ACE ASCII fast path, case-sensitive
//! prefix check, Unicode 3.2 nameprep, and ToASCII round-trip validation.
//! Source pins and literal-data conversion details live in idna/provenance.json.
//! No external Unicode/IDNA library or executable provider is used at runtime.
mod data;
#[cfg(test)]
mod tests;

use super::{Diagnostic, Phase};
fn invalid(message: &str) -> Diagnostic {
    Diagnostic::new(Phase::Decode, message)
}
fn scalar_string(points: &[u32]) -> String {
    points
        .iter()
        .map(|&cp| char::from_u32(cp).expect("pinned scalar mapping"))
        .collect()
}
fn b3(cp: u32, output: &mut Vec<u32>) {
    if !data::mapping(data::B3, cp, output) {
        output.push(cp);
    }
}
fn nfkc(input: &[u32]) -> Vec<u32> {
    let mut decomposed = Vec::new();
    let mut stack: Vec<u32> = input.iter().copied().rev().collect();
    while let Some(cp) = stack.pop() {
        if (0xac00..0xd7a4).contains(&cp) {
            let index = cp - 0xac00;
            decomposed.push(0x1100 + index / 588);
            decomposed.push(0x1161 + (index % 588) / 28);
            if !index.is_multiple_of(28) {
                decomposed.push(0x11a7 + index % 28);
            }
        } else {
            let start = stack.len();
            if data::mapping(data::DECOMP, cp, &mut stack) {
                stack[start..].reverse();
            } else {
                decomposed.push(cp);
            }
        }
    }
    // CPython's normalization uses its current CCC table even for ucd_3_2_0.
    // Stable ordering of nonstarter runs preserves equal-class input order.
    let mut start = 0;
    while start < decomposed.len() {
        if data::properties(decomposed[start]).0 == 0 {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < decomposed.len() && data::properties(decomposed[end]).0 != 0 {
            end += 1;
        }
        decomposed[start..end].sort_by_key(|&cp| data::properties(cp).0);
        start = end;
    }
    let mut composed = Vec::new();
    let mut starter: Option<usize> = None;
    let mut previous_class = 0;
    for cp in decomposed {
        let class = data::properties(cp).0;
        if let Some(index) = starter {
            if previous_class == 0 || previous_class < class {
                if let Some(value) = compose(composed[index], cp) {
                    composed[index] = value;
                    continue;
                }
            }
        }
        if class == 0 {
            starter = Some(composed.len());
        }
        composed.push(cp);
        previous_class = class;
    }
    composed
}
fn compose(first: u32, last: u32) -> Option<u32> {
    if (0x1100..0x1113).contains(&first) && (0x1161..0x1176).contains(&last) {
        Some(0xac00 + ((first - 0x1100) * 21 + last - 0x1161) * 28)
    } else if (0xac00..0xd7a4).contains(&first)
        && (first - 0xac00).is_multiple_of(28)
        && (0x11a8..0x11c3).contains(&last)
    {
        Some(first + last - 0x11a7)
    } else {
        data::composition(first, last)
    }
}
fn map_to_nothing(cp: u32) -> bool {
    matches!(cp, 0xad | 0x34f | 0x1806 | 0x180b..=0x180d | 0x200b..=0x200d | 0x2060 | 0xfe00..=0xfe0f | 0xfeff)
}
fn prohibited(cp: u32, flags: u8) -> bool {
    flags & 1 != 0
        || matches!(cp,
            0x6dd | 0x70f | 0x180e | 0x200c | 0x200d | 0x2028 | 0x2029 | 0xfeff |
            0x2060..=0x2063 | 0x206a..=0x206f | 0xfff9..=0xfffd | 0x1d173..=0x1d17a |
            0xfdd0..=0xfdef | 0x2ff0..=0x2ffb | 0x340 | 0x341 | 0x200e | 0x200f |
            0x202a..=0x202e | 0xe0001 | 0xe0020..=0xe007f)
        || cp & 0xffff >= 0xfffe
}
fn nameprep(text: &str) -> Result<String, Diagnostic> {
    let mut mapped = Vec::new();
    for cp in text
        .chars()
        .map(u32::from)
        .filter(|&cp| !map_to_nothing(cp))
    {
        // stringprep.map_table_b2: return the original B3 mapping when the two
        // normalization stages agree; the final whole-label NFKC is separate.
        let mut al = Vec::new();
        b3(cp, &mut al);
        let b = nfkc(&al);
        let mut bl = Vec::new();
        for &cp in &b {
            b3(cp, &mut bl);
        }
        let c = nfkc(&bl);
        mapped.extend(if b == c { al } else { c });
    }
    let prepared = nfkc(&mapped);
    let mut has_randal = false;
    let mut has_l = false;
    for &cp in &prepared {
        let flags = data::properties(cp).1;
        if prohibited(cp, flags) {
            return Err(invalid("IDNA nameprep prohibited character"));
        }
        has_randal |= flags & 2 != 0;
        has_l |= flags & 4 != 0;
    }
    if has_randal {
        if has_l {
            return Err(invalid("IDNA nameprep BIDI requirement 2"));
        }
        if prepared
            .first()
            .is_none_or(|&cp| data::properties(cp).1 & 2 == 0)
            || prepared
                .last()
                .is_none_or(|&cp| data::properties(cp).1 & 2 == 0)
        {
            return Err(invalid("IDNA nameprep BIDI requirement 3"));
        }
    }
    Ok(scalar_string(&prepared))
}
fn adapt(mut delta: u128, first: bool, count: u128) -> u128 {
    delta /= if first { 700 } else { 2 };
    delta += delta / count;
    let mut divisions = 0;
    while delta > 455 {
        delta /= 35;
        divisions += 36;
    }
    divisions + 36 * delta / (delta + 38)
}
fn punycode_encode(text: &str) -> Vec<u8> {
    // RFC 3492 encoding of the prepared label, used only for round-trip checks.
    // ToUnicode bounds the original encoded label to 1024 bytes; the fixed
    // scalar/mapping domain bounds these arithmetic intermediates below u128.
    const DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let points: Vec<u128> = text.chars().map(|cp| u128::from(u32::from(cp))).collect();
    let mut output: Vec<u8> = text.bytes().filter(u8::is_ascii).collect();
    let basic = output.len();
    let mut handled = basic;
    if basic != 0 {
        output.push(b'-');
    }
    let mut code = 128u128;
    let mut delta = 0u128;
    let mut bias = 72u128;
    while handled < points.len() {
        let next = *points.iter().filter(|&&cp| cp >= code).min().unwrap();
        delta += (next - code) * (handled as u128 + 1);
        code = next;
        for &cp in &points {
            if cp < code {
                delta += 1;
            }
            if cp == code {
                let mut value = delta;
                let mut k = 36u128;
                loop {
                    let threshold = k.saturating_sub(bias).clamp(1, 26);
                    if value < threshold {
                        break;
                    }
                    output.push(
                        DIGITS[(threshold + (value - threshold) % (36 - threshold)) as usize],
                    );
                    value = (value - threshold) / (36 - threshold);
                    k += 36;
                }
                output.push(DIGITS[value as usize]);
                bias = adapt(delta, handled == basic, handled as u128 + 1);
                delta = 0;
                handled += 1;
            }
        }
        delta += 1;
        code += 1;
    }
    output
}
fn label_size(raw: Vec<u8>) -> Result<Vec<u8>, Diagnostic> {
    if raw.is_empty() || raw.len() >= 64 {
        Err(invalid("IDNA label empty or too long"))
    } else {
        Ok(raw)
    }
}
fn to_ascii(text: &str) -> Result<Vec<u8>, Diagnostic> {
    // UseSTD3ASCIIRules is false; even ASCII controls pass the fast path.
    if text.is_ascii() {
        return label_size(text.as_bytes().to_vec());
    }
    let prepared = nameprep(text)?;
    if prepared.is_ascii() {
        return label_size(prepared.into_bytes());
    }
    if prepared.starts_with("xn--") {
        return Err(invalid("IDNA label starts with ACE prefix"));
    }
    let mut output = b"xn--".to_vec();
    output.extend(punycode_encode(&prepared));
    label_size(output)
}
fn to_unicode(raw: &[u8]) -> Result<String, Diagnostic> {
    if raw.len() > 1024 {
        return Err(invalid("IDNA label way too long"));
    }
    // The input is bytes, so CPython's pure_ascii flag is true, even if the
    // bytes are non-ASCII. ASCII decoding/Punycode subsequently rejects them.
    if !raw.is_ascii() {
        return Err(invalid("non-ASCII byte in IDNA label"));
    }
    let Some(extended) = raw.strip_prefix(b"xn--") else {
        return Ok(String::from_utf8(raw.to_vec()).expect("ASCII IDNA label"));
    };
    let decoded = super::punycode::decode(extended)?;
    let roundtrip = to_ascii(&decoded)?;
    if raw.to_ascii_lowercase() != roundtrip {
        return Err(invalid("IDNA does not round-trip"));
    }
    Ok(decoded)
}
pub(super) fn decode(raw: &[u8]) -> Result<String, Diagnostic> {
    if !raw.windows(4).any(|bytes| bytes == b"xn--") && raw.is_ascii() {
        return Ok(String::from_utf8(raw.to_vec()).expect("ASCII IDNA payload"));
    }
    let mut labels: Vec<&[u8]> = raw.split(|&byte| byte == b'.').collect();
    let trailing_dot = raw.ends_with(b".");
    if trailing_dot {
        labels.pop();
    }
    let mut result = Vec::with_capacity(labels.len());
    for label in labels {
        result.push(to_unicode(label)?);
    }
    let mut output = result.join(".");
    if trailing_dot {
        output.push('.');
    }
    Ok(output)
}
