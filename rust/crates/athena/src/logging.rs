use crate::{state::Shared, Error};
use openpilot_logging::{
    producer::Logger,
    record::{Level, Record},
    Value,
};
use serde_json::json;

pub fn event(logger: &mut Logger, name: &str, fields: serde_json::Value) -> Result<(), Error> {
    let Value::Object(fields) = serde_json::from_value(fields)? else {
        return Err(Error::Contract("event fields"));
    };
    logger.emit(
        openpilot_logging::log_site!(),
        Record::event(name, Vec::new(), fields)?,
    )?;
    Ok(())
}
pub fn failure(logger: &mut Logger, name: &str, error: &dyn std::fmt::Display) {
    if let Err(logging) = logger.emit(
        openpilot_logging::log_site!(),
        Record::text(Level::Error, name.into()).with_exception(error.to_string()),
    ) {
        eprintln!("athenad: {name}: {error}; logging: {logging}");
    }
}
pub fn bind(shared: &Shared) -> Result<(), Error> {
    let mut logger = shared.factory.logger();
    let metadata = openpilot_runtime_version::get_build_metadata(&shared.config.basedir)?;
    let value = json!({
        "dongle_id":shared.text("DongleId", &mut logger)?,
        "version":serde_json::from_str::<serde_json::Value>(&metadata.openpilot.version.to_json()?)?,
        "origin":serde_json::from_str::<serde_json::Value>(&metadata.openpilot.git_normalized_origin()?.to_json()?)?,
        "branch":serde_json::from_str::<serde_json::Value>(&metadata.channel.to_json()?)?,
        "commit":serde_json::from_str::<serde_json::Value>(&metadata.openpilot.git_commit.to_json()?)?,
        "dirty":metadata.openpilot.is_dirty,
        "device":openpilot_hardware_info::for_runtime().get_device_type()?,
    });
    let Value::Object(fields) = serde_json::from_value(value)? else {
        return Err(Error::Contract("logging context"));
    };
    shared.factory.bind_global(fields)?;
    Ok(())
}
pub fn inherit(shared: &Shared) -> Result<(), Error> {
    if let Ok(context) = std::env::var("ATHENA_LOG_CONTEXT") {
        let Value::Object(fields) = serde_json::from_str(&context)? else {
            return Err(Error::Contract("Athena inherited logging context"));
        };
        shared.factory.bind_global(fields)?;
        Ok(())
    } else {
        bind(shared)
    }
}
