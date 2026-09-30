#![forbid(unsafe_code)]

mod config;
mod linux;
pub use linux::LinuxPlatform;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown amplifier model {0}")]
    UnknownModel(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// One attempt owns its bus; close errors participate in transaction retries.
pub trait Bus {
    fn read_byte(&mut self, register: u8) -> io::Result<u8>;
    fn write_byte(&mut self, register: u8, value: u8) -> io::Result<()>;
    fn close(self) -> io::Result<()>;
}

/// Retry sleeps and retry-message writes run outside the caught bus attempt.
pub trait Platform {
    type Bus: Bus;
    fn open_bus(&mut self) -> io::Result<Self::Bus>;
    fn sleep(&mut self, seconds: f64) -> io::Result<()>;
    fn print(&mut self, text: &str) -> io::Result<()>;
}

#[derive(Clone, Copy)]
struct Config {
    name: &'static str,
    value: u8,
    register: u8,
    offset: u8,
    mask: u8,
}

impl Config {
    const fn new(name: &'static str, value: u8, register: u8, offset: u8, mask: u8) -> Self {
        assert!(offset < 8);
        Self {
            name,
            value,
            register,
            offset,
            mask,
        }
    }
    const fn shutdown(disabled: bool) -> Self {
        Self::new(
            "Global shutdown",
            if disabled { 0 } else { 1 },
            0x51,
            7,
            0x80,
        )
    }
}

pub struct Amplifier {
    debug: bool,
}

impl Amplifier {
    pub const fn new(debug: bool) -> Self {
        Self { debug }
    }

    pub fn set_global_shutdown(
        &self,
        platform: &mut impl Platform,
        disabled: bool,
    ) -> Result<bool, Error> {
        Ok(self.set_configs(platform, &[Config::shutdown(disabled)])?)
    }

    pub fn initialize_configuration(
        &self,
        platform: &mut impl Platform,
        model: &str,
    ) -> Result<bool, Error> {
        let specific = match model {
            "tici" => config::TICI,
            "tizi" => config::TIZI,
            _ => return Err(Error::UnknownModel(model.into())),
        };
        let mut configs = Vec::with_capacity(2 + config::BASE.len() + specific.len());
        configs.push(Config::shutdown(true));
        configs.extend_from_slice(config::BASE);
        configs.extend_from_slice(specific);
        configs.push(Config::shutdown(false));
        Ok(self.set_configs(platform, &configs)?)
    }

    fn set_configs(&self, platform: &mut impl Platform, configs: &[Config]) -> io::Result<bool> {
        let mut backoff = 0.0;
        for attempt in 0..15 {
            match self.attempt(platform, configs) {
                Ok(()) => return Ok(true),
                Err(_error) => {
                    backoff += 0.1;
                    platform.sleep(backoff)?;
                    platform.print(&format!(
                        "Failed to set amp config, {} retries left",
                        14 - attempt
                    ))?;
                }
            }
        }
        Ok(false)
    }

    fn attempt(&self, platform: &mut impl Platform, configs: &[Config]) -> io::Result<()> {
        let mut bus = platform.open_bus()?;
        let result = (|| {
            for config in configs {
                if self.debug {
                    platform.print(&format!("Setting \"{}\" to {}:", config.name, config.value))?;
                }
                let old = bus.read_byte(config.register)?;
                let new = (old & !config.mask) | ((config.value << config.offset) & config.mask);
                bus.write_byte(config.register, new)?;
                if self.debug {
                    platform.print(&format!(
                        "  Changed {:#x}: {old:#x} -> {new:#x}",
                        config.register
                    ))?;
                }
            }
            Ok(())
        })();
        bus.close()?;
        result
    }
}
