//! The source startup gate reads only IDs/speed and skips unreadable USB entries.
use crate::hardware::USB_IDS;
use std::{fs, path::Path};

fn read(path: &Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => Some(value.trim().to_owned()),
        Err(_) => None,
    }
}

fn identifier(text: &str) -> Option<u16> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let digits = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(digits) => digits.strip_prefix('_').unwrap_or(digits),
        None => text,
    };
    let groups = digits.split('_').collect::<Vec<_>>();
    if groups
        .iter()
        .any(|group| group.is_empty() || !group.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return None;
    }
    u16::from_str_radix(&groups.concat(), 16).ok()
}

fn speed(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.iter().enumerate().any(|(index, &byte)| {
        byte == b'_'
            && !(index
                .checked_sub(1)
                .is_some_and(|previous| bytes[previous].is_ascii_digit())
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
    }) {
        return None;
    }
    text.replace('_', "").parse::<f64>().ok()
}

/// Presence follows the source's SuperSpeed gate, without reading descriptive fields.
#[must_use]
pub fn present(root: &Path) -> bool {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(_) => continue,
        };
        let values = (
            read(&path.join("idVendor")).and_then(|text| identifier(&text)),
            read(&path.join("idProduct")).and_then(|text| identifier(&text)),
            read(&path.join("speed")).and_then(|text| speed(&text)),
        );
        if let (Some(vendor), Some(product), Some(speed)) = values {
            if USB_IDS.contains(&(vendor, product)) && speed >= 5000.0 {
                return true;
            }
        }
    }
    false
}
