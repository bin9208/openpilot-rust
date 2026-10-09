use super::Failure;
use std::{fs, path::Path};

pub(super) fn git_running(root: &Path) -> Result<bool, Failure> {
    if !root.is_dir() {
        return Ok(true);
    }
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(true);
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return Ok(true);
        };
        if !entry
            .file_name()
            .as_encoded_bytes()
            .iter()
            .all(u8::is_ascii_digit)
        {
            continue;
        }
        let bytes = match fs::read(entry.path().join("comm")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Ok(true),
        };
        let name = crate::request_text::decode(&bytes, "utf-8").map_err(Failure::Decode)?;
        let name =
            name.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        if name == "git" || name.starts_with("git-") {
            return Ok(true);
        }
    }
    Ok(false)
}
