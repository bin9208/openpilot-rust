use crate::Error;

pub(crate) fn parse(bytes: &[u8]) -> Result<Option<i32>, Error> {
    let space = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r');
    let mut value = bytes;
    while value.first().is_some_and(space) {
        value = &value[1..];
    }
    while value.last().is_some_and(space) {
        value = &value[..value.len() - 1];
    }
    let negative = value.first() == Some(&b'-');
    if matches!(value.first(), Some(b'-' | b'+')) {
        value = &value[1..];
    }
    let mut digits = 0usize;
    let mut previous_digit = false;
    let mut magnitude = Some(0u64);
    for &byte in value {
        if byte == b'_' && previous_digit {
            previous_digit = false;
        } else if byte.is_ascii_digit() {
            previous_digit = true;
            digits += 1;
            magnitude =
                magnitude.and_then(|v| v.checked_mul(10)?.checked_add(u64::from(byte - b'0')));
        } else {
            return Ok(None);
        }
    }
    if !previous_digit || digits > 4300 {
        return Ok(None);
    }
    let magnitude = magnitude.ok_or(Error::PidRange)?;
    let signed = i64::try_from(magnitude).map_err(|_| Error::PidRange)?;
    let signed = if negative { -signed } else { signed };
    Ok(Some(i32::try_from(signed).map_err(|_| Error::PidRange)?))
}

pub(crate) fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut output = String::from("b");
    output.push(char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => output.push_str("\\t"),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\\' => output.push_str("\\\\"),
            value if value == quote => {
                output.push('\\');
                output.push(char::from(value));
            }
            32..=126 => output.push(char::from(byte)),
            value => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                output.push_str("\\x");
                output.push(char::from(HEX[usize::from(value >> 4)]));
                output.push(char::from(HEX[usize::from(value & 15)]));
            }
        }
    }
    output.push(char::from(quote));
    output
}
