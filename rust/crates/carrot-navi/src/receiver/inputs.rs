use super::{insert, Receiver, Stream};
use crate::{
    json::Value,
    json_log,
    record::{Clock, Record},
    Error,
};
use num_bigint::BigInt;
use num_traits::Zero;
use std::{io::Write, sync::Arc};

fn nonnegative(envelope: &Value, key: &str) -> Result<BigInt, Error> {
    let rejected = || Error::value(&format!("invalid v2 {key}"));
    let value = envelope.get(key);
    if matches!(value, Value::Bool(_)) {
        return Err(rejected());
    }
    let value = match value.int() {
        Ok(value) => value,
        Err(error) if matches!(error.kind, "TypeError" | "ValueError") => return Err(rejected()),
        Err(error) => return Err(error),
    };
    if value < BigInt::zero() {
        return Err(rejected());
    }
    Ok(value)
}
fn required_integer(metadata: &Value, key: &str) -> Result<BigInt, Error> {
    if !metadata.has(key) {
        return Err(Error::typed("KeyError", format!("'{key}'")));
    }
    metadata.get(key).int()
}

impl Receiver {
    pub fn record_json(
        &mut self,
        session: &str,
        name: &str,
        envelope: &Value,
        peer: &str,
        clock: &mut impl Clock,
        stdout: &mut impl Write,
    ) -> Result<(), Error> {
        if !envelope.get("type").text_eq("item_update")
            || !envelope.get("protocol_version").number_eq(2)
            || !envelope.get("session_id").text_eq(session)
            || !envelope.get("kind").text_eq("json")
            || !envelope.get("name").text_eq(name)
        {
            return Err(Error::value("v2 JSON envelope/path mismatch"));
        }
        let stream = self.stream_config(session, "json", name, envelope)?;
        let sequence = nonnegative(envelope, "sequence")?;
        let source_timestamp_ms = nonnegative(envelope, "source_timestamp_ms")?;
        let sent_at_ms = nonnegative(envelope, "sent_at_ms")?;
        let Value::Bool(present) = envelope.get("present") else {
            return Err(Error::value("v2 JSON present must be boolean"));
        };
        let present = *present;
        if !envelope.has("value") {
            return Err(Error::value("v2 JSON item must contain value"));
        }
        let value = envelope.get("value");
        let reason = if present {
            if name == "lane_ahead" {
                if !matches!(value, Value::Array(values) if values.iter().all(|value| matches!(value, Value::Object(_))))
                {
                    return Err(Error::value(&format!(
                        "v2 JSON {name} value must be an array of objects"
                    )));
                }
            } else if !matches!(value, Value::Object(_)) {
                return Err(Error::value(&format!(
                    "v2 JSON {name} value must be an object"
                )));
            }
            None
        } else {
            if !matches!(value, Value::Null) {
                return Err(Error::value("absent JSON item must contain value=null"));
            }
            let Value::Text(reason) = envelope.get("reason") else {
                return Err(Error::value("absent JSON item must contain a valid reason"));
            };
            if reason.is_empty() || reason.len() > 64 {
                return Err(Error::value("absent JSON item must contain a valid reason"));
            }
            Some(envelope.get("reason").clone())
        };
        let key = format!("json:{name}");
        self.validate_sequence(&key, &sequence)?;
        let record = Arc::new(Record {
            kind: "json".into(),
            name: name.to_owned(),
            schema_version: stream.get("schema_version").int()?,
            stream_handle: stream.get("stream_handle").int()?,
            manifest_revision: BigInt::from(1),
            sequence: sequence.clone(),
            source_timestamp_ms,
            sent_at_ms: Some(sent_at_ms),
            present,
            value: value.clone(),
            payload: None,
            message_type: None,
            format_or_reason: None,
            flags: BigInt::zero(),
            width: BigInt::zero(),
            height: BigInt::zero(),
            reason,
            peer: peer.to_owned(),
            received_at_ms: clock.wall_ms(),
            received_mono_ns: clock.mono_ns(),
        });
        insert(&mut self.records, &key, record);
        self.mark_received(peer, clock);
        self.mark_state_changed();
        self.log_navigation(
            name,
            &sequence,
            present,
            value,
            envelope.get("reason"),
            stdout,
        )
    }
    fn log_navigation(
        &mut self,
        name: &str,
        sequence: &BigInt,
        present: bool,
        value: &Value,
        reason: &Value,
        stdout: &mut impl Write,
    ) -> Result<(), Error> {
        let label = match name {
            "guidance_current" => "TBT current",
            "guidance_next" => "TBT next",
            "speed" => "SDI",
            _ => return Ok(()),
        };
        let content = if present {
            value.clone()
        } else {
            Value::object([
                ("present", Value::Bool(false)),
                (
                    "reason",
                    if reason.truth() {
                        reason.py_string()?
                    } else {
                        Value::text("source_absent")
                    },
                ),
            ])
        };
        let points: Vec<_> = content
            .compact_sorted_points()?
            .into_iter()
            .take(1500)
            .collect();
        if self
            .navigation_log_values
            .iter()
            .find(|(key, _)| key == name)
            .is_some_and(|(_, old)| old == &points)
        {
            return Ok(());
        }
        if let Some((_, old)) = self
            .navigation_log_values
            .iter_mut()
            .find(|(key, _)| key == name)
        {
            *old = points.clone();
        } else {
            self.navigation_log_values
                .push((name.to_owned(), points.clone()));
        }
        let mut line: Vec<_> = format!("[carrot_navi][{label}] seq={sequence} ")
            .chars()
            .map(u32::from)
            .collect();
        line.extend(points);
        let text = json_log::utf8(&line)?;
        writeln!(stdout, "{text}")
            .and_then(|()| stdout.flush())
            .map_err(|error| Error::typed("OSError", error.to_string()))
    }
    pub fn record_binary(
        &mut self,
        stream: Stream<'_>,
        metadata: &Value,
        payload: &[u8],
        clock: &mut impl Clock,
    ) -> Result<(), Error> {
        let Stream {
            session,
            kind,
            name,
            peer,
        } = stream;
        if kind != "image" && kind != "render" {
            return Err(Error::value("invalid v2 binary stream kind"));
        }
        let message_type = required_integer(metadata, "message_type")?;
        if kind == "image" && message_type != BigInt::from(1) && message_type != BigInt::from(4) {
            return Err(Error::value("v2 image stream received non-image message"));
        }
        if kind == "render" && !(1..=4).any(|value| message_type == BigInt::from(value)) {
            return Err(Error::value("v2 render stream received invalid message"));
        }
        if kind == "image"
            && message_type == BigInt::from(1)
            && required_integer(metadata, "format_or_reason")? != BigInt::from(1)
        {
            return Err(Error::value("v2 image stream requires PNG frames"));
        }
        if kind == "render"
            && message_type == BigInt::from(1)
            && required_integer(metadata, "format_or_reason")? != BigInt::from(2)
        {
            return Err(Error::value("v2 render image frame requires JPEG"));
        }
        let stream = self.stream_config(session, kind, name, metadata)?;
        let sequence = required_integer(metadata, "sequence")?;
        let key = format!("{kind}:{name}");
        self.validate_sequence(&key, &sequence)?;
        let clear = message_type == BigInt::from(4);
        let format_or_reason = required_integer(metadata, "format_or_reason")?;
        let reason = if clear {
            (1..=5)
                .zip(["source_absent", "cleared", "expired", "passed", "invalid"])
                .find_map(|(number, text)| {
                    (format_or_reason == BigInt::from(number)).then(|| Value::text(text))
                })
        } else {
            None
        };
        let record = Arc::new(Record {
            kind: kind.to_owned(),
            name: name.to_owned(),
            schema_version: stream.get("schema_version").int()?,
            stream_handle: stream.get("stream_handle").int()?,
            manifest_revision: BigInt::from(1),
            sequence,
            source_timestamp_ms: required_integer(metadata, "source_timestamp_ms")?,
            sent_at_ms: None,
            present: !clear,
            value: Value::Null,
            payload: (!clear).then(|| Arc::from(payload)),
            message_type: Some(message_type.clone()),
            format_or_reason: Some(format_or_reason),
            flags: required_integer(metadata, "flags")?,
            width: required_integer(metadata, "width")?,
            height: required_integer(metadata, "height")?,
            reason,
            peer: peer.to_owned(),
            received_at_ms: clock.wall_ms(),
            received_mono_ns: clock.mono_ns(),
        });
        insert(&mut self.records, &key, Arc::clone(&record));
        if clear {
            self.binary_configs.retain(|(name, _)| name != &key);
            self.binary_keyframes.retain(|(name, _)| name != &key);
        } else if kind == "render" && message_type == BigInt::from(2) {
            insert(&mut self.binary_configs, &key, Arc::clone(&record));
            self.binary_keyframes.retain(|(name, _)| name != &key);
        } else if kind == "render"
            && message_type == BigInt::from(3)
            && !(&record.flags & BigInt::from(1)).is_zero()
        {
            insert(&mut self.binary_keyframes, &key, Arc::clone(&record));
        }
        self.media_generation += 1;
        self.media_updates.push_back(record);
        if self.media_updates.len() > 256 {
            self.media_updates.pop_front();
        }
        self.mark_received(peer, clock);
        self.state_changed = true;
        Ok(())
    }
}
