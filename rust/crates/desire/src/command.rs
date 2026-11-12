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
    /// (mtime, size) of a file whose every message has been looked at. Set only
    /// after a pass that reached the end of the file: a pass that returned an
    /// action may have left later messages unread.
    stamp: Option<(i64, i64, u64)>,
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
            stamp: None,
        }
    }

    pub fn read(&mut self, allowed: bool, now: f64) -> Option<String> {
        self.is_repeat = false;
        if now - self.last_check < 0.02 {
            return None;
        }
        self.last_check = now;
        // An unchanged file holds only messages already seen, so no action can result.
        let stamp = {
            use std::os::unix::fs::MetadataExt;
            let meta = fs::metadata(&self.path).ok()?;
            (meta.mtime(), meta.mtime_nsec(), meta.len())
        };
        if self.stamp == Some(stamp) {
            return None;
        }
        self.stamp = None;
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
                // carrot reads the file again 20 ms later and takes the next
                // unseen message. Remembering the stamp here would skip every
                // message after this one until the file is written again.
                return Some(action);
            }
        }
        self.stamp = Some(stamp);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("openpilot-desire-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn every_command_of_one_write_is_delivered_one_per_read() {
        let scratch = Scratch::new("batch");
        fs::write(
            scratch.0.join("remote.json"),
            r#"{"events": [
                {"id": "a", "time": 10.0, "action": "accelCruise"},
                {"id": "b", "time": 10.0, "action": "bogus"},
                {"id": "c", "time": 10.1, "action": "decelCruise", "repeat": true},
                {"id": "d", "time": 10.1, "action": "laneLeft"}
            ]}"#,
        )
        .unwrap();
        let mut reader = CommandReader::new(&scratch.0, "remote", 9.0);
        assert_eq!(reader.read(true, 10.2).as_deref(), Some("accelCruise"));
        // Inside the 20 ms poll interval nothing is read.
        assert_eq!(reader.read(true, 10.21), None);
        assert_eq!(reader.read(true, 10.23).as_deref(), Some("decelCruise"));
        assert!(reader.is_repeat);
        assert_eq!(reader.read(true, 10.26).as_deref(), Some("laneLeft"));
        assert!(!reader.is_repeat);
        // The file is exhausted: later reads return nothing and deliver nothing twice.
        assert_eq!(reader.read(true, 10.29), None);
        assert_eq!(reader.read(true, 10.32), None);
    }

    #[test]
    fn a_command_seen_while_not_allowed_or_too_old_is_dropped_for_good() {
        let scratch = Scratch::new("gates");
        fs::write(
            scratch.0.join("remote.json"),
            r#"{"events": [
                {"id": "old", "time": 9.0, "action": "accelCruise"},
                {"id": "new", "time": 10.0, "action": "decelCruise"}
            ]}"#,
        )
        .unwrap();
        let mut reader = CommandReader::new(&scratch.0, "remote", 8.0);
        assert_eq!(reader.read(false, 10.1), None);
        assert_eq!(reader.read(true, 10.2), None);
    }
}
