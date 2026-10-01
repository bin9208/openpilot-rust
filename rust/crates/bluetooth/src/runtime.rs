use crate::{enumerate, ipc, Config, Engine, Input, InputBatch, Seconds};
use indexmap::IndexMap;
use num_traits::ToPrimitive;
use openpilot_logmessaged::JsonValue;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{flock, FlockOperation},
};
use std::{
    fs::{self, File},
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Engine(#[from] crate::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error("{0}")]
    Contract(&'static str),
}

pub struct Paths {
    pub runtime: PathBuf,
    pub config: PathBuf,
    pub sysfs: PathBuf,
    pub devices: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            runtime: "/dev/shm/carrot-bluetooth".into(),
            config: "/data/carrot/bluetooth.json".into(),
            sysfs: "/sys/class/input".into(),
            devices: "/dev/input".into(),
        }
    }
}

fn clock() -> Result<Seconds, RuntimeError> {
    let stamp = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(Seconds(
        stamp
            .tv_sec
            .to_f64()
            .ok_or(RuntimeError::Contract("clock seconds"))?
            + stamp
                .tv_nsec
                .to_f64()
                .ok_or(RuntimeError::Contract("clock nanoseconds"))?
                / 1e9,
    ))
}

fn config(paths: &Paths) -> Config {
    fs::read_to_string(&paths.config)
        .ok()
        .and_then(|text| Config::parse(&text).ok())
        .unwrap_or_default()
}

struct Reader {
    engine: Engine,
    inputs: IndexMap<String, Input>,
    subscriber: SubMaster,
    last_reload: Seconds,
    last_status: Seconds,
}

impl Reader {
    fn cycle(&mut self, paths: &Paths, stop: &AtomicBool) -> Result<(), RuntimeError> {
        let now = clock()?;
        self.subscriber.update(Duration::ZERO)?;
        self.engine.update(ipc::snapshot(&self.subscriber)?);
        if now.0 - self.last_reload.0 >= 0.25 {
            self.last_reload = now;
            let learning = fs::read_to_string(paths.runtime.join("learn.json"))
                .ok()
                .and_then(|text| JsonValue::parse(&text).ok());
            let reload = self.engine.reload(
                config(paths),
                learning,
                &enumerate(&paths.sysfs, &paths.devices),
                now,
            );
            for path in reload.close {
                self.inputs.shift_remove(&path);
            }
            for (path, address) in reload.open {
                match Input::open_interruptible(std::path::Path::new(&path), stop) {
                    Ok(input) => {
                        self.engine.connect(&path, &address)?;
                        self.inputs.insert(path, input);
                    }
                    Err(_) if stop.load(Ordering::Relaxed) => return Ok(()),
                    Err(error) => self.engine.open_error(address, error.to_string()),
                }
            }
        }
        let ready = self.ready(stop)?;
        for (path, readable) in ready {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            if !readable {
                self.engine.flush(&path, clock()?, clock()?)?;
                continue;
            }
            let input = self
                .inputs
                .get_mut(&path)
                .ok_or(RuntimeError::Contract("owned input missing"))?;
            match input.read_interruptible(stop) {
                Ok(InputBatch::Pending) => {}
                Ok(InputBatch::Events(events)) => {
                    let observed = clock()?;
                    for event in events {
                        if stop.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        self.engine.event(&path, event, observed)?;
                    }
                    self.engine.flush(&path, observed, observed)?;
                }
                Err(_) => {
                    self.inputs.shift_remove(&path);
                    self.engine.disconnect(&path);
                }
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = clock()?;
        self.engine.prune(now)?;
        if now.0 - self.last_status.0 >= 0.2 {
            self.last_status = now;
            self.engine.write_status(now)?;
        }
        Ok(())
    }

    fn ready(&self, stop: &AtomicBool) -> Result<Vec<(String, bool)>, RuntimeError> {
        let mut fds: Vec<_> = self
            .inputs
            .values()
            .map(|input| PollFd::new(input, PollFlags::IN))
            .collect();
        let started = Instant::now();
        loop {
            let remaining = Duration::from_millis(10).saturating_sub(started.elapsed());
            let timeout = Timespec::try_from(remaining).map_err(io::Error::other)?;
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) => break,
                Err(rustix::io::Errno::INTR)
                    if !stop.load(Ordering::Relaxed) && !remaining.is_zero() =>
                {
                    continue
                }
                Err(rustix::io::Errno::INTR) => break,
                Err(error) => return Err(io::Error::from(error).into()),
            }
        }
        Ok(self
            .inputs
            .keys()
            .zip(fds.iter())
            .map(|(path, fd)| {
                (
                    path.clone(),
                    fd.revents()
                        .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR),
                )
            })
            .collect())
    }
}

pub fn run(paths: Paths, frames: Option<u64>) -> Result<bool, RuntimeError> {
    fs::create_dir_all(&paths.runtime)?;
    let lock = File::create(paths.runtime.join("reader.lock"))?;
    flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(io::Error::from)?;
    let subscriber = SubMaster::for_runtime(
        &["carState", "deviceState", "selfdriveState"],
        Options::default(),
    )?;
    let mut reader = Reader {
        engine: Engine::new(&paths.runtime, config(&paths))?,
        inputs: IndexMap::new(),
        subscriber,
        last_reload: Seconds(0.),
        last_status: Seconds(0.),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let signal = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let result = (|| {
        let mut count = 0_u64;
        while !stop.load(Ordering::Relaxed) && frames != Some(count) {
            reader.cycle(&paths, &stop)?;
            count = count
                .checked_add(1)
                .ok_or(RuntimeError::Contract("reader frame count exhausted"))?;
        }
        Ok::<(), RuntimeError>(())
    })();
    reader.inputs.clear();
    drop(lock);
    reader.engine.stopped(clock()?)?;
    signal_hook::low_level::unregister(signal);
    result?;
    Ok(stop.load(Ordering::Relaxed))
}
