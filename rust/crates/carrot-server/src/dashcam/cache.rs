use super::{catalog::Catalog, read_state::ReadState, Failure};
use crate::{config::Config, Error, Value};
use num_traits::ToPrimitive;
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

struct Cached {
    signature: Option<(i128, u64)>,
    time: f64,
    routes: Option<Vec<Value>>,
}
pub struct Service {
    pub(super) root: PathBuf,
    pub catalog: Catalog,
    pub(super) read_state: ReadState,
    cached: Mutex<Cached>,
    monotonic: Mutex<Option<f64>>,
    wall: Option<i64>,
}
impl Service {
    pub fn original(config: &Config) -> Arc<Self> {
        Self::for_test(
            "/data/media/0/realdata".into(),
            config.state.join("dashcam_read_state.json"),
            None,
            None,
        )
    }
    pub fn for_test(
        root: PathBuf,
        state: PathBuf,
        wall: Option<i64>,
        monotonic: Option<f64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            catalog: Catalog::new(root.clone()),
            root,
            read_state: ReadState::new(state),
            cached: Mutex::new(Cached {
                signature: None,
                time: 0.,
                routes: None,
            }),
            monotonic: Mutex::new(monotonic),
            wall,
        })
    }
    pub fn set_monotonic(&self, now: f64) -> Result<(), Failure> {
        *self
            .monotonic
            .lock()
            .map_err(|_| Error::Source("dashcam clock lock poisoned".into()))? = Some(now);
        Ok(())
    }
    fn now(&self) -> Result<f64, Failure> {
        if let Some(now) = *self
            .monotonic
            .lock()
            .map_err(|_| Error::Source("dashcam clock lock poisoned".into()))?
        {
            return Ok(now);
        }
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(time
            .tv_sec
            .to_f64()
            .ok_or_else(|| Error::Source("invalid monotonic seconds".into()))?
            + time
                .tv_nsec
                .to_f64()
                .ok_or_else(|| Error::Source("invalid monotonic fraction".into()))?
                / 1e9)
    }
    pub(super) fn wall(&self) -> i64 {
        self.wall.unwrap_or_else(|| {
            let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(time) => time.as_secs_f64(),
                Err(error) => -error.duration().as_secs_f64(),
            };
            Value::Float(seconds)
                .int()
                .ok()
                .and_then(|value| value.to_i64())
                .unwrap_or(0)
        })
    }
    pub fn cached_routes(&self) -> Result<Vec<Value>, Failure> {
        let signature = fs::metadata(&self.root).ok().map(|metadata| {
            (
                i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec()),
                metadata.size(),
            )
        });
        let now = self.now()?;
        {
            let cached = self
                .cached
                .lock()
                .map_err(|_| Error::Source("dashcam route lock poisoned".into()))?;
            if let Some(routes) = &cached.routes {
                if now - cached.time < 300. && signature == cached.signature {
                    return Ok(routes.clone());
                }
            }
        }
        self.catalog.invalidate()?;
        let routes = self.catalog.build_routes()?;
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| Error::Source("dashcam route lock poisoned".into()))?;
        cached.time = self.now()?;
        cached.signature = signature;
        cached.routes = Some(routes.clone());
        Ok(routes)
    }
    pub(super) fn visible_routes(&self) -> Result<Vec<Value>, Failure> {
        let routes = self.cached_routes()?;
        let mut hidden = HashSet::new();
        'tail: for entry in &routes {
            for segment in super::pages::segments(entry)?.iter().rev() {
                if self.catalog.segment_is_complete(segment)? {
                    break 'tail;
                }
                let Value::Text(points) = segment else {
                    return Err(Error::Source("expected segment text".into()).into());
                };
                hidden.insert(points.clone());
            }
        }
        if hidden.is_empty() {
            return Ok(routes);
        }
        let mut visible = Vec::new();
        for mut entry in routes {
            let segments = super::pages::segments(&entry)?
                .iter()
                .filter(|segment| !matches!(segment,Value::Text(points) if hidden.contains(points)))
                .cloned()
                .collect::<Vec<_>>();
            if segments.is_empty() {
                continue;
            }
            let count = segments.len();
            crate::json_fields::set(&mut entry, "segmentFolders", Value::Array(segments))?;
            crate::json_fields::set(&mut entry, "segmentCount", Value::integer(count))?;
            visible.push(entry);
        }
        Ok(visible)
    }
    pub(super) fn recent(&self, limit: usize) -> Result<Vec<Value>, Failure> {
        let mut completed = Vec::new();
        for entry in self.cached_routes()? {
            for segment in super::pages::segments(&entry)?.iter().rev() {
                if !self.catalog.segment_is_complete(segment)? {
                    continue;
                }
                completed.push(segment.clone());
                if completed.len() >= limit {
                    return Ok(completed);
                }
            }
        }
        Ok(completed)
    }
}
