use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStringExt,
};

fn points(value: &OsStr) -> Vec<u32> {
    let mut bytes = value.as_encoded_bytes();
    let mut output = Vec::new();
    loop {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                output.extend(text.chars().map(u32::from));
                return output;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if let Ok(text) = std::str::from_utf8(&bytes[..valid]) {
                    output.extend(text.chars().map(u32::from));
                }
                let invalid = error.error_len().unwrap_or(bytes.len() - valid);
                output.extend(
                    bytes[valid..valid + invalid]
                        .iter()
                        .map(|&byte| 0xdc00 + u32::from(byte)),
                );
                bytes = &bytes[valid + invalid..];
            }
        }
    }
}

pub fn sort_key(value: &OsStr) -> (bool, Vec<Vec<u32>>) {
    let values = points(value);
    let split = values.windows(2).rposition(|pair| pair == [45, 45]);
    let mut parts = match split {
        Some(index) => vec![values[..index].to_vec(), values[index + 2..].to_vec()],
        None => vec![values],
    };
    for part in &mut parts {
        if part.len() < 10 {
            part.splice(0..0, std::iter::repeat_n(48, 10 - part.len()));
        }
    }
    (!value.as_encoded_bytes().starts_with(b"2024-"), parts)
}

const DECIMAL_ZEROES: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60, 0x16ac0,
    0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    0x1fbf0,
];

fn nonnegative_integer(value: &[u8]) -> Option<Vec<u8>> {
    let value = std::str::from_utf8(value).ok()?.trim();
    let (negative, value) = if let Some(rest) = value.strip_prefix('-') {
        (true, rest)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut digits = Vec::new();
    let mut previous_digit = false;
    for point in value.chars().map(u32::from) {
        if point == 95 && previous_digit {
            previous_digit = false;
            continue;
        }
        let zero = DECIMAL_ZEROES
            .iter()
            .find(|&&zero| (zero..zero + 10).contains(&point))?;
        digits.push(b'0' + u8::try_from(point - zero).ok()?);
        previous_digit = true;
    }
    if !previous_digit || digits.len() > 4300 {
        return None;
    }
    let first = digits
        .iter()
        .position(|&digit| digit != b'0')
        .unwrap_or(digits.len() - 1);
    digits.drain(..first);
    if negative && digits != b"0" {
        None
    } else {
        Some(digits)
    }
}

fn previous(digits: &mut Vec<u8>) -> bool {
    if digits == b"0" {
        return false;
    }
    for digit in digits.iter_mut().rev() {
        if *digit == b'0' {
            *digit = b'9';
        } else {
            *digit -= 1;
            break;
        }
    }
    if digits[0] == b'0' && digits.len() > 1 {
        digits.remove(0);
    }
    true
}

pub fn segment_and_prior(name: &OsStr) -> Vec<OsString> {
    let bytes = name.as_encoded_bytes();
    let Some(index) = bytes.windows(2).rposition(|pair| pair == b"--") else {
        return Vec::new();
    };
    if index == 0 {
        return Vec::new();
    }
    let Some(mut digits) = nonnegative_integer(&bytes[index + 2..]) else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for _ in 0..3 {
        let mut path = bytes[..index + 2].to_vec();
        path.extend(&digits);
        output.push(OsString::from_vec(path));
        if !previous(&mut digits) {
            break;
        }
    }
    output
}
