//! Original camera runtime artifact verifier, independent of the camera policy.
use crate::{
    camera_exposure_check::shared,
    camera_lifecycle_check::{audit, values::*},
    camera_state_check::SchemaDecoder,
    control_tools_policy_check::json,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    cmp::Ordering,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
pub const SERVICES: [&str; 3] = [
    "wideRoadCameraState",
    "roadCameraState",
    "driverCameraState",
];
pub struct Options {
    pub binary: PathBuf,
    pub fixture: PathBuf,
    pub oracle: PathBuf,
    pub output: PathBuf,
    pub case: String,
    pub qemu: Option<PathBuf>,
    pub sysroot: Option<PathBuf>,
}
pub fn ports(case: &str) -> Result<&'static [usize]> {
    match case {
        "disabled-road" | "missing-road" => Ok(&[0, 2]),
        "term" | "int" | "pwr" | "wait-failure" | "poll-error" | "publication-order" => {
            Ok(&[0, 1, 2])
        }
        _ => Err(error("KeyError", repr(&JsonValue::text(case))?)),
    }
}
fn read_text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn read_json(path: &Path) -> Result<JsonValue> {
    json::parse_text(&read_text(path)?)
}
fn rows(value: &JsonValue) -> Result<Vec<JsonValue>> {
    sequence(value)
}
fn at(value: &JsonValue, index: usize) -> Result<JsonValue> {
    rows(value)?
        .get(index)
        .cloned()
        .ok_or_else(|| error("IndexError", "list index out of range"))
}
fn named(row: &JsonValue, op: &str) -> Result<bool> {
    Ok(get(row, "op")?.text_eq(op))
}
fn equal(a: &JsonValue, b: &JsonValue) -> Result<()> {
    assert(json::equal(a, b), "camera runtime comparison differs")
}
fn integer(value: &JsonValue) -> Option<BigInt> {
    match value.view() {
        JsonView::Integer(value) => BigInt::parse_bytes(value.as_bytes(), 10),
        JsonView::Bool(value) => Some(BigInt::from(u8::from(value))),
        _ => None,
    }
}
fn integer_float(integer: &BigInt, float: f64) -> Option<Ordering> {
    if float.is_nan() {
        None
    } else if float == f64::INFINITY {
        Some(Ordering::Less)
    } else if float == f64::NEG_INFINITY {
        Some(Ordering::Greater)
    } else {
        let floor = BigInt::parse_bytes(format!("{:.0}", float.floor()).as_bytes(), 10)
            .expect("finite floating integer has decimal representation");
        Some(match integer.cmp(&floor) {
            Ordering::Equal if float.fract() != 0.0 => Ordering::Less,
            order => order,
        })
    }
}
// Python min compares raw later values with its current item. NaN is never
// less; arbitrary integers remain exact rather than overflowing through f64.
fn less(a: &JsonValue, b: &JsonValue) -> Result<bool> {
    let order = match (a.view(), b.view()) {
        (JsonView::Text(a), JsonView::Text(b)) => Some(a.cmp(b)),
        (JsonView::Float(a), JsonView::Float(b)) => a.partial_cmp(&b),
        (JsonView::Float(a), _) if integer(b).is_some() => {
            integer_float(&integer(b).unwrap(), a).map(Ordering::reverse)
        }
        (_, JsonView::Float(b)) if integer(a).is_some() => integer_float(&integer(a).unwrap(), b),
        _ => match (integer(a), integer(b)) {
            (Some(a), Some(b)) => Some(a.cmp(&b)),
            _ => {
                return Err(error(
                    "TypeError",
                    format!(
                        "'<' not supported between instances of '{}' and '{}'",
                        typename(a),
                        typename(b)
                    ),
                ))
            }
        },
    };
    Ok(order == Some(Ordering::Less))
}
fn length(value: &JsonValue) -> Result<usize> {
    match value.view() {
        JsonView::Array(v) => Ok(v.len()),
        JsonView::Object(v) => Ok(v.len()),
        JsonView::Text(v) => Ok(v.len()),
        _ => Err(error(
            "TypeError",
            format!("object of type '{}' has no len()", typename(value)),
        )),
    }
}
fn contains_log(
    logs: &[JsonValue],
    wanted: &str,
    level: i32,
    prefix: bool,
    substring: bool,
) -> Result<bool> {
    let needle = JsonValue::text(wanted);
    let points = wanted.chars().map(u32::from).collect::<Vec<_>>();
    for row in logs {
        let message = get(row, "msg")?;
        let selected = if prefix {
            let JsonView::Text(text) = message.view() else {
                return Err(error(
                    "AttributeError",
                    format!(
                        "'{}' object has no attribute 'startswith'",
                        typename(&message)
                    ),
                ));
            };
            text.starts_with(&points)
        } else if substring {
            match message.view() {
                JsonView::Text(text) => {
                    points.is_empty() || text.windows(points.len()).any(|part| part == points)
                }
                JsonView::Array(items) => items.iter().any(|item| json::equal(item, &needle)),
                JsonView::Object(fields) => fields.iter().any(|(key, _)| *key == points),
                _ => {
                    return Err(error(
                        "TypeError",
                        format!("argument of type '{}' is not iterable", typename(&message)),
                    ))
                }
            }
        } else {
            json::equal(&message, &needle)
        };
        if selected && json::equal(&get(row, "levelnum")?, &number(level)) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn python_str(value: &JsonValue) -> Result<String> {
    openpilot_runtime_version::python_str(value)?
        .into_iter()
        .map(|point| {
            char::from_u32(point)
                .ok_or_else(|| error("UnicodeEncodeError", "surrogate in oracle input"))
        })
        .collect()
}
fn wire_bytes(value: &JsonValue) -> Result<Vec<u8>> {
    match value.view() {
        JsonView::Integer(v) => Ok(vec![0; v.parse::<usize>()?]),
        JsonView::Bool(v) => Ok(vec![0; usize::from(v)]),
        JsonView::Text(_) => Err(error("TypeError", "string argument without an encoding")),
        _ => sequence(value)?
            .iter()
            .map(|v| match v.view() {
                JsonView::Integer(v) => v
                    .parse::<u8>()
                    .map_err(|_| error("ValueError", "bytes must be in range(0, 256)")),
                JsonView::Bool(v) => Ok(u8::from(v)),
                _ => Err(error(
                    "TypeError",
                    format!(
                        "'{}' object cannot be interpreted as an integer",
                        typename(v)
                    ),
                )),
            })
            .collect(),
    }
}
fn write_json(path: &Path, value: &JsonValue) -> Result<()> {
    fs::write(path, json::pretty(value, 0)? + "\n")?;
    Ok(())
}
fn compact(value: &JsonValue) -> Result<String> {
    Ok(match value.view() {
        JsonView::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(compact)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        JsonView::Object(values) => format!(
            "{{{}}}",
            values
                .into_iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}: {}",
                        JsonValue::codepoints(key.to_vec())
                            .ok_or("invalid key")?
                            .to_json()?,
                        compact(&value)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        _ => value.to_json()?,
    })
}
/// Verify actual runtime artifacts using the independent state-source executable.
/// The caller supplies an already loaded actual Event schema decoder.
pub fn verify(
    opt: &Options,
    oracle: &mut dyn Runner,
    decoder: &mut dyn SchemaDecoder,
    printed: &mut dyn Write,
) -> Result<()> {
    let selected = ports(&opt.case)?;
    let messages = read_json(&opt.output.join("messages.json"))?;
    let images = read_json(&opt.output.join("vision.json"))?;
    let logs_value = read_json(&opt.output.join("logs.json"))?;
    let sent_signal = get(&read_json(&opt.output.join("run.json"))?, "signal")?;
    let trace = read_text(&opt.output.join("trace.jsonl"))?;
    let records = shared::splitlines(&trace)
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    let cleanup = audit(&array(&records)?)?;
    write_json(&opt.output.join("cleanup-audit.json"), &cleanup)?;
    let stderr = read_text(&opt.output.join("stderr"))?;
    assert(
        ![
            "ERROR: AddressSanitizer",
            "runtime error:",
            "Sanitizer CHECK failed",
        ]
        .iter()
        .any(|v| stderr.contains(v)),
        "sanitizer diagnostic",
    )?;
    for port in (0..3).filter(|port| !selected.contains(port)) {
        assert(
            !truth(&at(&messages, port)?) && !truth(&at(&images, port)?),
            "disabled camera published",
        )?;
    }
    let mut sessions = vec![];
    let mut queries = 0;
    let mut subscriptions = 0;
    let mut registers = vec![];
    let mut barriers = 0;
    for row in &records {
        let op = get(row, "op")?;
        match op.to_utf8().as_deref() {
            Some("runtime-event") => {
                let session = get(row, "session")?;
                hashable(&session)?;
                if !sessions.iter().any(|v| key_equal(v, &session)) {
                    sessions.push(session);
                }
            }
            Some("query") => queries += 1,
            Some("subscribe") => subscriptions += 1,
            _ => {}
        }
    }
    assert(
        sessions.len() == selected.len()
            && selected
                .iter()
                .all(|port| sessions.iter().any(|v| key_equal(v, &number(10001 + port)))),
        "runtime sessions differ",
    )?;
    assert(queries == 2, "runtime query count differs")?;
    assert(subscriptions == 1, "runtime subscribe count differs")?;
    let logs = rows(&logs_value)?;
    assert(
        contains_log(&logs, "-- Starting devices", 20, false, false)?,
        "missing startup log",
    )?;
    assert(
        contains_log(&logs, "-- Dequeueing Video events", 20, false, false)?,
        "missing dequeue log",
    )?;
    if opt.case != "missing-road" {
        for port in selected {
            assert(
                contains_log(
                    &logs,
                    &format!("camera {port} synced on frame_id_offset "),
                    30,
                    true,
                    false,
                )?,
                "missing camera synchronization log",
            )?;
        }
    }
    if opt.case == "missing-road" {
        assert(
            contains_log(&logs, "first frame sync timed out:", 40, false, true)?,
            "missing first frame timeout log",
        )?;
        let mut minimum = None;
        for row in &records {
            if named(row, "runtime-registers")? {
                let round = get(row, "round")?;
                if minimum
                    .as_ref()
                    .map_or(Ok(true), |prior| less(&round, prior))?
                {
                    minimum = Some(round);
                }
            }
        }
        assert(
            less(
                &number(40),
                &minimum.ok_or_else(|| error("ValueError", "min() arg is an empty sequence"))?,
            )?,
            "premature runtime registers",
        )?;
    } else if opt.case == "wait-failure" {
        assert(
            contains_log(&logs, "sync failed after", 40, false, true)?,
            "missing wait failure log",
        )?;
        assert(
            contains_log(&logs, "clearing and requeuing", 30, false, true)?,
            "missing requeue log",
        )?;
    } else if opt.case == "poll-error" {
        assert(
            contains_log(&logs, "poll failed", 40, false, true)?,
            "missing poll failure log",
        )?;
    } else if opt.case == "publication-order" {
        equal(
            &read_json(&opt.output.join("publication-order.json"))?,
            &JsonValue::parse("{\"messages\":[0,0,0],\"vision\":[1,0,0]}")?,
        )?;
        for row in &records {
            barriers += usize::from(named(row, "runtime-register-barrier-released")?);
        }
        assert(barriers == 1, "publication barrier count differs")?;
    }
    for row in &records {
        if named(row, "runtime-registers")? {
            let port = get(row, "port")?;
            let round = get(row, "round")?;
            hashable(&port)?;
            hashable(&round)?;
            // Python's dict comprehension retains the last matching register key.
            if let Some(index) =
                registers
                    .iter()
                    .position(|(p, r, _): &(JsonValue, JsonValue, JsonValue)| {
                        key_equal(p, &port) && key_equal(r, &round)
                    })
            {
                registers[index] = (port, round, row.clone());
            } else {
                registers.push((port, round, row.clone()));
            }
        }
    }
    for &port in selected {
        let port_messages = rows(&at(&messages, port)?)?;
        let port_images = rows(&at(&images, port)?)?;
        if port_messages.is_empty() {
            assert(opt.case == "poll-error", "missing camera messages")?;
            continue;
        }
        assert(
            port_messages.len() == port_images.len(),
            "camera message/image counts differ",
        )?;
        for (row, image) in port_messages.iter().zip(&port_images) {
            equal(
                &get(image, "frame_id")?,
                &get(&get(&get(row, "event")?, SERVICES[port])?, "frameId")?,
            )?;
        }
        let layout = &port_images[0];
        let mut lines = vec![format!(
            "reset 3 {port} {} {} {}",
            python_str(&get(layout, "width")?)?,
            python_str(&get(layout, "height")?)?,
            if port == 1 { "8.0" } else { "1.71" }
        )];
        for (row, image) in port_messages.iter().zip(&port_images) {
            let event = get(row, "event")?;
            let data = get(&event, SERVICES[port])?;
            assert(
                matches!(get(&event, "valid")?.view(), JsonView::Bool(true)),
                "camera event must be valid True",
            )?;
            equal(&get(image, "buffer_frame_id")?, &get(image, "frame_id")?)?;
            equal(&get(image, "frame_id")?, &get(&data, "frameId")?)?;
            equal(&get(image, "sof")?, &get(&data, "timestampSof")?)?;
            equal(&get(image, "eof")?, &get(&data, "timestampEof")?)?;
            equal(
                &get(image, "samples")?,
                &array(&vec![number(64 + port * 32); 3])?,
            )?;
            let values = [
                "frameId",
                "requestId",
                "timestampSof",
                "timestampEof",
                "processingTime",
            ]
            .iter()
            .map(|key| python_str(&get(&data, key)?))
            .collect::<Result<Vec<_>>>()?;
            lines.push(format!(
                "step {} {} {} 0 0 1 \"\" \"\"",
                values.join(" "),
                python_str(&get(&event, "logMonoTime")?)?,
                64 + port * 32
            ));
        }
        let input = lines.join("\n") + "\n";
        fs::write(opt.output.join(format!("oracle-{port}.input")), &input)?;
        let mut argv = vec![];
        if let Some(qemu) = &opt.qemu {
            argv.extend([
                qemu.as_os_str().to_owned(),
                "-L".into(),
                opt.sysroot
                    .as_ref()
                    .map_or_else(|| OsString::from("None"), |p| p.as_os_str().to_owned()),
            ]);
        }
        argv.push(opt.oracle.as_os_str().to_owned());
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(10));
        spec.input = Some(input.into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        spec.env.insert(
            "LD_LIBRARY_PATH".into(),
            "/usr/lib/llvm-18/lib/clang/18/lib/linux".into(),
        );
        let result = oracle.run(&spec)?;
        let stdout = String::from_utf8(result.stdout)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let stderr = String::from_utf8(result.stderr)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        fs::write(opt.output.join(format!("oracle-{port}.stdout")), &stdout)?;
        fs::write(opt.output.join(format!("oracle-{port}.stderr")), &stderr)?;
        assert(result.code == 0, stderr)?;
        let expected = shared::splitlines(&stdout)
            .into_iter()
            .map(json::parse_text)
            .collect::<Result<Vec<_>>>()?;
        assert(
            expected.len() == port_messages.len(),
            "oracle message count differs",
        )?;
        for (actual, wanted) in port_messages.iter().zip(expected) {
            let reference = decoder.decode(&wire_bytes(&get(&wanted, "wire")?)?)?;
            equal(&get(actual, "event")?, &reference.value)?;
            let data = get(&get(actual, "event")?, SERVICES[port])?;
            let request = get(&data, "requestId")?;
            hashable(&request)?;
            let record = registers
                .iter()
                .find(|(p, r, _)| key_equal(p, &number(port)) && key_equal(r, &request))
                .map(|(_, _, row)| row)
                .ok_or_else(|| error("KeyError", "missing runtime register record"))?;
            let payload = hex(&get(record, "payload")?)?;
            if !payload.len().is_multiple_of(8) {
                return Err(error(
                    "error",
                    "iterative unpacking requires a buffer of a multiple of 8 bytes",
                ));
            }
            let writes = payload
                .chunks_exact(8)
                .map(|chunk| array(&[number(word_bytes(chunk, 0)), number(word_bytes(chunk, 4))]))
                .collect::<Result<Vec<_>>>()?;
            equal(&array(&writes)?, &get(&get(&wanted, "state")?, "writes")?)?;
            equal(
                &get(&wanted, "actions")?,
                &JsonValue::parse("[\"vision\",\"registers\",\"publish\"]")?,
            )?;
        }
    }
    let mut closes = vec![];
    for row in &records {
        if named(row, "close")? {
            let fd = get(row, "fd")?;
            hashable(&fd)?;
            closes.push(fd);
        }
    }
    for fd in [501, 503, 504, 505, 510, 511, 512] {
        let count = closes.iter().filter(|v| key_equal(v, &number(fd))).count();
        assert(count == 1, "device cleanup count differs")?;
    }
    assert(
        !shared::splitlines(&stderr)
            .iter()
            .any(|v| v.contains("panicked at")),
        "camera panic diagnostic",
    )?;
    let counts = |value: &JsonValue| -> Result<JsonValue> {
        array(
            &rows(value)?
                .iter()
                .map(|v| Ok(number(length(v)?)))
                .collect::<Result<Vec<_>>>()?,
        )
    };
    let report = object(&[
        ("status".into(), JsonValue::text("PASS")),
        ("case".into(), JsonValue::text(&opt.case)),
        ("messages".into(), counts(&messages)?),
        ("vision".into(), counts(&images)?),
        ("logs".into(), number(logs.len())),
        ("signal".into(), sent_signal),
        ("cleanup".into(), cleanup),
        (
            "binary_sha256".into(),
            JsonValue::text(&shared::sha(&fs::read(&opt.binary)?)),
        ),
        (
            "fixture_sha256".into(),
            JsonValue::text(&shared::sha(&fs::read(&opt.fixture)?)),
        ),
        (
            "oracle_sha256".into(),
            JsonValue::text(&shared::sha(&fs::read(&opt.oracle)?)),
        ),
    ])?;
    write_json(&opt.output.join("report.json"), &report)?;
    writeln!(printed, "{}", compact(&report)?)?;
    Ok(())
}

#[path = "camera_runtime_controller.rs"]
mod controller;
pub use controller::{control, Runtime};

#[cfg(feature = "camera-runtime-native")]
#[path = "camera_runtime_native.rs"]
mod native;
#[cfg(feature = "camera-runtime-native")]
pub use native::{cli, NativeRuntime};
