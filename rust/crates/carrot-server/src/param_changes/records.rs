use super::{canonical, record_hash, text, History, GENESIS_HASH, MAX_RECORDS};
use crate::{json_fields::set, Error, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

impl History {
    fn lines(&self) -> Vec<String> {
        match fs::read_to_string(&self.paths.log) {
            Ok(text) => text
                .split(|c| {
                    matches!(
                        c,
                        '\n' | '\r'
                            | '\u{b}'
                            | '\u{c}'
                            | '\u{1c}'
                            | '\u{1d}'
                            | '\u{1e}'
                            | '\u{85}'
                            | '\u{2028}'
                            | '\u{2029}'
                    )
                })
                .filter(|line| {
                    !crate::state::trim(&line.chars().map(u32::from).collect::<Vec<_>>()).is_empty()
                })
                .map(str::to_owned)
                .collect(),
            Err(_) => Vec::new(),
        }
    }
    pub fn last(&self) -> Option<Value> {
        self.lines()
            .into_iter()
            .rev()
            .find_map(|line| match Value::parse(&line) {
                Ok(value @ Value::Object(_)) => Some(value),
                Ok(_) | Err(_) => None,
            })
    }
    pub fn read(&self, limit: usize, name: &Value, source: &Value) -> Result<Value, Error> {
        let mut records = Vec::new();
        for line in self.lines().into_iter().rev() {
            let Ok(record @ Value::Object(_)) = Value::parse(&line) else {
                continue;
            };
            if name.truth()
                && !text::equal(
                    &if record.has("name") {
                        record.get("name").py_string()?
                    } else {
                        Value::text("")
                    },
                    &name.py_string()?,
                )
            {
                continue;
            }
            if source.truth()
                && !text::equal(
                    &if record.has("source") {
                        record.get("source").py_string()?
                    } else {
                        Value::text("")
                    },
                    &source.py_string()?,
                )
            {
                continue;
            }
            records.push(record);
            if limit != 0 && records.len() >= limit {
                break;
            }
        }
        Ok(Value::Array(records))
    }
    pub(crate) fn append_record(&self, record: &Value) -> Result<(), Error> {
        let parent = self
            .paths
            .log
            .parent()
            .ok_or_else(|| Error::Source("invalid history directory".into()))?;
        fs::create_dir_all(parent)?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.log)?
            .write_all(format!("{}\n", canonical(record)?).as_bytes())?;
        Ok(())
    }
    pub(crate) fn trim(&self) -> Result<(), Error> {
        let lines = self.lines();
        if lines.len() <= MAX_RECORDS {
            return Ok(());
        }
        let mut records = lines
            .into_iter()
            .filter_map(|line| match Value::parse(&line) {
                Ok(value @ Value::Object(_)) => Some(value),
                Ok(_) | Err(_) => None,
            })
            .collect::<Vec<_>>();
        let start = records.len().saturating_sub(MAX_RECORDS);
        let mut previous = GENESIS_HASH.to_owned();
        let mut output = String::new();
        for record in &mut records[start..] {
            set(record, "prev_hash", Value::text(&previous))?;
            previous = record_hash(record)?;
            set(record, "hash", Value::text(&previous))?;
            output.push_str(&canonical(record)?);
            output.push('\n');
        }
        let temporary = self.paths.log.with_file_name(format!(
            "{}.tmp",
            self.paths
                .log
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        fs::write(&temporary, output)?;
        fs::rename(temporary, &self.paths.log)?;
        Ok(())
    }
    pub fn verify(&self) -> Result<Value, Error> {
        let mut previous = GENESIS_HASH.to_owned();
        let mut checked = 0;
        for (index, line) in self.lines().iter().enumerate() {
            let record = match Value::parse(line) {
                Ok(value @ Value::Object(_)) => value,
                Ok(_) | Err(_) => {
                    return Ok(report(
                        false,
                        checked,
                        Some(index),
                        "record is not valid JSON",
                    ));
                }
            };
            if !text::string(record.get("prev_hash"), true)?.text_eq(&previous) {
                return Ok(report(
                    false,
                    checked,
                    Some(index),
                    "record does not link to the previous hash",
                ));
            }
            let expected = record_hash(&record)?;
            if !text::string(record.get("hash"), true)?.text_eq(&expected) {
                return Ok(report(
                    false,
                    checked,
                    Some(index),
                    "record content does not match its hash",
                ));
            }
            previous = record.get("hash").string()?;
            checked += 1;
        }
        Ok(report(true, checked, None, ""))
    }
    pub fn count_since(&self, timestamp: &Value, allowed: Option<&Value>) -> Result<usize, Error> {
        let Value::Array(records) = self.read(0, &Value::Null, &Value::Null)? else {
            return Err(Error::Source("expected history records".into()));
        };
        let mut names = std::collections::BTreeSet::new();
        for record in records {
            let ts = if record.get("ts").truth() {
                record.get("ts").int()?
            } else {
                0.into()
            };
            if ts < timestamp.int()? {
                continue;
            }
            let Value::Text(name) = text::string(record.get("name"), true)? else {
                return Err(Error::Source("expected parameter name".into()));
            };
            if name.is_empty() || allowed.is_some_and(|allowed| !matches!(allowed,Value::Object(fields) if fields.iter().any(|(key,_)| *key==name))) { continue; }
            names.insert(name);
        }
        Ok(names.len())
    }
}
fn report(valid: bool, checked: usize, index: Option<usize>, reason: &str) -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("valid", Value::Bool(valid)),
        ("checked", Value::integer(checked)),
        ("broken_at", index.map_or(Value::Null, Value::integer)),
        ("reason", Value::text(reason)),
    ])
}
