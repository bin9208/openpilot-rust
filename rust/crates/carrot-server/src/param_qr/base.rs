use super::error;
use crate::Error;

const B45: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(crate) fn b45_encode(data: &[u8]) -> String {
    let mut output = String::new();
    for pair in data.chunks(2) {
        let value = if pair.len() == 2 {
            usize::from(pair[0]) * 256 + usize::from(pair[1])
        } else {
            usize::from(pair[0])
        };
        output.push(char::from(B45[value % 45]));
        output.push(char::from(B45[value / 45 % 45]));
        if pair.len() == 2 {
            output.push(char::from(B45[value / 2025]));
        }
    }
    output
}

pub(crate) fn b45_decode(text: &str) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    for group in text.as_bytes().chunks(3) {
        if group.len() == 1 {
            return Err(error("bad base45 payload"));
        }
        let mut value = 0;
        for (index, character) in group.iter().enumerate() {
            let digit = B45
                .iter()
                .position(|next| next == character)
                .ok_or_else(|| error("bad base45 payload"))?;
            value += digit * 45_usize.pow(index as u32);
        }
        if (group.len() == 3 && value > 65535) || (group.len() == 2 && value > 255) {
            return Err(error("bad base45 payload"));
        }
        if group.len() == 3 {
            output.push((value / 256) as u8);
        }
        output.push((value % 256) as u8);
    }
    Ok(output)
}

pub(crate) fn b64_encode(data: &[u8]) -> String {
    let mut output = String::new();
    for group in data.chunks(3) {
        let value = (u32::from(group[0]) << 16)
            | (u32::from(*group.get(1).unwrap_or(&0)) << 8)
            | u32::from(*group.get(2).unwrap_or(&0));
        for index in 0..group.len() + 1 {
            output.push(char::from(B64[((value >> (18 - index * 6)) & 63) as usize]));
        }
    }
    output
}

pub(crate) fn b64_decode(text: &str) -> Result<Vec<u8>, Error> {
    if !text.is_ascii() {
        return Err(error(
            "string argument should contain only ASCII characters",
        ));
    }
    let mut padded = text.as_bytes().to_vec();
    padded.extend(std::iter::repeat_n(b'=', (4 - text.len() % 4) % 4));
    let mut output = Vec::new();
    let mut bits = 0_u32;
    let mut count = 0;
    let mut digits = 0;
    let mut padding = 0;
    for character in padded {
        if character == b'=' {
            padding += 1;
            if (count == 2 && padding >= 2) || (count == 3 && padding >= 1) {
                return Ok(output);
            }
            continue;
        }
        let character = match character {
            b'+' => b'-',
            b'/' => b'_',
            _ => character,
        };
        let Some(digit) = B64.iter().position(|next| *next == character) else {
            continue;
        };
        padding = 0;
        digits += 1;
        bits = (bits << 6) | digit as u32;
        count = (count + 1) % 4;
        if count == 2 {
            output.push((bits >> 4) as u8);
        }
        if count == 3 {
            output.push((bits >> 2) as u8);
        }
        if count == 0 {
            output.push(bits as u8);
        }
    }
    if count == 1 {
        Err(error(format!(
            "Invalid base64-encoded string: number of data characters ({digits}) cannot be 1 more than a multiple of 4"
        )))
    } else if count != 0 {
        Err(error("Incorrect padding"))
    } else {
        Ok(output)
    }
}
