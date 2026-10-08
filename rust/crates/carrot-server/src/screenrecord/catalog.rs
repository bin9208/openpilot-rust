//! Source: features/screenrecord/catalog.py and dashcam/paths.py time/cache helpers.
use crate::Value;
use chrono::{Datelike, Timelike};
use num_traits::ToPrimitive;
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const DIRECTORIES: [&str; 7] = [
    "/data/media/0/videos",
    "/data/media/0/screenrecord",
    "/data/media/0/screen_recordings",
    "/data/media/0/screenrecords",
    "/data/media/0/ScreenRecords",
    "/data/media/0/Movies",
    "/sdcard/Movies",
];
const EXTENSIONS: [&str; 6] = [".mp4", ".mkv", ".avi", ".mov", ".ts", ".hevc"];

pub fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                result.push(component.as_os_str())
            }
        }
    }
    Ok(result)
}

pub(crate) fn text(value: &OsStr) -> Value {
    let mut bytes = value.as_bytes();
    let mut points = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(value) => {
                points.extend(value.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                points.extend(String::from_utf8_lossy(valid).chars().map(u32::from));
                let count = error.error_len().unwrap_or(rest.len());
                points.extend(rest[..count].iter().map(|byte| 0xdc00 + u32::from(*byte)));
                bytes = &rest[count..];
            }
        }
    }
    Value::Text(points)
}

pub(crate) fn path_text(value: &Value) -> Option<OsString> {
    let Value::Text(points) = value else {
        return None;
    };
    let mut bytes = Vec::new();
    for point in points {
        if (0xdc80..=0xdcff).contains(point) {
            bytes.push(u8::try_from(point - 0xdc00).ok()?);
        } else {
            bytes.extend_from_slice(char::from_u32(*point)?.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Some(OsString::from_vec(bytes))
}

pub fn token(value: &Value) -> String {
    let Value::Text(points) = value else {
        return String::new();
    };
    let utf8: String = points
        .iter()
        .filter_map(|point| char::from_u32(*point))
        .collect();
    format!("{:x}", Sha1::digest(utf8.as_bytes()))[..24].to_owned()
}

pub fn file_id(path: &Path) -> std::io::Result<String> {
    Ok(token(&text(absolute(path)?.as_os_str())))
}

pub fn date_label(epoch: i64) -> String {
    let Some(date) = chrono::DateTime::from_timestamp(epoch, 0) else {
        return "-".into();
    };
    let date = date.with_timezone(&chrono::Local);
    if !(1..=9999).contains(&date.year()) {
        return "-".into();
    }
    // datetime.fromtimestamp also constructs local time one day earlier for its fold probe.
    let probe = epoch
        .checked_sub(86400)
        .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0));
    if !probe.is_some_and(|probe| (1..=9999).contains(&probe.with_timezone(&chrono::Local).year()))
    {
        return "-".into();
    }
    format!(
        "{}-{:02}-{:02} {:02}:{:02}",
        date.year(),
        date.month(),
        date.day(),
        date.hour(),
        date.minute()
    )
}

pub fn relative_time(epoch: i64, now: i64) -> String {
    if epoch <= 0 {
        return "-".into();
    }
    let delta = now.saturating_sub(epoch).max(0);
    if delta < 60 {
        "방금 전".into()
    } else if delta < 3600 {
        format!("{}분 전", delta / 60)
    } else if delta < 86400 {
        format!("{}시간 전", delta / 3600)
    } else {
        format!("{}일 전", delta / 86400)
    }
}

pub fn build_videos(directories: &[PathBuf], now: i64) -> Vec<Value> {
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    for directory in directories {
        if !directory.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if !metadata.is_file() || metadata.len() == 0 {
                continue;
            }
            let name = text(&entry.file_name());
            let lowercase = String::from_utf8_lossy(entry.file_name().as_bytes()).to_lowercase();
            if !EXTENSIONS
                .iter()
                .any(|extension| lowercase.ends_with(extension))
            {
                continue;
            }
            let Ok(path) = absolute(&entry.path()) else {
                continue;
            };
            let Ok(real) = fs::canonicalize(&path) else {
                continue;
            };
            if !seen.insert(real) {
                continue;
            }
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let seconds = match modified.duration_since(UNIX_EPOCH) {
                Ok(value) => value.as_secs_f64(),
                Err(error) => -error.duration().as_secs_f64(),
            };
            let Ok(modified) = Value::Float(seconds).int() else {
                continue;
            };
            let Some(modified) = modified.to_i64() else {
                continue;
            };
            let extension = entry
                .path()
                .extension()
                .map(|value| String::from_utf8_lossy(value.as_bytes()).to_lowercase())
                .unwrap_or_default();
            videos.push(Value::object([
                ("id", Value::text(&token(&text(path.as_os_str())))),
                ("name", name),
                ("folder", text(directory.as_os_str())),
                ("size", Value::integer(metadata.len())),
                ("modifiedEpoch", Value::integer(modified)),
                ("modifiedLabel", Value::text(&date_label(modified))),
                (
                    "relativeModifiedLabel",
                    Value::text(&relative_time(modified, now)),
                ),
                ("ext", Value::text(&extension)),
            ]));
        }
    }
    videos.sort_by(|left, right| {
        let first = right
            .get("modifiedEpoch")
            .int()
            .unwrap_or_default()
            .cmp(&left.get("modifiedEpoch").int().unwrap_or_default());
        if first.is_eq() {
            match (left.get("name"), right.get("name")) {
                (Value::Text(left), Value::Text(right)) => right.cmp(left),
                _ => std::cmp::Ordering::Equal,
            }
        } else {
            first
        }
    });
    videos
}
