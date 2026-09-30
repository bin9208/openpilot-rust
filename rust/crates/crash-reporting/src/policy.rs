use crate::{Error, NativeException};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
    Fields, Value,
};
use openpilot_params::Params;
use openpilot_runtime_version::{BuildMetadata, JsonValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Project {
    Selfdrive,
    SelfdriveNative,
}
impl Project {
    pub const fn dsn(self) -> &'static str {
        match self {
            Self::Selfdrive => {
                "https://6f3c7076c1e14b2aa10f5dde6dda0cc4@o33823.ingest.sentry.io/77924"
            }
            Self::SelfdriveNative => {
                "https://3e4b586ed21a4479ad5d85083b639bc6@o33823.ingest.sentry.io/157615"
            }
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Configuration {
    pub project: Project,
    pub release: String,
    pub environment: &'static str,
    pub default_integrations: bool,
    pub threading_requested: bool,
    pub traces_sample_rate: f32,
    pub max_value_length: usize,
}
/// Project-owned calls; SDK transport and thread machinery remain an external boundary.
pub trait Sdk {
    fn init(&mut self, configuration: Configuration) -> Result<(), Error>;
    fn set_user(&mut self, id: Option<String>) -> Result<(), Error>;
    fn set_tag(&mut self, key: &str, value: &JsonValue) -> Result<(), Error>;
    fn set_extra(&mut self, key: &str, value: &JsonValue) -> Result<(), Error>;
    fn capture_message(&mut self, message: &str) -> Result<(), Error>;
    fn capture_exception(&mut self, exception: &NativeException) -> Result<(), Error>;
    fn flush(&mut self) -> Result<(), Error>;
}
/// Runtime I/O seams are evaluated in the same order as the source policy.
pub trait Inputs {
    fn build_metadata(&mut self) -> Result<BuildMetadata, Error>;
    fn params(&mut self) -> Result<Params, Error>;
    fn is_pc(&self) -> bool;
    fn version(&mut self) -> Result<String, Error>;
    fn device_type(&mut self) -> Result<String, Error>;
    fn dongle_id(&mut self, logger: &mut Logger) -> Result<Option<String>, Error> {
        Ok(openpilot_params_typed::get_string(
            &self.params()?,
            "DongleId",
            logger,
        )?)
    }
}

pub struct Reporter<S, I> {
    pub sdk: S,
    pub inputs: I,
    pub logger: Logger,
}
impl<S: Sdk, I: Inputs> Reporter<S, I> {
    /// Initialize only for a comma remote, registered device and non-PC hardware, in that order.
    pub fn init(&mut self, project: Project) -> Result<bool, Error> {
        let build = self.inputs.build_metadata()?;
        if !build.openpilot.comma_remote()? {
            return Ok(false);
        }
        let origin = build
            .openpilot
            .git_origin
            .to_utf8()
            .ok_or(Error::Unicode("git origin"))?;
        if !origin.contains("commaai") {
            return Ok(false);
        }
        let registered = self
            .inputs
            .dongle_id(&mut self.logger)?
            .is_some_and(|id| id != "UnregisteredDevice");
        if !registered || self.inputs.is_pc() {
            return Ok(false);
        }
        let id = self.inputs.dongle_id(&mut self.logger)?;
        self.sdk.init(Configuration {
            project,
            release: self.inputs.version()?,
            environment: if build.tested_channel() {
                "release"
            } else {
                "master"
            },
            default_integrations: false,
            threading_requested: project == Project::Selfdrive,
            traces_sample_rate: 1.0,
            max_value_length: 8192,
        })?;
        self.sdk.set_user(id)?;
        self.sdk.set_tag(
            "dirty",
            &JsonValue::parse(if build.openpilot.is_dirty {
                "true"
            } else {
                "false"
            })?,
        )?;
        self.sdk.set_tag("origin", &build.openpilot.git_origin)?;
        self.sdk.set_tag("branch", &build.channel)?;
        self.sdk.set_tag("commit", &build.openpilot.git_commit)?;
        self.sdk
            .set_tag("device", &JsonValue::text(&self.inputs.device_type()?))?;
        Ok(true)
    }
    pub fn set_tag(&mut self, key: &str, value: &JsonValue) -> Result<(), Error> {
        self.sdk.set_tag(key, value)
    }
    pub fn report_tombstone(
        &mut self,
        filename: &str,
        message: &str,
        contents: &str,
    ) -> Result<(), Error> {
        self.report_tombstone_filename(&JsonValue::text(filename), message, contents)
    }
    pub fn report_tombstone_filename(
        &mut self,
        filename: &JsonValue,
        message: &str,
        contents: &str,
    ) -> Result<(), Error> {
        if !matches!(filename.view(), openpilot_logmessaged::JsonView::Text(_)) {
            return Err(Error::Unicode("tombstone filename must be text"));
        }
        let fields: Fields = [("tombstone".into(), Value::Text(message.into()))]
            .into_iter()
            .collect();
        let json = JsonValue::parse(&fields.to_json()?)?;
        let console = openpilot_runtime_version::python_str(&json)?
            .into_iter()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .ok_or(Error::Unicode("log message"))?;
        let mut record = Record::text(Level::Error, console);
        record.message = Value::Object(fields);
        self.logger.emit(log_site!(), record)?;
        self.sdk.set_extra("tombstone_fn", filename)?;
        self.sdk
            .set_extra("tombstone", &JsonValue::text(contents))?;
        self.sdk.capture_message(message)?;
        self.sdk.flush()
    }
    /// Native callers supply their real Rust error/trace; no Python frames are manufactured.
    /// Params failures occur outside the SDK catch boundary, as in the source.
    pub fn capture_exception(
        &mut self,
        exception: &NativeException,
        log_exception: bool,
    ) -> Result<(), Error> {
        let mut record = Record::text(Level::Error, "crash".into());
        if log_exception {
            record = record.with_exception(exception.diagnostic());
        }
        self.logger.emit(log_site!(), record)?;
        let params = self.inputs.params()?;
        let sent = match params.get_bool("CarrotExceptionSent") {
            Ok(value) => value,
            Err(openpilot_params::Error::Io(_)) => false,
            Err(error) => return Err(error.into()),
        };
        if !sent {
            // The source Cython put() discards the C++ writer's I/O return code.
            match params.put("CarrotException", b"exception") {
                Ok(()) | Err(openpilot_params::Error::Io(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let result = self
            .sdk
            .capture_exception(exception)
            .and_then(|()| self.sdk.flush());
        if let Err(error) = result {
            self.logger.emit(
                log_site!(),
                Record::text(Level::Error, "sentry exception".into())
                    .with_exception(format!("{error}\n")),
            )?;
        }
        Ok(())
    }
}
