use crate::core::SettingsFlags;
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

struct Flags {
    metric: AtomicBool,
    experimental: AtomicBool,
}

pub struct ParamsPoller {
    flags: Arc<Flags>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), openpilot_params::Error>>>,
    pub failure: Option<openpilot_params::Error>,
}

impl ParamsPoller {
    pub fn new(settings: Arc<Params>, longitudinal: bool) -> Result<Self, crate::core::Error> {
        let flags = Arc::new(Flags {
            metric: AtomicBool::new(settings.get_bool("IsMetric")?),
            experimental: AtomicBool::new(settings.get_bool("ExperimentalMode")?),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let values = Arc::clone(&flags);
        let stopped = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("card-params-reader".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    values
                        .metric
                        .store(settings.get_bool("IsMetric")?, Ordering::Relaxed);
                    values.experimental.store(
                        settings.get_bool("ExperimentalMode")? && longitudinal,
                        Ordering::Relaxed,
                    );
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })?;
        Ok(Self {
            flags,
            stop,
            worker: Some(worker),
            failure: None,
        })
    }
    pub fn flags(&self) -> SettingsFlags {
        SettingsFlags {
            metric: self.flags.metric.load(Ordering::Relaxed),
            experimental: self.flags.experimental.load(Ordering::Relaxed),
        }
    }
    pub fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(())) => (),
                Ok(Err(error)) => self.failure = Some(error),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    }
}
impl Drop for ParamsPoller {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn periodic_reader_updates_without_can_and_masks_experimental_for_stock_longitudinal() {
        let root = tempfile::tempdir().unwrap();
        let settings = Arc::new(Params::open(root.path(), "d").unwrap());
        let mut reader = ParamsPoller::new(Arc::clone(&settings), false).unwrap();
        settings.put_bool("IsMetric", true).unwrap();
        settings.put_bool("ExperimentalMode", true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !reader.flags().metric {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!reader.flags().experimental);
        reader.finish();
        assert!(reader.failure.is_none());
    }
}
