use crate::Error;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct Paths {
    pub ready: PathBuf,
    pub start: PathBuf,
}

pub struct Fixture {
    pub paths: Paths,
    pub stop: Arc<AtomicUsize>,
}

impl Fixture {
    pub fn constructed(&self, started: u64, error: Option<&Error>) -> Result<(), Error> {
        let completed = super::platform::monotonic_ns();
        let record = serde_json::json!({
            "status": if error.is_some() { "error" } else { "ready" },
            "constructor_started_ns": started, "constructor_completed_ns": completed,
            "error": error.map(ToString::to_string),
        });
        let pending = self.paths.ready.with_extension("pending");
        std::fs::write(&pending, serde_json::to_vec(&record)?)?;
        std::fs::rename(pending, &self.paths.ready)?;
        if error.is_some() {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.paths.start.try_exists()? {
            let signal = self.stop.load(Ordering::Relaxed);
            if signal != 0 {
                return Err(Error::Signal(
                    i32::try_from(signal).map_err(|_| Error::IntegerOverflow)?,
                ));
            }
            if Instant::now() >= deadline {
                return Err(Error::Contract(
                    "bounded constructor fixture was not released within20s",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}
