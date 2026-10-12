use crate::Error;
use std::fmt::Write;

pub fn encode(
    fields: &[(&str, String)],
    filename: &str,
    file: &[u8],
) -> Result<(Vec<u8>, String), Error> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| Error::Source(error.to_string()))?;
    let mut boundary = "----CarrotRecovery".to_owned();
    for byte in random {
        write!(boundary, "{byte:02x}").map_err(|error| Error::Source(error.to_string()))?;
    }
    let mut bytes = Vec::new();
    for (name, value) in fields {
        bytes.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    bytes.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{filename}\"\r\nContent-Type: text/plain\r\n\r\n").as_bytes());
    bytes.extend_from_slice(file);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok((bytes, format!("multipart/form-data; boundary={boundary}")))
}
