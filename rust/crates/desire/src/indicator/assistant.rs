//! One-shot assistant intent; the existing lane-change FSM still owns
//! motion. Separate from physical remote input: a web authorization cannot
//! impersonate a Bluetooth button, steering torque or a lamp acknowledgement.
//! Port of carrot's `desire_lib/assistant_lane.py`.
use super::echo::TapEcho;
use crate::types::Car;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

pub const ROOT: &str = "/dev/shm/carrot-assistant-lane";
const TERMINAL: [&str; 6] = [
    "completed",
    "rejected",
    "expired",
    "interrupted",
    "unverified",
    "cancelled",
];

/// `IDENTITY`: `^[A-Za-z0-9_-]{43}$`.
pub fn identity(text: &str) -> bool {
    text.len() == 43
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn read_json(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

fn atomic_json(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, value.to_string())?;
    std::fs::rename(&temporary, path)
}

#[derive(Clone, Debug)]
pub struct Active {
    pub id: String,
    pub direction: String,
    pub began: bool,
    finishing: bool,
    crossed: bool,
    accepted: f64,
    signal_route: String,
    signal_since: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SideState {
    pub available: bool,
    pub blindspot: bool,
}

/// What `before` reads from the FSM and vehicle.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub valid: bool,
    pub lateral_active: bool,
    pub drive: bool,
    pub trailer: bool,
    pub speed: f64,
    pub phase: &'static str,
    pub driver_busy: bool,
    pub indicator_driver_busy: bool,
    pub driver_override: bool,
    pub other_request: bool,
    pub left: SideState,
    pub right: SideState,
}

#[derive(Debug)]
pub struct AssistantLane {
    root: PathBuf,
    fingerprint: String,
    native_signal: bool,
    started: Option<f64>,
    seen: VecDeque<String>,
    pub active: Option<Active>,
    last_publish: f64,
    result: Option<Value>,
    last_tick: Option<f64>,
    tap_echo: TapEcho,
}

impl Default for AssistantLane {
    fn default() -> Self {
        Self::new(ROOT, "", false)
    }
}

impl AssistantLane {
    pub fn new(root: impl Into<PathBuf>, fingerprint: &str, native_signal: bool) -> Self {
        Self {
            root: root.into(),
            fingerprint: fingerprint.into(),
            native_signal,
            started: None,
            seen: VecDeque::with_capacity(128),
            active: None,
            last_publish: 0.,
            result: None,
            last_tick: None,
            tap_echo: TapEcho::default(),
        }
    }

    pub fn publish(&mut self, now: f64, status: &str, reason: &str) {
        let Some(active) = &self.active else {
            return;
        };
        let mut value = json!({"id": active.id, "status": status, "reason": reason});
        if matches!(active.direction.as_str(), "left" | "right") {
            value["direction"] = json!(active.direction);
        }
        if TERMINAL.contains(&status) {
            self.active = None;
        }
        if self.result.as_ref() != Some(&value) || now - self.last_publish >= 0.25 {
            let mut record = value.clone();
            record["time"] = json!(now);
            // Missing feedback cannot undo a rejection or cancellation.
            if atomic_json(&self.root.join("result.json"), &record).is_ok() {
                self.result = Some(value);
                self.last_publish = now;
            }
        }
    }

    fn blocked(&self, state: &State, direction: &str) -> &'static str {
        if !state.valid {
            return "vehicle_state_unavailable";
        }
        if !state.lateral_active {
            return "lateral_control_inactive";
        }
        if !state.drive || state.trailer {
            return "vehicle_condition";
        }
        if !state.speed.is_finite() || state.speed < 30. / 3.6 {
            return "below_lane_change_speed";
        }
        if state.driver_busy
            || state.other_request
            || (self.signal_route() != "ordinary" && state.indicator_driver_busy)
        {
            return "driver_or_other_maneuver";
        }
        let side = if direction == "left" {
            state.left
        } else {
            state.right
        };
        if side.blindspot {
            return "blindspot";
        }
        if !side.available {
            return "lane_unavailable";
        }
        ""
    }

    /// `(direction to request, cancel our not-yet-started preparation)`.
    pub fn before(&mut self, now: f64, state: &State) -> (Option<String>, bool) {
        let started = *self.started.get_or_insert(now);
        let gap = self.last_tick.is_some_and(|t| now - t > 0.35);
        self.last_tick = Some(now);
        let command = read_json(&self.root.join("request.json"));
        let id = command
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if command.is_object() && identity(&id) && !self.seen.contains(&id) {
            if self.seen.len() == 128 {
                self.seen.pop_front();
            }
            self.seen.push_back(id.clone());
            if self.active.is_none() {
                let direction = command
                    .get("direction")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.active = Some(Active {
                    id,
                    direction: direction.clone(),
                    began: false,
                    finishing: false,
                    crossed: false,
                    accepted: now,
                    signal_route: String::new(),
                    signal_since: None,
                });
                let stamp = command.get("time").and_then(Value::as_f64);
                if !matches!(direction.as_str(), "left" | "right")
                    || !stamp.is_some_and(|s| started <= s && s <= now && now - s <= 1.)
                {
                    self.publish(now, "expired", "request_expired");
                } else if state.phase != "off" {
                    self.publish(now, "rejected", "maneuver_already_active");
                } else {
                    let route = self.indicator_route(now);
                    if let Some(a) = &mut self.active {
                        a.signal_route = route;
                    }
                    let reason = self.blocked(state, &direction);
                    if !reason.is_empty() {
                        self.publish(now, "rejected", reason);
                    }
                }
            }
        }
        let Some(active) = self.active.clone() else {
            return (None, false);
        };
        let cancelled = read_json(&self.root.join("cancel.json"));
        if cancelled.get("id").and_then(Value::as_str) == Some(active.id.as_str()) {
            self.publish(
                now,
                if active.began {
                    "interrupted"
                } else {
                    "cancelled"
                },
                "permission_revoked",
            );
            return (None, !active.began);
        }
        if active.began {
            // Never force a steering/path transition once a maneuver is underway.
            if !state.valid || !state.lateral_active || state.driver_override {
                self.publish(now, "interrupted", "driver_override_or_state_lost");
            } else if now - active.accepted > 15. {
                self.publish(now, "unverified", "completion_not_observed");
            }
            return (None, false);
        }
        let reason = self.blocked(state, &active.direction);
        if !reason.is_empty() || gap || now - active.accepted > 3. {
            let timeout = if self.signal_id().is_empty() {
                "start_not_observed"
            } else {
                "indicator_not_confirmed"
            };
            self.publish(
                now,
                if reason.is_empty() {
                    "expired"
                } else {
                    "rejected"
                },
                if reason.is_empty() { timeout } else { reason },
            );
            return (None, true);
        }
        self.publish(now, "waiting", "existing_lane_change_conditions");
        (Some(active.direction), false)
    }

    fn indicator_route(&self, now: f64) -> String {
        if self.native_signal {
            return "native".into();
        }
        let available = read_json(&self.root.join("indicator-device.json"));
        if !self.fingerprint.is_empty()
            && available.get("fingerprint").and_then(Value::as_str)
                == Some(self.fingerprint.as_str())
        {
            let stamp = available.get("time").and_then(Value::as_f64);
            let route = available.get("route").and_then(Value::as_str);
            if let (Some(stamp), Some(route)) = (stamp, route) {
                if (0. ..0.6).contains(&(now - stamp)) && identity(route) {
                    return route.into();
                }
            }
        }
        "ordinary".into()
    }

    pub fn signal_route(&self) -> &str {
        self.active.as_ref().map_or("", |a| a.signal_route.as_str())
    }

    pub fn signal_id(&self) -> &str {
        self.active
            .as_ref()
            .filter(|a| a.signal_since.is_some())
            .map_or("", |a| a.id.as_str())
    }

    pub fn own_tap(&mut self, now: f64, car: &Car) -> bool {
        if self.signal_id().is_empty() || car.blinker_lever == 0 {
            return false;
        }
        let value = read_json(&self.root.join("signal.json"));
        let Some(active) = &self.active else {
            return false;
        };
        if value.get("id").and_then(Value::as_str) != Some(active.id.as_str())
            || value.get("route").and_then(Value::as_str) != Some(active.signal_route.as_str())
            || value.get("direction").and_then(Value::as_str) != Some(active.direction.as_str())
        {
            return false;
        }
        let direction = if active.direction == "left" { 1 } else { 2 };
        let feedback = json!({"stamp": value.get("time"), "tap": value.get("tap")});
        self.tap_echo
            .matches(now, car, direction, 0., Some(&feedback))
    }

    pub fn request_signal(&mut self, now: f64, direction: &str, car: &Car) -> bool {
        let Some(active) = &mut self.active else {
            return false;
        };
        if active.direction != direction {
            return false;
        }
        let since = *active.signal_since.get_or_insert(now);
        // An external adapter follows the admitted maneuver; only native
        // output needs vehicle feedback before laneChangeStarting.
        if active.signal_route != "native" {
            return true;
        }
        let feedback = read_json(&self.root.join("signal.json"));
        let stamp = feedback.get("time").and_then(Value::as_f64);
        feedback.get("confirmed") == Some(&Value::Bool(true))
            && feedback.get("id").and_then(Value::as_str) == Some(active.id.as_str())
            && feedback.get("direction").and_then(Value::as_str) == Some(direction)
            && stamp.is_some_and(|s| since <= s && s <= now && now - s < 0.35)
            && feedback.get("route").and_then(Value::as_str) == Some(active.signal_route.as_str())
            && !(if direction == "left" {
                car.right_blinker
            } else {
                car.left_blinker
            })
    }

    pub fn after(&mut self, now: f64, phase: &str, direction: &str, crossed: bool) {
        let Some(active) = &mut self.active else {
            return;
        };
        if matches!(phase, "laneChangeStarting" | "laneChangeFinishing") {
            if direction != active.direction {
                self.publish(now, "interrupted", "different_maneuver");
                return;
            }
            active.began = true;
            active.crossed |= crossed;
            active.finishing |= phase == "laneChangeFinishing";
            self.publish(now, "running", "lane_change_in_progress");
        } else if !self.signal_id().is_empty()
            && !self.active.as_ref().is_some_and(|a| a.began)
            && phase != "preLaneChange"
        {
            self.publish(now, "rejected", "lane_conditions_changed");
        } else if let Some(active) = &self.active {
            if active.began {
                // An FSM reset or timeout alone never proves a crossing.
                let done = active.finishing && active.crossed;
                self.publish(
                    now,
                    if done { "completed" } else { "unverified" },
                    if done { "" } else { "completion_not_observed" },
                );
            }
        }
    }
}
