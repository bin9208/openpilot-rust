//! Persistent Tools history, preserving source permissive JSON fields and Unicode log counting.
use super::{jobs_load, text};
use crate::{json_fields::set, Error, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub struct Store {
    pub(super) path: PathBuf,
    pub(super) values: Mutex<Vec<Value>>,
    pub(super) clock: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub(super) last_persist: Mutex<f64>,
}
impl Store {
    pub fn new(path: PathBuf) -> Arc<Self> {
        Self::with_clock(path, text::time)
    }
    pub fn with_clock(path: PathBuf, clock: impl Fn() -> f64 + Send + Sync + 'static) -> Arc<Self> {
        let store = Arc::new(Self {
            path,
            values: Mutex::new(Vec::new()),
            clock: Arc::new(clock),
            last_persist: Mutex::new(0.),
        });
        if let Err(error) = jobs_load::load(&store) {
            eprintln!("Tools history load: {error}");
        }
        store
    }
    pub(super) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<Value>>, Error> {
        self.values
            .lock()
            .map_err(|_| Error::Source("Tools history lock poisoned".into()))
    }
    pub fn get(&self, id: &str) -> Result<Option<Value>, Error> {
        Ok(self
            .lock()?
            .iter()
            .find(|job| job.get("id").text_eq(id))
            .cloned()
            .map(Self::snapshot))
    }
    pub fn snapshots(&self, limit: usize) -> Result<Value, Error> {
        let mut jobs = self.lock()?.clone();
        Self::sort(&mut jobs);
        jobs.truncate(limit);
        Ok(Value::Array(jobs.into_iter().map(Self::snapshot).collect()))
    }
    pub(super) fn sort(jobs: &mut [Value]) {
        jobs.sort_by(|a, b| {
            text::float(b.get("updated_at"))
                .partial_cmp(&text::float(a.get("updated_at")))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    text::float(b.get("created_at"))
                        .partial_cmp(&text::float(a.get("created_at")))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    }
    pub fn snapshot(job: Value) -> Value {
        let mut value = Value::object([
            ("ok", Value::Bool(true)),
            ("id", job.get("id").clone()),
            ("action", job.get("action").clone()),
            (
                "payload",
                if matches!(job.get("payload"), Value::Object(_)) {
                    job.get("payload").clone()
                } else {
                    Value::object([])
                },
            ),
            ("status", job.get("status").clone()),
            ("done", Value::Bool(Self::finished(&job))),
        ]);
        for key in [
            "log",
            "progress",
            "message",
            "step_current",
            "step_total",
            "error",
            "error_code",
            "error_detail",
            "created_at",
            "updated_at",
            "result",
        ] {
            let field = if matches!(key, "log" | "message") && !job.get(key).truth() {
                Value::text("")
            } else {
                job.get(key).clone()
            };
            if let Err(error) = set(&mut value, key, field) {
                eprintln!("Tools snapshot: {error}");
            }
        }
        value
    }
    pub(super) fn finished(job: &Value) -> bool {
        job.get("status").text_eq("done") || job.get("status").text_eq("failed")
    }
    pub fn insert(&self, job: Value) -> Result<(), Error> {
        let mut jobs = self.lock()?;
        if let Some(existing) = jobs.iter_mut().find(|old| old.get("id") == job.get("id")) {
            *existing = job;
        } else {
            jobs.push(job);
        }
        drop(jobs);
        self.prune()?;
        self.persist();
        Ok(())
    }
    pub fn change(
        &self,
        id: &str,
        change: impl FnOnce(&mut Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut jobs = self.lock()?;
        if let Some(job) = jobs.iter_mut().find(|job| job.get("id").text_eq(id)) {
            change(job)?;
            set(job, "updated_at", Value::Float((self.clock)()))?;
        }
        drop(jobs);
        self.persist_changed();
        Ok(())
    }
    pub fn clear(&self) -> Result<usize, Error> {
        let mut jobs = self.lock()?;
        let before = jobs.len();
        jobs.retain(|job| !Self::finished(job));
        let count = before - jobs.len();
        drop(jobs);
        if count > 0 {
            self.persist();
        }
        Ok(count)
    }
    pub fn prune(&self) -> Result<bool, Error> {
        let mut jobs = self.lock()?;
        let before = jobs.len();
        let now = (self.clock)();
        jobs.retain(|job| {
            let updated = if job.get("updated_at").truth() {
                job.get("updated_at")
            } else {
                job.get("created_at")
            };
            !(Self::finished(job)
                && text::float(updated) > 0.
                && now - text::float(updated) > 604800.)
        });
        let mut finished: Vec<_> = jobs
            .iter()
            .filter(|job| Self::finished(job))
            .cloned()
            .collect();
        finished.sort_by(|a, b| {
            text::float(b.get("updated_at"))
                .partial_cmp(&text::float(a.get("updated_at")))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for old in finished.into_iter().skip(20) {
            jobs.retain(|job| job.get("id") != old.get("id"));
        }
        Ok(before != jobs.len())
    }
}
