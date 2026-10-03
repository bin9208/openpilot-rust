use super::{platform, Error};
use crate::native_transport;
use openpilot_logging::{log_site, native, rate::RateLimit, record::Level};
use openpilot_panda_usb::Log;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Logs {
    logger: native::Logger,
    timestamps: bool,
}

impl Logs {
    pub fn new(device: &str) -> Result<Self, Error> {
        let version = include_str!("../../../../../openpilot/common/version.h")
            .split('"')
            .nth(1)
            .ok_or(Error::Contract("missing source version"))?;
        Ok(Self {
            logger: native::Logger::for_runtime(version, device)?,
            timestamps: std::env::var_os("LOG_TIMESTAMPS").is_some(),
        })
    }

    pub fn write(&self, level: Level, text: impl Into<String>) {
        // C++ cloudlog ignores delivery errors; logging must not change transport recovery.
        match self.logger.emit(log_site!(), level, text.into()) {
            Ok(_) | Err(_) => {}
        }
    }

    pub fn close(&self) -> Result<(), Error> {
        Ok(self.logger.close()?)
    }

    pub fn named(&self, source: &str, level: Level, text: impl Into<String>) {
        match self
            .logger
            .emit_named(log_site!(), source, level, text.into())
        {
            Ok(_) | Err(_) => {}
        }
    }

    pub fn trace(&self, text: impl FnOnce() -> String) {
        if self.timestamps {
            match self
                .logger
                .emit_timestamp(log_site!(), Level::Debug, text(), None)
            {
                Ok(_) | Err(_) => {}
            }
        }
    }

    pub fn transport(&self) -> native_transport::Logger {
        let logger = self.clone();
        Arc::new(move |level, text| {
            let level = match level {
                0 => Level::NotSet,
                10 => Level::Debug,
                20 => Level::Info,
                30 => Level::Warning,
                40 => Level::Error,
                _ => Level::Critical,
            };
            logger.write(level, text);
        })
    }

    pub fn usb(&self) -> openpilot_panda_usb::Logger {
        let logger = self.clone();
        let limits = Mutex::new([RateLimit::default(), RateLimit::default()]);
        Arc::new(move |record| {
            let (level, text, limited) = match record {
                Log::Initialization => (Level::Error, "libusb initialization error".into(), None),
                Log::DeviceList => (Level::Error, "libusb can't get device list".into(), None),
                Log::Disconnected => (Level::Error, "lost connection".into(), None),
                Log::TransmitFull => (Level::Warning, "Transmit buffer full".into(), None),
                Log::Issue {
                    code,
                    description,
                    operation,
                } => (
                    Level::Error,
                    format!("usb error {code} \"{description}\" in {operation}"),
                    Some(0),
                ),
                Log::Overflow(count) => {
                    (Level::Error, format!("overflow got 0x{count:x}"), Some(1))
                }
            };
            if let Some(index) = limited {
                let decision = match limits.lock() {
                    Ok(mut limits) => limits[index].admit(platform::now_ns()),
                    Err(_) => {
                        logger.write(Level::Error, "Panda USB logging mutex poisoned");
                        return;
                    }
                };
                let decision = match decision {
                    Ok(value) => value,
                    Err(error) => {
                        logger.write(Level::Error, error.to_string());
                        return;
                    }
                };
                if decision.suppressed > 0 {
                    logger.write(
                        Level::Warning,
                        format!("cloudlog: {} messages suppressed", decision.suppressed),
                    );
                }
                if !decision.emit {
                    return;
                }
            }
            logger.write(level, text);
        })
    }

    pub fn serial(&self, index: usize, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let level = if text.contains("Register 0x") {
            Level::Error
        } else if text.contains("SPI:")
            || text.contains("incorrect header")
            || text.contains("incorrect data checksum")
        {
            Level::Warning
        } else {
            Level::Debug
        };
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if !line.is_empty() {
                self.named(&format!("panda[{index}]"), level, line);
            }
        }
    }
}
