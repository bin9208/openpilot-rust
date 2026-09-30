use serde_json::Value;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

const ACTIONS: &[&str] = &[
    "none",
    "accelCruise",
    "decelCruise",
    "gapAdjustCruise",
    "lfaButton",
    "cancel",
    "accelCruiseLong",
    "decelCruiseLong",
    "gapAdjustCruiseLong",
    "lfaButtonLong",
    "cancelLong",
    "laneLeft",
    "laneRight",
    "paddleDecel",
    "carrotCruise",
];

pub struct CommandReader {
    path: PathBuf,
    started: f64,
    pub last_id: Option<String>,
    seen: VecDeque<String>,
    last_check: f64,
    pub is_repeat: bool,
}

impl CommandReader {
    pub fn new(root: &Path, channel: &str, started: f64) -> Self {
        Self {
            path: root.join(format!("{channel}.json")),
            started,
            last_id: None,
            seen: VecDeque::with_capacity(128),
            last_check: 0.0,
            is_repeat: false,
        }
    }

    pub fn read(&mut self, allowed: bool, now: f64) -> Option<String> {
        self.is_repeat = false;
        if now - self.last_check < 0.02 {
            return None;
        }
        self.last_check = now;
        let payload = read_json(&self.path);
        let payload = payload.as_object()?;
        let messages: &[Value] = match payload.get("events") {
            Some(events) => events.as_array()?,
            None => return self.read_legacy(payload, allowed, now),
        };
        let root = self.path.parent()?;
        let learning = read_json(&root.join("learn.json"));
        let cancelled = read_json(&root.join("cancelled.json"));
        for message in messages.iter().skip(messages.len().saturating_sub(64)) {
            if let Some(action) = self.consume(message, allowed, now, &learning, &cancelled) {
                return Some(action);
            }
        }
        None
    }

    fn read_legacy(
        &mut self,
        payload: &serde_json::Map<String, Value>,
        allowed: bool,
        now: f64,
    ) -> Option<String> {
        let root = self.path.parent()?;
        let learning = read_json(&root.join("learn.json"));
        let cancelled = read_json(&root.join("cancelled.json"));
        self.consume(
            &Value::Object(payload.clone()),
            allowed,
            now,
            &learning,
            &cancelled,
        )
    }

    fn consume(
        &mut self,
        message: &Value,
        allowed: bool,
        now: f64,
        learning: &Value,
        cancelled: &Value,
    ) -> Option<String> {
        let id = message.get("id")?.as_str()?;
        if self.seen.iter().any(|seen| seen == id) {
            return None;
        }
        if self.seen.len() == 128 {
            self.seen.pop_front();
        }
        self.seen.push_back(id.to_owned());
        self.last_id = Some(id.to_owned());
        let created = number(message.get("time").unwrap_or(&Value::from(0)))?;
        if !(self.started <= created && created <= now) || now - created > 0.4 {
            return None;
        }
        let address = message.get("address").unwrap_or(&Value::Null);
        let cutoff = address
            .as_str()
            .and_then(|address| cancelled.get(address))
            .and_then(number)
            .unwrap_or(0.0);
        if created <= cutoff {
            return None;
        }
        if learning.is_object()
            && learning.get("address").unwrap_or(&Value::Null) == address
            && learning
                .get("until")
                .and_then(number)
                .is_some_and(|until| until > now)
        {
            return None;
        }
        let action = message.get("action")?.as_str()?;
        if allowed && ACTIONS.contains(&action) {
            self.is_repeat = message.get("repeat").is_some_and(truthy);
            Some(action.to_owned())
        } else {
            None
        }
    }
}

fn read_json(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|value| f64::from(u8::from(value))))
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
