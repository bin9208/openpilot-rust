mod comm;
mod step;
use crate::{
    can_wire,
    core::{Error, SERVICES},
    isotp,
};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use openpilot_logging::{
    producer::{Factory, Logger},
    runtime::RuntimeDiagnostics,
};
use openpilot_messaging::{
    runtime::PubMaster,
    services,
    state::{Options, State},
};
use openpilot_msgq::{MultiSubscriber, Subscriber, Subscription};
use openpilot_params::Params;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub struct NativeIo {
    can: Subscriber,
    subscriptions: MultiSubscriber,
    state: State,
    publisher: PubMaster,
    settings: Arc<Params>,
    writes: crate::async_params::AsyncParams,
    params_reader: Option<super::params_poll::ParamsPoller>,
    pub(super) logger: Logger,
    pub(super) carlog_level: openpilot_logging::record::Level,
    diagnostics: RuntimeDiagnostics,
    stop: Arc<AtomicBool>,
}

impl Drop for NativeIo {
    fn drop(&mut self) {
        if let Some(reader) = &mut self.params_reader {
            reader.finish();
        }
        self.writes.finish();
    }
}

pub fn monotonic_ns() -> u64 {
    clock_ns(rustix::time::ClockId::Monotonic)
}
fn clock_ns(id: rustix::time::ClockId) -> u64 {
    let time = rustix::time::clock_gettime(id);
    u64::try_from(i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec))
        .expect("nonnegative native clock fits u64 nanoseconds")
}
pub fn monotonic() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    time.tv_sec.to_f64().expect("i64 seconds fit f64")
        + time.tv_nsec.to_f64().expect("nanoseconds fit f64") / 1e9
}
pub(super) fn transport_error(error: Error) -> isotp::Error {
    io::Error::other(error).into()
}

impl NativeIo {
    pub fn new(settings: Params, stop: Arc<AtomicBool>) -> Result<Self, Error> {
        let logprint = match std::env::var("LOGPRINT") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(Error::LoggingLevel("non-UTF-8 value".into()));
            }
        };
        let carlog_level = super::carlog::level(logprint.as_deref())?;
        let settings = Arc::new(settings);
        let simulation = match std::env::var("SIMULATION") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(openpilot_messaging::state::Error::Configuration(
                    "invalid SIMULATION value",
                )
                .into());
            }
        };
        let simulation = simulation.trim().parse::<i64>().map_err(|_| {
            openpilot_messaging::state::Error::Configuration("SIMULATION must be an integer")
        })? != 0;
        let state = State::new(
            &SERVICES,
            Options {
                simulation,
                ..Options::default()
            },
        )?;
        let specs: Vec<_> = state
            .topics()
            .iter()
            .map(|topic| Subscription {
                endpoint: topic.service.name,
                capacity: topic.service.queue_size,
                polled: topic.polled,
            })
            .collect();
        let subscriptions = MultiSubscriber::for_runtime(&specs)?;
        let service = services::lookup("can").ok_or(Error::Event("can service registry"))?;
        Ok(Self {
            can: Subscriber::for_runtime("can", false, service.queue_size)?,
            subscriptions,
            state,
            writes: crate::async_params::AsyncParams::new(Arc::clone(&settings)),
            params_reader: None,
            settings,
            stop,
            publisher: PubMaster::for_runtime(&["sendcan", "carState", "carParams", "carOutput"])?,
            logger: Factory::for_runtime()?.logger(),
            carlog_level,
            diagnostics: RuntimeDiagnostics::new("card", 1.),
        })
    }
    pub fn wait_for_startup(&mut self) -> Result<usize, Error> {
        self.wait_for_can()?;
        self.wait_for_pandas()
    }
    pub fn start_params_reader(&mut self, longitudinal: bool) -> Result<(), Error> {
        self.params_reader = Some(super::params_poll::ParamsPoller::new(
            Arc::clone(&self.settings),
            longitudinal,
        )?);
        Ok(())
    }
    pub fn wait_for_can(&mut self) -> Result<(), Error> {
        loop {
            self.check_stop()?;
            if let Some(bytes) = self.can.receive(Duration::from_millis(20))? {
                if !can_wire::decode(&bytes)?.frames.is_empty() {
                    return Ok(());
                }
            }
        }
    }
    pub fn wait_for_pandas(&mut self) -> Result<usize, Error> {
        loop {
            self.check_stop()?;
            if let Some(bytes) = self.subscriptions.receive_one(0)? {
                let message = capnp::serialize::read_message(
                    std::io::Cursor::new(bytes),
                    capnp::message::ReaderOptions::new(),
                )?;
                let event::Which::PandaStates(pandas) =
                    message.get_root::<event::Reader>()?.which()?
                else {
                    return Err(Error::Event("pandaStates"));
                };
                return Ok(usize::try_from(pandas?.len())?);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn check_stop(&self) -> Result<(), Error> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "card interrupted").into());
        }
        Ok(())
    }
    fn drain_can(&mut self, wait: bool) -> Result<Vec<Vec<u8>>, Error> {
        self.check_stop()?;
        let mut result = Vec::new();
        let timeout = if wait {
            Duration::from_millis(20)
        } else {
            Duration::ZERO
        };
        if let Some(bytes) = self.can.receive(timeout)? {
            result.push(bytes);
            while let Some(bytes) = self.can.receive(Duration::ZERO)? {
                result.push(bytes);
            }
        }
        Ok(result)
    }
}
