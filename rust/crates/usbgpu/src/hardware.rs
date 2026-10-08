use crate::Error;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const FIRMWARE: &str = "custom ed4e39b7-CLEAN";
pub const USB_IDS: [(u16, u16); 2] = [(0xadd1, 0x0001), (0x3801, 0x0001)];
pub const SYSFS: &str = "/sys/bus/usb/devices";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub sysfs_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub speed_mbps: i64,
    pub manufacturer: String,
    pub product: String,
    pub busnum: i64,
    pub devnum: i64,
    pub link_error_count: u16,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub compiled: bool,
    pub loading: bool,
    pub active: bool,
    pub startup_failed: bool,
    pub compile_pending: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PowerStatus {
    pub voltage_mv: u16,
    pub current_ma: i16,
    pub fault: bool,
}
impl PowerStatus {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let Some(raw) = bytes.get(..5) else {
            return Err(Error::ShortPower(bytes.len()));
        };
        Ok(Self {
            voltage_mv: u16::from_le_bytes([raw[0], raw[1]]),
            current_ma: i16::from_le_bytes([raw[2], raw[3]]),
            fault: raw[4] != 0,
        })
    }
}

fn read(path: &Path) -> Result<String, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8(bytes)?.trim().to_owned()),
        Err(_) => Ok(String::new()),
    }
}
fn number(path: &Path, radix: u32) -> Result<i64, Error> {
    let text = read(path)?;
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1_i64, digits),
        None => (1_i64, text.strip_prefix('+').unwrap_or(&text)),
    };
    let lower = digits.to_ascii_lowercase();
    let (base, digits) = match radix {
        0 if lower.starts_with("0x") => (16, &digits[2..]),
        0 if lower.starts_with("0o") => (8, &digits[2..]),
        0 if lower.starts_with("0b") => (2, &digits[2..]),
        0 if digits.starts_with('0') && digits.bytes().any(|b| b != b'0') => return Ok(0),
        0 => (10, digits),
        16 if lower.starts_with("0x") => (16, &digits[2..]),
        base => (base, digits),
    };
    Ok(i64::from_str_radix(digits, base)
        .ok()
        .and_then(|value| value.checked_mul(sign))
        .unwrap_or(0))
}

pub fn devices(root: &Path) -> Result<Vec<Device>, Error> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut paths = entries
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    let mut devices = Vec::new();
    for path in paths {
        let vendor = number(&path.join("idVendor"), 16)?;
        let product = number(&path.join("idProduct"), 16)?;
        let Some(&(vendor_id, product_id)) = USB_IDS
            .iter()
            .find(|&&(v, p)| i64::from(v) == vendor && i64::from(p) == product)
        else {
            continue;
        };
        let controller = path.canonicalize().ok().and_then(|path| {
            path.ancestors()
                .skip(1)
                .find(|parent| {
                    parent
                        .file_name()
                        .is_some_and(|name| name.as_encoded_bytes().ends_with(b".ssusb"))
                })
                .map(Path::to_path_buf)
        });
        let errors = match controller {
            Some(path) => number(&path.join("portli"), 0)?,
            None => 0,
        };
        devices.push(Device {
            sysfs_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(Error::Contract("invalid USB sysfs name"))?
                .into(),
            vendor_id,
            product_id,
            speed_mbps: number(&path.join("speed"), 10)?,
            manufacturer: read(&path.join("manufacturer"))?,
            product: read(&path.join("product"))?,
            busnum: number(&path.join("busnum"), 10)?,
            devnum: number(&path.join("devnum"), 10)?,
            link_error_count: u16::try_from(errors & 0xffff)
                .map_err(|_| Error::Contract("link error mask"))?,
        });
    }
    Ok(devices)
}

pub fn single(devices: &[Device]) -> Option<&Device> {
    match devices {
        [device] => Some(device),
        [] | [_, _, ..] => None,
    }
}

pub fn status(devices: &[Device], state: RuntimeStatus) -> String {
    let device = match devices {
        [] => return "not detected".into(),
        [device] => device,
        [_, _, ..] => return "multiple devices".into(),
    };
    if device.speed_mbps < 5000 {
        return format!("slow USB ({} Mbps)", device.speed_mbps);
    }
    if device.product != FIRMWARE {
        return "firmware mismatch".into();
    }
    badge(state)
        .replace("compile_pending", "reboot to compile")
        .replace("not_compiled", "model not compiled")
        .replace("error", "startup failed")
}

pub const fn badge(state: RuntimeStatus) -> &'static str {
    if state.startup_failed {
        "error"
    } else if state.loading {
        "loading"
    } else if state.compile_pending {
        "compile_pending"
    } else if state.active {
        "active"
    } else if !state.compiled {
        "not_compiled"
    } else {
        "ready"
    }
}

pub fn connection_diagnostic(device: Option<&Device>) -> Option<String> {
    let Some(device) = device else {
        return Some("USB not connected".into());
    };
    if device.speed_mbps < 5000 {
        Some(format!("USB link {} Mbps", device.speed_mbps))
    } else if device.product != FIRMWARE {
        Some("firmware mismatch".into())
    } else {
        None
    }
}
pub fn power_diagnostic(power: Option<PowerStatus>) -> Option<String> {
    match power {
        None => Some("USB not connected".into()),
        Some(power) if power.fault => Some(format!(
            "eGPU power fault ({} mV, {} mA)",
            power.voltage_mv, power.current_ma
        )),
        Some(power) if power.voltage_mv < 8000 => {
            Some(format!("12V off ({} mV)", power.voltage_mv))
        }
        Some(_) => None,
    }
}
