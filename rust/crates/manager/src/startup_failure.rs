use openpilot_hardware_info::paths::Paths;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{format_record, Level},
    Value,
};
use openpilot_logmessaged::{LogFiles, RotationSettings};
use openpilot_manager::Error;

pub fn save(factory: &Factory, paths: &Paths, error: &Error) -> Result<(), Error> {
    let root = std::path::PathBuf::from(paths.swaglog_root().map_err(std::io::Error::other)?);
    std::fs::create_dir_all(&root)?;
    let logger = factory.logger();
    let hostname = rustix::system::uname();
    let hostname = hostname
        .nodename()
        .to_str()
        .map_err(std::io::Error::other)?;
    let packet = format_record(
        Level::Error,
        Value::Text("Manager failed to start".into()),
        logger.context_snapshot()?,
        Some(&error.to_string()),
        &log_site!().metadata(hostname)?,
    )?;
    let (_, json) = packet
        .split_first()
        .ok_or(Error::Contract("startup error log packet is empty"))?;
    let record = std::str::from_utf8(json).map_err(std::io::Error::other)?;
    let started = std::time::Instant::now();
    let mut files = LogFiles::new(
        &root.join("swaglog"),
        RotationSettings::default(),
        move || started.elapsed().as_secs_f64(),
    )
    .map_err(std::io::Error::other)?;
    files.emit(record).map_err(std::io::Error::other)?;
    Ok(())
}
