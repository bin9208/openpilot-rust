use crate::{lifecycle::ExitAction, runtime::ExitBoundary, Error};
use openpilot_crash_reporting::{Inputs, NativeException, Reporter, Sdk};
use openpilot_hardware_control::{HardwareControl, Platform};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
};

/// Composes existing native Sentry policy and board actions. Tests supply an
/// isolated platform and SDK; production platform choice belongs to installation.
pub struct NativeExit<S, I, P> {
    pub reporter: Reporter<S, I>,
    pub hardware: HardwareControl,
    pub platform: P,
}
impl<S: Sdk, I: Inputs, P: Platform> ExitBoundary for NativeExit<S, I, P> {
    fn capture_exception(&mut self, error: &Error) -> Result<(), Error> {
        let exception = NativeException::from_error(error);
        eprint!("{}", exception.diagnostic());
        self.reporter.capture_exception(&exception, true)?;
        Ok(())
    }
    fn exit(&mut self, action: ExitAction) -> Result<(), Error> {
        let message = match action {
            ExitAction::Uninstall => "uninstalling",
            ExitAction::Reboot => "reboot",
            ExitAction::Shutdown => "shutdown",
        };
        self.reporter
            .logger
            .emit(log_site!(), Record::text(Level::Warning, message.into()))?;
        match action {
            ExitAction::Uninstall => self.hardware.uninstall(&mut self.platform)?,
            ExitAction::Reboot => self.hardware.reboot(&mut self.platform)?,
            ExitAction::Shutdown => self.hardware.shutdown(&mut self.platform)?,
        }
        Ok(())
    }
}
