use crate::{
    diagnostics::Diagnostics, producer::Logger, record::Record, site::Site, Error, Fields, Number,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}

pub struct RuntimeDiagnostics {
    diagnostics: Diagnostics,
    scheduler_path: PathBuf,
}

impl RuntimeDiagnostics {
    pub fn new(component: &str, interval: f64) -> Self {
        let started = monotonic();
        let tid = rustix::thread::gettid().as_raw_nonzero();
        let scheduler_path = PathBuf::from(format!("/proc/self/task/{tid}/schedstat"));
        let previous = schedstat(&scheduler_path);
        let enabled = fs::read_to_string("/proc/sys/kernel/sched_schedstats")
            .ok()
            .map(|text| text.trim() == "1");
        Self {
            diagnostics: Diagnostics::new(
                component,
                interval,
                started,
                previous,
                enabled,
                std::process::id(),
            ),
            scheduler_path,
        }
    }

    pub fn record(
        &mut self,
        logger: &mut Logger,
        site: Site,
        values: impl IntoIterator<Item = (String, Number)>,
        context: Fields,
    ) -> Result<(), Error> {
        self.record_with(values, context, |fields| {
            logger.emit(site, Record::event("runtimeTiming", Vec::new(), fields)?)
        })
    }

    pub fn record_with<T, E>(
        &mut self,
        values: impl IntoIterator<Item = (String, Number)>,
        context: Fields,
        emit: impl FnOnce(Fields) -> Result<T, E>,
    ) -> Result<(), Error> {
        if let Some(fields) = self
            .diagnostics
            .record_with(values, context, monotonic, || {
                schedstat(&self.scheduler_path)
            })?
        {
            // The source resets its aggregate before emission and suppresses sink errors so
            // diagnostic transport failure cannot interrupt inference or planning.
            let _ = emit(fields);
        }
        Ok(())
    }
}

fn schedstat(path: &Path) -> Option<[u64; 3]> {
    let text = fs::read_to_string(path).ok()?;
    let mut values = text.split_whitespace().take(3).map(str::parse);
    Some([
        values.next()?.ok()?,
        values.next()?.ok()?,
        values.next()?.ok()?,
    ])
}
