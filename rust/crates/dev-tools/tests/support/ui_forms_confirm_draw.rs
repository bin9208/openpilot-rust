//! Immutable original drawing data, replayed only as a test oracle.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
const CAPTURE: &str = include_str!("../fixtures/ui-forms/confirm-original-draw.json");
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(value: &OsStr) -> String {
    use std::os::unix::ffi::OsStrExt;
    value
        .as_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
pub fn validate(text: &str, case: &Value) -> Result<Value> {
    if hash(text.as_bytes()) != "c6a320a0ff42599b481799f761f446766fbb054602601d5969e52f6cc1f81c5d" {
        return Err("immutable original Confirm draw capture changed".into());
    }
    let trace: Value = serde_json::from_str(text)?;
    let provenance = include_bytes!("../fixtures/ui-forms/input-provenance.json");
    if trace["original_sha256"]
        != "2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365"
        || trace["provenance_sha256"] != hash(provenance)
        || trace["scene"] != case["scene"]
        || trace["state"] != case["states"]
        || trace["png_sha256"] != case["png_sha256"]
    {
        return Err("Confirm capture source, scene, state or provenance binding changed".into());
    }
    Ok(trace)
}
pub struct Expected {
    pub path: PathBuf,
    pub hash: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub fn replay(
    case: &Value,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
    let trace = validate(CAPTURE, case)?;
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT").map(|p| {
                PathBuf::from(p)
                    .join("lib/libopenpilot-raylib.so")
                    .into_os_string()
            })
        })
        .ok_or("Confirm draw replay requires the same raylib library as the reference")?;
    let path = directory.join("original-draw-replay.png");
    let mut commands = Vec::new();
    let mut measurements = Vec::new();
    for captured in trace["commands"]
        .as_array()
        .ok_or("Confirm commands missing")?
    {
        if let Some(item) = captured.as_str() {
            if item == "begin" {
                for measurement in trace["measurements"]
                    .as_array()
                    .ok_or("Confirm measurements missing")?
                {
                    commands.push(format!("font-select {}", measurement["font"]));
                    let text = measurement["text"]
                        .as_str()
                        .ok_or("Confirm measured text missing")?;
                    commands.push(format!(
                        "measure {} {}",
                        hex(OsStr::new(text)),
                        measurement["size"]
                    ));
                    measurements.push((commands.len(), measurement));
                }
            }
            match item {
                "flush" => {} // Export executes this same raw batch flush.
                "@EXPORT@" => commands.push(format!("export {}", hex(path.as_os_str()))),
                _ => commands.push(item.into()),
            }
        } else {
            let name = captured["path"]
                .as_str()
                .ok_or("Confirm font path missing")?;
            if hash(&fs::read(root.join(name))?)
                != captured["sha256"]
                    .as_str()
                    .ok_or("Confirm original font digest missing")?
            {
                return Err("Confirm original font bytes changed".into());
            }
            let mut points = Vec::new();
            for range in captured["point_ranges"]
                .as_array()
                .ok_or("Confirm font ranges missing")?
            {
                let start = range[0].as_u64().ok_or("Confirm range start missing")?;
                let end = range[1].as_u64().ok_or("Confirm range end missing")?;
                if start > end || end > 0x110000 {
                    return Err("Confirm font range invalid".into());
                }
                points.extend(start..end);
            }
            if Some(points.len() as u64) != captured["point_count"].as_u64() {
                return Err("Confirm font point count changed".into());
            }
            commands.push(format!(
                "font-load {} {} {} {} {}",
                captured["font"],
                hex(root.join(name).as_os_str()),
                captured["size"],
                points.len(),
                points
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    commands.push("quit".into());
    let mut spec = CommandSpec::new_os(
        vec![helper.as_os_str().to_owned(), library],
        Duration::from_secs(45),
    );
    spec.input = Some((commands.join("\n") + "\n").into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            spec.os_env
                .get_or_insert_default()
                .insert(key.to_owned(), value.to_owned());
        } else {
            spec.env_remove.push(key.to_owned());
        }
    }
    let result = NativeRunner.run(&spec)?;
    if result.code != 0 {
        return Err(format!(
            "Confirm raw replay failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    let replies = std::str::from_utf8(&result.stdout)?
        .lines()
        .collect::<Vec<_>>();
    if replies.len() != commands.len() + 1
        || replies.iter().enumerate().any(|(index, reply)| {
            if measurements.iter().any(|(at, _)| *at == index) {
                !reply.starts_with("OK ")
            } else {
                *reply != "OK"
            }
        })
    {
        return Err(format!(
            "Confirm raw replay protocol failure: {}",
            String::from_utf8_lossy(&result.stdout)
        )
        .into());
    }
    for (index, measurement) in measurements {
        let values = replies[index]
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if values.len() != 2
            || values.iter().zip(["x", "y"]).any(|(actual, key)| {
                (*actual as f32).to_bits() != (measurement[key].as_f64().unwrap() as f32).to_bits()
            })
        {
            return Err(format!(
                "original Confirm font measurement changed: {measurement}, observed {}",
                replies[index]
            )
            .into());
        }
    }
    Ok(Expected {
        hash: hash(&fs::read(&path)?),
        path,
        stdout: result.stdout,
        stderr: result.stderr,
    })
}
