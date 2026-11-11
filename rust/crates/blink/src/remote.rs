//! Bluetooth remote indicator taps for the SPAS worker: carrot's
//! `blink_source.py` `Events.read` and its `remote` hold check, reading the
//! Bluetooth daemon's runtime files.
use super::{policy::Command, Side};
use serde_json::Value;
use std::path::{Path, PathBuf};

const RUNTIME: &str = "/dev/shm/carrot-bluetooth";
const CONFIG: &str = "/data/carrot/bluetooth.json";

fn read(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or(Value::Null)
}

/// `fresh(stamp, now)` with carrot's 0.4 s default.
fn fresh(stamp: Option<f64>, now: f64) -> bool {
    stamp.is_some_and(|s| s.is_finite() && (0. ..0.4).contains(&(now - s)))
}

fn action(direction: Side) -> &'static str {
    match direction {
        Side::Left => "blinkerLeft",
        Side::Right => "blinkerRight",
    }
}

/// One cycle's view of the Bluetooth runtime.
pub struct Snapshot {
    journal: Value,
    settings: Value,
    status: Value,
    learning: Value,
    cancelled: Value,
}

impl Snapshot {
    pub fn load(runtime: &Path, config: &Path) -> Self {
        Self {
            journal: read(&runtime.join("blinker.json")),
            settings: read(config),
            status: read(&runtime.join("status.json")),
            learning: read(&runtime.join("learn.json")),
            cancelled: read(&runtime.join("cancelled.json")),
        }
    }

    fn status_fresh(&self, now: f64) -> bool {
        fresh(self.status.get("time").and_then(Value::as_f64), now)
    }

    /// The remote is enabled, grabbed, maps `action`, was not cancelled
    /// after `time` and is not being learned.
    fn usable(&self, mac: &str, action: &str, time: f64, now: f64) -> bool {
        let device = &self.settings["devices"][mac];
        let grabbed = self.status["grabbed"]
            .as_array()
            .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(mac)));
        let mapped = device["mapping"]
            .as_object()
            .is_some_and(|m| m.values().any(|v| v.as_str() == Some(action)));
        let cutoff = self.cancelled[mac].as_f64().unwrap_or(0.);
        let learning = self.learning["address"].as_str() == Some(mac)
            && self.learning["until"].as_f64().unwrap_or(0.) > now;
        device["enabled"] == Value::Bool(true) && grabbed && mapped && time > cutoff && !learning
    }
}

pub struct Remote {
    runtime: PathBuf,
    config: PathBuf,
    started: Option<f64>,
}

impl Default for Remote {
    fn default() -> Self {
        Self::new(RUNTIME, CONFIG)
    }
}

impl Remote {
    pub fn new(runtime: impl Into<PathBuf>, config: impl Into<PathBuf>) -> Self {
        Self {
            runtime: runtime.into(),
            config: config.into(),
            started: None,
        }
    }

    pub fn load(&self) -> Snapshot {
        Snapshot::load(&self.runtime, &self.config)
    }

    /// `Events.read`: the last four fresh indicator taps.
    pub fn commands(&mut self, snapshot: &Snapshot, now: f64) -> Vec<Command> {
        let started = *self.started.get_or_insert(now);
        if !snapshot.status_fresh(now) {
            return Vec::new();
        }
        let Some(events) = snapshot.journal["events"].as_array() else {
            return Vec::new();
        };
        let mut result: Vec<Command> = events
            .iter()
            .skip(events.len().saturating_sub(64))
            .filter_map(|event| {
                let time = event["time"].as_f64()?;
                if !fresh(Some(time), now) || time < started {
                    return None;
                }
                let mac = event["address"].as_str()?;
                let direction = match event["action"].as_str()? {
                    "blinkerLeft" => Side::Left,
                    "blinkerRight" => Side::Right,
                    _ => return None,
                };
                if event["repeat"].as_bool().unwrap_or(false)
                    || !snapshot.usable(mac, action(direction), time, now)
                {
                    return None;
                }
                Some(Command {
                    id: event["id"].as_str()?.to_owned(),
                    time,
                    direction,
                    device: Some(mac.to_owned()),
                })
            })
            .collect();
        let excess = result.len().saturating_sub(4);
        result.drain(..excess);
        result
    }

    /// Whether a Bluetooth hold's remote may still hold the lamp.
    pub fn holds(
        snapshot: &Snapshot,
        device: Option<&str>,
        direction: Side,
        time: f64,
        now: f64,
    ) -> bool {
        snapshot.status_fresh(now)
            && device.is_some_and(|mac| snapshot.usable(mac, action(direction), time, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fresh_mapped_taps_and_honours_cancel_and_repeat() {
        let dir = std::env::temp_dir().join(format!("card-remote-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mac = "00:11:22:33:44:55";
        let write = |name: &str, value: Value| {
            std::fs::write(dir.join(name), value.to_string()).unwrap();
        };
        write(
            "bluetooth.json",
            serde_json::json!({"devices": {mac: {"enabled": true, "mapping": {"key:30": "blinkerLeft"}}}}),
        );
        write(
            "status.json",
            serde_json::json!({"time": 11.0, "grabbed": [mac]}),
        );
        write(
            "blinker.json",
            serde_json::json!({"events": [
                {"id": "a", "time": 11.0, "action": "blinkerLeft", "address": mac, "repeat": false},
                {"id": "b", "time": 11.0, "action": "blinkerRight", "address": mac, "repeat": false},
                {"id": "c", "time": 11.0, "action": "blinkerLeft", "address": mac, "repeat": true},
            ]}),
        );
        let mut remote = Remote::new(&dir, dir.join("bluetooth.json"));
        remote.started = Some(10.);
        let snapshot = remote.load();
        let ids: Vec<_> = remote
            .commands(&snapshot, 11.1)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, ["a"], "unmapped and repeated events are ignored");
        assert!(Remote::holds(&snapshot, Some(mac), Side::Left, 11.0, 11.1));
        assert!(remote.commands(&snapshot, 11.5).is_empty(), "stale");

        write("cancelled.json", serde_json::json!({mac: 11.0}));
        let snapshot = remote.load();
        assert!(remote.commands(&snapshot, 11.1).is_empty());
        assert!(!Remote::holds(&snapshot, Some(mac), Side::Left, 11.0, 11.1));
        std::fs::remove_dir_all(&dir).ok();
    }
}
