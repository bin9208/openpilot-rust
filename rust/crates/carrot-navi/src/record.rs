use crate::json::Value;
use num_bigint::BigInt;
use std::sync::Arc;

pub trait Clock {
    fn wall_ms(&mut self) -> i128;
    fn mono_ns(&mut self) -> u128;
}

#[derive(Debug, Clone)]
pub struct Record {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) schema_version: BigInt,
    pub(crate) stream_handle: BigInt,
    pub(crate) manifest_revision: BigInt,
    pub(crate) sequence: BigInt,
    pub(crate) source_timestamp_ms: BigInt,
    pub(crate) sent_at_ms: Option<BigInt>,
    pub(crate) present: bool,
    pub(crate) value: Value,
    pub(crate) payload: Option<Arc<[u8]>>,
    pub(crate) message_type: Option<BigInt>,
    pub(crate) format_or_reason: Option<BigInt>,
    pub(crate) flags: BigInt,
    pub(crate) width: BigInt,
    pub(crate) height: BigInt,
    pub(crate) reason: Option<Value>,
    pub(crate) peer: String,
    pub(crate) received_at_ms: i128,
    pub(crate) received_mono_ns: u128,
}

impl Record {
    pub fn payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }
    pub const fn received_mono_ns(&self) -> u128 {
        self.received_mono_ns
    }
    pub fn summary(&self) -> Value {
        let mut fields = vec![
            ("kind", Value::text(&self.kind)),
            ("name", Value::text(&self.name)),
            (
                "schema_version",
                Value::Integer(self.schema_version.clone()),
            ),
            ("stream_handle", Value::Integer(self.stream_handle.clone())),
            (
                "manifest_revision",
                Value::Integer(self.manifest_revision.clone()),
            ),
            ("sequence", Value::Integer(self.sequence.clone())),
            (
                "source_timestamp_ms",
                Value::Integer(self.source_timestamp_ms.clone()),
            ),
            ("present", Value::Bool(self.present)),
            ("reason", self.reason.clone().unwrap_or(Value::Null)),
            ("peer", Value::text(&self.peer)),
            ("received_at_ms", Value::integer(self.received_at_ms)),
            (
                "bytes",
                Value::integer(self.payload.as_deref().map_or(0, <[u8]>::len)),
            ),
        ];
        if self.kind == "json" {
            fields.push((
                "sent_at_ms",
                self.sent_at_ms
                    .clone()
                    .map(Value::Integer)
                    .unwrap_or(Value::Null),
            ));
            fields.push(("value", self.value.clone()));
        } else {
            fields.extend([
                (
                    "message_type",
                    self.message_type
                        .clone()
                        .map(Value::Integer)
                        .unwrap_or(Value::Null),
                ),
                (
                    "format_or_reason",
                    self.format_or_reason
                        .clone()
                        .map(Value::Integer)
                        .unwrap_or(Value::Null),
                ),
                ("flags", Value::Integer(self.flags.clone())),
                ("width", Value::Integer(self.width.clone())),
                ("height", Value::Integer(self.height.clone())),
            ]);
        }
        Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                .collect(),
        )
    }
}
