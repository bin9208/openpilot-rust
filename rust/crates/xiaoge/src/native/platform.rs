use super::Error;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn monotonic() -> Result<f64, Error> {
    Ok(openpilot_beepd::monotonic_seconds(
        rustix::time::clock_gettime(rustix::time::ClockId::Monotonic),
    )?)
}

pub fn thread_cpu() -> Result<f64, Error> {
    Ok(openpilot_beepd::monotonic_seconds(
        rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime),
    )?)
}

pub fn timestamp() -> Result<u64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec))
        .map_err(|_| Error::Contract("monotonic timestamp overflow"))
}

pub fn background_affinity() -> Result<(), Error> {
    if Path::new("/TICI").is_file() {
        let mut cores = rustix::thread::CpuSet::new();
        for index in 0..4 {
            cores.set(index);
        }
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}

pub fn sleep(seconds: f64) -> Result<(), Error> {
    thread::sleep(openpilot_beepd::sleep_duration(seconds)?);
    Ok(())
}

pub struct Stop {
    pub requested: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}

impl Stop {
    pub fn new() -> Result<Self, Error> {
        let requested = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&requested))?;
        Ok(Self {
            requested,
            registration,
        })
    }
    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
    pub fn wait(&self, seconds: f64) -> Result<(), Error> {
        let mut remaining = openpilot_beepd::sleep_duration(seconds)?;
        while !remaining.is_zero() && !self.requested() {
            let step = remaining.min(Duration::from_millis(20));
            thread::sleep(step);
            remaining -= step;
        }
        Ok(())
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.registration);
    }
}
