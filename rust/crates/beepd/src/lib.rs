//! Source: openpilot/selfdrive/controls/beep.py and common/params.h::getInt.
//! GPIO commands remain external; production manager selection is unchanged.
mod integer;
pub mod runtime;
pub use integer::{integer, IntegerError};
use openpilot_cereal::car_capnp::car_control::h_u_d_control::AudibleAlert;
use openpilot_params::Params;
use std::{
    io::{self, Write},
    process::{Command, ExitStatus, Stdio},
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("SoundVolumeAdjust: {0}")]
    Integer(#[from] IntegerError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Capnp(#[from] capnp::Error),
    #[error("{0}")]
    Contract(&'static str),
}

pub trait Commands: Send + Sync {
    fn run(&self, command: &str) -> io::Result<ExitStatus>;
}
pub struct Shell;
impl Commands for Shell {
    fn run(&self, command: &str) -> io::Result<ExitStatus> {
        Command::new("/bin/sh")
            .args(["-c", command])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    }
}
pub trait Clock: Send + Sync {
    fn monotonic(&self) -> Result<f64, Error>;
    fn sleep(&self, seconds: f64) -> Result<(), Error>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn monotonic(&self) -> Result<f64, Error> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        monotonic_seconds(time)
    }
    fn sleep(&self, seconds: f64) -> Result<(), Error> {
        thread::sleep(sleep_duration(seconds)?);
        Ok(())
    }
}
/// Preserve CPython's integral-seconds shortcut and total-nanoseconds rounding.
pub fn monotonic_seconds(time: rustix::time::Timespec) -> Result<f64, Error> {
    let nanos = i64::try_from(i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec))
        .map_err(|_| Error::Contract("monotonic clock exceeds Python time range"))?;
    Ok(if nanos % 1_000_000_000 == 0 {
        (nanos / 1_000_000_000) as f64
    } else {
        nanos as f64 / 1e9
    })
}
/// Match Python time.sleep's positive timeout rounding and signed time range.
pub fn sleep_duration(seconds: f64) -> Result<Duration, Error> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(Error::Contract("invalid sleep interval"));
    }
    let nanos = (seconds * 1e9).ceil();
    if nanos >= -(i64::MIN as f64) {
        return Err(Error::Contract("sleep interval exceeds Python time range"));
    }
    // The checked value is an integral, nonnegative double below 2^63.
    Ok(Duration::from_nanos(nanos as u64))
}
pub trait Output: Send + Sync {
    fn line(&self, text: &str) -> io::Result<()>;
}
pub struct Stdout;
impl Output for Stdout {
    fn line(&self, text: &str) -> io::Result<()> {
        writeln!(io::stdout().lock(), "{text}")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    Startup,
    Engage,
    Disengage,
    Warning,
    Ding,
    Dong,
    Beep,
}
impl Pattern {
    pub fn for_alert(raw: u16) -> Option<Self> {
        use AudibleAlert::*;
        match AudibleAlert::try_from(raw).ok()? {
            Engage => Some(Self::Engage),
            Disengage => Some(Self::Disengage),
            Refuse | Prompt | WarningImmediate | WarningSoft | RadarCutin => Some(Self::Warning),
            LongEngaged | LongDisengaged | TrafficSignGreen | TrafficSignChanged | TrafficError
            | BsdWarning | LaneChange => Some(Self::Ding),
            StopStop | Stopping | AutoHold | Engage2 | Disengage2 | SpeedDown | AudioTurn
            | ReverseGear => Some(Self::Dong),
            Audio1 | Audio2 | Audio3 | Audio4 | Audio5 | Audio6 | Audio7 | Audio8 | Audio9
            | Audio10 => Some(Self::Beep),
            None | PromptRepeat | PromptDistracted | Nnff | RadarStationaryLead => Option::None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup_beep",
            Self::Engage => "engage",
            Self::Disengage => "disengage",
            Self::Warning => "warning",
            Self::Ding => "ding",
            Self::Dong => "dong",
            Self::Beep => "beep",
        }
    }
    fn pulse(self) -> (u8, f64, Option<f64>) {
        match self {
            Self::Startup => (1, 0.1, None),
            Self::Engage => (1, 0.05, None),
            Self::Disengage => (2, 0.01, Some(0.01)),
            Self::Warning => (3, 0.01, Some(0.01)),
            Self::Ding => (1, 0.02, None),
            Self::Dong => (1, 0.03, None),
            Self::Beep => (1, 0.04, None),
        }
    }
}
struct Shared {
    params: Arc<Params>,
    commands: Arc<dyn Commands>,
    clock: Arc<dyn Clock>,
}
impl Shared {
    fn beep(&self, on: bool) -> Result<(), Error> {
        let bytes = match self.params.get("SoundVolumeAdjust") {
            Ok(value) => value.unwrap_or_default(),
            Err(openpilot_params::Error::Io(_)) => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let enabled = integer(&bytes)? > 5 && on;
        let command = if enabled {
            "echo \"1\" | sudo tee /sys/class/gpio/gpio42/value"
        } else {
            "echo \"0\" | sudo tee /sys/class/gpio/gpio42/value"
        };
        // Python subprocess.run has check=False: nonzero command status is ignored.
        self.commands.run(command).map(|_status| ())?;
        Ok(())
    }
    fn play(&self, pattern: Pattern) -> Result<(), Error> {
        let (count, on, off) = pattern.pulse();
        for _ in 0..count {
            self.beep(true)?;
            self.clock.sleep(on)?;
            self.beep(false)?;
            if let Some(off) = off {
                self.clock.sleep(off)?;
            }
        }
        Ok(())
    }
}
pub struct Driver {
    shared: Arc<Shared>,
    output: Arc<dyn Output>,
    current_alert: u16,
    workers: Vec<JoinHandle<()>>,
}
impl Driver {
    pub fn new(
        params: Arc<Params>,
        commands: Arc<dyn Commands>,
        clock: Arc<dyn Clock>,
        output: Arc<dyn Output>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                params,
                commands,
                clock,
            }),
            output,
            current_alert: u16::from(AudibleAlert::None),
            workers: Vec::new(),
        }
    }
    pub fn startup(&self) -> Result<(), Error> {
        // The source suppresses only the export call's exceptions, with no warning.
        match self
            .shared
            .commands
            .run("echo 42 | sudo tee /sys/class/gpio/export")
        {
            Ok(_status) => (),
            Err(_error) => (),
        }
        self.shared
            .commands
            .run("echo \"out\" | sudo tee /sys/class/gpio/gpio42/direction")
            .map(|_status| ())?;
        self.shared.play(Pattern::Startup)
    }
    pub fn play(&self, pattern: Pattern) -> Result<(), Error> {
        self.shared.play(pattern)
    }
    pub fn update_alert(&mut self, alert: u16) -> Result<(), Error> {
        self.reap(false);
        if alert == self.current_alert {
            return Ok(());
        }
        self.current_alert = alert;
        self.output.line(&format!("[BEEP] New alert: {alert}"))?;
        if let Some(pattern) = Pattern::for_alert(alert) {
            let shared = Arc::clone(&self.shared);
            self.workers
                .push(
                    thread::Builder::new()
                        .name(pattern.name().into())
                        .spawn(move || {
                            if let Err(error) = shared.play(pattern) {
                                eprintln!("beepd worker {}: {error}", pattern.name());
                                if matches!(error, Error::Integer(_)) {
                                    // Original get_int lets std::stoi escape Cython and aborts
                                    // the whole process, even on a daemon thread (source #82).
                                    std::process::exit(1);
                                }
                            }
                        })?,
                );
        }
        Ok(())
    }
    pub fn wait_workers(&mut self) {
        self.reap(true);
    }
    fn reap(&mut self, wait: bool) {
        let mut index = 0;
        while index < self.workers.len() {
            if wait || self.workers[index].is_finished() {
                if self.workers.swap_remove(index).join().is_err() {
                    eprintln!("beepd: alert worker panicked");
                }
            } else {
                index += 1;
            }
        }
    }
}
