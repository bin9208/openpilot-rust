use crate::{
    lifecycle::{ExitAction, Input, Runtime},
    processes::Processes,
    Error,
};
use openpilot_cereal::log_capnp::{event, panda_state::PandaType};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_process_supervision::ProcessState;
use std::{io::Write, path::PathBuf, time::Duration};

/// Only reporting and destructive board actions remain installation-owned.
pub trait ExitBoundary {
    fn capture_exception(&mut self, error: &Error) -> Result<(), Error>;
    fn exit(&mut self, action: ExitAction) -> Result<(), Error>;
}
pub struct NativeRuntime<B> {
    pub params: openpilot_params::Params,
    pub processes: Processes,
    pub subscriber: SubMaster,
    pub publisher: PubMaster,
    pub update_status: openpilot_checkout_status::UpdateStatus,
    pub watchdog_path: PathBuf,
    pub logger: Logger,
    pub boundary: B,
    pub signals: crate::signals::Signals,
    pub loop_time: Duration,
}

pub fn subscriptions(isolated: bool) -> Result<(SubMaster, PubMaster), Error> {
    let options = Options {
        poll: Poll::One("deviceState".into()),
        ..Options::default()
    };
    let services = &["deviceState", "carParams", "pandaStates"];
    Ok(if isolated {
        (
            SubMaster::isolated(services, options)?,
            PubMaster::isolated(&["managerState"])?,
        )
    } else {
        (
            SubMaster::for_runtime(services, options)?,
            PubMaster::for_runtime(&["managerState"])?,
        )
    })
}
fn monotonic() -> Duration {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(
        u64::try_from(time.tv_sec).unwrap_or(0),
        u32::try_from(time.tv_nsec).unwrap_or(0),
    )
}
impl<B: ExitBoundary> NativeRuntime<B> {
    fn not_car(&self) -> Result<bool, Error> {
        match self.subscriber.state.topic("carParams")?.event()?.which()? {
            event::CarParams(value) => Ok(value?.get_not_car()),
            _ => Err(Error::Contract("carParams has wrong event union")),
        }
    }
}
impl<B: ExitBoundary> Runtime for NativeRuntime<B> {
    fn poll(&mut self) -> Result<Input, Error> {
        if self.signals.requested() {
            return Err(Error::Interrupted);
        }
        self.subscriber.update(Duration::from_millis(1000))?;
        self.loop_time = monotonic();
        if self.signals.requested() {
            return Err(Error::Interrupted);
        }
        let started = match self
            .subscriber
            .state
            .topic("deviceState")?
            .event()?
            .which()?
        {
            event::DeviceState(value) => value?.get_started(),
            _ => return Err(Error::Contract("deviceState has wrong event union")),
        };
        let mut ignition = false;
        match self
            .subscriber
            .state
            .topic("pandaStates")?
            .event()?
            .which()?
        {
            event::PandaStates(values) => {
                for value in values? {
                    if value.get_panda_type()? != PandaType::Unknown
                        && (value.get_ignition_line() || value.get_ignition_can())
                    {
                        ignition = true;
                    }
                }
            }
            _ => return Err(Error::Contract("pandaStates has wrong event union")),
        }
        Ok(Input {
            started,
            ignition,
            not_car: self.not_car()?,
            device_checks: self.subscriber.state.all_checks(&["deviceState"])?,
        })
    }
    fn initial_not_car(&self) -> Result<bool, Error> {
        self.not_car()
    }
    fn ensure_running(
        &mut self,
        started: bool,
        not_car: bool,
        ignore: &[String],
    ) -> Result<(), Error> {
        self.processes.ensure(
            openpilot_manager_catalog::State { started, not_car },
            &mut self.params,
            &mut openpilot_manager_catalog::SystemGpsPaths,
            ignore,
        )
    }
    fn states(&mut self) -> Result<Vec<ProcessState>, Error> {
        self.processes.states()
    }
    fn publish(&mut self, states: &[ProcessState]) -> Result<(), Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(true);
        event.set_log_mono_time(
            u64::try_from(monotonic().as_nanos())
                .map_err(|_| Error::Contract("monotonic clock overflow"))?,
        );
        let mut manager = event.init_manager_state();
        let mut processes = manager.reborrow().init_processes(
            u32::try_from(states.len()).map_err(|_| Error::Contract("too many processes"))?,
        );
        for (index, state) in states.iter().enumerate() {
            state.write(
                processes.reborrow().get(
                    u32::try_from(index).map_err(|_| Error::Contract("process index overflow"))?,
                ),
            );
        }
        manager.set_reboot_required(self.update_status.update(self.loop_time.as_secs_f64()));
        self.publisher.send(
            "managerState",
            &capnp::serialize::write_message_to_words(&message),
        )?;
        Ok(())
    }
    fn watchdog(&mut self) -> Result<(), Error> {
        let parent = self
            .watchdog_path
            .parent()
            .ok_or(Error::Contract("watchdog has no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        write!(temporary, "{}", monotonic().as_secs_f64())?;
        temporary
            .persist(&self.watchdog_path)
            .map_err(|error| error.error)?;
        Ok(())
    }
    fn timestamp(&mut self) -> String {
        chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S%.6f")
            .to_string()
    }
    fn log_running(&mut self, states: &[ProcessState], print: bool) -> Result<(), Error> {
        let text = states
            .iter()
            .filter(|state| state.pid != 0)
            .map(|state| {
                format!(
                    "\x1b[{}m{}\x1b[0m",
                    if state.running { 32 } else { 31 },
                    state.name
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        if print {
            println!("{text}");
        }
        self.logger
            .emit(log_site!(), Record::text(Level::Debug, text))?;
        Ok(())
    }
    fn warning(&mut self, message: &str) -> Result<(), Error> {
        self.logger
            .emit(log_site!(), Record::text(Level::Warning, message.into()))?;
        Ok(())
    }
    fn stop(&mut self, block: bool) -> Result<(), Error> {
        self.processes.stop(block)
    }
    fn capture_exception(&mut self, error: &Error) -> Result<(), Error> {
        self.boundary.capture_exception(error)
    }
    fn exit(&mut self, action: ExitAction) -> Result<(), Error> {
        self.boundary.exit(action)
    }
}
