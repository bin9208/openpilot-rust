//! Source: features/dashcam/report.py; full log decompression and route aggregation.
mod aggregate;
pub mod codec;
mod excursions;
mod finish;
pub mod format;
mod state;

use crate::{
    dashcam::{catalog::Catalog, paths, Failure},
    Error, Value,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn pick(directory: &Path, prefer_rlog: bool) -> Result<Option<(PathBuf, &'static str)>, Failure> {
    let rlog = ["rlog.zst", "rlog.bz2", "rlog"];
    let qlog = ["qlog.zst", "qlog.bz2", "qlog"];
    let groups = if prefer_rlog {
        [rlog, qlog]
    } else {
        [qlog, rlog]
    };
    for name in groups.into_iter().flatten() {
        let path = directory.join(name);
        if fs::metadata(&path).is_ok_and(|stat| stat.is_file())
            && fs::metadata(&path)
                .map_err(|error| crate::state::io_error(error, &path))?
                .len()
                > 0
        {
            return Ok(Some((
                path,
                if name.starts_with("rlog") {
                    "rlog"
                } else {
                    "qlog"
                },
            )));
        }
    }
    Ok(None)
}
pub fn build(root: &Path, route: &Value, prefer_rlog: bool) -> Result<Value, Failure> {
    let points = paths::points(route)?;
    let parts = paths::parts(&points);
    let route = if parts.len() > 2
        && parts.last().is_some_and(|last| {
            !last.is_empty() && last.iter().all(|c| crate::param_qr::is_python_digit(*c))
        }) {
        paths::route_name(route)?
    } else {
        route.clone()
    };
    let entries = Catalog::new(root.to_owned()).build_routes()?;
    let Some(entry) = entries
        .into_iter()
        .find(|entry| entry.get("route") == &route)
    else {
        return Ok(Value::object([
            ("ok", Value::Bool(false)),
            ("error", Value::text("route not found")),
        ]));
    };
    let Value::Array(mut segments) = entry.get("segmentFolders").clone() else {
        return Err(Error::Source("dashcam catalogue segment list missing".into()).into());
    };
    segments.sort_by(|first, second| {
        paths::segment_index(first)
            .cmp(&paths::segment_index(second))
            .then_with(|| match (first, second) {
                (Value::Text(first), Value::Text(second)) => first.cmp(second),
                _ => std::cmp::Ordering::Equal,
            })
    });
    let mut state = state::State::new();
    for segment in &segments {
        let Some((path, kind)) = pick(&root.join(paths::value_path(segment)?), prefer_rlog)? else {
            continue;
        };
        if state.source.is_empty() {
            state.source = kind;
        }
        if let Ok(data) = codec::decompress(&path) {
            if state.segment(&data).is_err() {
                continue;
            }
        }
    }
    let title = if entry.get("title").truth() {
        entry.get("title").clone()
    } else {
        route.clone()
    };
    state
        .finish(route, title, segments.len())
        .map_err(Failure::from)
}
