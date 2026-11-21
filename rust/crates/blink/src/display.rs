//! Observes decisions after policy evaluation; never part of actuation.
//! Port of carrot's `tools/jetlink/blink_display.py` Publisher and
//! `common/indicator_display.py` `clean`.
use super::{observed::Meta, policy::Hold, Side};
use openpilot_desire::types::Car;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const PATH: &str = "/dev/shm/carrot-indicator-display.json";
pub const MAX_AGE: f64 = 0.6;
pub const ORIGINS: [&str; 9] = [
    "turn",
    "lane",
    "tbt",
    "manual",
    "manual_turn",
    "model_path",
    "model_desire",
    "model_turn",
    "bluetooth",
];
pub const REASONS: [&str; 35] = [
    "confirming",
    "approach",
    "turning",
    "boundary",
    "waiting_conditions",
    "front",
    "bsd",
    "admission",
    "inputs",
    "peer",
    "completed",
    "returned",
    "geometry",
    "relabel",
    "driver_opposite",
    "driver",
    "driver_override",
    "route_or_distance",
    "route",
    "timeout",
    "cancelled",
    "ended",
    "disabled",
    "start",
    "atc",
    "turn_started",
    "maneuver",
    "interrupted",
    "route_jump",
    "invalid",
    "active",
    "waiting",
    "changing",
    "finishing",
    "trigger",
];
const STATES: [&str; 4] = ["prepare", "request", "confirmed", "ended"];

/// `indicator_display.clean`: a bounded, validated display record.
pub fn clean(value: &Value, now: f64) -> Option<Value> {
    let stamp = value.get("stamp")?.as_f64()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    if !stamp.is_finite()
        || !(0. ..MAX_AGE).contains(&(now - stamp))
        || value.get("version").and_then(Value::as_i64) != Some(1)
        || !STATES.contains(&text("state")?)
        || !ORIGINS.contains(&text("origin")?)
        || !matches!(text("direction")?, "left" | "right")
        || !REASONS.contains(&text("reason")?)
        || !matches!(text("lamp")?, "left" | "right" | "both" | "off" | "unknown")
    {
        return None;
    }
    let mut out = serde_json::Map::new();
    for key in [
        "version",
        "stamp",
        "state",
        "origin",
        "direction",
        "reason",
        "lamp",
    ] {
        out.insert(key.into(), value[key].clone());
    }
    Some(Value::Object(out))
}

pub struct DisplayInput<'a> {
    pub now: f64,
    pub mode: i32,
    pub ok: bool,
    pub car: &'a Car,
    pub meta: Option<&'a Meta>,
    pub hold: Option<&'a Hold>,
    pub activated: bool,
    /// Peer state, `"missing"` when stale.
    pub peer: &'a str,
    pub automatic_reason: &'static str,
    pub tbt_phase: &'static str,
    pub tbt_reason: &'static str,
    pub tbt_enabled: bool,
    pub front_clear: bool,
    pub observed_source: &'a str,
    pub auto_valid: bool,
    pub atc_type: &'a str,
}

pub struct Publisher {
    path: PathBuf,
    next_write: f64,
    previous: Option<Value>,
    ended: Option<Value>,
    ended_at: f64,
}

impl Default for Publisher {
    fn default() -> Self {
        Self {
            path: PATH.into(),
            next_write: 0.,
            previous: None,
            ended: None,
            ended_at: 0.,
        }
    }
}

const PREPARE_SOURCES: [&str; 5] = [
    "manual",
    "manual_turn",
    "model_path",
    "model_turn",
    "model_desire",
];

impl Publisher {
    pub fn update(&mut self, i: DisplayInput<'_>) {
        let mut value = None;
        if i.mode != 0 {
            if let Some(hold) = i.hold {
                let tbt_active = matches!(i.tbt_phase, "waiting" | "changing" | "finishing");
                let origin = match hold.source.as_str() {
                    "observation" => i.observed_source,
                    "tbt_display" => "tbt",
                    "lane" if tbt_active => "tbt",
                    other => other,
                };
                let reason = match origin {
                    "turn" => i.automatic_reason,
                    "tbt" => i.tbt_reason,
                    "manual_turn" => "turning",
                    _ => "active",
                };
                value = Some(json!({"origin": origin, "direction": hold.direction.name(),
                    "state": if i.activated { "confirmed" } else { "request" },
                    "reason": if i.activated || i.peer == "ready" { reason } else { "peer" }}));
            } else if let (true, Some(meta)) = (i.ok, i.meta) {
                let direction = match meta.prepare_direction {
                    1 => Some(Side::Left),
                    2 => Some(Side::Right),
                    _ => None,
                };
                let observed = match meta.observed_indicator {
                    1 => Some(Side::Left),
                    2 => Some(Side::Right),
                    _ => None,
                };
                if let Some(d) =
                    direction.filter(|_| PREPARE_SOURCES.contains(&meta.prepare_source.as_str()))
                {
                    value = Some(json!({"origin": meta.prepare_source, "direction": d.name(),
                        "state": "prepare",
                        "reason": if meta.prepare_source == "manual" { "boundary" } else { "confirming" }}));
                } else if let Some(d) =
                    observed.filter(|_| PREPARE_SOURCES.contains(&i.observed_source))
                {
                    value = Some(json!({"origin": i.observed_source, "direction": d.name(),
                        "state": "prepare",
                        "reason": if i.peer != "ready" { "peer" } else { "admission" }}));
                } else if i.auto_valid
                    && meta.lane_change_state == "preLaneChange"
                    && matches!(
                        i.atc_type,
                        "atc left" | "atc right" | "fork left" | "fork right"
                    )
                {
                    let side = i.atc_type.rsplit(' ').next().unwrap_or("right");
                    value = Some(
                        json!({"origin": "lane", "direction": side, "state": "prepare",
                        "reason": "waiting_conditions"}),
                    );
                } else if i.tbt_enabled
                    && matches!(i.tbt_phase, "idle" | "cooldown")
                    && matches!(i.tbt_reason, "confirming" | "waiting_conditions")
                {
                    let mut reason = i.tbt_reason;
                    if reason == "waiting_conditions" {
                        if i.car.right_blindspot {
                            reason = "bsd";
                        } else if !i.front_clear {
                            reason = "front";
                        }
                    }
                    value = Some(
                        json!({"origin": "tbt", "direction": "right", "state": "prepare",
                        "reason": reason}),
                    );
                }
            }
        }
        if value.is_some() {
            self.previous.clone_from(&value);
            self.ended = None;
        } else if let Some(previous) = self.previous.take() {
            let origin = previous["origin"].as_str().unwrap_or("");
            let mut reason = match origin {
                "turn" => i.automatic_reason,
                "tbt" => i.tbt_reason,
                _ => i.observed_source,
            };
            if i.mode == 0 {
                reason = "disabled";
            } else if !i.ok {
                reason = "inputs";
            } else if i.peer == "driver_override" {
                reason = "driver_override";
            }
            let reason = if REASONS.contains(&reason) {
                reason
            } else {
                "ended"
            };
            let mut ended = previous;
            ended["state"] = json!("ended");
            ended["reason"] = json!(reason);
            self.ended = Some(ended);
            self.ended_at = i.now;
        }
        if value.is_none() && i.now - self.ended_at < 2. {
            value.clone_from(&self.ended);
        }
        if i.now < self.next_write {
            return;
        }
        self.next_write = i.now + 0.2;
        let lamp = if !i.ok {
            "unknown"
        } else {
            match (i.car.left_blinker, i.car.right_blinker) {
                (true, true) => "both",
                (true, false) => "left",
                (false, true) => "right",
                _ => "off",
            }
        };
        let record = value.and_then(|mut v| {
            v["version"] = json!(1);
            v["stamp"] = json!(i.now);
            v["lamp"] = json!(lamp);
            clean(&v, i.now)
        });
        // Display metadata only: failures never change commands or leases.
        let temporary = self.path.with_extension("tmp");
        let text = record.map_or_else(|| "null".to_owned(), |v| v.to_string());
        if std::fs::write(&temporary, text).is_ok() {
            let _ = std::fs::rename(&temporary, &self.path);
        }
    }
}
