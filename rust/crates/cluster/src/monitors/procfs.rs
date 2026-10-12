use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ThreadCpuSample {
    pub key: (i64, i64, i64),
    pub pid: i64,
    pub process_name: String,
    pub cpu_time_ticks: i64,
    pub processor: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProcessScan {
    pub samples: Vec<ThreadCpuSample>,
    pub candidate_processes: usize,
    pub matched_processes: usize,
}

fn read_lossy(path: &Path) -> Option<String> {
    Some(ignore_utf8(&fs::read(path).ok()?))
}

fn ignore_utf8(mut bytes: &[u8]) -> String {
    let mut output = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                output.push_str(text);
                break;
            }
            Err(error) => {
                if let Ok(text) = std::str::from_utf8(&bytes[..error.valid_up_to()]) {
                    output.push_str(text);
                }
                let Some(length) = error.error_len() else {
                    break;
                };
                bytes = &bytes[error.valid_up_to() + length..];
            }
        }
    }
    output
}

#[must_use]
pub fn read_thread_stat(pid: i64, tid: i64, name: &str, path: &Path) -> Option<ThreadCpuSample> {
    let text = read_lossy(path)?;
    let close = text.rfind(')')?;
    let fields: Vec<_> = text.get(close + 2..)?.split_whitespace().collect();
    if fields.len() <= 36 {
        return None;
    }
    let user = fields[11].parse::<i64>().ok()?;
    let system = fields[12].parse::<i64>().ok()?;
    Some(ThreadCpuSample {
        key: (pid, tid, fields[19].parse().ok()?),
        pid,
        process_name: name.into(),
        cpu_time_ticks: user.checked_add(system)?,
        processor: fields[36].parse().ok()?,
    })
}

fn process_name(path: &Path) -> String {
    if let Ok(bytes) = fs::read(path.join("cmdline")) {
        for bytes in bytes
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
        {
            let part = ignore_utf8(bytes);
            if part.to_lowercase().contains("cluster") {
                return Path::new(&part)
                    .file_name()
                    .map_or_else(|| part.clone(), |name| name.to_string_lossy().into_owned());
            }
        }
    }
    read_lossy(&path.join("comm"))
        .map(|text| {
            text.trim_matches(openpilot_startup_ui::text::whitespace)
                .to_owned()
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

#[must_use]
pub fn read_cluster_threads(proc_root: &Path, current_pid: i64) -> ProcessScan {
    if !proc_root.exists() {
        return ProcessScan::default();
    }
    let mut pids = vec![current_pid];
    if let Some(text) =
        read_lossy(&proc_root.join(format!("{current_pid}/task/{current_pid}/children")))
    {
        pids.extend(
            text.split_whitespace()
                .filter_map(|value| value.parse::<i64>().ok()),
        );
    }
    let mut result = ProcessScan {
        candidate_processes: pids.len(),
        ..ProcessScan::default()
    };
    for pid in pids {
        let path = proc_root.join(pid.to_string());
        let name = process_name(&path);
        if pid != current_pid && !name.to_lowercase().contains("cluster") {
            continue;
        }
        result.matched_processes += 1;
        let Ok(entries) = fs::read_dir(path.join("task")) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(tid) = entry
                .file_name()
                .to_str()
                .filter(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
                .and_then(|value| value.parse::<i64>().ok())
            else {
                continue;
            };
            if let Some(sample) = read_thread_stat(pid, tid, &name, &entry.path().join("stat")) {
                result.samples.push(sample);
            }
        }
    }
    result
}
