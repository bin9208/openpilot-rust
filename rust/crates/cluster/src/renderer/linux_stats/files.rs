use super::super::{Result, SystemStats};
use super::{numeric::ratio, MemoryBytes};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use openpilot_startup_ui::{is_digit, text::whitespace};
use std::{collections::BTreeMap, path::Path};

pub(super) type CpuTimes = Vec<(BigInt, BigInt)>;
fn read(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| bytes.utf8_chunks().map(|chunk| chunk.valid()).collect())
}
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split([
        '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
    ])
}
fn parts(line: &str) -> impl Iterator<Item = &str> {
    line.split(whitespace).filter(|part| !part.is_empty())
}
fn integer(text: &str) -> Option<BigInt> {
    openpilot_carrot_navi::json::Value::text(text).int().ok()
}
fn memory(path: &Path) -> Result<(Option<MemoryBytes>, Option<MemoryBytes>, Option<f64>)> {
    let Some(text) = read(path) else {
        return Ok((None, None, None));
    };
    let values: BTreeMap<_, _> = lines(&text)
        .filter_map(|line| {
            let mut row = parts(line);
            let name = row.next()?.trim_end_matches(':');
            Some((name, integer(row.next()?)? * 1024_u32))
        })
        .collect();
    let total = values.get("MemTotal");
    let available = values.get("MemAvailable").cloned().unwrap_or_else(|| {
        ["MemFree", "Buffers", "Cached"]
            .iter()
            .filter_map(|name| values.get(*name))
            .sum()
    });
    let Some(total) = total.filter(|total| **total > BigInt::zero()) else {
        return Ok((total.cloned().map(MemoryBytes::from_integer), None, None));
    };
    let used = (total - available).max(BigInt::zero()).min(total.clone());
    let percent = ratio(&used, total)? * 100.0;
    Ok((
        Some(MemoryBytes::from_integer(total.clone())),
        Some(MemoryBytes::from_integer(used)),
        Some(percent),
    ))
}
fn cpu(path: &Path) -> Option<CpuTimes> {
    let text = read(path)?;
    Some(
        lines(&text)
            .filter_map(|line| {
                let mut row = parts(line);
                let suffix = row.next()?.strip_prefix("cpu")?;
                if suffix.is_empty() || !suffix.chars().all(is_digit) {
                    return None;
                }
                let fields: Vec<_> = row.map(integer).collect::<Option<_>>()?;
                if fields.len() < 4 {
                    return None;
                }
                let idle = &fields[3] + fields.get(4).cloned().unwrap_or_default();
                Some((fields.into_iter().sum(), idle))
            })
            .collect(),
    )
}
pub(super) fn sample(root: &Path, previous: &mut Option<CpuTimes>) -> Result<SystemStats> {
    let (memory_total_bytes, memory_used_bytes, memory_used_percent) =
        memory(&root.join("meminfo"))?;
    let current = cpu(&root.join("stat"));
    let mut cpu_core_percents = Vec::new();
    if let Some(current) = current {
        let before = previous
            .as_ref()
            .filter(|before| before.len() == current.len());
        for (index, (total, idle)) in current.iter().enumerate() {
            let percent = if let Some(before) = before {
                let delta = total - &before[index].0;
                if delta > BigInt::zero() {
                    let busy = (&delta - (idle - &before[index].1))
                        .max(BigInt::zero())
                        .min(delta.clone());
                    Some(ratio(&busy, &delta)? * 100.0)
                } else {
                    None
                }
            } else {
                None
            };
            cpu_core_percents.push(percent);
        }
        *previous = Some(current);
    }
    let valid: Vec<_> = cpu_core_percents.iter().flatten().copied().collect();
    let cpu_used_percent = if valid.is_empty() {
        None
    } else {
        Some(
            super::average::sum(&valid)
                / valid
                    .len()
                    .to_f64()
                    .ok_or(super::super::Error::Contract("CPU count out of range"))?,
        )
    };
    let disk_used_percent = ["/data", "/"].iter().find_map(|path| {
        let value = rustix::fs::statvfs(*path).ok()?;
        if value.f_blocks == 0 {
            return None;
        }
        Some(
            (value.f_blocks.saturating_sub(value.f_bfree).to_f64()? / value.f_blocks.to_f64()?
                * 100.0)
                .clamp(0.0, 100.0),
        )
    });
    Ok(SystemStats {
        memory_total_bytes,
        memory_used_bytes,
        memory_used_percent,
        cpu_core_percents,
        cpu_used_percent,
        disk_used_percent,
    })
}
