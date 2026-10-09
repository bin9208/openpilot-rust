use super::{
    clock::round,
    sink::Snapshot,
    writer_state::{Stats, BYTES, FRAMES},
};
use crate::Value;
use num_traits::ToPrimitive;
pub(super) fn fields(
    state: &super::state::State,
    snapshot: (Stats, Snapshot),
    stream: (i128, f64),
) -> Vec<(&'static str, Value)> {
    let (stats, sink) = snapshot;
    let bitrate = |bytes: u64| {
        Value::integer(
            (bytes.to_f64().unwrap_or(0.0) * 8.0 / 1000.0 / stream.1)
                .to_i128()
                .unwrap_or(0),
        )
    };
    let session = stream.0;
    Vec::from([
        ("mux_input_bytes", Value::integer(sink.bytes_accepted)),
        ("mux_input_kbps", bitrate(sink.bytes_accepted)),
        ("mux_pending_bytes", Value::integer(sink.pending_bytes)),
        ("rtmp_drain_calls", Value::integer(sink.drain_calls)),
        ("rtmp_partial_writes", Value::integer(sink.partial_writes)),
        (
            "rtmp_write_ratio",
            if sink.bytes_accepted > 0 {
                Value::Float(round(
                    session.to_f64().unwrap_or(0.0) / sink.bytes_accepted.to_f64().unwrap_or(1.0),
                    3,
                ))
            } else {
                Value::Null
            },
        ),
        (
            "rtmp_writer_pending_frames",
            Value::integer(stats.pending_frames),
        ),
        ("rtmp_writer_capacity", Value::integer(FRAMES)),
        (
            "rtmp_writer_high_watermark",
            Value::integer(stats.high_frames),
        ),
        (
            "rtmp_writer_pending_bytes",
            Value::integer(stats.pending_bytes),
        ),
        ("rtmp_writer_capacity_bytes", Value::integer(BYTES)),
        (
            "rtmp_writer_high_watermark_bytes",
            Value::integer(stats.high_bytes),
        ),
        (
            "rtmp_writer_frames_written",
            Value::integer(stats.frames_written),
        ),
        (
            "rtmp_writer_last_write_ms",
            Value::integer(stats.last_write_ms),
        ),
        (
            "rtmp_writer_max_write_ms",
            Value::integer(stats.max_write_ms),
        ),
        ("rtmp_writer_error", Value::text(&stats.error)),
        (
            "rtmp_writer_backpressure_restarts",
            Value::integer(state.backlog_restarts),
        ),
        (
            "rtmp_writer_discarded_frames",
            Value::integer(state.discarded),
        ),
    ])
}
