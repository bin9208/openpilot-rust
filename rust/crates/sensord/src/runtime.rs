use crate::{
    clock::{Ratekeeper, SystemClock},
    linux::{self, Gpio, LinuxBus},
    loops::{self, Interrupt, Sink},
    sensor::{Event, Kind, Sensor},
    wire, Clock, Error,
};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_messaging::runtime::PubMaster;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub root: PathBuf,
    pub launcher: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            root: "/".into(),
            launcher: "openpilot-process-child".into(),
        }
    }
}
struct Output {
    publisher: PubMaster,
    logger: Logger,
}
impl Output {
    fn new(names: &[&str], factory: &Factory, isolated: bool) -> Result<Self, Error> {
        let publisher = if isolated {
            PubMaster::isolated(names)?
        } else {
            PubMaster::for_runtime(names)?
        };
        Ok(Self {
            publisher,
            logger: factory.logger(),
        })
    }
}
fn log(logger: &mut Logger, level: &str, text: &str, error: Option<&Error>) -> Result<(), Error> {
    let level = if level == "warning" {
        Level::Warning
    } else {
        Level::Error
    };
    let mut record = Record::text(level, text.into());
    if let Some(error) = error {
        record = record.with_exception(error.to_string());
    }
    logger.emit(log_site!(), record)?;
    Ok(())
}
impl Sink for Output {
    fn send(&mut self, kind: Kind, event: &Event, log_time: u64) -> Result<(), Error> {
        self.publisher
            .send(kind.service(), &wire::encode(kind, event, log_time)?)?;
        Ok(())
    }
    fn log(&mut self, level: &str, text: &str, error: Option<&Error>) -> Result<(), Error> {
        log(&mut self.logger, level, text, error)
    }
}
fn irq_affinity(config: &Config) -> Result<(), Error> {
    let primary = config.root.join("proc/irq/336/smp_affinity_list");
    let path = if primary.exists() {
        primary
    } else {
        config.root.join("proc/irq/335/smp_affinity_list")
    };
    if path.exists() {
        let mut platform = openpilot_hardware_control::LinuxPlatform::new(
            Path::new("/"),
            openpilot_hardware_control::ProcessCommands {
                launcher: config.launcher.clone(),
            },
        );
        openpilot_hardware_control::sudo_write(
            &mut platform,
            path.to_str()
                .ok_or(Error::Contract("IRQ path is not UTF-8"))?,
            "1\n",
        )?;
    }
    Ok(())
}
fn irq_loop(
    config: &Config,
    factory: &Factory,
    accel: &mut Sensor<LinuxBus>,
    gyro: &mut Sensor<LinuxBus>,
    stop: &AtomicBool,
) -> Result<(), Error> {
    let mut output = Output::new(
        &["accelerometer", "gyroscope"],
        factory,
        config.root != Path::new("/"),
    )?;
    let mut device = Gpio::open(&config.root.join("dev/gpiochip0"), "sensord", 84)?;
    irq_affinity(config)?;
    let mut clock = SystemClock;
    let mut interrupt = Interrupt::new(&mut clock);
    while !stop.load(Ordering::Relaxed) {
        interrupt.step(
            device.poll(100)?,
            &mut [accel, gyro],
            &mut clock,
            &mut output,
        )?;
    }
    Ok(())
}
fn temperature_loop(
    config: &Config,
    factory: &Factory,
    temperature: &mut Sensor<LinuxBus>,
    stop: &AtomicBool,
) -> Result<(), Error> {
    let mut output = Output::new(
        &["temperatureSensor"],
        factory,
        config.root != Path::new("/"),
    )?;
    let mut clock = SystemClock;
    let mut rate = Ratekeeper::default();
    while !stop.load(Ordering::Relaxed) {
        loops::polling_step(temperature, &mut clock, &mut output, &mut rate)?;
    }
    Ok(())
}
pub fn run(config: Config, stop: Arc<AtomicBool>) -> Result<(), Error> {
    linux::realtime(!config.root.join("TICI").is_file())?;
    let path = config.root.join("dev/i2c-1");
    let mut accel = Sensor::new(Kind::Accelerometer, LinuxBus::open(&path)?);
    let mut gyro = Sensor::new(Kind::Gyroscope, LinuxBus::open(&path)?);
    let mut temperature = Sensor::new(Kind::TemperatureSensor, LinuxBus::open(&path)?);
    let factory = Factory::for_runtime()?;
    let mut logger = factory.logger();
    let mut clock = SystemClock;
    for sensor in [&mut accel, &mut gyro, &mut temperature] {
        if let Err(error) = sensor.reset(&mut clock) {
            log(
                &mut logger,
                "exception",
                &format!("Error initializing {} sensor", sensor.kind.service()),
                Some(&error),
            )?;
        }
    }
    let self_test = std::env::var_os("LSM_SELF_TEST").map(|s| s.to_string_lossy().into_owned());
    let mut temperature_ready = false;
    for sensor in [&mut accel, &mut gyro, &mut temperature] {
        match sensor.init(&mut clock, self_test.as_deref()) {
            Ok(()) => {
                if sensor.kind == Kind::TemperatureSensor {
                    temperature_ready = true;
                }
            }
            Err(error) => log(
                &mut logger,
                "exception",
                &format!("Error initializing {} sensor", sensor.kind.service()),
                Some(&error),
            )?,
        }
    }
    std::thread::scope(|scope| -> Result<(), Error> {
        let mut workers =
            vec![scope.spawn(|| irq_loop(&config, &factory, &mut accel, &mut gyro, &stop))];
        if temperature_ready {
            workers
                .push(scope.spawn(|| temperature_loop(&config, &factory, &mut temperature, &stop)));
        }
        while workers.iter().any(|worker| !worker.is_finished()) && !stop.load(Ordering::Relaxed) {
            clock.sleep(1.);
        }
        stop.store(true, Ordering::Relaxed);
        for worker in workers {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log(
                    &mut logger,
                    "exception",
                    "Sensor worker stopped",
                    Some(&error),
                )?,
                Err(_) => log(&mut logger, "exception", "Sensor worker panicked", None)?,
            }
        }
        Ok(())
    })?;
    for sensor in [&mut accel, &mut gyro, &mut temperature] {
        if let Err(error) = sensor.shutdown() {
            log(
                &mut logger,
                "exception",
                "Error shutting down sensor",
                Some(&error),
            )?;
        }
    }
    Ok(())
}
