//! Python int(str)/int(bytes) normalization for typed Params and environment values.
pub fn integer_text(value: &str) -> Option<String> {
    let value = value.trim();
    let (negative, digits) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut output = String::new();
    let mut previous_digit = false;
    for point in digits.chars().map(u32::from) {
        if point == u32::from('_') && previous_digit {
            previous_digit = false;
            continue;
        }
        let digit = DECIMAL_ZEROES
            .iter()
            .find_map(|zero| point.checked_sub(*zero).filter(|digit| *digit < 10))?;
        output.push(char::from_u32(u32::from('0') + digit)?);
        previous_digit = true;
    }
    if !previous_digit || output.len() > 4300 {
        return None;
    }
    let output = output.trim_start_matches('0');
    Some(if output.is_empty() {
        "0".into()
    } else if negative {
        format!("-{output}")
    } else {
        output.into()
    })
}
pub fn integer_bytes(value: &[u8]) -> Option<String> {
    value.is_ascii().then_some(())?;
    integer_text(std::str::from_utf8(value).ok()?)
}
// Same Unicode 15.0 decimal table as the pinned Python 3.12 source and web-upload compatibility.
const DECIMAL_ZEROES: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60, 0x16ac0,
    0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    0x1fbf0,
];

pub fn integer_value(
    params: &dyn super::Read,
    key: &str,
    return_default: bool,
) -> Result<Option<String>, crate::Error> {
    let value = params.bytes(key)?.as_deref().and_then(integer_bytes);
    Ok(value.or_else(|| {
        return_default
            .then(|| {
                openpilot_params::metadata(key)
                    .and_then(|item| item.default)
                    .and_then(integer_text)
            })
            .flatten()
    }))
}
pub fn integer(
    params: &dyn super::Read,
    key: &str,
    return_default: bool,
) -> Result<Option<i32>, crate::Error> {
    integer_value(params, key, return_default)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| crate::Error::Parameter(key.into()))
        })
        .transpose()
}
