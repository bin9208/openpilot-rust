use crate::{Error, Value};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Default)]
pub struct History(Arc<Mutex<VecDeque<Value>>>);
pub(super) enum Outcome {
    Response { status: u16, bytes: usize },
    Error { status: u16, message: String },
}
impl History {
    pub fn snapshot(&self) -> Result<Value, Error> {
        let history = self
            .0
            .lock()
            .map_err(|_| Error::Source("stream history poisoned".into()))?;
        Ok(Value::Array(history.iter().cloned().collect()))
    }
    pub(super) fn record(
        &self,
        base: Value,
        outcome: Outcome,
        started: Instant,
    ) -> Result<(), Error> {
        let mut fields = vec![
            (
                "ts",
                Value::Float(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|error| Error::Source(error.to_string()))?
                        .as_secs_f64(),
                ),
            ),
            (
                "elapsed_ms",
                Value::Float((started.elapsed().as_secs_f64() * 10000.).round_ties_even() / 10.),
            ),
        ];
        match outcome {
            Outcome::Response { status, bytes } => fields.extend([
                ("ok", Value::Bool((200..300).contains(&status))),
                ("status", Value::integer(status)),
                ("response_bytes", Value::integer(bytes)),
            ]),
            Outcome::Error { status, message } => fields.extend([
                ("ok", Value::Bool(false)),
                ("status", Value::integer(status)),
                ("error", Value::text(&message)),
            ]),
        }
        let mut entry = Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                .collect(),
        );
        if let (Value::Object(entry), Value::Object(base)) = (&mut entry, base) {
            entry.extend(base);
        }
        let mut history = self
            .0
            .lock()
            .map_err(|_| Error::Source("stream history poisoned".into()))?;
        history.push_back(entry);
        if history.len() > 120 {
            history.pop_front();
        }
        Ok(())
    }
}
