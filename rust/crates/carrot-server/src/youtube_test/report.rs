use super::{config::Config, storage};
use crate::{Error, Value};

fn number(value: &Value) -> Result<f64, Error> {
    if value.truth() {
        Ok(value.float()?)
    } else {
        Ok(0.0)
    }
}
pub fn diagnose(status: &Value) -> Result<Value, Error> {
    let youtube = status.get("youtube");
    let mut failures = Vec::new();
    let mut warnings: Vec<String> = match youtube.get("warnings") {
        Value::Array(values) => values
            .iter()
            .map(storage::text)
            .filter(|text| !text.trim().is_empty())
            .collect(),
        _ => Vec::new(),
    };
    if !status.get("runner_alive").truth() {
        failures.push("test runner is not running".into());
    }
    if let Value::Object(children) = status.get("children") {
        for (name, child) in children {
            if !child.get("alive").truth() {
                failures.push(format!(
                    "{} is not running",
                    Value::Text(name.clone()).string()?
                ));
            }
        }
    }
    let state = storage::text(youtube.get("state"));
    if state != "live" {
        failures.push(format!(
            "YouTube state is {}",
            if state.is_empty() { "unknown" } else { &state }
        ));
    }
    if !youtube.get("transport_connected").truth() {
        failures.push("RTMPS transport is not connected".into());
    }
    if youtube.get("frame_matches_target") == &Value::Bool(false) {
        failures.push("encoder frame size does not match the selected profile".into());
    }
    let fallback_fps = Value::integer(20);
    let target_fps = storage::integer(if youtube.get("target_fps").truth() {
        youtube.get("target_fps")
    } else if youtube.get("declared_frame_fps").truth() {
        youtube.get("declared_frame_fps")
    } else {
        &fallback_fps
    })?
    .max(1);
    let minimum_fps = target_fps as f64 * 0.75;
    let fps = number(youtube.get("stream_source_recent_fps"))?;
    if fps < minimum_fps {
        failures.push(format!(
            "source rate is {fps:.1} fps (minimum {minimum_fps:.1})"
        ));
    }
    let minimum_kbps =
        ((storage::integer(youtube.get("target_video_kbps"))?.max(1) as f64) * 0.55) as i64;
    let kbps = storage::integer(youtube.get("upload_recent_kbps"))?;
    if kbps < minimum_kbps {
        failures.push(format!(
            "upload rate is {kbps} kbps (minimum {minimum_kbps} kbps)"
        ));
    }
    if storage::integer(youtube.get("rtmp_writer_frames_written"))? <= 0 {
        failures.push("RTMP writer has not published a frame".into());
    }
    let writer_error = storage::text(youtube.get("rtmp_writer_error"));
    if !writer_error.trim().is_empty() {
        failures.push(format!("RTMP writer error: {}", writer_error.trim()));
    }
    let pending = storage::integer(youtube.get("rtmp_writer_pending_frames"))?;
    let capacity = storage::integer(youtube.get("rtmp_writer_capacity"))?.max(1);
    let bytes = storage::integer(youtube.get("rtmp_writer_pending_bytes"))?;
    let limit = storage::integer(youtube.get("rtmp_writer_capacity_bytes"))?.max(1);
    if pending >= capacity || bytes >= limit {
        failures.push("RTMP writer backlog reached its limit".into());
    } else if pending as f64 >= capacity as f64 * 0.8 || bytes as f64 >= limit as f64 * 0.8 {
        warnings.push("RTMP writer backlog is above 80%".into());
    }
    if youtube.get("last_frame_age_ms") != &Value::Null
        && storage::integer(youtube.get("last_frame_age_ms"))? > 2000
    {
        failures.push(format!(
            "last source frame is {} ms old",
            storage::integer(youtube.get("last_frame_age_ms"))?
        ));
    }
    let error = storage::text(if status.get("error").truth() {
        status.get("error")
    } else {
        youtube.get("last_error")
    });
    if !error.trim().is_empty() {
        failures.push(error.trim().into());
    }
    let unique = |values: Vec<String>| {
        let mut result = Vec::new();
        for value in values {
            if !result.contains(&Value::text(&value)) {
                result.push(Value::text(&value));
            }
        }
        result
    };
    let healthy = failures.is_empty();
    Ok(Value::object([
        ("healthy", Value::Bool(healthy)),
        (
            "verdict",
            Value::text(if healthy { "pass" } else { "fail" }),
        ),
        ("failures", Value::Array(unique(failures))),
        ("warnings", Value::Array(unique(warnings))),
        (
            "thresholds",
            Value::object([
                (
                    "minimum_source_fps",
                    Value::Float(format!("{minimum_fps:.1}").parse().unwrap_or(minimum_fps)),
                ),
                ("minimum_upload_kbps", Value::integer(minimum_kbps)),
            ]),
        ),
    ]))
}
const FIELDS: &[&str] = &[
    "state",
    "quality",
    "target_width",
    "target_height",
    "target_fps",
    "target_video_kbps",
    "frame_width",
    "frame_height",
    "frame_matches_target",
    "stream_source_recent_fps",
    "stream_source_recent_kbps",
    "stream_source_keyframes",
    "upload_recent_kbps",
    "estimated_kbps",
    "transport_connected",
    "rtmp_writer_pending_frames",
    "rtmp_writer_capacity",
    "rtmp_writer_high_watermark",
    "rtmp_writer_pending_bytes",
    "rtmp_writer_capacity_bytes",
    "rtmp_writer_high_watermark_bytes",
    "rtmp_writer_frames_written",
    "rtmp_writer_last_write_ms",
    "rtmp_writer_max_write_ms",
    "rtmp_writer_error",
    "rtmp_writer_backpressure_restarts",
    "rtmp_writer_discarded_frames",
    "restart_count",
    "consecutive_failures",
    "retry_in_sec",
    "last_frame_age_ms",
    "last_error",
    "warnings",
    "log_tail",
];
pub fn compact(config: &Config, status: &Value) -> Result<Value, Error> {
    let fields = FIELDS
        .iter()
        .map(|name| {
            (
                name.chars().map(u32::from).collect(),
                status.get("youtube").get(name).clone(),
            )
        })
        .collect();
    Ok(Value::object([
        ("version", Value::integer(1)),
        (
            "captured_at",
            Value::Float(chrono::Utc::now().timestamp_millis() as f64 / 1000.0),
        ),
        ("diagnosis", diagnose(status)?),
        (
            "test",
            Value::object([
                ("status", status.get("status").clone()),
                (
                    "runner_alive",
                    Value::Bool(status.get("runner_alive").truth()),
                ),
                (
                    "quality",
                    if status.get("quality_label").truth() {
                        status.get("quality_label").clone()
                    } else {
                        status.get("quality").clone()
                    },
                ),
                ("children", status.get("children").clone()),
                ("vipc_streams", status.get("vipc_streams").clone()),
                ("device", status.get("device").clone()),
            ]),
        ),
        ("youtube", Value::Object(fields)),
        (
            "runner_log_tail",
            Value::Array(
                storage::tail(config, 40)
                    .iter()
                    .map(|value| Value::text(value))
                    .collect(),
            ),
        ),
    ]))
}
