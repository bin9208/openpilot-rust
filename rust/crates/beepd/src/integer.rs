#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IntegerError {
    #[error("std::stoi invalid_argument (no decimal prefix)")]
    Invalid,
    #[error("std::stoi out_of_range (outside signed 32-bit range)")]
    Range,
}

/// The Cython get_int calls std::stoi directly, not Python int or Params.get.
pub fn integer(bytes: &[u8]) -> Result<i32, IntegerError> {
    if bytes.is_empty() {
        return Ok(0);
    }
    let mut input = bytes.iter().copied().peekable();
    while input
        .peek()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
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
    let mut magnitude = 0_i64;
    let mut has_digits = false;
    while let Some(byte @ b'0'..=b'9') = input.next() {
        magnitude = magnitude * 10 + i64::from(byte - b'0');
        if magnitude > i64::from(i32::MAX) + i64::from(negative) {
            return Err(IntegerError::Range);
        }
        has_digits = true;
    }
    if !has_digits {
        return Err(IntegerError::Invalid);
    }
    i32::try_from(if negative { -magnitude } else { magnitude }).map_err(|_| IntegerError::Range)
}
