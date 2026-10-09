use crate::Error;

#[allow(unsafe_code)]
pub(crate) fn float_text(value: f32) -> Result<String, Error> {
    let mut buffer = [0_u8; 64];
    // SAFETY: snprintf receives a valid writable buffer/size, a static format and the promoted float argument.
    let length = unsafe {
        libc::snprintf(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            c"%f".as_ptr(),
            f64::from(value),
        )
    };
    let length = usize::try_from(length)
        .ok()
        .filter(|&length| length < buffer.len())
        .ok_or_else(|| Error::Source("float parameter formatting failed".into()))?;
    String::from_utf8(buffer[..length].to_vec())
        .map_err(|_| Error::Source("float parameter formatting is not UTF-8".into()))
}
