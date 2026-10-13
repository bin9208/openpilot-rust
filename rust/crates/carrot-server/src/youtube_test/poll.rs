use crate::{
    youtube_live::{h264, profiles},
    Error, Value,
};
use capnp::dynamic_value::Reader;
use openpilot_carrot_state::value::{data, field, integer};
use openpilot_msgq::Subscriber;
use std::time::Duration;

pub(super) fn subscribe() -> Result<Subscriber, Error> {
    let capacity = openpilot_messaging::services::lookup(profiles::SOURCE)
        .map_or(1024 * 1024, |service| service.queue_size);
    Subscriber::for_runtime(profiles::SOURCE, false, capacity)
        .map_err(|error| Error::Source(error.to_string()))
}
pub(super) async fn vipc(timeout: Duration) -> Vec<i32> {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let streams = super::status::streams();
        if !streams.is_empty() {
            return streams;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Vec::new()
}
pub(super) async fn keyframe(socket: &mut Subscriber, timeout: Duration) -> Result<Value, Error> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut frames = 0;
    while tokio::time::Instant::now() < deadline {
        let Some(bytes) = socket
            .receive(Duration::ZERO)
            .map_err(|error| Error::Source(error.to_string()))?
        else {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        };
        let message = openpilot_carrot_state::value::message(&bytes)
            .map_err(|error| Error::Source(error.to_string()))?;
        let root: openpilot_cereal::log_capnp::event::Reader<'_> = message
            .get_root()
            .map_err(|error| Error::Source(error.to_string()))?;
        let Reader::Struct(root) = root.into() else {
            continue;
        };
        let Some(selected) = root
            .which()
            .map_err(|error| Error::Source(error.to_string()))?
        else {
            continue;
        };
        let frame = root
            .get(selected)
            .map_err(|error| Error::Source(error.to_string()))?;
        let header = data(field(frame, "header"));
        let payload = data(field(frame, "data"));
        if payload.is_empty() {
            continue;
        }
        frames += 1;
        if h264::validate_start(&header, &payload).is_err() {
            continue;
        }
        return Ok(Value::object([
            ("frames", Value::integer(frames)),
            ("header_bytes", Value::integer(header.len())),
            ("frame_bytes", Value::integer(payload.len())),
            ("width", Value::integer(integer(field(frame, "width")))),
            ("height", Value::integer(integer(field(frame, "height")))),
        ]));
    }
    Ok(Value::Object(Vec::new()))
}
