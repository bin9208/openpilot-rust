use super::{linux_stats::LinuxStatsReader, StatsProvider, SystemStats};
use crate::monitors::RefreshWorker;
use std::path::Path;

pub(super) struct Sampler {
    worker: RefreshWorker<SystemStats>,
}
impl Default for Sampler {
    fn default() -> Self {
        let mut reader = LinuxStatsReader::new(Path::new("/proc"));
        let mut last = SystemStats::default();
        Self {
            worker: RefreshWorker::new(
                "cluster-system-stats",
                1.0,
                SystemStats::default(),
                move || {
                    if let Some(value) = reader.sample().map_err(std::io::Error::other)? {
                        last = value;
                    }
                    Ok(last.clone())
                },
            ),
        }
    }
}
impl StatsProvider for Sampler {
    fn sample(&mut self, now: f64) -> SystemStats {
        match self.worker.request(now) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("cluster-system-stats refresh failed: {error}");
                self.worker.snapshot()
            }
        }
    }
    fn close(&mut self) {
        if let Err(error) = self.worker.close() {
            eprintln!("cluster-system-stats refresh failed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
