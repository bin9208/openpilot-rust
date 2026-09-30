use crate::{Error, NativeCommand, PersistentDaemonProcess, ProcessLog, ProcessState, Signal};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
};
use rustix::process::{kill_process, Pid};
use std::{
    os::unix::process::ExitStatusExt,
    process::ExitStatus,
    time::{Duration, Instant},
};

pub enum Execution {
    Native(NativeCommand),
    Persistent(PersistentDaemonProcess),
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessPolicy {
    pub enabled: bool,
    pub sigkill: bool,
    pub restart_if_crash: bool,
}

impl Default for ProcessPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            sigkill: false,
            restart_if_crash: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StopOptions {
    pub retry: bool,
    pub block: bool,
    pub signal: Option<Signal>,
}

impl Default for StopOptions {
    fn default() -> Self {
        Self {
            retry: true,
            block: true,
            signal: None,
        }
    }
}

pub struct ManagedProcess {
    name: String,
    execution: Execution,
    child: Option<crate::launch::ChildHandle>,
    shutting_down: bool,
    logger: ProcessLog,
    pub policy: ProcessPolicy,
}

fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(0))
}

impl ManagedProcess {
    pub fn new(name: String, execution: Execution, logger: ProcessLog) -> Self {
        Self {
            name,
            execution,
            child: None,
            shutting_down: false,
            logger,
            policy: ProcessPolicy::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn has_process(&self) -> bool {
        self.child.is_some()
    }
    pub fn shutting_down(&self) -> bool {
        self.shutting_down
    }
    pub fn prepare(&self) {}

    pub fn exit_code(&mut self) -> Result<Option<i32>, Error> {
        match &mut self.child {
            Some(child) => Ok(child.process.try_wait()?.map(exit_code)),
            None => Ok(None),
        }
    }

    pub fn start(&mut self) -> Result<(), Error> {
        if let Execution::Persistent(daemon) = &mut self.execution {
            return daemon.start(&self.name, &self.logger);
        }
        if self.shutting_down {
            self.stop(StopOptions::default())?;
        }
        if self.child.is_some() {
            return Ok(());
        }
        self.logger.emit(
            log_site!(),
            Record::text(Level::Info, format!("starting process {}", self.name)),
        )?;
        match &mut self.execution {
            Execution::Native(command) => {
                self.child = Some(command.spawn(&self.name)?);
            }
            Execution::Persistent(daemon) => return daemon.start(&self.name, &self.logger),
        }
        self.shutting_down = false;
        Ok(())
    }

    pub fn signal(&mut self, signal: Signal) -> Result<(), Error> {
        let Some(child) = &mut self.child else {
            return Ok(());
        };
        if child.process.try_wait()?.is_some() {
            return Ok(());
        }
        let pid = i32::try_from(child.process.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or(Error::PidRange)?;
        self.logger.emit(
            log_site!(),
            Record::text(
                Level::Info,
                format!("sending signal {} to {}", signal.as_raw(), self.name),
            ),
        )?;
        kill_process(pid, signal).map_err(std::io::Error::from)?;
        Ok(())
    }

    pub fn stop(&mut self, options: StopOptions) -> Result<Option<i32>, Error> {
        if self.child.is_none() {
            return Ok(None);
        }
        if self.exit_code()?.is_none() {
            if !self.shutting_down {
                self.logger.emit(
                    log_site!(),
                    Record::text(Level::Info, format!("killing {}", self.name)),
                )?;
                let signal = options.signal.unwrap_or(if self.policy.sigkill {
                    Signal::KILL
                } else {
                    Signal::INT
                });
                self.signal(signal)?;
                self.shutting_down = true;
                if !options.block {
                    return Ok(None);
                }
            }
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(5) && self.exit_code()?.is_none() {
                std::thread::sleep(Duration::from_millis(1));
            }
            if self.exit_code()?.is_none() && options.retry {
                self.logger.emit(
                    log_site!(),
                    Record::text(Level::Info, format!("killing {} with SIGKILL", self.name)),
                )?;
                self.signal(Signal::KILL)?;
                if let Some(child) = &mut self.child {
                    child.process.wait()?;
                }
            }
        }
        let result = self.exit_code()?;
        let status = result.map_or_else(|| "None".into(), |value| value.to_string());
        self.logger.emit(
            log_site!(),
            Record::text(Level::Info, format!("{} is dead with {status}", self.name)),
        )?;
        if self.exit_code()?.is_some() {
            self.shutting_down = false;
            self.child = None;
        }
        Ok(result)
    }

    pub fn restart(&mut self) -> Result<(), Error> {
        self.stop(StopOptions {
            signal: Some(Signal::KILL),
            ..StopOptions::default()
        })?;
        self.start()
    }

    pub fn state(&mut self) -> Result<ProcessState, Error> {
        let mut state = ProcessState {
            name: self.name.clone(),
            pid: 0,
            running: false,
            should_be_running: false,
            exit_code: 0,
        };
        if let Some(child) = &mut self.child {
            state.running = child.process.try_wait()?.is_none();
            state.should_be_running = !self.shutting_down;
            state.pid = i32::try_from(child.process.id()).map_err(|_| Error::PidRange)?;
            state.exit_code = child.process.try_wait()?.map_or(0, exit_code);
        }
        Ok(state)
    }

    pub(crate) fn report_restart(&mut self) -> Result<(), Error> {
        let status = self
            .exit_code()?
            .map_or_else(|| "None".into(), |value| value.to_string());
        self.logger.emit(
            log_site!(),
            Record::text(
                Level::Error,
                format!("Restarting {} (exitcode {status})", self.name),
            ),
        )?;
        Ok(())
    }
}
