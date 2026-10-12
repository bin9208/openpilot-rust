use num_traits::ToPrimitive;
use std::{collections::BTreeMap, fs};

pub struct RuntimeUsage {
    process: f64,
    system: Option<(i128, i128)>,
}
impl Default for RuntimeUsage {
    fn default() -> Self {
        Self {
            process: process_time(),
            system: cpu_stat(),
        }
    }
}
impl RuntimeUsage {
    pub fn summary(&mut self, elapsed: f64) -> String {
        let process = process_time();
        let cpu = (process - self.process).max(0.0) / elapsed * 100.0;
        self.process = process;
        let mut parts = vec![format!("proc_cpu={cpu:5.1}% one_core")];
        let system = cpu_stat();
        if let (Some((last_total, last_idle)), Some((total, idle))) = (self.system, system) {
            let delta = total - last_total;
            if delta > 0 {
                if let (Some(active), Some(delta)) =
                    ((delta - idle + last_idle).to_f64(), delta.to_f64())
                {
                    let cpu = (active / delta * 100.0).clamp(0.0, 100.0);
                    parts.push(format!("sys_cpu={cpu:5.1}% all_cores"));
                }
            }
        }
        self.system = system;
        let status = status();
        if let Some(rss) = status
            .get("VmRSS")
            .and_then(|text| text.split_whitespace().next())
            .and_then(|text| text.parse::<i64>().ok())
            .and_then(|value| value.to_f64())
        {
            parts.push(format!("rss={:.1}MB", rss / 1024.0));
        }
        if let Some(threads) = status.get("Threads") {
            parts.push(format!("threads={threads}"));
        }
        if let (Some(voluntary), Some(nonvoluntary)) = (
            status.get("voluntary_ctxt_switches"),
            status.get("nonvoluntary_ctxt_switches"),
        ) {
            parts.push(format!("ctx={voluntary}/{nonvoluntary}"));
        }
        if let Some(load) = fs::read_to_string("/proc/loadavg").ok().and_then(|text| {
            text.split_whitespace()
                .next()
                .and_then(|text| text.parse::<f64>().ok())
        }) {
            parts.push(format!("load1={load:.2}"));
        }
        parts.join(" ")
    }
}
fn process_time() -> f64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    time.tv_sec.to_f64().unwrap_or(0.0) + time.tv_nsec.to_f64().unwrap_or(0.0) / 1e9
}
fn cpu_stat() -> Option<(i128, i128)> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    let mut fields = text.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values = fields
        .map(str::parse::<i128>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    let total = values
        .iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(*value))?;
    Some((
        total,
        values[3].checked_add(values.get(4).copied().unwrap_or(0))?,
    ))
}
fn status() -> BTreeMap<String, String> {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| {
            matches!(
                *key,
                "VmRSS" | "Threads" | "voluntary_ctxt_switches" | "nonvoluntary_ctxt_switches"
            )
        })
        .map(|(key, value)| (key.into(), value.trim().into()))
        .collect()
}
