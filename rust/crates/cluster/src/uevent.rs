//! Exact autorun uevent action/product precedence; kernel events are UTF-8-lossy.
use std::collections::BTreeMap;

#[must_use]
pub fn decode(payload: &[u8]) -> BTreeMap<String, String> {
    let mut event = BTreeMap::new();
    for part in String::from_utf8_lossy(payload)
        .split('\0')
        .filter(|part| !part.is_empty())
    {
        if let Some((key, value)) = part.split_once('=') {
            event.insert(key.to_owned(), value.to_owned());
        } else if let Some((action, _)) = part.split_once('@') {
            event
                .entry("ACTION".into())
                .or_insert_with(|| action.to_owned());
        }
    }
    event
}

fn hex(value: Option<&str>) -> Option<i64> {
    let mut value = value?.trim();
    let negative = value.starts_with('-');
    if value.starts_with(['-', '+']) {
        value = &value[1..];
    }
    let prefixed = value.starts_with("0x") || value.starts_with("0X");
    if prefixed {
        value = &value[2..];
    }
    if prefixed && value.starts_with('_') {
        value = &value[1..];
    }
    if value.is_empty() || value.starts_with('_') || value.ends_with('_') || value.contains("__") {
        return None;
    }
    let digits = value.replace('_', "");
    let result = i64::from_str_radix(&digits, 16).ok()?;
    if negative {
        result.checked_neg()
    } else {
        Some(result)
    }
}

#[must_use]
pub fn matches(payload: &[u8], product: u16) -> bool {
    let event = decode(payload);
    if event.get("SUBSYSTEM").map(String::as_str) != Some("usb") {
        return false;
    }
    if !matches!(
        event.get("ACTION").map(String::as_str),
        Some("add" | "bind" | "change" | "move")
    ) {
        return false;
    }
    if let Some(raw) = event.get("PRODUCT").filter(|value| !value.is_empty()) {
        let parts: Vec<&str> = raw.split('/').collect();
        if let [vendor, device, ..] = parts.as_slice() {
            return hex(Some(vendor)) == Some(0x1cbe)
                && hex(Some(device)) == Some(i64::from(product));
        }
    }
    let vendor = hex(event.get("ID_VENDOR_ID").map(String::as_str));
    let device = hex(event.get("ID_MODEL_ID").map(String::as_str));
    if vendor.is_some() || device.is_some() {
        return vendor == Some(0x1cbe) && device == Some(i64::from(product));
    }
    event.get("DEVTYPE").map(String::as_str) == Some("usb_device")
}
