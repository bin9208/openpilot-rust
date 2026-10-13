use super::{read_cluster_threads, RefreshWorker, ThreadCpuSample};
use num_traits::AsPrimitive;
use serde::Serialize;
use std::{collections::BTreeMap, fmt::Display, io, path::PathBuf};

#[derive(Clone, Debug, Default, Serialize)]
pub struct CoreUsageSnapshot {
    pub text: Option<String>,
    pub by_core: Vec<(i64, f64)>,
    pub by_process: Vec<(String, f64)>,
    pub active_threads: usize,
}

pub struct CoreHistory {
    clock_ticks: i64,
    previous: Option<BTreeMap<(i64, i64, i64), i64>>,
    last_time: Option<f64>,
}

impl CoreHistory {
    #[must_use]
    pub fn new(clock_ticks: i64) -> Self {
        Self {
            clock_ticks,
            previous: None,
            last_time: None,
        }
    }

    pub fn observe(&mut self, samples: &[ThreadCpuSample], now: f64) -> CoreUsageSnapshot {
        let mut snapshot = CoreUsageSnapshot::default();
        if let Some((previous, last_time)) = self.previous.as_ref().zip(self.last_time) {
            let elapsed = (now - last_time).max(0.001);
            for sample in samples {
                let Some(previous) = previous.get(&sample.key) else {
                    continue;
                };
                if sample.processor < 0 {
                    continue;
                }
                let delta = (i128::from(sample.cpu_time_ticks) - i128::from(*previous)).max(0);
                if delta == 0 {
                    continue;
                }
                let ticks: f64 = delta.as_();
                let clock_ticks: f64 = self.clock_ticks.max(1).as_();
                let percent = ticks / clock_ticks / elapsed * 100.0;
                accumulate(&mut snapshot.by_core, sample.processor, percent);
                accumulate(
                    &mut snapshot.by_process,
                    sample.process_name.clone(),
                    percent,
                );
                snapshot.active_threads += 1;
            }
            let mut cores = snapshot.by_core.clone();
            cores.sort_by_key(|(core, _)| *core);
            let parts: Vec<_> = cores
                .iter()
                .filter(|(_, percent)| *percent >= 0.5)
                .map(|(core, percent)| {
                    let percent: u64 = percent.round_ties_even().max(1.0).as_();
                    format!("{core}({percent})")
                })
                .collect();
            snapshot.text = Some(format!("[{}]", parts.join(",")));
        }
        self.previous = Some(
            samples
                .iter()
                .map(|sample| (sample.key, sample.cpu_time_ticks))
                .collect(),
        );
        self.last_time = Some(now);
        snapshot
    }
}

fn accumulate<T: PartialEq>(items: &mut Vec<(T, f64)>, key: T, percent: f64) {
    if let Some((_, value)) = items.iter_mut().find(|(item, _)| *item == key) {
        *value += percent;
    } else {
        items.push((key, percent));
    }
}

fn format_percent_map<T: Display>(items: &[(T, f64)]) -> String {
    let mut values: Vec<_> = items
        .iter()
        .filter(|(_, percent)| *percent >= 0.5)
        .collect();
    values.sort_by(|(_, a), (_, b)| b.total_cmp(a));
    let parts: Vec<_> = values
        .into_iter()
        .map(|(key, percent)| format!("{key}:{percent:.1}%"))
        .collect();
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(",")
    }
}

pub struct CoreUsageSampler {
    worker: RefreshWorker<Option<String>>,
}

impl CoreUsageSampler {
    #[must_use]
    pub fn new(proc_root: PathBuf, current_pid: i64, interval: f64, debug: bool) -> Self {
        let mut history = CoreHistory::new(rustix::param::clock_ticks_per_second().as_());
        Self {
            worker: RefreshWorker::new("cluster-core-usage", interval, None, move || {
                let now = monotonic();
                let started = monotonic();
                let scan = read_cluster_threads(&proc_root, current_pid);
                let elapsed = (monotonic() - started) * 1000.0;
                let snapshot = history.observe(&scan.samples, now);
                if debug {
                    println!(
                        "CLUSTER_CORE_USAGE scan={elapsed:.2}ms candidates={} matched={} threads={} active_threads={} cores={} procs={} text={}",
                        scan.candidate_processes,
                        scan.matched_processes,
                        scan.samples.len(),
                        snapshot.active_threads,
                        format_percent_map(&snapshot.by_core),
                        format_percent_map(&snapshot.by_process),
                        snapshot.text.as_deref().unwrap_or("-")
                    );
                }
                Ok(snapshot.text)
            }),
        }
    }

    /// # Errors
    /// Returns an OS worker creation failure.
    pub fn sample_text(&mut self, now: f64) -> io::Result<Option<String>> {
        self.worker.request(now)
    }

    /// # Errors
    /// Returns a worker panic after joining it.
    pub fn close(&mut self) -> io::Result<()> {
        self.worker.close()
    }
}

pub(super) fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let seconds: f64 = now.tv_sec.as_();
    let nanos: f64 = now.tv_nsec.as_();
    seconds + nanos * 1e-9
}
