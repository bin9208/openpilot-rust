use crate::{
    assistance::Downloader, config::Config, decode::Log, framing, serial::Diagnostic, setup, Error,
};
use openpilot_hardware_control::{gpio_init, gpio_set, LinuxPlatform, ProcessCommands};
use openpilot_logging::{
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::runtime::PubMaster;
use openpilot_timed::clock::{Clock, SystemClock};
use std::{path::Path, sync::atomic::AtomicBool};
pub fn antenna(config: &Config, high: bool) -> Result<(), Error> {
    let mut platform = LinuxPlatform::new(
        &config.root,
        ProcessCommands {
            launcher: "openpilot-process-launcher".into(),
        },
    );
    if high {
        gpio_init(&mut platform, 34, true)?;
    }
    gpio_set(&mut platform, 34, high)?;
    Ok(())
}
pub fn run(config: &Config, stop: &AtomicBool) -> Result<(), Error> {
    let mut logger = Factory::for_runtime()?.logger();
    logger.emit(
        openpilot_logging::log_site!(),
        Record::text(Level::Warning, "waiting for modem to come up".into()),
    )?;
    setup::wait(config, stop)?;
    let mut downloader = Downloader::start(config)?;
    let mut diag = Diagnostic::open(&config.diagnostic)?;
    let result = (|| {
        let mut want_assistance = !setup::quectel(&mut diag, config, stop)?;
        logger.emit(
            openpilot_logging::log_site!(),
            Record::text(Level::Warning, "quectel setup done".into()),
        )?;
        antenna(config, true)?;
        let mut publisher = PubMaster::for_runtime(&["qcomGnss", "gpsLocation"])?;
        loop {
            if config.assistance.exists() && want_assistance {
                setup::quectel(&mut diag, config, stop)?;
                want_assistance = false;
            }
            let (opcode, payload) = diag.recv(stop)?;
            if opcode != framing::DIAG_LOG {
                logger.emit(
                    openpilot_logging::log_site!(),
                    Record::text(Level::Error, format!("Unhandled opcode: {opcode}")),
                )?;
                continue;
            }
            let log = Log::parse(&payload)?;
            if log.pending > 0 {
                logger.emit(
                    openpilot_logging::log_site!(),
                    Record::text(
                        Level::Debug,
                        format!("have {} pending messages", log.pending),
                    ),
                )?;
            }
            if std::env::var("DEBUG").as_deref() == Ok("1")
                && framing::LOG_TYPES.contains(&log.kind)
            {
                println!(
                    "{:.4}: got log: {} len {}",
                    SystemClock.wall_seconds()?,
                    log.kind,
                    log.payload.len()
                );
            }
            if let Some(message) = log.publication(SystemClock.monotonic()?)? {
                if message.has_fix {
                    want_assistance = false;
                    downloader.stop_retrying()?;
                }
                publisher.send(message.topic, &message.bytes)?;
            }
        }
    })();
    if setup::stopping(stop) {
        logger.emit(
            openpilot_logging::log_site!(),
            Record::text(Level::Warning, "caught sig disabling quectel gps".into()),
        )?;
        antenna(config, false)?;
        setup::teardown(&mut diag, config)?;
        logger.emit(
            openpilot_logging::log_site!(),
            Record::text(Level::Warning, "quectel cleanup done".into()),
        )?;
        Ok(())
    } else {
        result
    }
}
/// Fixture entrypoints may only redirect hardware into a private filesystem/PTY namespace.
pub fn fixture(path: &Path) -> Result<Config, Error> {
    let config: Config = serde_json::from_slice(&std::fs::read(path)?)?;
    if !config.root.is_absolute()
        || config.root == Path::new("/")
        || !config.at.path.starts_with("/dev/pts")
        || !config.diagnostic.starts_with("/dev/pts")
        || !config.assistance.starts_with(&config.root)
        || !config.at.lock.starts_with(&config.root)
        || !config.assistance_url.starts_with("http://127.0.0.1:")
    {
        return Err(Error::Protocol(
            "fixture requires private root, owned PTYs and loopback assistance",
        ));
    }
    Ok(config)
}
