use crate::{settings::Catalog, Error, Value};
use std::{fs, path::PathBuf, time::UNIX_EPOCH};

pub struct SettingsCache {
    pub path: PathBuf,
    mtime: Option<i128>,
    cached: Option<Catalog>,
}

impl SettingsCache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            mtime: None,
            cached: None,
        }
    }

    pub fn load(&mut self, maximum: i64) -> Result<Catalog, Error> {
        let modified = fs::metadata(&self.path)?.modified()?;
        let mtime = match modified.duration_since(UNIX_EPOCH) {
            Ok(duration) => i128::from(duration.as_secs()),
            Err(error) => -i128::from(error.duration().as_secs()),
        };
        if self.cached.is_none() || self.mtime != Some(mtime) {
            let source = fs::read_to_string(&self.path)?;
            self.cached = Some(Catalog::from_data(Value::parse(&source)?)?);
            self.mtime = Some(mtime);
        }
        self.cached
            .as_ref()
            .ok_or_else(|| Error::Source("settings cache unavailable".into()))?
            .with_gap_limits(maximum)
    }
}
