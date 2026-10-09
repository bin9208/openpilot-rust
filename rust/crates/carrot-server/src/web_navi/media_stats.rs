use super::media::Media;
use crate::Value;
use std::time::Instant;

#[derive(Default)]
pub(super) struct Stats {
    pub map_at: Option<Instant>,
    pub keyframe_at: Option<Instant>,
    config_count: usize,
    frame_count: usize,
    keyframe_count: usize,
    sequence: u64,
    timestamp_ms: u64,
    source_delta: Option<i128>,
    wire_bytes: usize,
}
pub(super) fn age(at: Option<Instant>) -> Value {
    at.map_or(Value::Null, |at| Value::integer(at.elapsed().as_millis()))
}
impl Stats {
    pub fn remember(&mut self, media: &Media) {
        if !media.is_map() {
            return;
        }
        if !media.present || media.message_type == 4 {
            self.map_at = None;
            return;
        }
        if media.message_type == 2 {
            self.config_count += 1;
        }
        if media.message_type != 3 {
            return;
        }
        if self.timestamp_ms != 0 && media.timestamp_ms != 0 {
            self.source_delta =
                Some(i128::from(media.timestamp_ms) - i128::from(self.timestamp_ms));
        }
        self.timestamp_ms = media.timestamp_ms;
        self.sequence = media.sequence;
        self.wire_bytes = media.raw_wire.len();
        self.frame_count += 1;
        if media.keyframe() {
            self.keyframe_count += 1;
            self.keyframe_at = Some(Instant::now());
        }
        self.map_at = Some(Instant::now());
    }
    pub fn status(&self, pipeline: Value, initialized: bool, gop: usize) -> Value {
        Value::object([
            (
                "mapFresh",
                Value::Bool(
                    self.map_at
                        .is_some_and(|at| at.elapsed().as_millis() <= 3000),
                ),
            ),
            ("mapAgeMs", age(self.map_at)),
            (
                "mapStream",
                Value::object([
                    ("configPresent", Value::Bool(initialized)),
                    ("gopFrames", Value::integer(gop)),
                    ("configMessages", Value::integer(self.config_count)),
                    ("frames", Value::integer(self.frame_count)),
                    ("keyframes", Value::integer(self.keyframe_count)),
                    ("lastSequence", Value::integer(self.sequence)),
                    ("lastSourceTimestampMs", Value::integer(self.timestamp_ms)),
                    (
                        "sourceDeltaMs",
                        self.source_delta.map_or(Value::Null, Value::integer),
                    ),
                    ("lastWireBytes", Value::integer(self.wire_bytes)),
                    ("keyframeAgeMs", age(self.keyframe_at)),
                    ("webPipeline", pipeline),
                ]),
            ),
        ])
    }
}
