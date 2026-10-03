use super::Backend;
use crate::{state::SlowParams, Error};
use openpilot_usbgpu::{check, hardware, model};
use std::sync::atomic::AtomicBool;

pub struct Native {
    pub options: check::Options,
    pub models: model::Paths,
}
fn error(error: openpilot_usbgpu::Error) -> Error {
    Error::Io(std::io::Error::other(error))
}
impl Backend for Native {
    fn status(&self, state: &SlowParams) -> Result<String, Error> {
        let devices = hardware::devices(&self.options.devices).map_err(error)?;
        Ok(hardware::status(
            &devices,
            hardware::RuntimeStatus {
                compiled: state.usbgpu_compiled,
                loading: state.usbgpu_loading,
                active: state.usbgpu_active,
                startup_failed: state.usbgpu_startup_failed,
                compile_pending: state.usbgpu_compile_pending,
            },
        ))
    }
    fn link(&self) -> Result<String, Error> {
        let devices = hardware::devices(&self.options.devices).map_err(error)?;
        Ok(match hardware::single(&devices) {
            Some(device) if device.speed_mbps >= 1000 && device.speed_mbps % 1000 == 0 => {
                format!("{} Gbps", device.speed_mbps / 1000)
            }
            Some(device) => format!("{} Mbps", device.speed_mbps),
            None => "not connected".into(),
        })
    }
    fn check(&self, cancelled: &AtomicBool) -> Result<Option<String>, Error> {
        check::run(&self.options, cancelled).map_err(error)
    }
    fn remove_compiled_manifest(&self) -> Result<(), Error> {
        model::remove_active_chunk_manifest(&self.models)
            .map(|_| ())
            .map_err(error)
    }
}
