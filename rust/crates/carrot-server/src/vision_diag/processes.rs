use super::{text, Config};
use crate::Value;
use std::{collections::BTreeMap, path::Path};

const SOURCE_MATCHES: [(&str, &str); 5] = [
    ("camerad", "openpilot/system/camerad/camerad"),
    (
        "stream_encoderd",
        "openpilot/system/loggerd/encoderd\0--carrot-vision-road",
    ),
    ("encoderd", "openpilot/system/loggerd/encoderd"),
    ("webrtcd", "openpilot.system.webrtc.carrot_webrtcd"),
    ("carrot_web", "openpilot.selfdrive.carrot.server"),
];
fn snapshot(path: &Path, pid: u32, cmdline: &str) -> Value {
    let status = text::lines(&text::read(&path.join("status"), 32_000))
        .into_iter()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            [
                "Name", "State", "Pid", "PPid", "Tgid", "Threads", "VmRSS", "VmSize",
            ]
            .contains(&key)
            .then(|| {
                (
                    key.chars().map(u32::from).collect(),
                    Value::text(value.trim_matches(crate::tools::text::whitespace)),
                )
            })
        })
        .collect();
    let raw_stat = text::read(&path.join("stat"), 8_000);
    let stat: Vec<_> = raw_stat
        .split(crate::tools::text::whitespace)
        .filter(|word| !word.is_empty())
        .collect();
    let ticks = |index, bound| {
        if stat.len() > bound {
            Value::text(stat[index])
        } else {
            Value::Null
        }
    };
    let descriptors = std::fs::read_dir(path.join("fd"))
        .ok()
        .map(|items| items.count());
    Value::object([
        ("pid", Value::integer(pid)),
        (
            "cmdline",
            Value::text(&text::trim(
                cmdline.trim_matches(crate::tools::text::whitespace),
                800,
            )),
        ),
        ("status", Value::Object(status)),
        ("cpu_user_ticks", ticks(13, 14)),
        ("cpu_system_ticks", ticks(14, 15)),
        ("fd_count", descriptors.map_or(Value::Null, Value::integer)),
    ])
}

pub(super) fn collect(config: &Config) -> Value {
    let mut groups: BTreeMap<_, Vec<Value>> = config
        .process_matches
        .iter()
        .map(|(name, _)| (*name, Vec::new()))
        .collect();
    if let Ok(entries) = std::fs::read_dir(&config.proc_root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .filter(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let cmdline = std::fs::read(entry.path().join("cmdline"))
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            if cmdline.is_empty() {
                continue;
            }
            for (name, pattern) in &config.process_matches {
                if cmdline.contains(pattern)
                    || SOURCE_MATCHES
                        .iter()
                        .any(|(logical, original)| logical == name && cmdline.contains(original))
                {
                    if let Some(group) = groups.get_mut(name) {
                        group.push(snapshot(&entry.path(), pid, &cmdline.replace('\0', " ")));
                    }
                }
            }
        }
    }
    Value::Object(
        groups
            .into_iter()
            .map(|(name, rows)| (name.chars().map(u32::from).collect(), Value::Array(rows)))
            .collect(),
    )
}
