use crate::Error;
use openpilot_agnos::{cli::Config, runtime, Observer};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use std::path::Path;
/// Project-owned flashing is supplied by the native AGNOS port (#119).
/// Callers never fall back to the original Python runtime.
pub trait Agnos {
    fn get_target_slot_number(&mut self) -> Result<u32, Error>;
    fn flash_agnos_update(&mut self, manifest: &Path, target_slot: u32) -> Result<(), Error>;
}
pub struct Native {
    config: Config,
    logger: Logger,
}
impl Native {
    pub fn new(config: Config, logger: Logger) -> Self {
        Self { config, logger }
    }
}
impl Agnos for Native {
    fn get_target_slot_number(&mut self) -> Result<u32, Error> {
        Ok(runtime::target_slot(&mut runtime::NativeCommands {
            launcher: self.config.launcher.clone(),
            abctl: self.config.abctl.clone(),
        })?)
    }
    fn flash_agnos_update(&mut self, manifest: &Path, target_slot: u32) -> Result<(), Error> {
        let mut commands = runtime::NativeCommands {
            launcher: self.config.launcher.clone(),
            abctl: self.config.abctl.clone(),
        };
        let mut observer = BackgroundObserver {
            logger: &mut self.logger,
        };
        // updated.py uses background casync and the bounded download retry policy.
        runtime::flash(
            &self.config.paths,
            manifest,
            target_slot,
            false,
            false,
            &mut commands,
            &mut observer,
        )?;
        Ok(())
    }
}

struct BackgroundObserver<'a> {
    logger: &'a mut Logger,
}
impl Observer for BackgroundObserver<'_> {
    fn log(&mut self, level: &str, text: &str) {
        let level = match level {
            "error" | "exception" => Level::Error,
            "warning" => Level::Warning,
            "debug" => Level::Debug,
            _ => Level::Info,
        };
        if let Err(error) = self
            .logger
            .emit(log_site!(), Record::text(level, text.into()))
        {
            eprintln!("updated AGNOS logging failed: {error}; {text}");
        }
    }
    fn progress(&mut self, stage: &str, progress: i64) {
        openpilot_agnos::cli::Console.progress(stage, progress);
    }
    fn sleep(&mut self, seconds: u64) {
        openpilot_agnos::cli::Console.sleep(seconds);
    }
}
