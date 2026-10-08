use super::{paths, Failure};
use crate::Value;
use num_traits::ToPrimitive;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const RLOG_NAMES: [&str; 3] = ["rlog.zst", "rlog.bz2", "rlog"];
const QLOG_NAMES: [&str; 3] = ["qlog.zst", "qlog.bz2", "qlog"];
const VIDEO_NAMES: [&str; 2] = ["qcamera.mp4", "qcamera.ts"];
const RECORDED_VIDEO_NAMES: [&str; 2] = ["qcamera.ts", "qcamera.mp4"];
fn positive_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}
fn modified(path: &Path) -> std::io::Result<i64> {
    let modified = fs::symlink_metadata(path)?.modified()?;
    let number = match modified.duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    Value::Float(number)
        .int()
        .ok()
        .and_then(|epoch| epoch.to_i64())
        .ok_or_else(|| std::io::Error::other("recording epoch out of range"))
}
pub fn source_video_end_epoch(directory: &Path) -> i64 {
    for name in RECORDED_VIDEO_NAMES {
        let path = directory.join(name);
        if positive_file(&path) {
            if let Ok(epoch) = modified(&path) {
                return epoch;
            }
        }
    }
    0
}
fn source(directory: &Path, names: &[&str], message: &str) -> Result<(PathBuf, String), Failure> {
    for name in names {
        let path = directory.join(name);
        if positive_file(&path) {
            return Ok((path, (*name).into()));
        }
    }
    Err(Failure::http(404, message))
}
pub fn source_video(directory: &Path) -> Result<(PathBuf, String), Failure> {
    source(directory, &VIDEO_NAMES, "qcamera video not found")
}
pub fn source_rlog(directory: &Path) -> Result<(PathBuf, String), Failure> {
    source(directory, &RLOG_NAMES, "rlog not found")
}
pub fn source_qlog(directory: &Path) -> Result<(PathBuf, String), Failure> {
    source(directory, &QLOG_NAMES, "qlog not found")
}
pub fn segment_file_summary(directory: &Path) -> Result<Vec<Value>, Failure> {
    let mut output = Vec::new();
    let mut has_rlog = false;
    for (kind, names) in [
        ("qcamera", RECORDED_VIDEO_NAMES.as_slice()),
        ("rlog", RLOG_NAMES.as_slice()),
    ] {
        for name in names {
            let Ok(metadata) = fs::metadata(directory.join(name)) else {
                continue;
            };
            if !metadata.is_file() || metadata.len() == 0 {
                continue;
            }
            output.push(Value::object([
                ("kind", Value::text(kind)),
                ("name", Value::text(name)),
                ("size", Value::integer(metadata.len())),
                (
                    "sizeLabel",
                    Value::text(&paths::file_size_label(&Value::integer(metadata.len()))?),
                ),
            ]));
            if kind == "rlog" {
                has_rlog = true;
            }
            break;
        }
    }
    if !has_rlog {
        return Err(Failure::http(404, "rlog not found"));
    }
    Ok(output)
}
pub(super) fn segment_is_complete(root: &Path, segment: &Value) -> Result<bool, Failure> {
    let raw = Value::Text(paths::points(segment)?);
    let path = root.join(paths::value_path(&raw)?);
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(false);
    };
    let mut has_rlog = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.as_encoded_bytes().ends_with(b".lock") {
            return Ok(false);
        }
        if !RLOG_NAMES.iter().any(|candidate| name == *candidate) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            has_rlog = false;
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        has_rlog = metadata.len() > 0;
    }
    Ok(has_rlog)
}
