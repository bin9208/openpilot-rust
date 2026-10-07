use super::Receiver;
use crate::{json::Value, manifest::CLUSTER_NAMES, record::Record};
use num_bigint::BigInt;
use num_traits::Zero;
use std::sync::Arc;

#[derive(Debug)]
pub struct Dashboard {
    pub metadata: Value,
    pub records: Vec<(String, Arc<Record>)>,
    pub binary_configs: Vec<(String, Arc<Record>)>,
}

fn summaries(records: &[(String, Arc<Record>)]) -> Value {
    Value::Object(
        records
            .iter()
            .map(|(key, record)| (key.chars().map(u32::from).collect(), record.summary()))
            .collect(),
    )
}

impl Receiver {
    fn app_foreground(&self) -> Value {
        let record = self
            .records
            .iter()
            .find(|(key, _)| key == "json:app_status");
        match record {
            Some((_, record)) if record.present && matches!(record.value, Value::Object(_)) => {
                match record.value.get("foreground") {
                    Value::Bool(value) => Value::Bool(*value),
                    Value::Null
                    | Value::Integer(_)
                    | Value::Float(_)
                    | Value::Text(_)
                    | Value::Array(_)
                    | Value::Object(_) => Value::Null,
                }
            }
            Some(_) | None => Value::Null,
        }
    }
    pub fn health(&self) -> Value {
        Value::object([
            ("ok", Value::Bool(true)),
            ("service", Value::text("carrot_navi_receiver")),
            ("port", self.port.clone()),
            ("protocol_version", Value::integer(2)),
            ("map_theme", Value::text(&self.config.theme)),
            ("map_type", Value::text(&self.config.map_type)),
            ("map_hz", Value::integer(self.config.hz)),
            ("map_bitrate_kbps", Value::integer(self.config.bitrate_kbps)),
            (
                "screen_center_y_ratio",
                Value::Float(self.config.screen_center_y_ratio),
            ),
            ("app_foreground", self.app_foreground()),
            (
                "control_connected",
                Value::Bool(self.control_connections > BigInt::zero()),
            ),
            (
                "control_connections",
                Value::Integer(self.control_connections.clone()),
            ),
            (
                "session_id",
                self.session_id
                    .as_deref()
                    .map(Value::text)
                    .unwrap_or(Value::Null),
            ),
            ("app_version", Value::text(&self.app_version)),
            (
                "manifest_revision",
                Value::integer(u8::from(self.manifest.is_some())),
            ),
            (
                "received_count",
                Value::Integer(self.received_count.clone()),
            ),
            (
                "session_received_count",
                Value::Integer(self.session_received_count.clone()),
            ),
            (
                "last_received_at_ms",
                Value::integer(self.last_received_at_ms),
            ),
            ("peer", Value::text(&self.last_peer)),
            ("error", self.last_error.clone().unwrap_or(Value::Null)),
            (
                "control_event_count",
                Value::integer(self.control_events.len()),
            ),
            (
                "state_generation",
                Value::Integer(self.state_generation.clone()),
            ),
            (
                "cereal_publish_count",
                Value::Integer(self.cereal_publish_count.clone()),
            ),
            (
                "last_cereal_publish_mono_ns",
                Value::integer(self.last_cereal_publish_mono_ns),
            ),
            (
                "cereal_error",
                self.cereal_error.clone().unwrap_or(Value::Null),
            ),
            ("items", summaries(&self.records)),
        ])
    }
    pub fn latest(&self) -> Value {
        Value::object([
            (
                "session_id",
                self.session_id
                    .as_deref()
                    .map(Value::text)
                    .unwrap_or(Value::Null),
            ),
            (
                "manifest_revision",
                Value::integer(u8::from(self.manifest.is_some())),
            ),
            (
                "received_count",
                Value::Integer(self.received_count.clone()),
            ),
            (
                "session_received_count",
                Value::Integer(self.session_received_count.clone()),
            ),
            (
                "last_received_at_ms",
                Value::integer(self.last_received_at_ms),
            ),
            ("peer", Value::text(&self.last_peer)),
            ("app_foreground", self.app_foreground()),
            ("error", self.last_error.clone().unwrap_or(Value::Null)),
            ("items", summaries(&self.records)),
            (
                "last_control_events",
                Value::Array(
                    self.control_events
                        .iter()
                        .skip(self.control_events.len().saturating_sub(20))
                        .cloned()
                        .collect(),
                ),
            ),
        ])
    }
    pub fn cereal_snapshot(&self) -> Value {
        let mut items = Vec::new();
        for name in CLUSTER_NAMES {
            if let Some((_, record)) = self
                .records
                .iter()
                .find(|(key, _)| key == &format!("json:{name}"))
            {
                items.push((
                    name.chars().map(u32::from).collect(),
                    Value::object([
                        ("present", Value::Bool(record.present)),
                        ("sequence", Value::Integer(record.sequence.clone())),
                        (
                            "source_timestamp_ms",
                            Value::Integer(record.source_timestamp_ms.clone()),
                        ),
                        ("received_mono_ns", Value::integer(record.received_mono_ns)),
                        (
                            "value",
                            if record.present {
                                record.value.clone()
                            } else {
                                Value::Null
                            },
                        ),
                    ]),
                ));
            }
        }
        Value::object([
            ("generation", Value::Integer(self.state_generation.clone())),
            (
                "session_id",
                Value::text(self.session_id.as_deref().unwrap_or("")),
            ),
            (
                "connected",
                Value::Bool(self.control_connections > BigInt::zero() && self.session_id.is_some()),
            ),
            ("items", Value::Object(items)),
        ])
    }
    pub fn dashboard_snapshot(&self) -> Dashboard {
        Dashboard {
            metadata: Value::object([
                (
                    "connected",
                    Value::Bool(
                        self.control_connections > BigInt::zero() && self.session_id.is_some(),
                    ),
                ),
                (
                    "session_id",
                    Value::text(self.session_id.as_deref().unwrap_or("")),
                ),
                ("app_version", Value::text(&self.app_version)),
                (
                    "manifest_revision",
                    Value::integer(u8::from(self.manifest.is_some())),
                ),
                (
                    "received_count",
                    Value::Integer(self.received_count.clone()),
                ),
                (
                    "last_received_at_ms",
                    Value::integer(self.last_received_at_ms),
                ),
                ("peer", Value::text(&self.last_peer)),
                ("error", self.last_error.clone().unwrap_or(Value::Null)),
                (
                    "state_generation",
                    Value::Integer(self.state_generation.clone()),
                ),
                (
                    "media_generation",
                    Value::Integer(self.media_generation.clone()),
                ),
            ]),
            records: self
                .records
                .iter()
                .map(|(key, record)| (key.clone(), Arc::clone(record)))
                .collect(),
            binary_configs: self
                .binary_configs
                .iter()
                .map(|(key, record)| (key.clone(), Arc::clone(record)))
                .collect(),
        }
    }
}
