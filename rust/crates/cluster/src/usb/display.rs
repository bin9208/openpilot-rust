use super::{connection::Connection, profile::Samples, write::NoAck, Options};
use crate::Error;
use openpilot_usbgpu::{bus_lock::BusLock, native_usb::Scanner, transport::Setup};
use std::{path::Path, time::Duration};

/// Clock and Params effects used by the original display transport.
pub trait Runtime {
    /// # Errors
    /// Returns a clock/conversion error if local-midnight milliseconds cannot be represented.
    fn milliseconds(&self) -> Result<u32, Error>;
    fn sleep(&mut self, duration: Duration);
    fn gpu_active(&self) -> bool;
    fn reset_protected(&self) -> bool;
    fn connected(&mut self, value: bool);
    fn warning(&mut self, message: &str);
}

pub struct Display<R: Runtime> {
    pub(super) options: Options,
    pub(super) runtime: R,
    pub(super) connection: Option<Connection>,
    pub(super) frame_errors: u32,
    pub(super) orientation: u8,
    pub(super) chunk_gap: Duration,
    pub(super) profile: Samples,
    dimensions: [u32; 2],
    bus: BusLock,
    scanner: Option<Scanner>,
}

impl<R: Runtime> Display<R> {
    /// # Errors
    /// Returns failure to open the shared eGPU bus lock.
    pub fn new(mut options: Options, runtime: R, lock: &Path) -> Result<Self, Error> {
        options.brightness = options.brightness.clamp(0, 100);
        options.display_fps = options.display_fps.clamp(0, 255);
        options.frame_drain_attempts = options.frame_drain_attempts.max(0);
        options.frame_drain_timeout_ms = options.frame_drain_timeout_ms.max(0);
        options.fast_frame_drain_attempts = options.fast_frame_drain_attempts.max(0);
        options.fast_frame_drain_timeout_ms = options.fast_frame_drain_timeout_ms.max(0);
        Ok(Self {
            options,
            runtime,
            connection: None,
            frame_errors: 0,
            orientation: 0,
            chunk_gap: Duration::ZERO,
            profile: Samples::from_environment(),
            dimensions: [1920, 480],
            bus: BusLock::open(lock)?,
            scanner: None,
        })
    }

    #[must_use]
    pub const fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }

    pub fn set_profile_enabled(&mut self, enabled: bool) {
        self.profile.enabled = enabled;
    }

    pub fn clear_profile_samples(&mut self) {
        self.profile.values.clear();
    }

    #[must_use]
    pub fn profile_samples(&self) -> &[(&'static str, f64)] {
        &self.profile.values
    }

    /// # Errors
    /// Returns discovery, access or initialization failure after one recovery attempt.
    pub fn open(&mut self) -> Result<(), Error> {
        if self.scanner.is_none() {
            self.scanner = Some(Scanner::new()?);
        }
        self.connect()?;
        if let Err(error) = self.initialize() {
            self.runtime.warning(&format!(
                "USB display did not respond during init; resetting device once: {error}"
            ));
            self.reconnect()?;
            self.initialize()?;
        }
        self.runtime.connected(true);
        Ok(())
    }

    pub fn close(&mut self) {
        self.runtime.connected(false);
        if self.connection.is_none() {
            return;
        }
        self.optional_command(14, &[(8, 0)], NoAck::NONE);
        self.connection = None;
        self.frame_errors = 0;
    }

    fn connect(&mut self) -> Result<(), Error> {
        let connection = Connection::open(&self.bus, self.options.expected_product_id)?;
        self.connection = Some(connection);
        let connection = self
            .connection
            .as_mut()
            .ok_or(Error::Contract("USB display is not open"))?;
        let dimensions = connection.dimensions()?;
        connection.cache()?;
        self.dimensions = dimensions;
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), Error> {
        if self.runtime.gpu_active() {
            self.optional_command(10, &[], NoAck::NONE);
        } else {
            self.command_ack(10, &[])?;
        }
        self.runtime.sleep(Duration::from_millis(200));
        if self.options.display_fps > 0 {
            self.optional_command(
                15,
                &[(8, i64::from(self.options.display_fps))],
                NoAck::FRAME_RATE,
            );
        }
        self.optional_command(14, &[(8, self.brightness_raw())], NoAck::NONE);
        Ok(())
    }

    pub(super) fn reconnect(&mut self) -> Result<(), Error> {
        if let Some(connection) = self.connection.as_mut() {
            if self.runtime.reset_protected() {
                self.runtime.warning("Skipping TURZX USB device reset while eGPU is loading or active; reconnecting without reset");
            } else if let Err(error) = connection.setup(Setup::Reset, 0) {
                self.runtime.warning(&format!("USB reset failed: {error}"));
            }
        }
        self.connection = None;
        self.runtime.sleep(Duration::from_millis(1500));
        self.connect()
    }

    pub(super) fn frame_error(&mut self, error: Error) -> Result<(), Error> {
        if super::write::disconnected(&error) {
            self.connection = None;
            self.runtime.connected(false);
            return Err(Error::Disconnected(Box::new(error)));
        }
        self.frame_errors = self.frame_errors.saturating_add(1);
        self.runtime.warning(&format!(
            "USB frame upload failed ({}/3): {error}",
            self.frame_errors
        ));
        self.clear_halt();
        if let Err(error) = self.reconnect().and_then(|()| self.initialize()) {
            self.runtime
                .warning(&format!("USB recovery failed: {error}"));
        }
        if self.frame_errors >= 3 {
            return Err(Error::Contract("TURZX USB display is not accepting frame data. Unplug/replug the display, then retry with lower --fps or lower --usb-jpeg-quality."));
        }
        Ok(())
    }
}

impl<R: Runtime> Drop for Display<R> {
    fn drop(&mut self) {
        self.close();
    }
}
