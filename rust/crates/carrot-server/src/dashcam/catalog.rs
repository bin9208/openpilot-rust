pub use super::selectors::{
    segment_file_summary, source_qlog, source_rlog, source_video, source_video_end_epoch,
};
use super::{paths, selectors, Failure};
use crate::{Error, Value};
use num_bigint::BigInt;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct Catalog {
    root: PathBuf,
    end_epochs: Mutex<HashMap<Vec<u32>, i64>>,
}
pub type RouteKey = (u8, u32, Vec<u32>, Vec<u32>);
pub fn route_creation_key(route: &Value) -> Result<RouteKey, Failure> {
    let raw = paths::points(route)?;
    let hex = |point: &u32| matches!(*point, 48..=57 | 65..=70 | 97..=102);
    if raw.len() == 20
        && raw[8..10] == [45, 45]
        && raw[..8].iter().all(hex)
        && raw[10..].iter().all(hex)
    {
        let number = raw[..8]
            .iter()
            .filter_map(|point| char::from_u32(*point))
            .collect::<String>();
        let number = u32::from_str_radix(&number, 16)
            .map_err(|_| Error::Source("invalid route count".into()))?;
        Ok((1, number, Vec::new(), raw))
    } else {
        Ok((0, 0, raw.clone(), raw))
    }
}
pub fn segment_creation_key(segment: &Value) -> Result<(RouteKey, BigInt, Vec<u32>), Failure> {
    let raw = paths::points(segment)?;
    let parts = paths::parts(&raw);
    let route = if parts.len() >= 2 {
        Value::Text(parts[..parts.len() - 1].join(&[45, 45][..]))
    } else {
        Value::Text(raw.clone())
    };
    Ok((
        route_creation_key(&route)?,
        paths::segment_index(segment),
        raw,
    ))
}
impl Catalog {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            end_epochs: Mutex::new(HashMap::new()),
        }
    }
    pub fn invalidate(&self) -> Result<(), Failure> {
        self.end_epochs
            .lock()
            .map_err(|_| Error::Source("dashcam epoch lock poisoned".into()))?
            .clear();
        Ok(())
    }
    pub fn segment_is_complete(&self, segment: &Value) -> Result<bool, Failure> {
        selectors::segment_is_complete(&self.root, segment)
    }
    pub fn build_routes(&self) -> Result<Vec<Value>, Failure> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }
        let entries =
            fs::read_dir(&self.root).map_err(|error| crate::state::io_error(error, &self.root))?;
        let mut groups: Vec<(Vec<u32>, Vec<Vec<u32>>)> = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Value::Text(name) = paths::path_value(Path::new(&entry.file_name())) else {
                continue;
            };
            let parts = paths::parts(&name);
            if parts.len() < 2
                || !parts.last().is_some_and(|part| {
                    !part.is_empty()
                        && part
                            .iter()
                            .all(|point| crate::param_qr::is_python_digit(*point))
                })
            {
                continue;
            }
            let route = parts[..parts.len() - 1].join(&[45, 45][..]);
            if let Some((_, names)) = groups.iter_mut().find(|(value, _)| value == &route) {
                names.push(name);
            } else {
                groups.push((route, vec![name]));
            }
        }
        let mut routes = Vec::new();
        for (route, mut segments) in groups {
            segments.sort_by(|first, second| {
                paths::segment_index(&Value::Text(first.clone()))
                    .cmp(&paths::segment_index(&Value::Text(second.clone())))
                    .then_with(|| first.cmp(second))
            });
            let key = route_creation_key(&Value::Text(route.clone()))?;
            let title = route
                .iter()
                .position(|point| *point != 48)
                .map_or_else(|| route.clone(), |start| route[start..].to_vec());
            let route = Value::Text(route);
            let count = segments.len();
            routes.push((
                key,
                Value::object([
                    ("route", route.clone()),
                    ("title", Value::Text(title)),
                    ("dateLabel", paths::route_date_label(&route)),
                    (
                        "segmentFolders",
                        Value::Array(segments.into_iter().map(Value::Text).collect()),
                    ),
                    ("segmentCount", Value::integer(count)),
                ]),
            ));
        }
        routes.sort_by(|(first, _), (second, _)| second.cmp(first));
        Ok(routes.into_iter().map(|(_, route)| route).collect())
    }
    fn end_epoch(&self, segment: &Value) -> Result<i64, Failure> {
        let points = paths::points(segment)?;
        if let Some(epoch) = self
            .end_epochs
            .lock()
            .map_err(|_| Error::Source("dashcam epoch lock poisoned".into()))?
            .get(&points)
            .copied()
        {
            return Ok(epoch);
        }
        let epoch = source_video_end_epoch(
            &self
                .root
                .join(paths::value_path(&Value::Text(points.clone()))?),
        );
        if epoch > 0 {
            self.end_epochs
                .lock()
                .map_err(|_| Error::Source("dashcam epoch lock poisoned".into()))?
                .insert(points, epoch);
        }
        Ok(epoch)
    }
    pub fn compute_segment_times(
        &self,
        segments: &[Value],
        seed: &Value,
    ) -> Result<Value, Failure> {
        let mut times = Vec::new();
        let mut previous_index: Option<BigInt> = None;
        let mut previous_end = 0;
        if seed.truth() {
            let end = self.end_epoch(seed)?;
            if end > 0 {
                previous_index = Some(paths::segment_index(seed));
                previous_end = end;
            }
        }
        for name in segments {
            let index = paths::segment_index(name);
            let end = self.end_epoch(name)?;
            if end <= 0 {
                previous_index = None;
                previous_end = 0;
                continue;
            }
            let contiguous = previous_end > 0
                && end >= previous_end
                && previous_index
                    .as_ref()
                    .is_some_and(|previous| index == previous + 1);
            let start = if contiguous {
                previous_end
            } else {
                end.saturating_sub(60)
            }
            .clamp(0, end);
            let Value::Text(key) = name else {
                return Err(Error::Source("unhashable segment name".into()).into());
            };
            crate::json_fields::insert(
                &mut times,
                key.clone(),
                Value::object([
                    ("startEpoch", Value::integer(start)),
                    ("endEpoch", Value::integer(end)),
                ]),
            );
            previous_index = Some(index);
            previous_end = end;
        }
        Ok(Value::Object(times))
    }
    pub fn route_time_bounds(&self, segments: &[Value]) -> Result<(i64, i64), Failure> {
        let Some(first) = segments.first() else {
            return Ok((0, 0));
        };
        let first_end = self.end_epoch(first)?;
        let last_end = if segments.last() == Some(first) {
            first_end
        } else {
            self.end_epoch(&segments[segments.len() - 1])?
        };
        Ok((
            if first_end > 0 {
                first_end.saturating_sub(60).max(0)
            } else {
                0
            },
            last_end.max(0),
        ))
    }
}
