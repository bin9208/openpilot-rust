//! common/params.h decimal-prefix std::stoi contract, also used by beepd.
#[expect(
    unsafe_code,
    reason = "CXX borrowed byte slice parser; no retained memory"
)]
mod bridge;

pub fn float(bytes: &[u8]) -> Option<f64> {
    let result = bridge::ffi::params_float(bytes);
    result.valid.then(|| f64::from(result.value))
}

pub fn integer(bytes: &[u8]) -> Option<i32> {
    if bytes.is_empty() {
        return Some(0);
    }
    let mut input = bytes.iter().copied().peekable();
    while input
        .peek()
        .is_some_and(|v| matches!(v, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
    {
        input.next();
    }
    let negative = match input.peek() {
        Some(b'-') => {
            input.next();
            true
        }
        Some(b'+') => {
            input.next();
            false
        }
        _ => false,
    };
    let mut magnitude = 0i64;
    let mut digits = false;
    while let Some(byte @ b'0'..=b'9') = input.next() {
        magnitude = magnitude * 10 + i64::from(byte - b'0');
        if magnitude > i64::from(i32::MAX) + i64::from(negative) {
            return None;
        }
        digits = true;
    }
    if !digits {
        return None;
    }
    i32::try_from(if negative { -magnitude } else { magnitude }).ok()
}
