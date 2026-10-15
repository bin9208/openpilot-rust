//! Bounded ISO-BMFF metadata walk from the pinned screenrecord catalog.
use super::catalog;
use crate::Value;
use chrono::{Local, NaiveDate, TimeZone};
use num_traits::ToPrimitive;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

pub fn start_epoch(name: &str) -> Option<i64> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        regex::Regex::new(r"^(\d{8})-(\d{6})").expect("literal recording timestamp")
    });
    let found = pattern.captures(name)?;
    let stamp = format!("{}{}", &found[1], &found[2]);
    static FIELDS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let fields = FIELDS.get_or_init(|| {
        regex::Regex::new(
            r"^(\d{4})(1[0-2]|0[1-9])([1-2]\d|3[0-1]|0[1-9])([0-1]\d|2[0-3])([0-5]\d)([0-5]\d)$",
        )
        .expect("fixed strptime fields")
    });
    let captures = fields.captures(&stamp)?;
    let number = |index: usize| -> Option<u32> {
        captures[index].chars().try_fold(0u32, |value, digit| {
            Some(value * 10 + u32::from(openpilot_dashcam_upload::catalog::decimal_digit(digit)?))
        })
    };
    let value = NaiveDate::from_ymd_opt(i32::try_from(number(1)?).ok()?, number(2)?, number(3)?)?
        .and_hms_opt(number(4)?, number(5)?, number(6)?)?;
    match Local.from_local_datetime(&value) {
        chrono::LocalResult::Single(date) => return Some(date.timestamp()),
        // Chrono's local TZ backend may return offsets in reverse UTC order.
        chrono::LocalResult::Ambiguous(first, second) => {
            return Some(first.timestamp().min(second.timestamp()))
        }
        chrono::LocalResult::None => (),
    }
    // datetime.timestamp(fold=0) uses the offset preceding a skipped local interval.
    for hours in 1..=26 {
        let prior = value.checked_sub_signed(chrono::Duration::hours(hours))?;
        if let Some(date) = Local.from_local_datetime(&prior).earliest() {
            return value
                .and_utc()
                .timestamp()
                .checked_sub(i64::from(date.offset().local_minus_utc()));
        }
    }
    None
}
fn bytes(file: &mut File, count: u64) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    file.take(count).read_to_end(&mut result)?;
    Ok(result)
}
fn integer(file: &mut File, count: u64) -> io::Result<u64> {
    Ok(bytes(file, count)?
        .into_iter()
        .fold(0, |value, byte| (value << 8) | u64::from(byte)))
}
#[derive(Clone)]
struct Atom {
    kind: [u8; 4],
    start: u64,
    end: u64,
}
fn atoms(file: &mut File, start: u64, end: u64) -> io::Result<Vec<Atom>> {
    let mut offset = start;
    let mut result = Vec::new();
    for _ in 0..4096 {
        if offset.saturating_add(8) > end {
            break;
        }
        file.seek(SeekFrom::Start(offset))?;
        let header = bytes(file, 8)?;
        if header.len() != 8 {
            break;
        }
        let mut size = u64::from(u32::from_be_bytes(
            header[..4].try_into().expect("fixed header"),
        ));
        let mut header_size = 8;
        if size == 1 {
            let extended = bytes(file, 8)?;
            if extended.len() != 8 {
                break;
            }
            size = u64::from_be_bytes(extended.try_into().expect("fixed extended header"));
            header_size = 16;
        } else if size == 0 {
            size = end - offset;
        }
        if size < header_size {
            break;
        }
        let next = end.min(offset.saturating_add(size));
        result.push(Atom {
            kind: header[4..8].try_into().expect("fixed type"),
            start: offset.saturating_add(header_size),
            end: next,
        });
        if next <= offset {
            break;
        }
        offset = next;
    }
    Ok(result)
}
fn find(file: &mut File, start: u64, end: u64, path: &[[u8; 4]]) -> io::Result<Option<Atom>> {
    for atom in atoms(file, start, end)? {
        if atom.kind != path[0] {
            continue;
        }
        if path.len() == 1 {
            return Ok(Some(atom));
        }
        if let Some(found) = find(file, atom.start, atom.end, &path[1..])? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}
fn duration(file: &mut File, start: u64) -> io::Result<Option<f64>> {
    file.seek(SeekFrom::Start(start))?;
    let version = bytes(file, 1)?;
    if version.is_empty() {
        return Ok(None);
    }
    bytes(file, 3)?;
    bytes(file, if version[0] == 1 { 16 } else { 8 })?;
    let scale = integer(file, 4)?;
    let duration = integer(file, if version[0] == 1 { 8 } else { 4 })?;
    Ok((scale > 0).then(|| duration.to_f64().unwrap_or(0.0) / scale.to_f64().unwrap_or(1.0)))
}
fn round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}
fn optional(value: Option<f64>, digits: usize) -> Value {
    value
        .filter(|value| *value != 0.0)
        .map_or(Value::Null, |value| Value::Float(round(value, digits)))
}
fn details(path: &Path, size: u64) -> io::Result<Vec<(&'static str, Value)>> {
    let mut file = File::open(path)?;
    let Some(movie) = find(&mut file, 0, size, &[*b"moov"])? else {
        return Ok(vec![]);
    };
    let movie_duration = match find(&mut file, movie.start, movie.end, &[*b"mvhd"])? {
        Some(atom) => duration(&mut file, atom.start)?,
        None => None,
    };
    for track in atoms(&mut file, movie.start, movie.end)? {
        if track.kind != *b"trak" {
            continue;
        }
        let Some(handler) = find(&mut file, track.start, track.end, &[*b"mdia", *b"hdlr"])? else {
            continue;
        };
        file.seek(SeekFrom::Start(handler.start + 8))?;
        if bytes(&mut file, 4)? != b"vide" {
            continue;
        }
        let track_duration = match find(&mut file, track.start, track.end, &[*b"mdia", *b"mdhd"])? {
            Some(atom) => duration(&mut file, atom.start)?,
            None => None,
        };
        let frames = if let Some(atom) = find(
            &mut file,
            track.start,
            track.end,
            &[*b"mdia", *b"minf", *b"stbl", *b"stsz"],
        )? {
            file.seek(SeekFrom::Start(atom.start + 8))?;
            integer(&mut file, 4)?
        } else {
            0
        };
        let (mut width, mut height) = (0, 0);
        if let Some(atom) = find(
            &mut file,
            track.start,
            track.end,
            &[*b"mdia", *b"minf", *b"stbl", *b"stsd"],
        )? {
            file.seek(SeekFrom::Start(atom.start + 40))?;
            let dimensions = bytes(&mut file, 4)?;
            if dimensions.len() == 4 {
                width = u16::from_be_bytes(dimensions[..2].try_into().expect("width bytes"));
                height = u16::from_be_bytes(dimensions[2..].try_into().expect("height bytes"));
            }
        }
        let seconds = track_duration.filter(|v| *v != 0.0).or(movie_duration);
        return Ok(vec![
            ("width", Value::integer(width)),
            ("height", Value::integer(height)),
            ("frameCount", Value::integer(frames)),
            ("durationSeconds", optional(seconds, 3)),
            (
                "fps",
                optional(
                    seconds
                        .filter(|seconds| *seconds != 0.0 && frames > 0)
                        .map(|seconds| frames.to_f64().unwrap_or(0.0) / seconds),
                    2,
                ),
            ),
            ("codec", Value::text("H.264")),
        ]);
    }
    Ok(vec![])
}
pub fn probe(path: &Path) -> io::Result<Value> {
    let metadata = std::fs::metadata(path)?;
    use std::os::unix::fs::MetadataExt;
    let modified = (metadata.mtime().to_f64().unwrap_or(0.0)
        + metadata.mtime_nsec().to_f64().unwrap_or(0.0) / 1e9)
        .to_i64()
        .unwrap_or(0);
    let mut fields = vec![
        ("size", Value::integer(metadata.len())),
        ("modifiedEpoch", Value::integer(modified)),
        ("modifiedLabel", Value::text(&catalog::date_label(modified))),
    ];
    if let Some(start) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(start_epoch)
    {
        fields.extend([
            ("startEpoch", Value::integer(start)),
            ("startLabel", Value::text(&catalog::date_label(start))),
        ]);
    }
    if let Ok(details) = details(path, metadata.len()) {
        fields.extend(details);
    }
    Ok(Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
            .collect(),
    ))
}
