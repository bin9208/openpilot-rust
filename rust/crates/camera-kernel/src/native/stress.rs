use std::ffi::CStr;

#[derive(Debug, thiserror::Error)]
pub enum DoubleParseError {
    #[error("invalid_argument")]
    InvalidArgument,
    #[error("out_of_range")]
    OutOfRange,
}

pub fn parse_double_prefix(value: &CStr) -> Result<f64, DoubleParseError> {
    let mut end = std::ptr::null_mut();
    // SAFETY: strtod reads the live NUL-terminated CStr and writes only the local end pointer;
    // errno is thread-local. Its prefix/range behavior is the source std::stod contract.
    let (number, errno) = unsafe {
        let previous = *libc::__errno_location();
        *libc::__errno_location() = 0;
        let number = libc::strtod(value.as_ptr(), &mut end);
        let errno = *libc::__errno_location();
        if errno == 0 {
            *libc::__errno_location() = previous;
        }
        (number, errno)
    };
    if end.cast_const() == value.as_ptr() {
        Err(DoubleParseError::InvalidArgument)
    } else if errno == libc::ERANGE {
        Err(DoubleParseError::OutOfRange)
    } else {
        Ok(number)
    }
}

pub fn random_unit() -> f64 {
    // SAFETY: rand has no pointer arguments; sharing the libc sequence matches the source.
    f64::from(unsafe { libc::rand() }) / f64::from(libc::RAND_MAX)
}
