//! Composition of existing native startup implementations. Visual spinner,
//! boot snapshot/lock ownership and generated vehicle documentation are injected.
use crate::{initialization::Startup, processes::Processes, Error};
use openpilot_crash_reporting::{Inputs, Project, Reporter, Sdk};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
    Fields, Value,
};
use openpilot_registration::{Clock, Hardware, Registration, Spinner};
use openpilot_runtime_version::BuildMetadata;
use std::path::Path;

pub trait BootBoundary {
    fn save_bootlog(&mut self) -> Result<(), Error>;
    fn release_boot_lock(&mut self) -> Result<(), Error>;
}

pub struct NativeStartup<'a, S, I, B> {
    pub source_root: &'a Path,
    pub launcher: &'a Path,
    pub params: &'a openpilot_params::Params,
    pub persist: &'a Path,
    pub api_host: &'a str,
    pub hardware: &'a mut dyn Hardware,
    pub device_type: &'a str,
    pub clock: &'a mut dyn Clock,
    pub spinner: Option<&'a mut dyn Spinner>,
    pub reporter: &'a mut Reporter<S, I>,
    pub logging: &'a Factory,
    pub processes: &'a Processes,
    pub boot: B,
    pub update_status: Option<openpilot_checkout_status::UpdateStatus>,
}
impl<S: Sdk, I: Inputs, B: BootBoundary> Startup for NativeStartup<'_, S, I, B> {
    fn save_bootlog(&mut self) -> Result<(), Error> {
        self.boot.save_bootlog()
    }
    fn build_metadata(&mut self) -> Result<BuildMetadata, Error> {
        Ok(openpilot_runtime_version::get_build_metadata(
            self.source_root,
        )?)
    }
    fn checkout_status(&mut self) -> Result<(), Error> {
        self.update_status = Some(openpilot_checkout_status::UpdateStatus::new(
            self.source_root,
            self.launcher,
        ));
        Ok(())
    }
    fn serial(&mut self) -> Result<String, Error> {
        Ok(self.hardware.serial()?)
    }
    fn register(&mut self) -> Result<String, Error> {
        let mut logger = self.logging.logger();
        let mut registration = Registration {
            params: self.params,
            persist: self.persist,
            api_host: self.api_host,
            source_root: self.source_root,
            logger: &mut logger,
        };
        let spinner = self
            .spinner
            .as_mut()
            .map(|spinner| &mut **spinner as &mut dyn Spinner);
        registration
            .register(self.hardware, self.clock, spinner)?
            .to_utf8()
            .ok_or(Error::Contract("registration returned nonstring identity"))
    }
    fn initialize_logging(&mut self, metadata: &BuildMetadata, dongle: &str) -> Result<(), Error> {
        self.reporter.init(Project::Selfdrive)?;
        let mut fields = Fields::new();
        for (key, value) in [
            ("dongle_id", dongle.to_owned()),
            ("version", string(&metadata.openpilot.version)?),
            (
                "origin",
                string(&metadata.openpilot.git_normalized_origin()?)?,
            ),
            ("branch", string(&metadata.channel)?),
            ("commit", string(&metadata.openpilot.git_commit)?),
            ("device", self.device_type.to_owned()),
        ] {
            fields.insert(key.into(), Value::Text(value));
        }
        fields.insert("dirty".into(), Value::Bool(metadata.openpilot.is_dirty));
        self.logging.bind_global(fields)?;
        Ok(())
    }
    fn prepare_processes(&mut self) -> Result<(), Error> {
        self.processes.prepare();
        Ok(())
    }
    fn supported_cars(&mut self, brand: &str) -> Result<Vec<String>, Error> {
        crate::supported_cars::names(brand)
    }
    fn exception(&mut self, message: &str, error: &Error) -> Result<(), Error> {
        self.logging.logger().emit(
            log_site!(),
            Record::text(Level::Error, message.into()).with_exception(format!("{error}\n")),
        )?;
        Ok(())
    }
    fn release_boot_lock(&mut self) -> Result<(), Error> {
        self.boot.release_boot_lock()
    }
}
fn string(value: &openpilot_runtime_version::JsonValue) -> Result<String, Error> {
    value
        .to_utf8()
        .ok_or(Error::Contract("logging metadata is not UTF-8 STRING"))
}
