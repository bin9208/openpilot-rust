//! Read-only procfs collection for the canonical procLog fields.
use crate::proc_stat::ProcessStat;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    num::NonZeroU64,
    path::{Path, PathBuf},
};

const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const SMAPS_MIN_BYTES: u64 = 5 * 1024 * 1024;

/// Per-core CPU counters in the kernel's clock ticks, not percentages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTimes {
    pub cpu: u32,
    /// user, nice, system, idle, iowait, irq, softirq.
    pub ticks: [u64; 7],
}

/// The eight procLog memory fields, all in bytes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub active: u64,
    pub inactive: u64,
    pub shared: u64,
}

/// Proportional memory from smaps_rollup, or the per-mapping fallback.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProportionalMemory {
    pub pss: u64,
    pub anon: u64,
    pub shared: u64,
}

/// A process record and metadata belonging to the same observed identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub stat: ProcessStat,
    pub resident_bytes: u64,
    pub exe: String,
    pub cmdline: Vec<String>,
    pub proportional: ProportionalMemory,
}

/// Failed system-file reads are reported without inventing measurements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionWarning {
    pub source: &'static str,
    pub kind: io::ErrorKind,
}

/// One collection with explicit units, suitable for canonical serialization.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub ticks_per_second: NonZeroU64,
    pub page_size: NonZeroU64,
    pub cpu_times: Vec<CpuTimes>,
    pub memory: Memory,
    pub processes: Vec<Process>,
    pub warnings: Vec<CollectionWarning>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct Identity {
    pid: u32,
    start_ticks: u64,
}

struct Metadata {
    name: String,
    exe: String,
    cmdline: Vec<String>,
}

/// Stateful collector; all paths are relative to an explicit procfs root.
pub struct Collector {
    root: PathBuf,
    ticks: NonZeroU64,
    page_size: NonZeroU64,
    metadata: HashMap<Identity, Metadata>,
    smaps: HashMap<Identity, ProportionalMemory>,
    smaps_cycle: u8,
    smaps_file: Option<&'static str>,
}

impl Collector {
    /// Construct without opening procfs or changing any system state.
    #[must_use]
    pub fn new(root: &Path, ticks: NonZeroU64, page_size: NonZeroU64) -> Self {
        Self {
            root: root.to_path_buf(),
            ticks,
            page_size,
            metadata: HashMap::new(),
            smaps: HashMap::new(),
            smaps_cycle: 0,
            smaps_file: None,
        }
    }

    /// Collect readable processes, dropping vanished or malformed records.
    ///
    /// # Errors
    /// Returns the directory error if the procfs root cannot be enumerated.
    /// Individual process races are skipped; CPU/memory failures are warnings.
    pub fn snapshot(&mut self) -> io::Result<Snapshot> {
        let entries = fs::read_dir(&self.root)?;
        let mut processes = Vec::new();
        let mut seen = HashSet::new();
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let dir = entry.path();
            let Ok(bytes) = read_record(&dir.join("stat")) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let Some(stat) = ProcessStat::parse(text) else {
                continue;
            };
            if stat.pid != pid {
                continue;
            }
            let Some(resident_bytes) = u64::try_from(stat.rss_pages)
                .ok()
                .and_then(|pages| pages.checked_mul(self.page_size.get()))
            else {
                continue;
            };
            let identity = Identity {
                pid,
                start_ticks: stat.start_ticks,
            };
            seen.insert(identity);
            if self
                .metadata
                .get(&identity)
                .is_none_or(|m| m.name != stat.name)
            {
                self.metadata
                    .insert(identity, read_metadata(&dir, &stat.name));
            }
            let proportional = if resident_bytes > SMAPS_MIN_BYTES {
                let file = *self.smaps_file.get_or_insert_with(|| {
                    if dir.join("smaps_rollup").exists() {
                        "smaps_rollup"
                    } else {
                        "smaps"
                    }
                });
                if self.smaps_cycle == 0 || !self.smaps.contains_key(&identity) {
                    let value = read_smaps(&dir.join(file)).unwrap_or_default();
                    self.smaps.insert(identity, value);
                }
                self.smaps.get(&identity).cloned().unwrap_or_default()
            } else {
                ProportionalMemory::default()
            };
            let stable = read_record(&dir.join("stat"))
                .ok()
                .and_then(|bytes| {
                    std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(ProcessStat::parse)
                })
                .is_some_and(|latest| {
                    latest.pid == pid
                        && latest.start_ticks == stat.start_ticks
                        && latest.name == stat.name
                });
            if !stable {
                seen.remove(&identity);
                self.metadata.remove(&identity);
                self.smaps.remove(&identity);
                continue;
            }
            if let Some(metadata) = self.metadata.get(&identity) {
                processes.push(Process {
                    stat,
                    resident_bytes,
                    proportional,
                    exe: metadata.exe.clone(),
                    cmdline: metadata.cmdline.clone(),
                });
            }
        }
        self.metadata.retain(|identity, _| seen.contains(identity));
        self.smaps.retain(|identity, _| seen.contains(identity));
        self.smaps_cycle = self.smaps_cycle.wrapping_add(1) % 20;
        processes.sort_by_key(|p| p.stat.pid);

        let mut warnings = Vec::new();
        let cpu_times = read_cpu_times(&self.root.join("stat")).unwrap_or_else(|e| {
            warnings.push(CollectionWarning {
                source: "stat",
                kind: e.kind(),
            });
            Vec::new()
        });
        let memory = read_memory(&self.root.join("meminfo")).unwrap_or_else(|e| {
            warnings.push(CollectionWarning {
                source: "meminfo",
                kind: e.kind(),
            });
            Memory::default()
        });
        Ok(Snapshot {
            ticks_per_second: self.ticks,
            page_size: self.page_size,
            cpu_times,
            memory,
            processes,
            warnings,
        })
    }
}

fn invalid_record() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid procfs record")
}

fn read_record(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| invalid_record())? > MAX_RECORD_BYTES {
        return Err(invalid_record());
    }
    Ok(bytes)
}

fn read_metadata(dir: &Path, name: &str) -> Metadata {
    let exe = fs::read_link(dir.join("exe"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cmdline = read_record(&dir.join("cmdline"))
        .unwrap_or_default()
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect();
    Metadata {
        name: name.to_owned(),
        exe,
        cmdline,
    }
}

fn read_cpu_times(path: &Path) -> io::Result<Vec<CpuTimes>> {
    let bytes = read_record(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid_record())?;
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let Some(cpu) = fields.next().and_then(|v| v.strip_prefix("cpu")) else {
            break;
        };
        let cpu = cpu.parse().map_err(|_| invalid_record())?;
        let mut ticks = [0; 7];
        for tick in &mut ticks {
            *tick = fields
                .next()
                .ok_or_else(invalid_record)?
                .parse()
                .map_err(|_| invalid_record())?;
        }
        result.push(CpuTimes { cpu, ticks });
    }
    Ok(result)
}

fn kilobytes(value: Option<&str>) -> io::Result<u64> {
    value
        .ok_or_else(invalid_record)?
        .parse::<u64>()
        .map_err(|_| invalid_record())?
        .checked_mul(1024)
        .ok_or_else(invalid_record)
}

fn read_memory(path: &Path) -> io::Result<Memory> {
    let bytes = read_record(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid_record())?;
    let mut memory = Memory::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let target = match fields.next() {
            Some("MemTotal:") => &mut memory.total,
            Some("MemFree:") => &mut memory.free,
            Some("MemAvailable:") => &mut memory.available,
            Some("Buffers:") => &mut memory.buffers,
            Some("Cached:") => &mut memory.cached,
            Some("Active:") => &mut memory.active,
            Some("Inactive:") => &mut memory.inactive,
            Some("Shmem:") => &mut memory.shared,
            _ => continue,
        };
        *target = kilobytes(fields.next())?;
    }
    Ok(memory)
}

fn read_smaps(path: &Path) -> io::Result<ProportionalMemory> {
    let mut memory = ProportionalMemory::default();
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        let mut fields = line.split_whitespace();
        let target = match fields.next() {
            Some("Pss:") => &mut memory.pss,
            Some("Pss_Anon:") => &mut memory.anon,
            Some("Pss_Shmem:") => &mut memory.shared,
            _ => continue,
        };
        *target = target
            .checked_add(kilobytes(fields.next())?)
            .ok_or_else(invalid_record)?;
    }
    Ok(memory)
}
