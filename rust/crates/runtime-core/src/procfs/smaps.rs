//! Incremental source-compatible PSS sampling; publication only reads the cache.
use super::{invalid_record, read_record, Identity, Process, ProcessStat, ProportionalMemory};
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

const READ_BYTES: usize = 32 * 1024;
const REFRESH_NS: u64 = 40_000_000_000;
const MAX_AGE_NS: u64 = 120_000_000_000;
const MIN_PAUSE_NS: u64 = 5_000_000;

/// Clock seam used to measure this thread's sampling duty, independently of wall time.
pub trait SamplingClock {
    fn monotonic_ns(&self) -> u64;
    fn thread_cpu_ns(&self) -> u64;
}
struct Scan {
    identity: Identity,
    file: File,
    started: u64,
    pending: Vec<u8>,
    totals: ProportionalMemory,
}
pub(super) struct Sampler {
    root: PathBuf,
    due: Vec<(Identity, u64)>,
    cache: HashMap<Identity, ProportionalMemory>,
    active: Option<Scan>,
    pub next_step: u64,
}
impl Sampler {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            due: Vec::new(),
            cache: HashMap::new(),
            active: None,
            next_step: 0,
        }
    }
    pub fn update(&mut self, processes: &[Process]) {
        let eligible: Vec<_> = processes
            .iter()
            .filter(|p| p.resident_bytes > super::SMAPS_MIN_BYTES)
            .map(|p| Identity {
                pid: p.stat.pid,
                start_ticks: p.stat.start_ticks,
            })
            .collect();
        self.due.retain(|(identity, _)| eligible.contains(identity));
        self.cache.retain(|identity, _| eligible.contains(identity));
        if self
            .active
            .as_ref()
            .is_some_and(|scan| !eligible.contains(&scan.identity))
        {
            self.active = None;
        }
        for identity in eligible {
            if !self.due.iter().any(|(existing, _)| *existing == identity) {
                self.due.push((identity, 0));
            }
        }
    }
    pub fn get(&self, identity: Identity, now: u64) -> ProportionalMemory {
        self.cache
            .get(&identity)
            .filter(|sample| now.saturating_sub(sample.mono_time_ns) <= MAX_AGE_NS)
            .cloned()
            .unwrap_or_default()
    }
    fn current_start(&self, pid: u32) -> io::Result<u64> {
        let bytes = read_record(&self.root.join(pid.to_string()).join("stat"))?;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid_record())?;
        ProcessStat::parse(text)
            .filter(|stat| stat.pid == pid)
            .map(|stat| stat.start_ticks)
            .ok_or_else(invalid_record)
    }
    pub fn step(&mut self, clock: &impl SamplingClock) {
        let now = clock.monotonic_ns();
        if now < self.next_step {
            return;
        }
        let cpu_start = clock.thread_cpu_ns();
        if self.read_step(now, clock).is_err() {
            self.active = None;
        }
        let used = clock.thread_cpu_ns().saturating_sub(cpu_start);
        self.next_step = clock
            .monotonic_ns()
            .saturating_add(MIN_PAUSE_NS.max(used.saturating_mul(19)));
    }
    fn read_step(&mut self, now: u64, clock: &impl SamplingClock) -> io::Result<()> {
        if self.active.is_none() {
            let Some(index) = self
                .due
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, due))| *due)
                .map(|(index, _)| index)
            else {
                return Ok(());
            };
            if self.due[index].1 > now {
                return Ok(());
            }
            let identity = self.due[index].0;
            self.due[index].1 = now.saturating_add(REFRESH_NS);
            if self.current_start(identity.pid)? != identity.start_ticks {
                self.cache.remove(&identity);
                return Ok(());
            }
            let dir = self.root.join(identity.pid.to_string());
            let file = match File::open(dir.join("smaps_rollup")) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    File::open(dir.join("smaps"))?
                }
                Err(error) => return Err(error),
            };
            self.active = Some(Scan {
                identity,
                file,
                started: now,
                pending: Vec::new(),
                totals: ProportionalMemory::default(),
            });
        }
        let scan = self.active.as_mut().ok_or_else(invalid_record)?;
        if now.saturating_sub(scan.started) > MAX_AGE_NS {
            let identity = scan.identity;
            if let Some((_, due)) = self.due.iter_mut().find(|(item, _)| *item == identity) {
                *due = now.saturating_add(REFRESH_NS);
            }
            self.active = None;
            return Ok(());
        }
        let mut bytes = [0; READ_BYTES];
        let length = scan.file.read(&mut bytes)?; // Exactly one bounded kernel read per step.
        scan.parse(&bytes[..length], length == 0)?;
        if length == 0 {
            let mut finished = self.active.take().ok_or_else(invalid_record)?;
            if self.current_start(finished.identity.pid)? == finished.identity.start_ticks {
                finished.totals.mono_time_ns = finished.started;
                self.cache.insert(finished.identity, finished.totals);
            } else {
                self.cache.remove(&finished.identity);
            }
            if let Some((_, due)) = self
                .due
                .iter_mut()
                .find(|(item, _)| *item == finished.identity)
            {
                *due = clock.monotonic_ns().saturating_add(REFRESH_NS);
            }
        }
        Ok(())
    }
}
impl Scan {
    fn parse(&mut self, data: &[u8], eof: bool) -> io::Result<()> {
        self.pending.extend_from_slice(data);
        let consumed = if eof {
            self.pending.len()
        } else {
            self.pending
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |index| index + 1)
        };
        for line in self.pending[..consumed].split(|byte| *byte == b'\n') {
            let target = if line.starts_with(b"Pss:") {
                &mut self.totals.pss
            } else if line.starts_with(b"Pss_Anon:") {
                &mut self.totals.anon
            } else if line.starts_with(b"Pss_Shmem:") {
                &mut self.totals.shared
            } else {
                continue;
            };
            let amount = super::kilobytes(
                line.split(u8::is_ascii_whitespace)
                    .filter(|part| !part.is_empty())
                    .nth(1)
                    .and_then(|part| std::str::from_utf8(part).ok()),
            )?;
            *target = target.checked_add(amount).ok_or_else(invalid_record)?;
        }
        self.pending.drain(..consumed);
        if self.pending.len() > READ_BYTES {
            return Err(invalid_record());
        }
        Ok(())
    }
}
