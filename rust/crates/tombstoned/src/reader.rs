//! Match UTF-8 TextIOWrapper's 8192-byte decode-ahead before yielding apport lines.
use crate::{parse::Metadata, Error};
use std::{fs::File, io::Read, path::Path};

pub(crate) fn metadata(path: &Path) -> Result<Metadata, Error> {
    let mut file = File::open(path)?;
    let mut metadata = Metadata::default();
    let mut pending_bytes = Vec::new();
    let mut pending_line = String::new();
    let mut pending_cr = false;
    loop {
        let mut chunk = [0_u8; 8192];
        let count = file.read(&mut chunk)?;
        pending_bytes.extend_from_slice(&chunk[..count]);
        let valid = match std::str::from_utf8(&pending_bytes) {
            Ok(_) => pending_bytes.len(),
            Err(error) if error.error_len().is_none() && count != 0 => error.valid_up_to(),
            Err(error) => return Err(Error::Decode(error)),
        };
        let text = std::str::from_utf8(&pending_bytes[..valid]).map_err(Error::Decode)?;
        let mut normalized = String::new();
        for point in text.chars() {
            if pending_cr {
                normalized.push('\n');
                pending_cr = false;
                if point == '\n' {
                    continue;
                }
            }
            if point == '\r' {
                pending_cr = true;
            } else {
                normalized.push(point);
            }
        }
        if count == 0 && pending_cr {
            normalized.push('\n');
        }
        pending_line.push_str(&normalized);
        pending_bytes.drain(..valid);
        while let Some(end) = pending_line.find('\n') {
            if !metadata.line(&pending_line[..=end]) {
                return Ok(metadata);
            }
            pending_line.drain(..=end);
        }
        if count == 0 {
            if !pending_line.is_empty() {
                metadata.line(&pending_line);
            }
            return Ok(metadata);
        }
    }
}
