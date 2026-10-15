//! Physical cable admission and negotiated speed policy from the pinned daemon.
use crate::{mobile::config::Mode, peer, Error};
use serde_json::Value;
use std::{fs, path::Path};

pub fn attached(mode: Mode, data_role: &Path, power_role: &Path) -> bool {
    if matches!(mode, Mode::Auto | Mode::Ios | Mode::Android) {
        match fs::read_to_string(data_role) {
            Ok(value) => return value.trim() == "ufp",
            Err(error) if mode == Mode::Auto && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    fs::read_to_string(power_role).is_ok_and(|value| value.trim().starts_with("Source attached"))
}

pub fn device_attached(mode: Mode) -> bool {
    attached(
        mode,
        Path::new("/sys/class/usbpd/usbpd0/current_dr"),
        Path::new("/sys/class/power_supply/usb/typec_mode"),
    )
}

pub fn legacy_usb_peer(peer: &Value) -> bool {
    if !matches!(peer["protocol"].as_u64(), Some(2 | 3)) {
        return false;
    }
    if peer["carrot_host"] == "jetson" && peer["carrot_boot_update_v1"] == true {
        return true;
    }
    (peer["carrot_host"].is_null() || peer["carrot_host"] == "jetson")
        && peer["backend"] == "trt"
        && peer["device"]
            .as_str()
            .is_some_and(|value| value.to_lowercase().contains("orin"))
}

pub fn admit(speed: &str, mode: Mode, peer: &Value, tcp: bool) -> Result<(), Error> {
    let recognized = peer::classify(peer, if tcp { "tcp" } else { "usb" }).is_some();
    if mode == Mode::Auto && !recognized && (tcp || !legacy_usb_peer(peer)) {
        return Err(Error::Contract(
            "unrecognized App HELLO on selected auto transport",
        ));
    }
    let phone_or_mac =
        matches!(mode, Mode::Ios | Mode::Android) || (mode == Mode::Auto && recognized);
    if matches!(speed, "super-speed" | "super-speed-plus")
        || (phone_or_mac && speed == "high-speed")
    {
        if mode == Mode::Auto && speed == "high-speed" {
            eprintln!(
                "Jetlink recognized App is using USB2; real-time throughput is not guaranteed"
            );
        }
        Ok(())
    } else {
        Err(Error::Contract(
            "unsupported USB speed; use USB3 data cable",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn role_admission_never_substitutes_power_for_present_data_role() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let power = root.path().join("power");
        fs::write(&power, "Source attached (default)").unwrap();
        assert!(attached(Mode::Auto, &data, &power));
        assert!(!attached(Mode::Ios, &data, &power));
        fs::write(&data, "dfp").unwrap();
        assert!(!attached(Mode::Auto, &data, &power));
        assert!(attached(Mode::Usb, &data, &power));
        fs::write(&data, "ufp").unwrap();
        assert!(attached(Mode::Ios, &data, &power));
    }
    #[test]
    fn high_speed_requires_source_phone_or_mac_admission() {
        let phone = serde_json::json!({"protocol":3,"backend":"ort","device":"coreml-Apple A18"});
        let jetson = serde_json::json!({"protocol":3,"backend":"trt","device":"NVIDIA Orin"});
        assert!(admit("high-speed", Mode::Auto, &phone, true).is_ok());
        assert!(admit("high-speed", Mode::Auto, &jetson, false).is_err());
        assert!(admit("super-speed", Mode::Auto, &jetson, false).is_ok());
        assert!(admit("super-speed", Mode::Auto, &phone, false).is_err());
        assert!(admit("high-speed", Mode::Usb, &phone, false).is_err());
        assert!(admit("full-speed", Mode::Android, &phone, false).is_err());
    }
}
