use crate::{Error, Hardware};
use openpilot_hardware_info::HardwareInfo;
use openpilot_logmessaged::JsonValue;

/// Borrow native read-only hardware without changing its identity values.
pub struct NativeHardware<'a> {
    hardware: &'a dyn HardwareInfo,
}

impl<'a> NativeHardware<'a> {
    pub fn new(hardware: &'a dyn HardwareInfo) -> Self {
        Self { hardware }
    }
}

impl Hardware for NativeHardware<'_> {
    fn serial(&mut self) -> Result<String, Error> {
        self.hardware
            .get_serial()
            .map_err(|error| Error::Hardware(error.to_string()))
    }

    fn imei(&mut self, slot: usize) -> Result<JsonValue, Error> {
        self.hardware
            .get_imei(slot)
            .map_err(|error| Error::Hardware(error.to_string()))
    }
}
