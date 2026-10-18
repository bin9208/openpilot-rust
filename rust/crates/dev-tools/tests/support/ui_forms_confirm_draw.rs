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
/// Exact, iterative fixture binding: no source comparator tolerance or recursion cap.
#[allow(dead_code)] // Not all test-oracle consumers bind a general corpus.
pub fn same_input(
    a: &openpilot_log_format::JsonValue,
    b: &openpilot_log_format::JsonValue,
) -> bool {
    use openpilot_log_format::JsonView;
    let mut pending = vec![(a.clone(), b.clone())];
    while let Some((a, b)) = pending.pop() {
        match (a.view(), b.view()) {
            (JsonView::Null, JsonView::Null) => {}
            (JsonView::Bool(a), JsonView::Bool(b)) if a == b => {}
            (JsonView::Integer(a), JsonView::Integer(b)) if a == b => {}
            (JsonView::Float(a), JsonView::Float(b)) if a.to_bits() == b.to_bits() => {}
            (JsonView::Text(a), JsonView::Text(b)) if a == b => {}
            (JsonView::Array(a), JsonView::Array(b)) if a.len() == b.len() => {
                pending.extend(a.into_iter().zip(b));
            }
            (JsonView::Object(a), JsonView::Object(b)) if a.len() == b.len() => {
                for (key, value) in a {
                    let Some((_, other)) = b.iter().find(|(other, _)| *other == key) else {
                        return false;
                    };
                    pending.push((value, other.clone()));
                }
            }
            _ => return false,
        }
    }
    true
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
const ADMISSION_CAPTURE: &str = include_str!("../fixtures/ui-forms/admission-original-draw.json");
pub fn validate_admission(text: &str, case: &Value) -> Result<Value> {
    if hash(text.as_bytes()) != "ade66cf5a559c9b0d8c55a65e6cf532a7a093a71429cb47d26c1f64c037dc03d" {
        return Err("immutable original admission draw capture changed".into());
    }
    let corpus: Value = serde_json::from_str(text)?;
    if corpus["original_sha256"]
        != "2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365"
        || corpus["provenance_sha256"]
            != hash(include_bytes!("../fixtures/ui-forms/input-provenance.json"))
    {
        return Err("admission capture source or provenance binding changed".into());
    }
    let trace = corpus["captures"]
        .as_array()
        .ok_or("admission captures missing")?
        .iter()
        .find(|trace| trace["name"].as_str().is_some() && trace["name"] == case["name"])
        .ok_or("successful original admission capture missing")?;
    if case["exit"] != 0
        || case["png"] != true
        || trace.get("scene") != Some(case.get("scene").unwrap_or(&Value::Null))
        || trace.get("fixture_scene_text") != Some(case.get("scene_text").unwrap_or(&Value::Null))
        || trace["state"] != case["states"]
        || trace["png_sha256"] != case["png_sha256"]
        || trace["env"]
            != case
                .get("env")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}))
        || hash(
            trace["scene_text"]
                .as_str()
                .ok_or("original scene bytes missing")?
                .as_bytes(),
        ) != trace["scene_sha256"]
            .as_str()
            .ok_or("original scene digest missing")?
    {
        return Err(
            "admission capture scene, environment, state or raster provenance changed".into(),
        );
    }
    Ok(trace.clone())
}
pub struct Expected {
    pub path: PathBuf,
    pub hash: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub capture: &'static str,
    #[allow(dead_code)] // Admission captures do not record encoder frames.
    pub frames: Vec<PathBuf>,
}
pub fn replay(
    case: &Value,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
    replay_trace(
        validate(CAPTURE, case)?,
        CAPTURE,
        root,
        helper,
        command,
        directory,
    )
}
pub fn replay_admission(
    case: &Value,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
    replay_trace(
        validate_admission(ADMISSION_CAPTURE, case)?,
        ADMISSION_CAPTURE,
        root,
        helper,
        command,
        directory,
    )
}
pub fn replay_trace(
    trace: Value,
    capture: &'static str,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
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
    let mut measured = false;
    let mut drawing = false;
    let mut frames = Vec::new();
    for captured in trace["commands"]
        .as_array()
        .ok_or("Confirm commands missing")?
    {
        if let Some(item) = captured.as_str() {
            if matches!(
                item,
                "begin" | "target-begin" | "@EXPORT@" | "@EXPORT-TARGET@"
            ) && !measured
            {
                measured = true;
                for measurement in trace["measurements"]
                    .as_array()
                    .ok_or("Confirm measurements missing")?
                {
                    commands.push(format!("font-select {}", measurement["font"]));
                    let text = measurement["text"]
                        .as_str()
                        .ok_or("Confirm measured text missing")?;
                    let spaced = measurement
                        .get("spacing")
                        .is_some_and(|v| v.as_f64() != Some(0.0));
                    let encoded = hex(OsStr::new(text));
                    commands.push(format!(
                        "{} {} {}{}",
                        if spaced { "measure-spaced" } else { "measure" },
                        if encoded.is_empty() { "-" } else { &encoded },
                        measurement["size"],
                        if spaced {
                            format!(" {}", measurement["spacing"])
                        } else {
                            String::new()
                        }
                    ));
                    measurements.push((commands.len(), measurement));
                }
            }
            match item {
                "flush" => {} // Export executes this same raw batch flush.
                "@RECORD-FRAME@" => {
                    let frame = directory.join(format!("original-frame-{}.rgba", frames.len()));
                    commands.push(format!("target-rgba {}", hex(frame.as_os_str())));
                    frames.push(frame);
                }
                "@EXPORT@" | "@EXPORT-TARGET@" => {
                    if !drawing {
                        commands.push("export-idle".into());
                    }
                    commands.push(format!(
                        "{} {}",
                        if item == "@EXPORT-TARGET@" {
                            "export-target"
                        } else {
                            "export"
                        },
                        hex(path.as_os_str())
                    ));
                }
                _ => commands.push(item.into()),
            }
            match item {
                "begin" => drawing = true,
                "end" => drawing = false,
                _ => {}
            }
        } else {
            if let Some(encoded) = captured.get("rgba").and_then(Value::as_str) {
                if encoded.len() != 128 * 128 * 8
                    || captured["width"] != 128
                    || captured["height"] != 128
                {
                    return Err("original emoji RGBA shape changed".into());
                }
                let bytes = encoded
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|v| Ok(u8::from_str_radix(std::str::from_utf8(v)?, 16)?))
                    .collect::<Result<Vec<_>>>()?;
                if Some(hash(&bytes).as_str()) != captured["sha256"].as_str() {
                    return Err("original emoji RGBA digest changed".into());
                }
                let file = directory.join(format!("original-rgba-{}.bin", captured["texture"]));
                fs::write(&file, bytes)?;
                commands.push(format!(
                    "texture-rgba {} {}",
                    captured["texture"],
                    hex(file.as_os_str())
                ));
                continue;
            }
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
            if let Some(companions) = captured.get("companions").and_then(Value::as_object) {
                for (path, digest) in companions {
                    if Some(hash(&fs::read(root.join(path))?).as_str()) != digest.as_str() {
                        return Err("original font atlas companion changed".into());
                    }
                }
            }
            if captured.get("texture").is_some() {
                commands.push(format!(
                    "texture-load {} {} {} {}",
                    captured["texture"],
                    hex(root.join(name).as_os_str()),
                    captured["width"],
                    captured["height"]
                ));
                commands.push(format!(
                    "texture-size {} {} {}",
                    captured["texture"], captured["logical_width"], captured["logical_height"]
                ));
                continue;
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
                "{} {} {} {} {} {}",
                match (captured.get("mipmaps"), captured["atlas"] == true) {
                    (Some(_), true) => "font-atlas-plain",
                    (Some(_), false) => "font-load-plain",
                    (None, true) => "font-atlas",
                    (None, false) => "font-load",
                },
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
            if captured["mipmaps"] == true {
                commands.push(format!("font-mipmaps {}", captured["font"]));
            }
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
        capture,
        frames,
    })
}
