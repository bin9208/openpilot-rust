use crate::{
    Error,
    usb::{Display, Options, ParamsRuntime},
};
use openpilot_usbgpu::{bus_lock::BusLock, native_usb::Scanner};
use std::path::{Path, PathBuf};

pub struct Usb {
    lock: PathBuf,
    bus: Option<BusLock>,
    scanner: Option<Scanner>,
}

#[must_use]
pub fn label(product: u16) -> String {
    match product {
        0x0092 => "TURZX 9.2".into(),
        0x0123 => "TURZX 12.3".into(),
        _ => format!("TURZX USB pid=0x{product:04x}"),
    }
}

impl Usb {
    #[must_use]
    pub fn new(_root: &Path, lock: &Path) -> Self {
        Self {
            lock: lock.to_owned(),
            bus: None,
            scanner: None,
        }
    }

    fn scan(&mut self, expected: u16) -> Result<Option<u16>, Error> {
        if self.bus.is_none() {
            self.bus = Some(BusLock::open(&self.lock)?);
        }
        let bus = self
            .bus
            .as_ref()
            .ok_or(Error::Contract("USB bus lock missing"))?;
        let _guard = bus.enter()?;
        if self.scanner.is_none() {
            self.scanner = Some(Scanner::new()?);
        }
        let found = self
            .scanner
            .as_ref()
            .ok_or(Error::Contract("USB scanner missing"))?
            .find(0x1cbe, expected)?;
        Ok(found.map(|device| device.product))
    }

    #[must_use]
    pub fn find(&mut self, expected: u16) -> Option<u16> {
        match self.scan(expected) {
            Ok(found) => found,
            Err(error) => {
                eprintln!("TURZX USB scan failed for pid=0x{expected:04x}: {error}");
                None
            }
        }
    }

    #[must_use]
    pub fn dim(&mut self, expected: u16, reason: &str) -> bool {
        let registration = if expected == 0x0092 {
            match openpilot_cluster_wifi::Registration::load(&openpilot_cluster_wifi::config_path())
            {
                Ok(registration) => registration,
                Err(_) => return false,
            }
        } else {
            None
        };
        if registration.is_none() && self.find(expected).is_none() {
            return false;
        }
        match Display::new_with_wifi(
            Options {
                brightness: 0,
                display_fps: 0,
                expected_product_id: Some(expected),
                ..Options::default()
            },
            ParamsRuntime::default(),
            &self.lock,
            registration,
        ) {
            Ok(mut display) => Self::dim_result(display.open(), expected, reason),
            Err(error) => Self::dim_result(Err(error), expected, reason),
        }
    }

    fn dim_result(result: Result<(), Error>, expected: u16, reason: &str) -> bool {
        match result {
            Ok(()) => {
                println!(
                    "[cluster_autorun] sent brightness 0 to {} ({reason})",
                    label(expected)
                );
                true
            }
            Err(error) => {
                eprintln!(
                    "[cluster_autorun] failed to turn off {} ({reason}): {error}",
                    label(expected)
                );
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_scan_attempts_bus_ownership_without_a_python_vendor_tree() {
        let directory = tempfile::tempdir().unwrap();
        let lock = directory.path().join("bus.lock");
        let mut usb = Usb::new(directory.path(), &lock);
        assert_eq!(usb.find(u16::MAX), None);
        assert!(
            lock.is_file(),
            "legacy Python absence must not suppress native bus scan"
        );
        assert!(usb.bus.is_some());
    }

    #[test]
    fn unavailable_native_bus_remains_a_failed_scan() {
        let directory = tempfile::tempdir().unwrap();
        let mut usb = Usb::new(directory.path(), &directory.path().join("absent/bus.lock"));
        assert_eq!(usb.find(u16::MAX), None);
        assert!(usb.bus.is_none());
    }
}
