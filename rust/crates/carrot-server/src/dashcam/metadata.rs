use super::{catalog, paths, Failure, Service};
use crate::{Error, Value};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const QUOTE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');
fn quoted(segment: &Value) -> Result<String, Failure> {
    Ok(utf8_percent_encode(&segment.string().map_err(Error::from)?, QUOTE).to_string())
}
pub(super) fn directory(service: &Service, segment: &Value) -> Result<PathBuf, Failure> {
    paths::Paths::new(service.root.clone(), PathBuf::new()).segment_dir(segment)
}
pub(super) fn compression(name: &str) -> &'static str {
    if name.ends_with(".zst") {
        "zstd"
    } else if name.ends_with(".bz2") {
        "bzip2"
    } else {
        "none"
    }
}
fn size(path: &Path) -> Result<u64, Failure> {
    Ok(fs::metadata(path)
        .map_err(|error| crate::state::io_error(error, path))?
        .len())
}
pub(super) fn replay(service: &Service, segment: &Value) -> Result<Value, Failure> {
    let segment = paths::safe_segment(segment)?;
    let directory = directory(service, &segment)?;
    let (rlog, rlog_name) = catalog::source_rlog(&directory)?;
    let (video, video_name) = catalog::source_video(&directory)?;
    let encoded = quoted(&segment)?;
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("mode", Value::text("client")),
        ("segment", segment.clone()),
        (
            "segmentIndex",
            Value::Integer(paths::segment_index(&segment)),
        ),
        (
            "rlog",
            Value::object([
                ("name", Value::text(&rlog_name)),
                ("size", Value::integer(size(&rlog)?)),
                ("compression", Value::text(compression(&rlog_name))),
                (
                    "url",
                    Value::text(&format!("/api/dashcam/replay-source/{encoded}/rlog")),
                ),
            ]),
        ),
        (
            "video",
            Value::object([
                ("name", Value::text(&video_name)),
                ("size", Value::integer(size(&video)?)),
                (
                    "container",
                    Value::text(if video_name.ends_with(".mp4") {
                        "mp4"
                    } else {
                        "mpegts"
                    }),
                ),
                (
                    "url",
                    Value::text(&format!("/api/dashcam/replay-source/{encoded}/video")),
                ),
            ]),
        ),
    ]))
}
fn segment_source(service: &Service, segment: &Value) -> Result<Option<Value>, Failure> {
    let directory = directory(service, segment)?;
    let (kind, path, name) = match catalog::source_qlog(&directory) {
        Ok((path, name)) => ("qlog", path, name),
        Err(Failure::Http { status: 404, .. }) => match catalog::source_rlog(&directory) {
            Ok((path, name)) => ("rlog", path, name),
            Err(Failure::Http { status: 404, .. }) => return Ok(None),
            Err(error) => return Err(error),
        },
        Err(error) => return Err(error),
    };
    let metadata =
        fs::symlink_metadata(&path).map_err(|error| crate::state::io_error(error, &path))?;
    let modified = i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec());
    let encoded = quoted(segment)?;
    Ok(Some(Value::object([
        ("segment", segment.clone()),
        ("index", Value::Integer(paths::segment_index(segment))),
        ("kind", Value::text(kind)),
        ("name", Value::text(&name)),
        ("size", Value::integer(metadata.len())),
        ("modifiedMs", Value::integer(modified.div_euclid(1_000_000))),
        ("compression", Value::text(compression(&name))),
        (
            "url",
            Value::text(&format!("/api/dashcam/replay-source/{encoded}/{kind}")),
        ),
    ])))
}
pub(super) fn summary(service: &Service, route: &str) -> Result<Option<Value>, Failure> {
    let routes = service.visible_routes()?;
    if route.is_empty() || route.contains(['/', '\\']) || matches!(route, "." | "..") {
        return Ok(None);
    }
    let Some(entry) = routes
        .iter()
        .find(|entry| entry.get("route").text_eq(route))
    else {
        return Ok(None);
    };
    let mut segments = super::pages::segments(entry)?.to_vec();
    segments.sort_by(|first, second| {
        paths::segment_index(first)
            .cmp(&paths::segment_index(second))
            .then_with(|| match (first, second) {
                (Value::Text(first), Value::Text(second)) => first.cmp(second),
                _ => std::cmp::Ordering::Equal,
            })
    });
    let count = segments.len();
    let mut sources = Vec::new();
    let mut skipped = 0usize;
    for segment in segments {
        match segment_source(service, &segment)? {
            Some(source) => sources.push(source),
            None => skipped += 1,
        }
    }
    Ok(Some(Value::object([
        ("ok", Value::Bool(true)),
        ("mode", Value::text("client-worker")),
        ("schemaVersion", Value::integer(2)),
        ("route", Value::text(route)),
        ("segments", Value::Array(sources)),
        ("segmentCount", Value::integer(count)),
        ("skippedSegments", Value::integer(skipped)),
    ])))
}
