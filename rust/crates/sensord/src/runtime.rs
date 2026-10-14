use crate::{
    clock::{Ratekeeper, SystemClock},
    linux::{self, Gpio, LinuxBus},
    loops::{self, Interrupt, Sink},
    sensor::{Event, Kind, Sensor},
    wire, Bus, Clock, Error,
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
fn supervise(
    workers: Vec<std::thread::ScopedJoinHandle<'_, Result<(), Error>>>,
    stop: &AtomicBool,
    clock: &mut impl Clock,
    mut report: impl FnMut(&str, Option<&Error>) -> Result<(), Error>,
) -> Result<(), Error> {
    while workers.iter().all(|worker| !worker.is_finished()) && !stop.load(Ordering::Relaxed) {
        clock.sleep(1.);
    }
    let unexpected_exit = !stop.load(Ordering::Relaxed);
    stop.store(true, Ordering::Relaxed);
    let mut failure = None;
    let mut logging_failure = None;
    for worker in workers {
        let report_result = match worker.join() {
            Ok(Ok(())) => continue,
            Ok(Err(error)) => {
                let logged = report("Sensor worker stopped", Some(&error));
                failure.get_or_insert(error);
                logged
            }
            Err(_) => {
                failure.get_or_insert(Error::Contract("sensor worker panicked"));
                report("Sensor worker panicked", None)
            }
        };
        if let Err(error) = report_result {
            logging_failure.get_or_insert(error);
        }
    }
    match failure.or(logging_failure) {
        Some(error) => Err(error),
        None if unexpected_exit => Err(Error::Contract("sensor worker exited unexpectedly")),
        None => Ok(()),
    }
}
fn shutdown_sensors<B: Bus>(
    sensors: &mut [&mut Sensor<B>],
    mut result: Result<(), Error>,
    mut report: impl FnMut(&Error) -> Result<(), Error>,
) -> Result<(), Error> {
    for sensor in sensors {
        if let Err(error) = sensor.shutdown() {
            if let Err(error) = report(&error) {
                if result.is_ok() {
                    result = Err(error);
                }
            }
        }
    }
    result
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
    let result = std::thread::scope(|scope| -> Result<(), Error> {
        let mut workers =
            vec![scope.spawn(|| irq_loop(&config, &factory, &mut accel, &mut gyro, &stop))];
        if temperature_ready {
            workers
                .push(scope.spawn(|| temperature_loop(&config, &factory, &mut temperature, &stop)));
        }
        supervise(workers, &stop, &mut clock, |text, error| {
            log(&mut logger, "exception", text, error)
        })
    });
    shutdown_sensors(
        &mut [&mut accel, &mut gyro, &mut temperature],
        result,
        |error| {
            log(
                &mut logger,
                "exception",
                "Error shutting down sensor",
                Some(error),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StopClock<'a> {
        stop: &'a AtomicBool,
        sleeps: usize,
    }
    impl Clock for StopClock<'_> {
        fn monotonic(&mut self) -> f64 {
            unreachable!()
        }
        fn monotonic_ns(&mut self) -> i128 {
            unreachable!()
        }
        fn realtime_ns(&mut self) -> i128 {
            unreachable!()
        }
        fn sleep(&mut self, seconds: f64) {
            assert_eq!(seconds, 1.);
            self.sleeps += 1;
            // Deterministic escape for the broken supervisor: no real sleeping.
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    fn failed_worker(failure: fn() -> Result<(), Error>, expected: &str) {
        let stop = AtomicBool::new(false);
        let sibling_joined = AtomicBool::new(false);
        let mut clock = StopClock {
            stop: &stop,
            sleeps: 0,
        };
        let result = std::thread::scope(|scope| {
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let stop = &stop;
            let sibling_joined = &sibling_joined;
            let sibling = scope.spawn(move || {
                ready_tx.send(()).unwrap();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                sibling_joined.store(true, Ordering::Relaxed);
                Ok(())
            });
            ready_rx.recv().unwrap();
            let irq = scope.spawn(failure);
            while !irq.is_finished() {
                std::thread::yield_now();
            }
            supervise(vec![irq, sibling], stop, &mut clock, |_, _| Ok(()))
        });
        assert!(stop.load(Ordering::Relaxed));
        assert!(sibling_joined.load(Ordering::Relaxed));
        assert_eq!(
            clock.sleeps, 0,
            "finished IRQ worker must stop the live sibling immediately"
        );
        assert_eq!(result.unwrap_err().to_string(), expected);
    }

    #[test]
    fn supervisor_stops_and_joins_sibling_after_irq_error() {
        failed_worker(
            || Err(Error::Contract("IRQ fixture failed")),
            "IRQ fixture failed",
        );
    }
    #[test]
    fn supervisor_stops_and_joins_sibling_after_irq_panic() {
        failed_worker(|| panic!("IRQ fixture panic"), "sensor worker panicked");
    }
    #[test]
    fn supervisor_rejects_unrequested_successful_worker_exit() {
        failed_worker(|| Ok(()), "sensor worker exited unexpectedly");
    }
    #[test]
    fn supervisor_keeps_both_healthy_workers_until_requested_stop() {
        let stop = AtomicBool::new(false);
        let joined = std::sync::atomic::AtomicUsize::new(0);
        let mut clock = StopClock {
            stop: &stop,
            sleeps: 0,
        };
        let result = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for _ in 0..2 {
                workers.push(scope.spawn(|| {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::yield_now();
                    }
                    joined.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }));
            }
            supervise(workers, &stop, &mut clock, |_, _| Ok(()))
        });
        result.unwrap();
        assert_eq!(clock.sleeps, 1);
        assert_eq!(joined.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn supervisor_joins_all_workers_even_if_error_logging_fails() {
        let stop = AtomicBool::new(false);
        let joined = AtomicBool::new(false);
        let mut clock = StopClock {
            stop: &stop,
            sleeps: 0,
        };
        std::thread::scope(|scope| {
            let (release, wait) = std::sync::mpsc::channel();
            let joined = &joined;
            let sibling = scope.spawn(move || {
                wait.recv().unwrap();
                joined.store(true, Ordering::Relaxed);
                Ok(())
            });
            let irq = scope.spawn(|| Err(Error::Contract("IRQ fixture failed")));
            while !irq.is_finished() {
                std::thread::yield_now();
            }
            let result = supervise(vec![irq, sibling], &stop, &mut clock, |_, _| {
                release.send(()).unwrap();
                Err(Error::Contract("logging fixture failed"))
            });
            assert_eq!(result.unwrap_err().to_string(), "IRQ fixture failed");
            assert!(
                joined.load(Ordering::Relaxed),
                "join must finish before returning, not just on scope destruction"
            );
        });
    }

    #[test]
    fn shutdown_attempts_every_sensor_despite_bus_and_logging_failures() {
        struct FailedBus<'a>(&'a std::cell::Cell<usize>);
        impl Bus for FailedBus<'_> {
            fn read(&mut self, _: u8, _: usize) -> Result<Vec<u8>, Error> {
                self.0.set(self.0.get() + 1);
                Err(Error::Contract("shutdown bus fixture failed"))
            }
            fn write(&mut self, _: u8, _: u8) -> Result<(), Error> {
                unreachable!()
            }
        }
        for worker_failed in [false, true] {
            let reads = std::cell::Cell::new(0);
            let mut accel = Sensor::new(Kind::Accelerometer, FailedBus(&reads));
            let mut gyro = Sensor::new(Kind::Gyroscope, FailedBus(&reads));
            let mut temperature = Sensor::new(Kind::TemperatureSensor, FailedBus(&reads));
            let result = if worker_failed {
                Err(Error::Contract("IRQ fixture failed"))
            } else {
                Ok(())
            };
            let result = shutdown_sensors(
                &mut [&mut accel, &mut gyro, &mut temperature],
                result,
                |_| Err(Error::Contract("logging fixture failed")),
            );
            assert_eq!(reads.get(), 2);
            assert_eq!(
                result.unwrap_err().to_string(),
                if worker_failed {
                    "IRQ fixture failed"
                } else {
                    "logging fixture failed"
                }
            );
        }
    }
}
