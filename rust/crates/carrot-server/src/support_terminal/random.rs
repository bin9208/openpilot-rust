use crate::Error;
use base64::Engine;
use std::{fs::File, io::Read};

pub(super) fn token<const N: usize>() -> Result<String, Error> {
    let mut bytes = [0; N];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
pub(super) fn pin() -> Result<String, Error> {
    let mut random = File::open("/dev/urandom")?;
    loop {
        let mut bytes = [0; 4];
        random.read_exact(&mut bytes)?;
        let number = u32::from_ne_bytes(bytes);
        // Rejection leaves equally many samples for each six-digit PIN.
        if number < u32::MAX - (u32::MAX % 1_000_000) {
            return Ok(format!("{:06}", number % 1_000_000));
        }
    }
}
