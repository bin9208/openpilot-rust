use super::Failure;
use crate::{screenrecord::catalog as shared, Error, Value};
use num_bigint::BigInt;
use std::{
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub struct Paths {
    pub root: PathBuf,
    pub cache: PathBuf,
}
impl Paths {
    pub fn new(root: PathBuf, cache: PathBuf) -> Self {
        Self { root, cache }
    }
    pub fn segment_dir(&self, segment: &Value) -> Result<PathBuf, Failure> {
        let segment = safe_segment(segment)?;
        let root = shared::absolute(&self.root).map_err(Error::from)?;
        let path = shared::absolute(
            &root.join(
                shared::path_text(&segment)
                    .ok_or_else(|| Error::Source("invalid filesystem text".into()))?,
            ),
        )
        .map_err(Error::from)?;
        let mut prefix = root.as_os_str().as_bytes().to_vec();
        prefix.push(b'/');
        if !path.as_os_str().as_bytes().starts_with(&prefix) {
            return Err(Failure::http(400, "bad segment path"));
        }
        if !path.is_dir() {
            return Err(Failure::http(404, "segment not found"));
        }
        Ok(path)
    }
    pub fn cache_path(
        &self,
        kind: &str,
        segment: &Value,
        extension: &str,
    ) -> Result<PathBuf, Failure> {
        let directory = self.cache.join(kind);
        fs::create_dir_all(&directory)
            .map_err(|error| crate::state::io_error(error, &directory))?;
        Ok(directory.join(format!("{}{extension}", shared::token(segment))))
    }
}

pub(super) fn points(value: &Value) -> Result<Vec<u32>, Failure> {
    let value = crate::param_changes::text::string(value, true)?;
    match value {
        Value::Text(points) => Ok(points),
        _ => Err(Error::Source("expected Python string".into()).into()),
    }
}
pub(super) fn parts(points: &[u32]) -> Vec<&[u32]> {
    let mut result = Vec::new();
    let mut start = 0;
    for index in 0..points.len().saturating_sub(1) {
        if index >= start && points[index] == 45 && points[index + 1] == 45 {
            result.push(&points[start..index]);
            start = index + 2;
        }
    }
    result.push(&points[start..]);
    result
}
pub fn safe_segment(segment: &Value) -> Result<Value, Failure> {
    let empty = Vec::new();
    let raw = match segment {
        Value::Text(points) => points,
        value if !value.truth() => &empty,
        value => {
            return Err(Error::Source(format!(
                "'{}' object has no attribute 'strip'",
                value.type_name()
            ))
            .into())
        }
    };
    let raw = crate::state::trim(raw);
    let invalid = || Failure::http(400, "bad segment");
    if raw.is_empty() || raw.contains(&47) || raw.contains(&92) || raw == [46] || raw == [46, 46] {
        return Err(invalid());
    }
    let parts = parts(raw);
    if parts.len() < 2
        || !parts.last().is_some_and(|part| {
            !part.is_empty()
                && part
                    .iter()
                    .all(|point| crate::param_qr::is_python_digit(*point))
        })
    {
        return Err(invalid());
    }
    Ok(Value::Text(raw.to_vec()))
}
pub fn segment_index(segment: &Value) -> BigInt {
    let Value::Text(raw) = segment else {
        return BigInt::from(0);
    };
    parts(raw)
        .last()
        .map(|raw| Value::Text(raw.to_vec()).int().unwrap_or_default())
        .unwrap_or_default()
}
pub fn route_name(segment: &Value) -> Result<Value, Failure> {
    let raw = points(segment)?;
    let mut parts = parts(&raw);
    parts.pop();
    Ok(Value::Text(parts.join(&[45, 45][..])))
}
pub fn file_size_label(size: &Value) -> Result<String, Failure> {
    let Ok(number) = size.float() else {
        return Ok("-".into());
    };
    if number < 1024. {
        return Ok(format!(
            "{} B",
            Value::Float(number).int().map_err(Error::from)?
        ));
    }
    let (number, unit) = if number < 1024. * 1024. {
        (number / 1024., "KB")
    } else if number < 1024. * 1024. * 1024. {
        (number / (1024. * 1024.), "MB")
    } else {
        (number / (1024. * 1024. * 1024.), "GB")
    };
    Ok(format!(
        "{} {unit}",
        if number.is_nan() {
            "nan".into()
        } else {
            format!("{number:.1}")
        }
    ))
}
pub fn route_date_label(route: &Value) -> Value {
    let Value::Text(raw) = route else {
        return route.clone();
    };
    let parts = parts(raw);
    if raw.contains(&45) && parts.len() >= 2 {
        let time = parts[1].split(|point| *point == 45).collect::<Vec<_>>();
        if time.len() >= 2 {
            return Value::Text([parts[0], &[32], time[0], &[58], time[1]].concat());
        }
        return Value::Text(parts[0].to_vec());
    }
    if parts.len() >= 2 && parts[0].len() >= 8 && parts[1].len() >= 4 {
        return Value::Text(
            [
                &parts[0][..4],
                &[45],
                &parts[0][4..6],
                &[45],
                &parts[0][6..8],
                &[32],
                &parts[1][..2],
                &[58],
                &parts[1][2..4],
            ]
            .concat(),
        );
    }
    route.clone()
}
pub fn relative_time(epoch: i64, now: i64) -> String {
    shared::relative_time(epoch, now)
}
pub(super) fn path_value(path: &Path) -> Value {
    shared::text(path.as_os_str())
}
pub(super) fn value_path(value: &Value) -> Result<PathBuf, Failure> {
    shared::path_text(value)
        .map(PathBuf::from)
        .ok_or_else(|| Error::Source("invalid filesystem text".into()).into())
}
