use super::value;
use crate::{Error, Value};
use capnp::dynamic_value::Reader;
use std::time::{SystemTime, UNIX_EPOCH};

fn optional(value: Reader<'_>) -> Value {
    if matches!(value, Reader::Void) {
        Value::Null
    } else {
        Value::integer(value::integer(value))
    }
}
fn nal(payload: &[u8], needed: usize, wanted: u8) -> Option<usize> {
    let mut index = 0;
    while index + needed < payload.len() {
        if payload[index] != 0 || payload[index + 1] != 0 {
            index += 1;
            continue;
        }
        let start = if payload[index + 2] == 1 {
            index + 3
        } else if payload[index + 2] == 0 && payload[index + 3] == 1 {
            index + 4
        } else {
            index += 1;
            continue;
        };
        if start < payload.len() && payload[start] & 0x1f == wanted {
            return Some(start);
        }
        index = start;
    }
    None
}
pub(super) fn pack(
    frame: Reader<'_>,
    codec: &mut String,
    ready_id: i128,
) -> Result<Vec<u8>, Error> {
    let header = value::data(value::field(frame, "header"));
    let data = value::data(value::field(frame, "data"));
    let size = header.len() + data.len();
    if codec.is_empty() {
        let scan = if size < 4096 {
            let mut scan = header.clone();
            scan.extend(&data);
            scan
        } else {
            header.iter().take(256).copied().collect()
        };
        if let Some(start) = nal(&scan, 6, 7).filter(|start| start + 3 < scan.len()) {
            *codec = format!(
                "avc1.{:02X}{:02X}{:02X}",
                scan[start + 1],
                scan[start + 2],
                scan[start + 3]
            );
        }
    }
    let idx = value::field(frame, "idx");
    let flags = value::field(idx, "flags");
    let flag_int = value::integer(flags);
    let key = flag_int & 8 != 0
        || (size > 0
            && nal(
                if header.is_empty() {
                    &data[..data.len().min(64)]
                } else {
                    &header
                },
                5,
                5,
            )
            .is_some());
    let id = value::field(frame, "frameId");
    let id = if matches!(id, Reader::Void) {
        value::field(idx, "frameId")
    } else {
        id
    };
    let id = if matches!(id, Reader::Void) || value::integer(id) < 0 {
        ready_id
    } else {
        value::integer(id)
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let meta = Value::object([
        ("camera", Value::text("road")),
        (
            "codec",
            Value::text(if codec.is_empty() {
                "avc1.640028"
            } else {
                codec
            }),
        ),
        ("frameId", Value::integer(id)),
        ("width", optional(value::field(frame, "width"))),
        ("height", optional(value::field(frame, "height"))),
        ("flags", optional(flags)),
        ("encodeId", optional(value::field(idx, "encodeId"))),
        ("segmentId", optional(value::field(idx, "segmentId"))),
        (
            "frameType",
            if matches!(idx, Reader::Void) {
                Value::Null
            } else {
                Value::text(&value::text(value::field(idx, "type")))
            },
        ),
        (
            "timestampSof",
            optional(value::field(frame, "timestampSof")),
        ),
        (
            "timestampEof",
            optional(value::field(frame, "timestampEof")),
        ),
        ("keyFrame", Value::Bool(key)),
        ("size", Value::integer(size)),
        ("ts", Value::Float(now)),
    ]);
    let encoded = crate::state_json::compact_encoded(&meta.encode()?);
    let encoded = encoded.as_bytes();
    let mut packet = Vec::with_capacity(4 + encoded.len() + size);
    packet.extend(
        u32::try_from(encoded.len())
            .map_err(|_| Error::Source("camera metadata too large".into()))?
            .to_be_bytes(),
    );
    packet.extend(encoded);
    packet.extend(header);
    packet.extend(data);
    Ok(packet)
}
