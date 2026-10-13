mod usage;
use num_traits::ToPrimitive;
use std::{cmp::Ordering, fmt::Write, time::Instant};
pub use usage::RuntimeUsage;

#[derive(Clone, Copy, Debug, Default)]
pub struct SampleStats {
    pub total_ms: f64,
    pub max_ms: f64,
    pub last_ms: f64,
    pub count: u64,
}
impl SampleStats {
    #[must_use]
    pub fn average(self) -> f64 {
        self.total_ms / self.count.max(1).to_f64().unwrap_or(f64::MAX)
    }
    pub fn add(&mut self, milliseconds: f64) {
        self.total_ms += milliseconds;
        if self.count == 0 || milliseconds > self.max_ms {
            self.max_ms = milliseconds;
        }
        self.last_ms = milliseconds;
        self.count = self.count.saturating_add(1);
    }
}

pub struct Reporter {
    enabled: bool,
    interval: f64,
    samples: Vec<(String, SampleStats)>,
    reported: f64,
    frames: u64,
}
impl Reporter {
    #[must_use]
    pub fn new(enabled: bool, interval: f64, now: f64) -> Self {
        Self {
            enabled,
            interval: interval.max(0.2),
            samples: Vec::new(),
            reported: now,
            frames: 0,
        }
    }
    pub fn add(&mut self, name: &str, milliseconds: f64) {
        if !self.enabled {
            return;
        }
        if let Some((_, stats)) = self.samples.iter_mut().find(|(key, _)| key == name) {
            stats.add(milliseconds);
        } else {
            let mut stats = SampleStats::default();
            stats.add(milliseconds);
            self.samples.push((name.into(), stats));
        }
    }
    pub fn elapsed(&mut self, name: &str, started: Instant) {
        self.add(name, started.elapsed().as_secs_f64() * 1000.0);
    }
    pub fn samples(&mut self, samples: impl IntoIterator<Item = (String, f64)>) {
        for (name, milliseconds) in samples {
            self.add(&name, milliseconds);
        }
    }
    pub fn frame_done(&mut self) {
        if self.enabled {
            self.frames = self.frames.saturating_add(1);
        }
    }
    #[must_use]
    pub fn ready(&self, now: f64) -> bool {
        self.enabled
            && !matches!(
                (now - self.reported).partial_cmp(&self.interval),
                Some(Ordering::Less)
            )
    }
    #[must_use]
    pub fn elapsed_seconds(&self, now: f64) -> f64 {
        (now - self.reported).max(0.001)
    }
    pub fn report(&mut self, now: f64, runtime: &str) -> Option<String> {
        if !self.ready(now) {
            return None;
        }
        let elapsed = self.elapsed_seconds(now);
        let mut output = format!("PROFILE {} frames / {elapsed:.1}s\n", self.frames);
        if !runtime.is_empty() {
            let _ = writeln!(output, "  runtime {runtime}");
        }
        self.samples.sort_by(|(_, left), (_, right)| {
            right
                .average()
                .partial_cmp(&left.average())
                .unwrap_or(Ordering::Equal)
        });
        for (name, stats) in &self.samples {
            if stats.count == 0 {
                continue;
            }
            let _ = writeln!(
                output,
                "  {name:<42} avg={:7.2}ms max={:7.2}ms last={:7.2}ms n={}",
                stats.average(),
                stats.max_ms,
                stats.last_ms,
                stats.count,
            );
        }
        self.samples.clear();
        self.frames = 0;
        self.reported = now;
        Some(output)
    }
}
