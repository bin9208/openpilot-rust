use crate::{protocol, Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Debug)]
pub struct Tlv<'a> {
    pub tag: u64,
    pub value: &'a [u8],
    pub start: usize,
    pub end: usize,
    pub value_start: usize,
}
pub fn tlvs(data: &[u8]) -> Vec<Tlv<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let start = i;
        let mut tag = u64::from(data[i]);
        i += 1;
        if tag & 31 == 31 {
            while i < data.len() {
                let next = data[i];
                i += 1;
                let Some(v) = tag
                    .checked_mul(256)
                    .and_then(|v| v.checked_add(u64::from(next)))
                else {
                    return out;
                };
                tag = v;
                if next & 128 == 0 {
                    break;
                }
            }
        }
        let Some(&size) = data.get(i) else {
            break;
        };
        i += 1;
        let size = if size & 128 != 0 {
            let count = usize::from(size & 127);
            let Some(bytes) = data.get(i..i + count) else {
                break;
            };
            i += count;
            let Some(size) = bytes.iter().try_fold(0usize, |a, b| {
                a.checked_mul(256)?.checked_add(usize::from(*b))
            }) else {
                break;
            };
            size
        } else {
            usize::from(size)
        };
        let Some(end) = i.checked_add(size).filter(|end| *end <= data.len()) else {
            break;
        };
        out.push(Tlv {
            tag,
            value: &data[i..end],
            start,
            end,
            value_start: i,
        });
        i = end;
    }
    out
}
pub fn find(data: &[u8], tag: u64) -> Option<&[u8]> {
    tlvs(data)
        .into_iter()
        .find(|v| v.tag == tag)
        .map(|v| v.value)
}
pub fn require<'a>(data: &'a [u8], tag: u64, label: &str) -> Result<&'a [u8]> {
    find(data, tag).ok_or_else(|| {
        protocol(format!(
            "Missing {}",
            if label.is_empty() {
                format!("tag 0x{tag:X}")
            } else {
                label.to_owned()
            }
        ))
    })
}
pub fn integer(bytes: &[u8]) -> Result<u64> {
    bytes
        .iter()
        .try_fold(0u64, |n, b| n.checked_mul(256)?.checked_add(u64::from(*b)))
        .ok_or_else(|| protocol("integer exceeds 64 bits"))
}
pub fn int_bytes(n: u64) -> Vec<u8> {
    let bytes = n.to_be_bytes();
    bytes[bytes.iter().position(|v| *v != 0).unwrap_or(7)..].to_vec()
}
pub fn encode(tag: u64, value: &[u8]) -> Vec<u8> {
    let mut out = if tag > 255 {
        vec![(tag >> 8) as u8, tag as u8]
    } else {
        vec![tag as u8]
    };
    if value.len() <= 127 {
        out.push(value.len() as u8);
    } else {
        let len = int_bytes(value.len() as u64);
        out.push(128 | len.len() as u8);
        out.extend(len);
    }
    out.extend(value);
    out
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
pub fn unhex(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(Error::Value("invalid hex data".into()));
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| {
            let h = (v[0] as char).to_digit(16);
            let l = (v[1] as char).to_digit(16);
            h.zip(l)
                .map(|(h, l)| (h * 16 + l) as u8)
                .ok_or_else(|| Error::Value("invalid hex data".into()))
        })
        .collect()
}
pub fn utf8_ignore(mut bytes: &[u8]) -> String {
    let mut out = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(e) => {
                let (good, tail) = bytes.split_at(e.valid_up_to());
                if let Ok(s) = std::str::from_utf8(good) {
                    out.push_str(s);
                }
                bytes = &tail[e.error_len().unwrap_or(tail.len())..];
            }
        }
    }
    out
}
pub fn tbcd(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [b & 15, b >> 4])
        .filter(|n| *n <= 9)
        .map(|n| char::from(b'0' + n))
        .collect()
}
pub fn to_tbcd(s: &str) -> Vec<u8> {
    let digits: Vec<u8> = s
        .chars()
        .filter_map(|c| c.to_digit(10).map(|n| n as u8))
        .collect();
    digits
        .chunks(2)
        .map(|d| d[0] | (d.get(1).copied().unwrap_or(15) << 4))
        .collect()
}
pub fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}
pub fn trim_b64(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\n' | '\r' | ' ' | '\t'))
        .collect()
}
pub fn unb64(s: &str) -> Result<Vec<u8>> {
    STANDARD
        .decode(trim_b64(s))
        .map_err(|e| Error::Value(e.to_string()))
}
pub fn activation(code: &str) -> Result<(&str, &str)> {
    let parts: Vec<_> = code.strip_prefix("LPA:").unwrap_or("").split('$').collect();
    if !code.starts_with("LPA:") || parts.len() != 3 {
        return Err(Error::Value("Invalid activation code format".into()));
    }
    Ok((parts[1], parts[2]))
}
