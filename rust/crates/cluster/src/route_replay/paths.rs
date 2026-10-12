use super::Error;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

fn decimal(ch: char) -> bool {
    openpilot_ui_application::params::typed::integer_text(&ch.to_string()).is_some()
}

#[must_use]
pub fn segment_index_from_name(name: &str) -> Option<i64> {
    let suffix = name.rsplit_once("--")?.1;
    if suffix.is_empty() || !suffix.chars().all(decimal) {
        return None;
    }
    openpilot_ui_application::params::typed::integer_text(suffix)?
        .parse()
        .ok()
}
#[must_use]
pub fn segment_route_id_from_name(name: &str) -> Option<&str> {
    segment_index_from_name(name)?;
    Some(name.rsplit_once("--")?.0)
}
#[must_use]
pub fn segment_index(path: &Path) -> Option<i64> {
    segment_index_from_name(path.parent()?.file_name()?.to_str()?)
}
#[must_use]
pub fn segment_route_id(path: &Path) -> Option<&str> {
    segment_route_id_from_name(path.parent()?.file_name()?.to_str()?)
}
fn filename(kind: &str) -> Result<&'static str, Error> {
    match kind {
        "rlog" => Ok("rlog.zst"),
        "qlog" => Ok("qlog.zst"),
        _ => Err(Error::Invalid(format!(
            "unsupported route log kind: {kind}"
        ))),
    }
}
fn name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

/// # Errors
/// Returns filesystem errors while resolving existing symbolic links.
pub fn resolve(path: &Path) -> Result<PathBuf, Error> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => (),
            other => {
                result.push(other.as_os_str());
                match fs::canonicalize(&result) {
                    Ok(real) => result = real,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(result)
}
#[must_use]
pub fn route_search_spec(
    path: &Path,
    start: Option<i64>,
) -> (PathBuf, Option<i64>, Option<String>) {
    let segment = segment_index_from_name(name(path));
    let route = segment_route_id_from_name(name(path)).map(str::to_owned);
    if path.exists() && segment.is_none() {
        return (path.to_path_buf(), start, None);
    }
    let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    if segment.is_some() {
        (parent, start.or(segment), route)
    } else {
        (parent, start, Some(name(path).into()))
    }
}
#[must_use]
pub fn route_sort_key(path: &Path) -> (String, i64, String) {
    let parent = path.parent().unwrap_or(Path::new(""));
    let route =
        segment_route_id(path).unwrap_or_else(|| name(parent.parent().unwrap_or(Path::new(""))));
    (
        route.into(),
        segment_index(path).unwrap_or(1_000_000_000),
        name(parent).into(),
    )
}
fn walk(root: &Path, target: &str, files: &mut Vec<PathBuf>) -> Result<(), Error> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == target {
            files.push(entry.path());
        }
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), target, files)?;
        }
    }
    Ok(())
}
/// # Errors
/// Returns an unsupported log kind, missing route path, or directory-read error.
pub fn discover_route_logs(
    path: &Path,
    kind: &str,
    start: Option<i64>,
    maximum: Option<i64>,
) -> Result<Vec<PathBuf>, Error> {
    let target = filename(kind)?;
    let path = resolve(path)?;
    if path.is_file() {
        return Ok(vec![path]);
    }
    let (root, start, route) = route_search_spec(&path, start);
    if !root.exists() {
        return Err(Error::Invalid(format!(
            "route path does not exist: {}",
            path.display()
        )));
    }
    let mut files = Vec::new();
    walk(&root, target, &mut files)?;
    files.sort_by_cached_key(|file| route_sort_key(file));
    files.retain(|file| {
        route
            .as_ref()
            .is_none_or(|route| segment_route_id(file) == Some(route.as_str()))
            && start.is_none_or(|start| segment_index(file).is_some_and(|index| index >= start))
    });
    if let Some(maximum) = maximum {
        let length = i64::try_from(files.len()).unwrap_or(i64::MAX);
        let maximum = if maximum < 0 {
            length.saturating_add(maximum).max(0)
        } else {
            maximum.min(length)
        };
        files.truncate(usize::try_from(maximum).unwrap_or(usize::MAX));
    }
    Ok(files)
}
/// # Errors
/// Returns an invalid direction, unsupported log kind, or path-resolution error.
pub fn adjacent_route_log_path(
    path: &Path,
    direction: i64,
    kind: &str,
) -> Result<Option<PathBuf>, Error> {
    if !matches!(direction, -1 | 1) {
        return Err(Error::Invalid("direction must be -1 or 1".into()));
    }
    let target = filename(kind)?;
    let resolved = resolve(path)?;
    let folder = if path.is_file() {
        resolved.parent().unwrap_or(Path::new("/")).to_path_buf()
    } else {
        resolved
    };
    let text = name(&folder);
    let offset = text.trim_end_matches(decimal).len();
    let (prefix, digits) = text.split_at(offset);
    if digits.is_empty() {
        return Ok(None);
    }
    let Some(index) = openpilot_ui_application::params::typed::integer_text(digits)
        .and_then(|digits| digits.parse::<i64>().ok())
        .and_then(|index| index.checked_add(direction))
        .filter(|index| *index >= 0)
    else {
        return Ok(None);
    };
    let next = folder.parent().unwrap_or(Path::new("/")).join(format!(
        "{prefix}{index:0width$}",
        width = digits.chars().count()
    ));
    for candidate in [next.join(target), next.join(format!("{kind}.bz2"))] {
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
