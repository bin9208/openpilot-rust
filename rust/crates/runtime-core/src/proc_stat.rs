//! Linux /proc/<pid>/stat parsing; CPU counters remain integer until differenced.
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessStat {
    pub pid: u32,
    pub name: String,
    pub state: u8,
    pub ppid: u32,
    pub user_ticks: u64,
    pub system_ticks: u64,
    pub children_user_ticks: i64,
    pub children_system_ticks: i64,
    pub priority: i64,
    pub nice: i64,
    pub threads: u64,
    pub start_ticks: u64,
    pub virtual_bytes: u64,
    pub rss_pages: i64,
    pub processor: u32,
}

impl ProcessStat {
    pub fn parse(line: &str) -> Option<Self> {
        let open = line.find('(')?;
        let close = line.rfind(')')?;
        if close <= open {
            return None;
        }
        let pid = line[..open].trim().parse().ok()?;
        let fields: Vec<&str> = line[close + 1..].split_whitespace().collect();
        // Preserve the source parser's minimum of 52 fields, including pid/comm.
        if fields.len() < 50 || fields[0].len() != 1 {
            return None;
        }
        Some(Self {
            pid,
            name: line[open + 1..close].to_owned(),
            state: fields[0].as_bytes()[0],
            ppid: fields[1].parse().ok()?,
            user_ticks: fields[11].parse().ok()?,
            system_ticks: fields[12].parse().ok()?,
            children_user_ticks: fields[13].parse().ok()?,
            children_system_ticks: fields[14].parse().ok()?,
            priority: fields[15].parse().ok()?,
            nice: fields[16].parse().ok()?,
            threads: fields[17].parse().ok()?,
            start_ticks: fields[19].parse().ok()?,
            virtual_bytes: fields[20].parse().ok()?,
            rss_pages: fields[21].parse().ok()?,
            processor: fields[36].parse().ok()?,
        })
    }
}

/// 100% means one logical CPU, not the whole SoC. Children are not double-counted.
pub fn cpu_percent(
    previous: &ProcessStat,
    current: &ProcessStat,
    elapsed: Duration,
    ticks_per_second: u64,
) -> Option<f64> {
    if elapsed.is_zero()
        || ticks_per_second == 0
        || previous.pid != current.pid
        || previous.start_ticks != current.start_ticks
    {
        return None;
    }
    let user = current.user_ticks.checked_sub(previous.user_ticks)?;
    let system = current.system_ticks.checked_sub(previous.system_ticks)?;
    let ticks = user.checked_add(system)?;
    Some(ticks as f64 / ticks_per_second as f64 / elapsed.as_secs_f64() * 100.0)
}
