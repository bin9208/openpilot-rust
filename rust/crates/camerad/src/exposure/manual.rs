use thiserror::Error;

#[derive(Clone, Copy, Debug, Default)]
pub struct ManualExposure<'a> {
    pub gain: &'a str,
    pub time: &'a str,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid_argument")]
    InvalidArgument,
    #[error("out_of_range")]
    OutOfRange,
}

pub(super) fn stoi(value: &str) -> Result<i32, ParseError> {
    let value = value.trim_start_matches([' ', '\t', '\n', '\r', '\u{b}', '\u{c}']);
    let sign = usize::from(value.starts_with(['+', '-']));
    let digits = value
        .bytes()
        .skip(sign)
        .take_while(u8::is_ascii_digit)
        .count();
    if digits == 0 {
        return Err(ParseError::InvalidArgument);
    }
    value[..sign + digits]
        .parse()
        .map_err(|_| ParseError::OutOfRange)
}
