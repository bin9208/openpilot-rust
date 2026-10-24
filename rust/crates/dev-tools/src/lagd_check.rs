//! Native batch lagd QA checkers. Expected values are recomputed by the
//! independent source-derived reference, never read from captured test fixtures.
use crate::{
    control_tools_policy_check::json,
    lagd_frames as frames, lagd_loop_reference,
    lagd_numeric_cases::{self, compact},
    lagd_packet_reference as packet, lagd_reference, python_fs,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
mod compare;
mod repr;

#[derive(Clone, Copy)]
pub enum Kind {
    Numeric,
    Loop,
    Cache,
    Packet,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
fn node(value: impl std::fmt::Display) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn array(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(values) => Ok(values),
        _ => Err("expected response list".into()),
    }
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    frames::field(value, key)
}
fn ordered(value: &JsonValue, names: &[&str]) -> Result<JsonValue> {
    packet::object(
        &names
            .iter()
            .map(|name| Ok((*name, field(value, name)?)))
            .collect::<Result<Vec<_>>>()?,
    )
}
fn normalized_source(input: &JsonValue) -> Result<JsonValue> {
    let value = lagd_reference::numeric(input)?;
    match field(input, "kind")?.to_utf8().as_deref() {
        Some("delay") => ordered(
            &value,
            &[
                "delay",
                "correlation",
                "confidence",
                "peak",
                "run",
                "width",
                "starts",
                "ends",
            ],
        ),
        Some("blocks") => frames::array(
            &array(&value)?
                .iter()
                .map(|row| {
                    ordered(
                        row,
                        &["values", "block_idx", "idx", "valid_blocks", "statistics"],
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        Some("pose") => {
            let pose = frames::array(
                &array(&field(&value, "pose")?)?
                    .iter()
                    .map(|row| ordered(row, &["xyz", "std"]))
                    .collect::<Result<Vec<_>>>()?,
            )?;
            packet::object(&[
                ("valid", field(&value, "valid")?),
                ("rotation", field(&value, "rotation")?),
                ("pose", pose),
            ])
        }
        _ => Ok(value),
    }
}
fn write_json(path: &Path, value: &JsonValue) -> Result<()> {
    fs::write(path, format!("{}\n", json::pretty(value, 0)?))?;
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn lines(text: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(
            c,
            '\n' | '\r'
                | '\u{000b}'
                | '\u{000c}'
                | '\u{001c}'
                | '\u{001d}'
                | '\u{001e}'
                | '\u{0085}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            rows.push(&text[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                let (i, c) = chars.next().unwrap();
                end = i + c.len_utf8();
            }
            start = end;
        }
    }
    if start < text.len() {
        rows.push(&text[start..]);
    }
    rows
}
fn command(
    options: &Options,
    input: Vec<u8>,
    timeout: u64,
    runner: &mut impl Runner,
) -> Result<(String, String)> {
    let path = crate::timed_fixtures::lexical(&options.binary);
    let mut spec = CommandSpec::new_os(vec![path.into_os_string()], Duration::from_secs(timeout));
    spec.input = Some(input);
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = runner.run(&spec)?;
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("lagd candidate exited {}", captured.code).into());
    }
    Ok((stdout, stderr))
}
fn parsed_lines(text: &str) -> Result<Vec<JsonValue>> {
    lines(text)
        .iter()
        .map(|line| json::parse_text(line))
        .collect()
}
fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn float_budget(root: &Path, name: &str) -> Result<(f64, f64)> {
    let values = json::parse_text(&fs::read_to_string(
        root.join("rust/crates/lagd/tests/tolerances.json"),
    )?)?;
    let values = field(&values, name)?;
    Ok((
        frames::number(&field(&values, "absolute")?)?,
        frames::number(&field(&values, "relative")?)?,
    ))
}
fn numeric(
    options: &Options,
    runner: &mut impl Runner,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<()> {
    let cases = lagd_numeric_cases::cases()?;
    let (actual, _) = command(options, lagd_numeric_cases::request()?, 30, runner)?;
    let actual = parsed_lines(&actual)?;
    let mut records = Vec::new();
    let mut failures = Vec::new();
    let mut warned = false;
    // strict zip evaluates each complete pair before rejecting a trailing side.
    for (case, actual) in cases.iter().zip(&actual) {
        let expected = normalized_source(&case.input)?;
        if !warned
            && field(&case.input, "kind")?.to_utf8().as_deref() == Some("peak")
            && expected.to_utf8().as_deref() == Some("nan")
        {
            writeln!(stderr,"{}:41: RuntimeWarning: invalid value encountered in scalar divide\n  offset = 0.5 * (y_p1 - y_m1) / (2 * y_0 - y_p1 - y_m1)",options.root.join("openpilot/selfdrive/locationd/helpers.py").display())?;
            warned = true;
        }
        let (absolute, relative) = float_budget(&options.root, case.budget)?;
        if let Err(error) = compare::compare(&expected, actual, absolute, relative, &case.name) {
            if let Some(error) = error.downcast_ref::<compare::Mismatch>() {
                failures.push(JsonValue::text(&error.0));
            } else {
                return Err(error);
            }
        }
        records.push(packet::object(&[
            ("case", JsonValue::text(&case.name)),
            ("input", case.input.clone()),
            ("source", expected),
            ("native", actual.clone()),
        ])?);
    }
    require(
        cases.len() == actual.len(),
        "zip() response cardinality mismatch",
    )?;
    fs::create_dir_all(options.output.parent().unwrap_or(Path::new(".")))?;
    write_json(
        &options.output,
        &packet::object(&[
            ("records", frames::array(&records)?),
            ("failures", frames::array(&failures)?),
        ])?,
    )?;
    require(
        failures.is_empty(),
        &failures
            .iter()
            .take(12)
            .filter_map(JsonValue::to_utf8)
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    writeln!(stdout,"PASS: {} original-source numeric/candidate scenarios with unchanged preimplementation tolerances",records.len())?;
    Ok(())
}
fn cache(options: &Options, runner: &mut impl Runner, stdout: &mut dyn Write) -> Result<()> {
    let car = frames::car_params("SYNTHETIC-LAGD", 0.2)?;
    let named = |blocks, status| frames::cached(0.4, blocks, &JsonValue::text(status));
    let cases = vec![
        ("missing", None, Some(car.clone())),
        ("empty", Some(Vec::new()), Some(car.clone())),
        ("valid", Some(named(5, "estimated")?), Some(car.clone())),
        (
            "unestimated",
            Some(named(1, "unestimated")?),
            Some(car.clone()),
        ),
        ("invalid", Some(named(5, "invalid")?), Some(car.clone())),
        ("excess", Some(named(51, "estimated")?), Some(car.clone())),
        ("negative", Some(named(-1, "estimated")?), Some(car.clone())),
        (
            "wrong-car",
            Some(named(5, "estimated")?),
            Some(frames::car_params("OTHER", 0.2)?),
        ),
        ("missing-previous", Some(named(5, "estimated")?), None),
        (
            "bad-previous",
            Some(named(5, "estimated")?),
            Some(b"bad".to_vec()),
        ),
        ("truncated", Some(b"bad".to_vec()), Some(car.clone())),
        (
            "nan",
            Some(frames::cached(f64::NAN, 5, &JsonValue::text("estimated"))?),
            Some(car.clone()),
        ),
        (
            "infinity",
            Some(frames::cached(
                f64::INFINITY,
                5,
                &JsonValue::text("estimated"),
            )?),
            Some(car.clone()),
        ),
        (
            "unknown-status",
            Some(frames::cached(0.4, 5, &node(3)?)?),
            Some(car.clone()),
        ),
    ];
    let mut records = Vec::new();
    for (name, saved, previous) in cases {
        let expected = packet::restore(&car, saved.as_deref(), previous.as_deref())?;
        let root = tempfile::Builder::new().prefix("lagd-cache-").tempdir()?;
        let optional = |bytes: Option<&[u8]>| -> Result<JsonValue> {
            match bytes {
                Some(value) => frames::json_bytes(value),
                None => node("null"),
            }
        };
        let request = packet::object(&[
            ("root", python_fs::json_value(root.path().as_os_str())),
            ("car", frames::json_bytes(&car)?),
            ("saved", optional(saved.as_deref())?),
            ("previous", optional(previous.as_deref())?),
        ])?;
        let (actual, stderr) = command(
            options,
            format!("{}\n", compact(&request)?).into_bytes(),
            5,
            runner,
        )?;
        let actual = json::parse_text(&actual)?;
        require(
            json::equal(&actual, &expected),
            "cache restore/remove response differs",
        )?;
        records.push(packet::object(&[
            ("case", JsonValue::text(name)),
            ("source", expected),
            ("native", actual),
            ("stderr", JsonValue::text(&stderr)),
            ("pass", node(true)?),
        ])?);
    }
    fs::create_dir_all(options.output.parent().unwrap_or(Path::new(".")))?;
    write_json(&options.output, &frames::array(&records)?)?;
    writeln!(
        stdout,
        "PASS: {} source/native Params restore/remove cases",
        records.len()
    )?;
    Ok(())
}
pub(crate) fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v.bytes().any(|b| b.is_ascii_digit() && b != b'0'),
        JsonView::Float(v) => v != 0.,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn decode_row(row: &JsonValue) -> Result<JsonValue> {
    let fields = match row.view() {
        JsonView::Object(fields) => fields
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    JsonValue::codepoints(key.to_vec())
                        .ok_or("invalid key")?
                        .to_utf8()
                        .ok_or("surrogate key")?,
                    value,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        JsonView::Array(values) => values
            .iter()
            .map(|entry| {
                let pair = array(entry)?;
                require(pair.len() == 2, "dictionary entry must have length two")?;
                Ok((
                    pair[0].to_utf8().ok_or("expected text row key")?,
                    pair[1].clone(),
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err("expected loop row dictionary".into()),
    };
    require(
        fields.iter().any(|(key, _)| key == "packet"),
        "missing packet key",
    )?;
    let fields = fields
        .into_iter()
        .map(|(key, value)| {
            let value = if key == "packet" && !matches!(value.view(), JsonView::Null) {
                packet::project_event(&frames::bytes(&value)?)?
            } else {
                value
            };
            Ok((key, value))
        })
        .collect::<Result<Vec<_>>>()?;
    packet::object(
        &fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.clone()))
            .collect::<Vec<_>>(),
    )
}
fn loop_scenario(
    options: &Options,
    runner: &mut impl Runner,
    stdout: &mut dyn Write,
    mode: &str,
    seed: Option<Vec<u8>>,
) -> Result<()> {
    let car = frames::car_params("SYNTHETIC-LAGD", 0.2)?;
    let config = packet::object(&[
        ("car", frames::json_bytes(&car)?),
        (
            "saved",
            match seed {
                Some(seed) => frames::json_bytes(&seed),
                None => node("null"),
            }?,
        ),
        ("previous", frames::json_bytes(&car)?),
    ])?;
    let count = if mode == "recovery" { 1301 } else { 1201 };
    let mut input = Vec::with_capacity(count);
    let mut request = Vec::new();
    for index in 0..count {
        let mut messages = frames::messages(index as i64, mode)?;
        if mode == "ordering" {
            messages.reverse();
            for (offset, wire) in messages.iter_mut().enumerate() {
                *wire = frames::rewrite_timestamp(wire, offset as u64 * 1000)?;
            }
        }
        let row = packet::object(&[
            (
                "configuration",
                if index == 0 {
                    config.clone()
                } else {
                    node("null")?
                },
            ),
            ("now", node(format!("{:?}", 100. + index as f64 * 0.05))?),
            (
                "messages",
                frames::array(
                    &messages
                        .iter()
                        .map(|wire| frames::json_bytes(wire))
                        .collect::<Result<Vec<_>>>()?,
                )?,
            ),
            ("debug", node(true)?),
        ])?;
        request.extend_from_slice(compact(&row)?.as_bytes());
        request.push(b'\n');
        input.push(row);
    }
    let (response, _) = command(options, request, 60, runner)?;
    let actual = parsed_lines(&response)?
        .iter()
        .map(decode_row)
        .collect::<Result<Vec<_>>>()?;
    let expected = array(&lagd_loop_reference::run(&frames::array(&input)?, &config)?)?
        .iter()
        .map(decode_row)
        .collect::<Result<Vec<_>>>()?;
    require(
        actual.len() == expected.len() && expected.len() == count,
        "loop response cardinality differs",
    )?;
    let (absolute, relative) = float_budget(&options.root, "cereal_float32")?;
    for (index, (want, got)) in expected.iter().zip(&actual).enumerate() {
        compare::compare(want, got, absolute, relative, &format!("{mode}[{index}]"))?;
    }
    let output = options.output.join(mode);
    fs::create_dir_all(&output)?;
    let publications = |rows: &[JsonValue]| -> Result<Vec<JsonValue>> {
        rows.iter()
            .filter_map(|row| match field(row, "publish") {
                Ok(value) if truth(&value) => Some(Ok(row.clone())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    };
    write_json(
        &output.join("source.json"),
        &frames::array(&publications(&expected)?)?,
    )?;
    write_json(
        &output.join("native.json"),
        &frames::array(&publications(&actual)?)?,
    )?;
    let persisted = actual
        .iter()
        .filter_map(|row| match field(row, "persist") {
            Ok(value) if truth(&value) => Some(field(row, "frame")),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    require(
        json::equal(&frames::array(&persisted)?, &JsonValue::parse("[0,1200]")?),
        "persisted frames differ",
    )?;
    write_json(
        &output.join("result.json"),
        &packet::object(&[
            ("frames", node(count)?),
            ("publications", node(publications(&actual)?.len())?),
            ("persisted", frames::array(&persisted)?),
            ("pass", node(true)?),
        ])?,
    )?;
    writeln!(
        stdout,
        "PASS: {mode}, {count} full source/native loop frames"
    )?;
    stdout.flush()?;
    Ok(())
}
fn contains_error(value: &JsonValue) -> Result<bool> {
    match value.view() {
        JsonView::Object(_) => Ok(value.get("error").is_some()),
        JsonView::Text(points) => Ok(points
            .windows(5)
            .any(|points| points == [101, 114, 114, 111, 114])),
        JsonView::Array(values) => Ok(values
            .iter()
            .any(|value| json::equal(value, &JsonValue::text("error")))),
        _ => Err("error membership on noncontainer".into()),
    }
}
fn packet_cases() -> Result<Vec<(String, JsonValue)>> {
    let mut output = Vec::new();
    let row =
        |values: &[f64], blocks: i64, idx: i64, valid: bool, debug: bool| -> Result<JsonValue> {
            packet::object(&[
                (
                    "values",
                    frames::array(
                        &values
                            .iter()
                            .map(|value| node(format!("{value:?}")))
                            .collect::<Result<Vec<_>>>()?,
                    )?,
                ),
                ("blocks", node(blocks)?),
                ("idx", node(idx)?),
                ("valid", node(valid)?),
                ("debug", node(debug)?),
            ])
        };
    for blocks in [0, 4, 5, 50, -1, -7] {
        output.push((
            format!("blocks-{blocks}"),
            row(&[0.4; 50], blocks, 0, true, true)?,
        ));
    }
    for value in [0.05, 1.5] {
        output.push((
            format!("clamp-{value}"),
            row(&[value; 50], 5, 0, false, false)?,
        ));
    }
    for delta in [-1e-12, 0., 1e-12] {
        let mut values = vec![0.4; 50];
        for (index, value) in values.iter_mut().take(6).enumerate() {
            *value = if index % 2 == 0 { 0. } else { 0.2 + delta };
        }
        let text = JsonValue::parse(&format!("{delta:?}"))?.to_json()?;
        output.push((
            format!("std-boundary-{text}"),
            row(&values, 6, 0, true, true)?,
        ));
    }
    let mut values = vec![0.4; 50];
    values[5] = 0.9;
    output.push((
        "current-excluded-from-valid-std".to_owned(),
        row(&values, 5, 1, true, true)?,
    ));
    let mut values = vec![0.4; 50];
    values[0] = 0.;
    output.push((
        "full-ring-excludes-current".to_owned(),
        row(&values, 50, 0, true, true)?,
    ));
    Ok(output)
}
fn packets(options: &Options, runner: &mut impl Runner, stdout: &mut dyn Write) -> Result<()> {
    let cases = packet_cases()?;
    let mut request = Vec::new();
    for (_, row) in &cases {
        request.extend_from_slice(compact(row)?.as_bytes());
        request.push(b'\n');
    }
    let (actual, _) = command(options, request, 10, runner)?;
    let actual = parsed_lines(&actual)?;
    let mut records = Vec::new();
    let (absolute, relative) = float_budget(&options.root, "cereal_float32")?;
    for ((name, row), actual) in cases.iter().zip(&actual) {
        let (expected, actual) = if let Some(wire) = packet::packet(row)? {
            let expected = packet::project_event(&wire)?;
            let actual = packet::project_event(&frames::bytes(&field(actual, "bytes")?)?)?;
            compare::compare(&expected, &actual, absolute, relative, name)?;
            (expected, actual)
        } else {
            require(
                contains_error(actual)?,
                "missing expected packet error marker",
            )?;
            (JsonValue::parse("{\"error\":true}")?, actual.clone())
        };
        records.push(packet::object(&[
            ("case", JsonValue::text(name)),
            ("source", expected),
            ("native", actual),
            ("pass_", node(true)?),
        ])?);
    }
    require(
        cases.len() == actual.len(),
        "zip() response cardinality mismatch",
    )?;
    fs::create_dir_all(options.output.parent().unwrap_or(Path::new(".")))?;
    write_json(&options.output, &frames::array(&records)?)?;
    writeln!(
        stdout,
        "PASS: {} source liveDelay status/clipping/progress/STD boundaries",
        records.len()
    )?;
    Ok(())
}
pub fn run(
    kind: Kind,
    options: &Options,
    runner: &mut impl Runner,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<()> {
    lagd_reference::sources(&options.root)?;
    match kind {
        Kind::Numeric => numeric(options, runner, stdout, stderr),
        Kind::Cache => cache(options, runner, stdout),
        Kind::Packet => packets(options, runner, stdout),
        Kind::Loop => {
            for (mode, seed) in [
                ("normal", None),
                (
                    "recovery",
                    Some(frames::cached(0.4, 5, &JsonValue::text("estimated"))?),
                ),
                (
                    "ordering",
                    Some(frames::cached(0.4, 50, &JsonValue::text("estimated"))?),
                ),
            ] {
                loop_scenario(options, runner, stdout, mode, seed)?;
            }
            Ok(())
        }
    }
}

/// Reuse the original positional argparse grammar without narrowing Unix paths.
pub(crate) fn arguments(
    args: &[std::ffi::OsString],
    name: &str,
    description: &str,
    names: &[&str],
    stdout: &mut dyn Write,
) -> Result<Option<Vec<std::ffi::OsString>>> {
    use crate::argparse_scan::{scan_os_with_positionals, ArgumentError};
    let program = format!("check_lagd_{name}.py");
    let usage = format!("usage: {program} [-h] {}\n", names.join(" "));
    let parsed = scan_os_with_positionals(args, &[], names.len())
        .map_err(|error| ArgumentError(format!("{usage}{program}: error: {}", error.0)))?;
    if parsed.help {
        write!(stdout, "{usage}\n{description}\n\npositional arguments:\n")?;
        for name in names {
            writeln!(stdout, "  {name}")?;
        }
        writeln!(
            stdout,
            "\noptions:\n  -h, --help  show this help message and exit"
        )?;
        return Ok(None);
    }
    Ok(Some(
        parsed
            .positionals
            .into_iter()
            .map(|value| crate::timed_fixtures::lexical(Path::new(&value)).into_os_string())
            .collect(),
    ))
}
pub(crate) fn compare_values(
    expected: &JsonValue,
    actual: &JsonValue,
    absolute: f64,
    relative: f64,
    path: &str,
) -> Result<()> {
    compare::compare(expected, actual, absolute, relative, path)
}
pub(crate) fn cereal_budget(root: &Path) -> Result<(f64, f64)> {
    float_budget(root, "cereal_float32")
}
/// Testable entrypoint; Python traceback frames are replaced by native diagnostics.
/// The original argument admission, exit classes and file/output phases are retained.
pub fn cli_with(
    kind: Kind,
    args: &[std::ffi::OsString],
    root: &Path,
    runner: &mut impl Runner,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let (name, description) = match kind {
        Kind::Numeric => (
            "numeric",
            "Source/numeric parity including masking, candidate choices and pre-fixed\ntolerances.",
        ),
        Kind::Loop => (
            "loop",
            "Original main-loop ordering, masks, recovery, block and cache/publication\nparity.",
        ),
        Kind::Cache => (
            "cache",
            "Unchanged cached-lag restore/reject/remove policy and actual private Params\nfiles.",
        ),
        Kind::Packet => (
            "packet",
            "LiveDelay status, clipping, partial/full-ring and threshold boundary\ncomparison.",
        ),
    };
    let result = (|| -> Result<()> {
        let Some(values) = arguments(args, name, description, &["binary", "output"], stdout)?
        else {
            return Ok(());
        };
        let options = Options {
            root: root.to_owned(),
            binary: PathBuf::from(&values[0]),
            output: PathBuf::from(&values[1]),
        };
        run(kind, &options, runner, stdout, stderr)
    })();
    if let Err(error) = result {
        let _ = writeln!(stderr, "{error}");
        if error.is::<crate::argparse_scan::ArgumentError>() {
            2
        } else {
            1
        }
    } else {
        0
    }
}
pub fn cli(kind: Kind) -> i32 {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    cli_with(
        kind,
        &args,
        &root,
        &mut crate::qa_process::NativeRunner,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}
