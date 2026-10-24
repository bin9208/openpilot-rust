//! Independent lagd.main translation with source SubMaster/Params boundaries.
//! No candidate estimator is linked. Rows retain the source's pre-update snapshot
//! timing, latest-per-topic inputs and stable timestamp ordering.
use crate::{lagd_frames as frames, lagd_packet_reference as packet, seal::Result};
use capnp::{dynamic_value, message::ReaderOptions};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};
use openpilot_log_format::{JsonValue, JsonView};
use std::collections::HashMap;

mod estimator;
use estimator::Estimator;

fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(rows) => Ok(rows),
        JsonView::Object(rows) if rows.is_empty() => Ok(Vec::new()),
        JsonView::Text(_) if value.to_utf8().is_some_and(|value| value.is_empty()) => {
            Ok(Vec::new())
        }
        _ => Err("TypeError: expected frame sequence".into()),
    }
}
fn node(value: impl std::fmt::Display) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
pub(super) fn structure(
    value: dynamic_value::Reader<'_>,
) -> Result<capnp::dynamic_struct::Reader<'_>> {
    match value {
        dynamic_value::Reader::Struct(value) => Ok(value),
        _ => Err("expected cereal struct".into()),
    }
}
pub(super) fn real(value: dynamic_value::Reader<'_>) -> Result<f64> {
    use dynamic_value::Reader as D;
    match value {
        D::Float32(value) => Ok(f64::from(value)),
        D::Float64(value) => Ok(value),
        _ => Err("expected cereal floating field".into()),
    }
}
pub(super) fn flag(value: dynamic_value::Reader<'_>) -> Result<bool> {
    if let dynamic_value::Reader::Bool(value) = value {
        Ok(value)
    } else {
        Err("expected cereal boolean field".into())
    }
}
struct Input {
    bytes: Vec<u8>,
    stamp: u64,
    valid: bool,
}
fn header(bytes: &[u8]) -> Result<(String, u64, bool)> {
    let message = capnp::serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
    let event = message.get_root::<event::Reader<'_>>()?;
    let value = structure(event.into())?;
    let topic = value.which()?.ok_or("unknown Event union discriminant")?;
    // Match getattr(event, event.which()).as_builder(): validate access before
    // storing the row, even if all_checks will subsequently be false.
    structure(value.get(topic)?)?;
    Ok((
        topic.get_proto().get_name()?.to_str()?.to_owned(),
        event.get_log_mono_time(),
        event.get_valid(),
    ))
}

pub fn run(input: &JsonValue, configuration: &JsonValue) -> Result<JsonValue> {
    // Original comprehension converts all three byte arguments before reading CP.
    let car = frames::optional_bytes(&frames::field(configuration, "car")?)?;
    let saved = frames::optional_bytes(&frames::field(configuration, "saved")?)?;
    let previous = frames::optional_bytes(&frames::field(configuration, "previous")?)?;
    let car = car
        .filter(|bytes| !bytes.is_empty())
        .ok_or("missing CarParams")?;
    let car_message = capnp::serialize::read_message(&mut &car[..], ReaderOptions::new())?;
    let cp = car_message.get_root::<car_params::Reader<'_>>()?;
    let initial = f64::from(cp.get_steer_actuator_delay()) + 0.2;
    let restored = packet::restore(
        &car,
        saved.as_deref().filter(|bytes| !bytes.is_empty()),
        previous.as_deref().filter(|bytes| !bytes.is_empty()),
    )?;
    let seed = frames::field(&restored, "seed")?;
    let removed = saved.as_ref().is_some_and(|value| !value.is_empty())
        && matches!(
            frames::field(&restored, "remaining")?.view(),
            JsonView::Null
        );
    let mut estimator = Estimator::new(initial);
    if let JsonView::Array(seed) = seed.view() {
        estimator.reset(estimator::json_real(&seed[0])?, frames::integer(&seed[1])?);
    }
    let mut topics = frames::TOPICS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let mut latest = HashMap::<String, Input>::new();
    let mut output = Vec::new();
    for (index, row) in sequence(input)?.iter().enumerate() {
        let mut updated = frames::TOPICS
            .iter()
            .map(|name| ((*name).to_owned(), false))
            .collect::<Vec<_>>();
        for data in sequence(&frames::field(row, "messages")?)? {
            let bytes = frames::bytes(&data)?;
            let (topic, stamp, valid) = header(&bytes)?;
            if let Some((_, seen)) = updated.iter_mut().find(|(name, _)| name == &topic) {
                *seen = true;
            } else {
                updated.push((topic.clone(), true));
            }
            if !topics.contains(&topic) {
                topics.push(topic.clone());
            }
            latest.insert(
                topic,
                Input {
                    bytes,
                    stamp,
                    valid,
                },
            );
        }
        let valid = topics
            .iter()
            .all(|topic| latest.get(topic).is_some_and(|value| value.valid));
        if valid {
            // Source sorts even false updated slots; all five have timestamps
            // by the time this branch can run. sort_by_key is stable on ties.
            updated.sort_by_key(|(topic, _)| latest.get(topic).map_or(0, |value| value.stamp));
            for (topic, changed) in updated {
                if changed {
                    let value = &latest[&topic];
                    estimator.handle(&topic, &value.bytes, value.stamp as f64 * 1e-9)?;
                }
            }
            estimator.update_points();
        }
        let publish = index.is_multiple_of(5);
        let wire = if publish {
            estimator.update_estimate()?;
            let bytes = estimator
                .packet(valid)?
                .ok_or("liveDelay schema range error")?;
            frames::json_bytes(&bytes)?
        } else {
            JsonValue::parse("null")?
        };
        // The next SubMaster.update snapshots this exact state before reading
        // any later frame (including its terminal EndOfInput invocation).
        output.push(packet::object(&[
            ("frame", node(index)?),
            ("valid", node(valid)?),
            ("publish", node(publish)?),
            ("persist", node(publish && index.is_multiple_of(1200))?),
            ("packet", wire),
            ("removed", node(removed)?),
            ("state", estimator.snapshot()?),
        ])?);
    }
    frames::array(&output)
}
