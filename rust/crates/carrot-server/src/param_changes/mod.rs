//! Original services/param_changes.py history and drift baseline.
mod baseline;
mod json;
mod records;
pub(crate) mod text;
use crate::{json_fields::set, Error, Value};
pub use json::{canonical, fingerprint, record_hash};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const MAX_RECORDS: usize = 1000;
pub struct Paths {
    pub log: PathBuf,
    pub baseline: PathBuf,
}
pub struct Change<'a> {
    pub name: &'a Value,
    pub previous: &'a Value,
    pub next: &'a Value,
    pub source: &'a Value,
    pub engaged: bool,
}
enum Clock {
    System,
    Fixed(Value),
}
pub struct History {
    pub paths: Paths,
    write_lock: Mutex<()>,
    known: Mutex<BTreeMap<Vec<u32>, Value>>,
    clock: Clock,
}
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub fn normalize_source(source: &Value) -> Result<Value, Error> {
    let source = text::stripped(source, true)?;
    Ok(
        if [
            "web_ui",
            "profile",
            "restore",
            "reset_defaults",
            "intro",
            "undo",
            "device",
            "unknown",
        ]
        .iter()
        .any(|allowed| source.text_eq(allowed))
        {
            source
        } else {
            Value::text("unknown")
        },
    )
}

impl History {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            write_lock: Mutex::new(()),
            known: Mutex::new(BTreeMap::new()),
            clock: Clock::System,
        }
    }
    pub fn with_timestamp(mut self, timestamp: Value) -> Self {
        self.clock = Clock::Fixed(timestamp);
        self
    }
    pub(crate) fn timestamp(&self) -> Result<Value, Error> {
        match &self.clock {
            Clock::Fixed(value) => Ok(Value::Integer(value.int()?)),
            Clock::System => Ok(match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(duration) => Value::integer(duration.as_secs()),
                Err(error) => Value::Integer(
                    -num_bigint::BigInt::from(error.duration().as_secs())
                        - num_bigint::BigInt::from(u8::from(error.duration().subsec_nanos() != 0)),
                ),
            }),
        }
    }
    pub fn note(&self, name: &Value, value: &Value) -> Result<(), Error> {
        let Value::Text(name) = name.py_string()? else {
            return Err(Error::Source("expected parameter name text".into()));
        };
        lock(&self.known).insert(name, value.clone());
        Ok(())
    }
    pub fn append(&self, change: Change<'_>) -> Option<Value> {
        self.append_checked(change).unwrap_or_default()
    }
    fn append_checked(&self, change: Change<'_>) -> Result<Option<Value>, Error> {
        let name = text::stripped(change.name, true)?;
        if !name.truth() {
            return Ok(None);
        }
        self.note(&name, change.next)?;
        let _guard = lock(&self.write_lock);
        let previous = self.last();
        let previous_hash = previous
            .as_ref()
            .map(|record| record.get("hash"))
            .filter(|value| value.truth())
            .map_or(Ok(Value::text(GENESIS_HASH)), Value::py_string)?;
        let mut record = Value::object([
            ("ts", self.timestamp()?),
            ("name", name),
            ("prev", change.previous.clone()),
            ("next", change.next.clone()),
            ("source", normalize_source(change.source)?),
            ("engaged", Value::Bool(change.engaged)),
            ("prev_hash", previous_hash),
        ]);
        let hash = record_hash(&record)?;
        set(&mut record, "hash", Value::text(&hash))?;
        self.append_record(&record)?;
        self.trim()?;
        Ok(Some(record))
    }
    pub fn observe(&self, values: &Value, allowed: Option<&Value>) -> Result<usize, Error> {
        let Value::Object(values) = values else {
            return Ok(0);
        };
        let mut drift = Vec::new();
        {
            let mut known = lock(&self.known);
            for (name, value) in values {
                if allowed.is_some_and(|allowed| !matches!(allowed,Value::Object(fields) if fields.iter().any(|(key,_)| key==name))) { continue; }
                match known.get(name) {
                    Some(previous) if !text::equal(previous, value) => {
                        drift.push((name.clone(), previous.clone(), value.clone()));
                        known.insert(name.clone(), value.clone());
                    }
                    None => {
                        known.insert(name.clone(), value.clone());
                    }
                    Some(_) => {}
                }
            }
        }
        let count = drift.len();
        for (name, previous, next) in drift {
            self.append(Change {
                name: &Value::Text(name),
                previous: &previous,
                next: &next,
                source: &Value::text("device"),
                engaged: false,
            });
        }
        Ok(count)
    }
}
