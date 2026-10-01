use openpilot_logmessaged::{JsonValue, JsonView};
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
    pub last_id: Option<JsonValue>,
    seen: VecDeque<Vec<u32>>,
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
        let payload = read_json(&self.path)?;
        if !payload.is_object() {
            return None;
        }
        let messages = match payload.get("events") {
            Some(events) => match events.view() {
                JsonView::Array(values) => values,
                _ => return None,
            },
            None => vec![payload],
        };
        let root = self.path.parent()?;
        let learning = read_json(&root.join("learn.json"));
        let cancelled = read_json(&root.join("cancelled.json"));
        for message in messages.iter().skip(messages.len().saturating_sub(64)) {
            if let Some(action) =
                self.consume(message, allowed, now, learning.as_ref(), cancelled.as_ref())
            {
                return Some(action);
            }
        }
        None
    }

    fn consume(
        &mut self,
        message: &JsonValue,
        allowed: bool,
        now: f64,
        learning: Option<&JsonValue>,
        cancelled: Option<&JsonValue>,
    ) -> Option<String> {
        let id = message.get("id")?;
        let JsonView::Text(points) = id.view() else {
            return None;
        };
        if self.seen.iter().any(|seen| seen == points) {
            return None;
        }
        if self.seen.len() == 128 {
            self.seen.pop_front();
        }
        self.seen.push_back(points.to_vec());
        self.last_id = Some(id);
        let created = message
            .get("time")
            .map_or(Some(0.0), |value| number(&value))?;
        if !(self.started <= created && created <= now) || now - created > 0.4 {
            return None;
        }
        let address = message.get("address");
        let cutoff = cancelled
            .and_then(|value| lookup(value, address.as_ref()))
            .as_ref()
            .and_then(number)
            .unwrap_or(0.0);
        if created <= cutoff {
            return None;
        }
        if learning.is_some_and(|learning| {
            learning.is_object()
                && same_address(learning.get("address").as_ref(), address.as_ref())
                && learning
                    .get("until")
                    .as_ref()
                    .and_then(number)
                    .is_some_and(|until| until > now)
        }) {
            return None;
        }
        let action = message.get("action")?.to_utf8()?;
        if allowed && ACTIONS.contains(&action.as_str()) {
            self.is_repeat = message.get("repeat").as_ref().is_some_and(truthy);
            Some(action)
        } else {
            None
        }
    }
}

fn read_json(path: &Path) -> Option<JsonValue> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| JsonValue::parse(&text).ok())
}

fn number(value: &JsonValue) -> Option<f64> {
    match value.view() {
        JsonView::Integer(value) => value.parse().ok(),
        JsonView::Float(value) => Some(value),
        JsonView::Bool(value) => Some(f64::from(u8::from(value))),
        _ => None,
    }
}

fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0" && value != "-0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}

fn lookup(value: &JsonValue, key: Option<&JsonValue>) -> Option<JsonValue> {
    let JsonView::Text(key) = key?.view() else {
        return None;
    };
    let JsonView::Object(entries) = value.view() else {
        return None;
    };
    entries
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

fn same_address(left: Option<&JsonValue>, right: Option<&JsonValue>) -> bool {
    match (left.map(JsonValue::view), right.map(JsonValue::view)) {
        (None | Some(JsonView::Null), None | Some(JsonView::Null)) => true,
        (Some(JsonView::Text(left)), Some(JsonView::Text(right))) => left == right,
        _ => left
            .and_then(number)
            .zip(right.and_then(number))
            .is_some_and(|(left, right)| left == right),
    }
}
