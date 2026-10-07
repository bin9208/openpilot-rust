use openpilot_carrot_navi::{
    json::Value,
    manifest::{self, MapConfig},
    packet, projection,
    receiver::{Receiver, Stream},
    record::{Clock, Record},
    Error,
};
use std::{fs, io::Write};

fn required_text(step: &Value, key: &str) -> Result<String, Error> {
    step.get(key).string()
}

fn hex(text: &str) -> Result<Vec<u8>, Error> {
    if !text.len().is_multiple_of(2) {
        return Err(Error::value("invalid fixture hex"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let text =
                std::str::from_utf8(chunk).map_err(|_| Error::value("invalid fixture hex"))?;
            u8::from_str_radix(text, 16).map_err(|_| Error::value("invalid fixture hex"))
        })
        .collect()
}

fn hex_text(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        if write!(result, "{byte:02x}").is_err() {
            return String::new();
        }
    }
    result
}

fn core_operation(step: &Value) -> Result<Value, Error> {
    let op = required_text(step, "op")?;
    match op.as_str() {
        "manifest" => {
            let config = MapConfig::parse(step.get("config"))?;
            let session = if step.has("session") {
                step.get("session").string()?
            } else {
                "0011223344556677".into()
            };
            Ok(config.manifest(&session, Value::integer(1)))
        }
        "parse_binary" => {
            let bytes = hex(&required_text(step, "packet")?)?;
            let (metadata, payload) = packet::parse(&bytes)?;
            Ok(Value::object([
                ("metadata", metadata.value()),
                ("payload_hex", Value::text(&hex_text(payload))),
            ]))
        }
        "parse_json" => {
            let parsed = Value::parse(&required_text(step, "value")?)?;
            if !matches!(parsed, Value::Object(_)) {
                return Err(Error::value("message must be a JSON object"));
            }
            Ok(parsed)
        }
        "hz" => Ok(Value::integer(manifest::resolve_hz(step.get("value"))?)),
        "bitrate" => {
            let Value::Array(args) = step.get("value") else {
                return Err(Error::value("invalid bitrate fixture"));
            };
            let [width, height, hz] = args.as_slice() else {
                return Err(Error::value("invalid bitrate fixture"));
            };
            Ok(Value::Integer(manifest::resolve_bitrate(
                width, height, hz,
            )?))
        }
        _ => Err(Error::value("unsupported policy fixture operation")),
    }
}

fn error_value(error: Error) -> Value {
    Value::object([
        ("type", Value::text(error.kind)),
        ("message", error.message_value()),
    ])
}

#[derive(Default)]
struct TestClock {
    wall_calls: u64,
    mono_calls: u64,
}
impl Clock for TestClock {
    fn wall_ms(&mut self) -> i128 {
        self.wall_calls += 1;
        1_700_000_000_000 + i128::from(self.wall_calls)
    }
    fn mono_ns(&mut self) -> u128 {
        self.mono_calls += 1;
        1_000_000_000 + u128::from(self.mono_calls) * 1000
    }
}
fn records(records: &[std::sync::Arc<Record>]) -> Value {
    Value::Array(
        records
            .iter()
            .map(|record| {
                Value::object([
                    ("summary", record.summary()),
                    (
                        "payload_hex",
                        record
                            .payload()
                            .map(|payload| Value::text(&hex_text(payload)))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "received_mono_ns",
                        Value::integer(record.received_mono_ns()),
                    ),
                ])
            })
            .collect(),
    )
}
fn dashboard(receiver: &Receiver) -> Value {
    let snapshot = receiver.dashboard_snapshot();
    let mut metadata = snapshot.metadata;
    if let Value::Object(fields) = &mut metadata {
        for (name, records) in [
            ("records", snapshot.records),
            ("binary_configs", snapshot.binary_configs),
        ] {
            let encoded = Value::Object(
                records
                    .iter()
                    .map(|(key, record)| {
                        (
                            key.chars().map(u32::from).collect(),
                            Value::object([
                                ("summary", record.summary()),
                                (
                                    "payload_hex",
                                    record
                                        .payload()
                                        .map(|payload| Value::text(&hex_text(payload)))
                                        .unwrap_or(Value::Null),
                                ),
                                (
                                    "received_mono_ns",
                                    Value::integer(record.received_mono_ns()),
                                ),
                            ]),
                        )
                    })
                    .collect(),
            );
            fields.push((name.chars().map(u32::from).collect(), encoded));
        }
    }
    metadata
}
fn operation(
    receiver: &mut Receiver,
    clock: &mut TestClock,
    stdout: &mut Vec<u8>,
    step: &Value,
) -> Result<Value, Error> {
    let operation = required_text(step, "op")?;
    let session = if step.has("session") {
        step.get("session").string()?
    } else {
        "0011223344556677".into()
    };
    let name = if step.has("name") {
        step.get("name").string()?
    } else {
        "vehicle".into()
    };
    let kind = if step.has("kind") {
        step.get("kind").string()?
    } else {
        "json".into()
    };
    let peer = if step.has("peer") {
        step.get("peer").string()?
    } else {
        "fixture-peer".into()
    };
    match operation.as_str() {
        "negotiate" => {
            return receiver.negotiate(step.get("value"), "fixture-app", || {
                Ok("0011223344556677".into())
            })
        }
        "connect" => receiver.control_connected(),
        "disconnect" => receiver.control_disconnected(),
        "control" => receiver.record_control(step.get("value"), &peer)?,
        "json" => receiver.record_json(&session, &name, step.get("value"), &peer, clock, stdout)?,
        "binary" => receiver.record_binary(
            Stream {
                session: &session,
                kind: &kind,
                name: &name,
                peer: &peer,
            },
            step.get("metadata"),
            &hex(&step.get("packet").string()?)?,
            clock,
        )?,
        "stream" => return receiver.stream_config(&session, &kind, &name, &Value::Null),
        "config" => {
            return Ok(Value::Bool(
                receiver.set_map_config(MapConfig::parse(step.get("config"))?),
            ))
        }
        "fail" => receiver.fail(step.get("value"), &peer)?,
        "drain" => return Ok(records(&receiver.drain_media_updates())),
        "bootstrap" => return Ok(records(&receiver.media_bootstrap())),
        "publish" => receiver.record_cereal_publish(
            if matches!(step.get("value"), Value::Null) {
                None
            } else {
                Some(step.get("value"))
            },
            clock,
        )?,
        "payload" => {
            let snapshot = receiver.cereal_snapshot();
            return projection::payload(
                if matches!(step.get("value"), Value::Null) {
                    &snapshot
                } else {
                    step.get("value")
                },
                || 999,
            );
        }
        _ => return core_operation(step),
    }
    Ok(Value::Null)
}

fn capture(case: &Value) -> Value {
    let id = case.get("id").clone();
    let config = match MapConfig::parse(case.get("config")) {
        Ok(config) => config,
        Err(error) => return Value::object([("id", id), ("init_error", error_value(error))]),
    };
    let port = if case.get("config").has("port") {
        case.get("config").get("port").clone()
    } else {
        Value::integer(7714)
    };
    let mut receiver = Receiver::new(port, config);
    let mut clock = TestClock::default();
    let mut stdout = Vec::new();
    let mut rows = Vec::new();
    if let Value::Array(steps) = case.get("steps") {
        for step in steps {
            let (result, error) = match operation(&mut receiver, &mut clock, &mut stdout, step) {
                Ok(value) => (value, Value::Null),
                Err(error) => (Value::Null, error_value(error)),
            };
            rows.push(Value::object([
                ("result", result),
                ("error", error),
                ("health", receiver.health()),
                ("latest", receiver.latest()),
                ("cereal", receiver.cereal_snapshot()),
                ("bootstrap", records(&receiver.media_bootstrap())),
                ("dashboard", dashboard(&receiver)),
                (
                    "clock_calls",
                    Value::Array(vec![
                        Value::integer(clock.wall_calls),
                        Value::integer(clock.mono_calls),
                    ]),
                ),
            ]));
        }
    }
    Value::object([
        ("id", id),
        ("rows", Value::Array(rows)),
        ("stdout", Value::text(&String::from_utf8_lossy(&stdout))),
    ])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| Error::value("expected fixture path"))?;
    let input = Value::parse(&fs::read_to_string(path)?)?;
    let Value::Array(cases) = input else {
        return Err(Error::value("expected fixture array").into());
    };
    let result = Value::Array(cases.iter().map(capture).collect());
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{}", result.encode()?)?;
    Ok(())
}
