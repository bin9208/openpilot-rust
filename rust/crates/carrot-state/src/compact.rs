//! CVS1/CVB1 display fields from `carrot/realtime/compact_state.py`.
use super::{compact_schema::SERVICES, value};
use crate::Error;
pub fn services() -> impl Iterator<Item = &'static str> {
    SERVICES.iter().map(|(name, _, _)| *name)
}
/// Encodes one original CVS1 frame.
///
/// # Errors
/// Returns Cereal or source packing failures.
pub fn encode(service: &str, bytes: &[u8], sequence: u16) -> Result<Vec<u8>, Error> {
    let schema = schema(service)?;
    let message = value::message(bytes)?;
    let value = value::service(&message, service)?;
    encoded(schema, value, sequence)
}

/// Encodes a borrowed service using the original field schema.
///
/// # Errors
/// Returns unknown-service or source packing failures.
pub fn encode_reader(
    service: &str,
    value: capnp::dynamic_value::Reader<'_>,
    sequence: u16,
) -> Result<Vec<u8>, Error> {
    encoded(schema(service)?, value, sequence)
}

type Schema = (&'static str, u8, &'static [super::compact_fields::Field]);

fn schema(service: &str) -> Result<&'static Schema, Error> {
    SERVICES
        .iter()
        .find(|(name, _, _)| *name == service)
        .ok_or_else(|| Error::Source("unknown compact service".into()))
}

fn encoded(
    schema: &Schema,
    value: capnp::dynamic_value::Reader<'_>,
    sequence: u16,
) -> Result<Vec<u8>, Error> {
    let (_, id, schema) = schema;
    let mut out = Vec::new();
    out.extend(b"CVS1");
    out.extend([*id, 0]);
    out.extend(sequence.to_le_bytes());
    super::compact_fields::fields(&mut out, value, schema)?;
    Ok(out)
}
pub fn batch(frames: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let frames: Vec<_> = frames
        .into_iter()
        .filter(|frame| !frame.is_empty())
        .collect();
    let mut out = Vec::new();
    out.extend(b"CVB1");
    out.extend(u16::try_from(frames.len()).unwrap_or(0).to_le_bytes());
    for frame in frames {
        out.extend(u32::try_from(frame.len()).unwrap_or(0).to_le_bytes());
        out.extend(frame);
    }
    out
}
