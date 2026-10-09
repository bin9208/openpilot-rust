use super::{
    clock::{round, Stamp},
    profiles,
    state::State,
};
use crate::Value;
use num_traits::ToPrimitive;

pub(super) fn status(state: &mut State, now: Stamp) -> Value {
    let key = state.keys.get();
    let running = state.connected;
    let elapsed = if state.started.mono != 0.0 && running {
        (now.mono - state.started.mono).max(1.0)
    } else {
        1.0
    };
    let session = if running {
        state
            .bytes_sent
            .saturating_sub(state.session_start_bytes)
            .max(0)
    } else {
        0
    };
    let stats = if running {
        state
            .writer
            .as_ref()
            .and_then(|writer| writer.snapshot().ok())
            .unwrap_or_default()
    } else {
        Default::default()
    };
    let sink = if running {
        state
            .writer
            .as_ref()
            .and_then(|writer| writer.sink_snapshot().ok())
            .unwrap_or_default()
    } else {
        Default::default()
    };
    let recent = if running {
        state.samples.recent(now.mono)
    } else {
        (0, 0.0)
    };
    let target = state.profile(now.mono);
    let resource = state.resources.status(&mut state.settings, now.mono);
    let warnings = super::warnings::warnings(state, &resource, target, now.mono);
    let integer = |n: f64| Value::integer(n.to_i128().unwrap_or(0));
    let bitrate = |bytes: i128| integer(bytes.to_f64().unwrap_or(0.0) * 8.0 / 1000.0 / elapsed);
    let megabytes =
        |bytes: i128| Value::Float(round(bytes.to_f64().unwrap_or(0.0) / (1024.0 * 1024.0), 2));
    let retry = if state.next_retry != 0.0 {
        (state.next_retry - now.mono).to_i128().unwrap_or(0).max(0)
    } else {
        0
    };
    let warmup = state.warmup.started != 0.0 && !running;
    let mut fields = Vec::from([
        ("state", Value::text(state.state)),
        ("enabled", Value::Bool(state.enabled(now.mono))),
        ("configured", Value::Bool(key.configured())),
        ("masked_key", key.mask()),
        ("source", Value::text(profiles::SOURCE)),
        ("muxer", Value::text("flv-h264-aac")),
        (
            "muxer_available",
            Value::Bool(
                state.capabilities.muxer.get("available").truth()
                    && state.capabilities.muxer.get("h264").truth()
                    && state.capabilities.muxer.get("aac").truth(),
            ),
        ),
        ("transport", Value::text("librtmp-rtmps")),
        (
            "transport_available",
            state.capabilities.transport.get("available").clone(),
        ),
        ("transport_connected", Value::Bool(running)),
        ("quality", Value::text(target.label)),
        (
            "requested_quality",
            Value::integer(state.settings.integer("CarrotYouTubeQuality", now.mono)),
        ),
        ("target_width", Value::integer(target.width)),
        ("target_height", Value::integer(target.height)),
        ("target_fps", Value::integer(profiles::FPS)),
        ("target_video_kbps", Value::integer(target.video_kbps)),
        ("target_gop_seconds", Value::integer(2)),
        (
            "timestamp_caption_enabled",
            Value::Bool(state.settings.boolean("CarrotYouTubeTimestamp", now.mono)),
        ),
        ("timestamp_caption_mode", Value::text("cea608-sei")),
        (
            "timestamp_caption_packets",
            Value::integer(state.captions.packets),
        ),
        ("phase", Value::integer(1)),
        ("running", Value::Bool(running)),
        ("pid", Value::Null),
        (
            "started_at",
            Value::Float(if running { state.started.wall } else { 0.0 }),
        ),
        (
            "uptime_sec",
            integer(if state.started.mono != 0.0 && running {
                now.mono - state.started.mono
            } else {
                0.0
            }),
        ),
        ("bytes_sent", Value::integer(state.bytes_sent)),
        ("total_mb", megabytes(state.bytes_sent.max(0))),
        ("session_bytes", Value::integer(session)),
        ("session_mb", megabytes(session)),
        ("estimated_kbps", bitrate(session)),
        (
            "upload_recent_kbps",
            Value::integer(if running {
                state
                    .samples
                    .upload_recent(now.mono, state.bytes_sent)
                    .unwrap_or(0)
            } else {
                0
            }),
        ),
    ]);
    fields.extend(super::status_writer::fields(
        state,
        (stats, sink),
        (session, elapsed),
    ));
    fields.extend([
        (
            "stream_source_kbps",
            bitrate(if running {
                i128::from(state.samples.bytes)
            } else {
                0
            }),
        ),
        (
            "stream_source_fps",
            Value::Float(if running {
                round(state.samples.frames.to_f64().unwrap_or(0.0) / elapsed, 1)
            } else {
                0.0
            }),
        ),
        (
            "stream_source_frames",
            Value::integer(if running { state.samples.frames } else { 0 }),
        ),
        (
            "stream_source_keyframes",
            Value::integer(if running { state.samples.keyframes } else { 0 }),
        ),
        ("stream_source_recent_kbps", Value::integer(recent.0)),
        ("stream_source_recent_fps", Value::Float(recent.1)),
        ("restart_count", Value::integer(state.restart_count)),
        ("consecutive_failures", Value::integer(state.failures)),
        (
            "next_retry_at",
            Value::Float(if retry > 0 {
                now.wall + retry.to_f64().unwrap_or(0.0)
            } else {
                0.0
            }),
        ),
        ("retry_in_sec", Value::integer(retry)),
        ("last_error", Value::text(&state.error)),
        ("last_frame_at", Value::Float(state.last_frame.wall)),
        (
            "last_frame_age_ms",
            if state.last_frame.mono != 0.0 {
                integer((now.mono - state.last_frame.mono) * 1000.0)
            } else {
                Value::Null
            },
        ),
        (
            "last_frame_id",
            state.frame_id.map_or(Value::Null, Value::integer),
        ),
        ("frame_width", Value::integer(state.width)),
        ("frame_height", Value::integer(state.height)),
        ("frame_fps", Value::integer(profiles::FPS)),
        ("declared_frame_fps", Value::integer(profiles::FPS)),
        ("observed_frame_fps", Value::Float(recent.1)),
        (
            "frame_matches_target",
            Value::Bool(
                state.width == u32::from(target.width) && state.height == u32::from(target.height),
            ),
        ),
        (
            "source_warmup_sec",
            Value::Float(if warmup {
                round(now.mono - state.warmup.started, 1)
            } else {
                0.0
            }),
        ),
        (
            "source_warmup_frames",
            Value::integer(if !running { state.warmup.frames } else { 0 }),
        ),
        (
            "source_warmup_kbps",
            integer(if !running { state.warmup.kbps } else { 0.0 }),
        ),
        (
            "source_warmup_required_kbps",
            integer(f64::from(target.video_kbps) * 0.55),
        ),
        ("source_warmup_required_sec", Value::Float(6.0)),
        ("log_tail", state.log_tail()),
        ("resource_status", resource),
        ("warnings", warnings),
    ]);
    Value::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.chars().map(u32::from).collect(), value))
            .collect(),
    )
}
