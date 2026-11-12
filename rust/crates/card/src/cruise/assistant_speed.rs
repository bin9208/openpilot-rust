//! Consume a fresh, single voice speed intent; no engagement or pedal
//! commands. Port of carrot's `carrot/assistant_speed.py`.
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf};

const ROOT: &str = "/dev/shm/carrot-assistant-lane/speed";

fn identity(text: &str) -> bool {
    text.len() == 43
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn read_json(path: &std::path::Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

pub(super) struct Request {
    pub action: String,
    pub target: Option<f64>,
}

struct Active {
    id: String,
    action: String,
    previous: f64,
    target: Option<f64>,
}

pub(super) struct AssistantSpeed {
    root: PathBuf,
    started: f64,
    seen: VecDeque<String>,
    active: Option<Active>,
}

impl AssistantSpeed {
    pub fn new(started: f64) -> Self {
        Self {
            root: ROOT.into(),
            started,
            seen: VecDeque::with_capacity(128),
            active: None,
        }
    }

    pub fn active(&self) -> bool {
        self.active.is_some()
    }

    fn publish(&mut self, now: f64, status: &str, reason: &str, current: Option<f64>) {
        let Some(active) = self.active.take() else {
            return;
        };
        let mut result = json!({"id": active.id, "status": status, "reason": reason, "time": now});
        if let Some(current) = current {
            result["previous"] = json!(active.previous);
            result["current"] = json!(current);
        }
        // The consumed command stays consumed without a result file.
        let path = self.root.join("result.json");
        let _ = std::fs::create_dir_all(&self.root).and_then(|()| {
            let temporary = path.with_extension("tmp");
            std::fs::write(&temporary, result.to_string())?;
            std::fs::rename(&temporary, &path)
        });
    }

    pub fn read(
        &mut self,
        now: f64,
        allowed: bool,
        current: f64,
        minimum: f64,
        maximum: f64,
    ) -> Option<Request> {
        let command = read_json(&self.root.join("request.json"));
        let id = command
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if !command.is_object() || !identity(&id) || self.seen.contains(&id) {
            return None;
        }
        if self.seen.len() == 128 {
            self.seen.pop_front();
        }
        self.seen.push_back(id.clone());
        let action = command
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.active = Some(Active {
            id: id.clone(),
            action: action.clone(),
            previous: current,
            target: None,
        });
        let stamp = command.get("time").and_then(Value::as_f64);
        let cancelled = read_json(&self.root.join("cancel.json"));
        if cancelled.get("id").and_then(Value::as_str) == Some(id.as_str()) {
            self.publish(now, "cancelled", "permission_revoked", None);
        } else if !stamp.is_some_and(|s| self.started <= s && s <= now && now - s <= 1.) {
            self.publish(now, "expired", "request_expired", None);
        } else if !allowed {
            self.publish(now, "rejected", "cruise_inactive_or_driver_override", None);
        } else if action == "speed_up" {
            return Some(Request {
                action,
                target: None,
            });
        } else if matches!(action.as_str(), "speed_down" | "speed_set") {
            let value = command
                .get("value")
                .and_then(Value::as_i64)
                .filter(|v| (1..=199).contains(v));
            match value {
                None => self.publish(now, "rejected", "invalid_speed", None),
                Some(value) => {
                    let target = if action == "speed_down" {
                        current - value as f64
                    } else {
                        value as f64
                    };
                    if minimum <= target && target <= maximum {
                        if let Some(a) = &mut self.active {
                            a.target = Some(target);
                        }
                        return Some(Request {
                            action,
                            target: Some(target),
                        });
                    }
                    self.publish(now, "rejected", "speed_out_of_range", None);
                }
            }
        } else {
            self.publish(now, "rejected", "unsupported_command", None);
        }
        None
    }

    pub fn finish(&mut self, now: f64, current: f64) {
        let Some(active) = &self.active else {
            return;
        };
        if active.target.is_some_and(|t| (current - t).abs() > 0.01)
            || (active.action == "speed_up" && current < active.previous)
        {
            self.publish(now, "unverified", "target_overridden", Some(current));
        } else {
            let status = if (current - active.previous).abs() < 0.01 {
                "unchanged"
            } else {
                "applied"
            };
            self.publish(now, status, "", Some(current));
        }
    }
}
