//! Per-attempt capacity admission, a hard private memory limit and durable
//! aggregate kernel memory / observed filesystem-growth evidence.
use crate::{
    generation,
    memory_cgroup::{Group, Memory},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiskBudget {
    pub maximum_growth_bytes: u64,
    pub minimum_free_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    /// An already delegated memory controller. The build never enables or
    /// remounts a shared parent controller and never silently runs uncontained.
    pub memory_cgroup_parent: PathBuf,
    pub maximum_memory_bytes: u64,
    pub minimum_available_memory_bytes: u64,
    /// One operation-specific budget per filesystem (aliases must agree).
    pub filesystems: BTreeMap<PathBuf, DiskBudget>,
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.maximum_memory_bytes == 0
            || self.minimum_available_memory_bytes == 0
            || self.filesystems.is_empty()
            || self.filesystems.len() > 16
            || !absolute(&self.memory_cgroup_parent)
            || self.filesystems.iter().any(|(path, budget)| {
                !absolute(path)
                    || budget.minimum_free_bytes == 0
                    || budget
                        .maximum_growth_bytes
                        .checked_add(budget.minimum_free_bytes)
                        .is_none()
            })
            || self
                .maximum_memory_bytes
                .checked_add(self.minimum_available_memory_bytes)
                .is_none()
        {
            return Err("native build resource policy needs explicit operation-specific memory/filesystem budgets".into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct Disk {
    path: PathBuf,
    device: u64,
    filesystem: u64,
    budget: DiskBudget,
    initial_available_bytes: u64,
    minimum_available_bytes: u64,
    last_available_bytes: u64,
    observed_peak_growth_bytes: u64,
}

fn existing(path: &Path) -> Result<PathBuf> {
    let mut path = path.to_owned();
    while !path.try_exists()? {
        if !path.pop() {
            return Err("build filesystem ancestor missing".into());
        }
    }
    Ok(path.canonicalize()?)
}

fn available(path: &Path) -> Result<(u64, u64, u64)> {
    let path = existing(path)?;
    let stat = rustix::fs::statvfs(&path)?;
    Ok((
        fs::metadata(path)?.dev(),
        stat.f_fsid,
        stat.f_bavail
            .checked_mul(stat.f_frsize)
            .ok_or("build filesystem space overflow")?,
    ))
}

pub fn available_memory() -> Result<u64> {
    let text = fs::read_to_string("/proc/meminfo")?;
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))
        .ok_or("kernel does not expose available memory")?;
    let mut fields = value.split_whitespace();
    let amount: u64 = fields.next().ok_or("invalid available memory")?.parse()?;
    if fields.next() != Some("kB") || fields.next().is_some() {
        return Err("unexpected memory unit".into());
    }
    Ok(amount
        .checked_mul(1024)
        .ok_or("available memory overflow")?)
}

fn parent_headroom(parent: &Path, requested: u64, minimum: u64) -> Result<()> {
    let required = requested
        .checked_add(minimum)
        .ok_or("memory budget overflow")?;
    let available = available_memory()?;
    if available < required {
        return Err(format!(
            "insufficient memory for this build plus normal runtime margin; boundary=host available_bytes={available} requested_bytes={requested} normal_reserve_bytes={minimum} required_bytes={required}"
        )
        .into());
    }
    // Include visible ancestor limits: host MemAvailable alone can overstate
    // the memory actually available inside a runner/service/container.
    let mut parent = Some(parent);
    while let Some(path) = parent {
        let pair = if path.join("memory.max").is_file() {
            Some(("memory.max", "memory.current"))
        } else if path.join("memory.limit_in_bytes").is_file() {
            Some(("memory.limit_in_bytes", "memory.usage_in_bytes"))
        } else {
            None
        };
        if let Some((limit, usage)) = pair {
            let limit = fs::read_to_string(path.join(limit))?;
            if limit.trim() != "max" {
                let limit: u64 = limit.trim().parse()?;
                let usage: u64 = fs::read_to_string(path.join(usage))?.trim().parse()?;
                let stat = fs::read_to_string(path.join("memory.stat"))?;
                let reclaimable = stat
                    .lines()
                    .filter_map(|line| line.split_once(' '))
                    .filter(|(key, _)| matches!(*key, "inactive_file" | "total_inactive_file"))
                    .map(|(_, value)| value.parse::<u64>())
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .max()
                    .unwrap_or(0)
                    .min(usage);
                if limit.saturating_sub(usage.saturating_sub(reclaimable)) < required {
                    return Err(format!(
                        "ancestor memory cgroup has insufficient headroom for this build; boundary=ancestor path={} limit_bytes={limit} usage_bytes={usage} reclaimable_bytes={reclaimable} requested_bytes={requested} normal_reserve_bytes={minimum} required_bytes={required}",
                        path.display()
                    )
                    .into());
                }
            }
        } else {
            break;
        }
        parent = path.parent();
    }
    Ok(())
}

pub struct Monitor {
    pub group: Group,
    policy: Policy,
    file: File,
    disks: Vec<Disk>,
    started: Instant,
    last_sample: Instant,
    last_write: Instant,
    memory: Memory,
    minimum_available_memory: u64,
    samples: u64,
    context: serde_json::Value,
}

impl Monitor {
    pub fn start(
        policy: &Policy,
        writes: &[&Path],
        journal: &Path,
        context: serde_json::Value,
    ) -> Result<Self> {
        policy.validate()?;
        let mut disks: BTreeMap<(u64, u64), Disk> = BTreeMap::new();
        for (path, budget) in &policy.filesystems {
            let (device, filesystem, free) = available(path)?;
            if free
                < budget
                    .maximum_growth_bytes
                    .checked_add(budget.minimum_free_bytes)
                    .ok_or("space budget overflow")?
            {
                return Err(
                    "insufficient filesystem capacity for this operation and normal runtime margin"
                        .into(),
                );
            }
            if let Some(previous) = disks.get(&(device, filesystem)) {
                if previous.budget.maximum_growth_bytes != budget.maximum_growth_bytes
                    || previous.budget.minimum_free_bytes != budget.minimum_free_bytes
                {
                    return Err("conflicting budgets for the same native build filesystem".into());
                }
            } else {
                disks.insert(
                    (device, filesystem),
                    Disk {
                        path: existing(path)?,
                        device,
                        filesystem,
                        budget: budget.clone(),
                        initial_available_bytes: free,
                        minimum_available_bytes: free,
                        last_available_bytes: free,
                        observed_peak_growth_bytes: 0,
                    },
                );
            }
        }
        for path in writes.iter().copied().chain(std::iter::once(journal)) {
            let (device, filesystem, _) = available(path)?;
            if !disks.contains_key(&(device, filesystem)) {
                return Err("a native build writable filesystem has no resource budget".into());
            }
        }
        parent_headroom(
            &policy.memory_cgroup_parent,
            policy.maximum_memory_bytes,
            policy.minimum_available_memory_bytes,
        )?;
        let group = Group::create(&policy.memory_cgroup_parent, policy.maximum_memory_bytes)?;
        let memory = group.memory()?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(journal)?;
        let now = Instant::now();
        let mut monitor = Self {
            group,
            policy: policy.clone(),
            file,
            disks: disks.into_values().collect(),
            started: now,
            last_sample: now - Duration::from_secs(1),
            last_write: now - Duration::from_secs(1),
            memory,
            minimum_available_memory: available_memory()?,
            samples: 0,
            context,
        };
        monitor.write("admitted", None)?;
        Ok(monitor)
    }

    fn sample(&mut self) -> Result<()> {
        self.samples = self
            .samples
            .checked_add(1)
            .ok_or("resource sample overflow")?;
        self.group.check(self.policy.maximum_memory_bytes)?;
        self.memory = self.group.memory()?;
        parent_headroom(
            &self.policy.memory_cgroup_parent,
            self.policy
                .maximum_memory_bytes
                .saturating_sub(self.memory.current_bytes),
            self.policy.minimum_available_memory_bytes,
        )?;
        self.minimum_available_memory = self.minimum_available_memory.min(available_memory()?);
        if self.minimum_available_memory < self.policy.minimum_available_memory_bytes {
            return Err("normal runtime memory margin exhausted during native build".into());
        }
        if self.memory.limit_events != 0 {
            return Err(
                "private native build memory limit reached; normal runtime retained".into(),
            );
        }
        for disk in &mut self.disks {
            let (device, filesystem, free) = available(&disk.path)?;
            if device != disk.device || filesystem != disk.filesystem {
                return Err("native build filesystem identity changed".into());
            }
            disk.last_available_bytes = free;
            disk.minimum_available_bytes = disk.minimum_available_bytes.min(free);
            disk.observed_peak_growth_bytes = disk
                .initial_available_bytes
                .saturating_sub(disk.minimum_available_bytes);
            if free < disk.budget.minimum_free_bytes
                || disk.observed_peak_growth_bytes > disk.budget.maximum_growth_bytes
            {
                return Err(
                    "native build filesystem growth or normal runtime margin exhausted".into(),
                );
            }
        }
        Ok(())
    }

    pub fn observe(&mut self) -> Result<()> {
        if self.last_sample.elapsed() < Duration::from_millis(100) {
            return Ok(());
        }
        self.last_sample = Instant::now();
        let result = self.sample();
        if result.is_err() || self.last_write.elapsed() >= Duration::from_secs(1) {
            self.write(
                if result.is_ok() {
                    "running"
                } else {
                    "resource_limit"
                },
                result.as_ref().err().map(|e| e.to_string()),
            )?;
        }
        result
    }

    pub fn finish(&mut self, success: bool, reason: Option<String>) -> Result<()> {
        // Kernel peak remains available after all compiler processes exit.
        let sample = self.sample();
        self.group.stop()?;
        self.memory = self.group.memory()?;
        let reason = reason.or_else(|| sample.as_ref().err().map(|e| e.to_string()));
        self.write(
            if success && sample.is_ok() {
                "success"
            } else {
                "failed"
            },
            reason,
        )?;
        sample
    }

    fn write(&mut self, state: &str, reason: Option<String>) -> Result<()> {
        let status = fs::read_to_string("/proc/self/status")?;
        let frontend_peak_rss = status
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .and_then(|value| value.split_whitespace().next())
            .ok_or("frontend peak RSS missing")?
            .parse::<u64>()?
            .checked_mul(1024)
            .ok_or("frontend RSS overflow")?;
        let value = serde_json::json!({"schema":1,"state":state,"reason":reason,"elapsed_ms":self.started.elapsed().as_millis(),
            "state_scope":"supervised worker/resource completion; final source/seal verification and pending publication are separate",
            "policy":self.policy,"memory_group":self.group.path,"memory":self.memory,"samples":self.samples,
            "minimum_available_system_memory_bytes":self.minimum_available_memory,"filesystems":self.disks,"context":self.context,
            "frontend_kernel_peak_rss_bytes":frontend_peak_rss,
            "memory_scope":"kernel private cgroup charge peak after worker entry, including descendants/file cache; not single-process RSS",
            "disk_scope":"observed filesystem free-space decrease including concurrent writers; not a hard filesystem quota",
            "execution_mode":std::env::var("OPENPILOT_CI_EXECUTION_MODE").unwrap_or_else(|_|"native".into()),
            "host_architecture":std::env::var("OPENPILOT_CI_HOST_ARCH").ok(),"process_architecture":std::env::consts::ARCH});
        serde_json::to_writer(&mut self.file, &value)?;
        self.file.write_all(b"\n")?;
        self.file.sync_data()?;
        self.last_write = Instant::now();
        Ok(())
    }
}

pub const ATTEMPT: &str = "OPENPILOT_NATIVE_BUILD_ATTEMPT";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    schema: u32,
    configuration_sha256: String,
    generation_id: String,
    source: crate::Source,
}

/// The worker records only a fully sealed candidate. The frontend publishes the
/// pending update only after worker exit, resource checks and descendant cleanup.
pub fn publish_candidate(
    config: &generation::Config,
    config_path: &Path,
    root: &Path,
    id: &str,
    configuration_sha256: &str,
) -> Result<()> {
    generation::Config::verify_pin(config_path, configuration_sha256)?;
    let attempt = PathBuf::from(std::env::var_os(ATTEMPT).ok_or("native build attempt missing")?);
    if attempt.canonicalize()? != attempt
        || attempt.parent() != Some(config.stores.generations.join("build-attempts").as_path())
        || !attempt
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.starts_with("attempt-"))
    {
        return Err("native worker attempt is not owned by this configuration".into());
    }
    generation::write(
        &attempt.join("candidate.json"),
        &Candidate {
            schema: 1,
            configuration_sha256: configuration_sha256.into(),
            generation_id: id.into(),
            source: crate::source::inspect(root)?,
        },
        false,
    )
}

pub fn candidate(
    config: &generation::Config,
    config_path: &Path,
    root: &Path,
    attempt: &Path,
    configuration_sha256: &str,
    requested_source: &crate::Source,
) -> Result<String> {
    generation::Config::verify_pin(config_path, configuration_sha256)?;
    let result: Candidate = generation::read(&attempt.join("candidate.json"))?;
    if result.schema != 1
        || result.configuration_sha256 != configuration_sha256
        || result.source != *requested_source
        || result.source != crate::source::inspect(root)?
        || generation::load(&config.stores, &result.generation_id)?
            .runtime
            .source
            != result.source
    {
        return Err(
            "native build candidate differs from its source/configuration after worker completion"
                .into(),
        );
    }
    generation::Config::verify_pin(config_path, configuration_sha256)?;
    if crate::source::inspect(root)? != *requested_source {
        return Err("native requested source changed during candidate verification".into());
    }
    Ok(result.generation_id)
}

/// Create only an owned attempt directory. This ledger survives cancellation or
/// a missing final line, and never replaces an active runtime or its logs.
pub fn attempt(
    config: &generation::Config,
    config_path: &Path,
    root: &Path,
    configuration_sha256: &str,
) -> Result<(Monitor, PathBuf, crate::Source)> {
    generation::Config::verify_pin(config_path, configuration_sha256)?;
    let requested_source = crate::source::inspect(root)?;
    generation::offroad(&config.stores.params)?;
    generation::directory(&config.stores.generations)?;
    let attempts = config.stores.generations.join("build-attempts");
    generation::directory(&attempts)?;
    let directory = tempfile::Builder::new()
        .prefix("attempt-")
        .tempdir_in(&attempts)?
        .keep();
    let temporary = std::env::temp_dir();
    let writes = [
        config.stores.sources.as_path(),
        config.stores.objects.as_path(),
        config.stores.generations.as_path(),
        config.cache.as_path(),
        temporary.as_path(),
    ];
    let monitor = Monitor::start(
        &config.resources,
        &writes,
        &directory.join("resources.jsonl"),
        serde_json::json!({"configuration_sha256":configuration_sha256,"requested_source":requested_source}),
    )?;
    Ok((monitor, directory, requested_source))
}
