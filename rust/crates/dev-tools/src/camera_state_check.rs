//! Independent original camera state inputs and dynamically loaded schema oracle.
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    camera_exposure_check::shared,
    camera_sensor_check::{compare_with_limit, lexical, parse_lines, split_runner_os},
    control_tools_policy_check::json,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    ffi::{OsStr, OsString},
    fmt::Write as _,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
#[path = "camera_state_schema.rs"]
mod schema;
pub use schema::NativeSchemaDecoder;
pub struct Decoded {
    pub value: JsonValue,
    pub serialization_error: Option<String>,
}
pub trait SchemaDecoder {
    fn load(&mut self, path: &Path) -> Result<()>;
    fn decode(&mut self, wire: &[u8]) -> Result<Decoded>;
    fn decode_many(&mut self, wires: &[Vec<u8>]) -> Vec<Result<Decoded>> {
        let mut decoded = vec![];
        for wire in wires {
            let result = self.decode(wire);
            let failed = result.is_err();
            decoded.push(result);
            if failed {
                break;
            }
        }
        decoded
    }
}
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
pub fn inputs(frames: usize) -> Result<(String, String)> {
    if !(1..=400).contains(&frames) {
        return Err("camera state frames must be between 1 and 400".into());
    }
    let (mut source, mut native) = (String::new(), String::new());
    for sensor in 1..4 {
        let (width, height) = if sensor == 3 {
            (1344, 760)
        } else {
            (1928, 1208)
        };
        for (camera, focal) in [(0, 1.71), (1, 8.0), (2, 1.71)] {
            writeln!(
                source,
                "reset {sensor} {camera} {width} {height} {}",
                shared::float(focal)?
            )?;
            writeln!(native,"{{\"op\": \"reset\", \"sensor\": {sensor}, \"camera\": {camera}, \"width\": {width}, \"height\": {height}, \"focal\": {}}}",shared::float(focal)?)?;
            for step in 0..frames {
                let frame_id = if step < 100 {
                    step as u32
                } else {
                    (u32::MAX - 8).wrapping_add((step - 100) as u32)
                };
                let request_id = (u32::MAX - 6).wrapping_add((step * 27) as u32);
                let sof = 1_000_000_000u64 + step as u64 * 50_000_000;
                let eof = 1_011_000_000u64 + step as u64 * 50_000_000;
                let processing = (step % 17) as f64 / 1024.0;
                let log_time = 1_025_000_000u64 + step as u64 * 50_000_000;
                let seed = [0, 255, 32, 100][step / 20 % 4];
                let pattern = step % 13 == 6;
                let log_raw = step % 23 != 5;
                let enabled = step % 11 != 4;
                let (mut gain, mut time) = if step % 19 == 5 {
                    ("0", "2309")
                } else {
                    ("", "")
                };
                if step % 19 == 8 {
                    (gain, time) = ("  +0tail", "\t 37suffix");
                }
                if step % 31 == 7 {
                    (gain, time) = ("x", "1");
                }
                if step % 31 == 9 {
                    (gain, time) = ("1", "99999999999999999999");
                }
                let processing = shared::float(processing)?;
                writeln!(source,"step {frame_id} {request_id} {sof} {eof} {processing} {log_time} {seed} {} {} {} {} {}",u8::from(pattern),u8::from(log_raw),u8::from(enabled),quoted(gain),quoted(time))?;
                writeln!(native,"{{\"op\": \"step\", \"frame_id\": {frame_id}, \"request_id\": {request_id}, \"sof\": {sof}, \"eof\": {eof}, \"processing\": {processing}, \"log_time\": {log_time}, \"seed\": {seed}, \"pattern\": {pattern}, \"log_raw\": {log_raw}, \"enabled\": {enabled}, \"gain\": {}, \"time\": {}}}",JsonValue::text(gain).to_json()?,JsonValue::text(time).to_json()?)?;
            }
        }
    }
    Ok((source, native))
}
fn array(values: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))?)
}
fn replace_wire(row: &JsonValue, wire: JsonValue) -> Result<JsonValue> {
    let JsonView::Object(fields) = row.view() else {
        return Err("wire row is not a mapping".into());
    };
    let mut output = vec![];
    for (key, value) in fields {
        let selected = key.iter().copied().eq("wire".chars().map(u32::from));
        output.push(format!(
            "{}: {}",
            JsonValue::codepoints(key.to_vec()).unwrap().to_json()?,
            if selected {
                wire.to_json()?
            } else {
                value.to_json()?
            }
        ));
    }
    Ok(JsonValue::parse(&format!("{{{}}}", output.join(", ")))?)
}
fn wire_bytes(value: &JsonValue) -> Result<Vec<u8>> {
    match value.view() {
        JsonView::Integer(v) => {
            let n = v.parse::<usize>()?;
            Ok(vec![0; n])
        }
        JsonView::Bool(v) => Ok(vec![0; usize::from(v)]),
        JsonView::Text(_) => Err("string argument without an encoding".into()),
        _ => json::sequence(value)?
            .iter()
            .map(|v| match v.view() {
                JsonView::Bool(v) => Ok(u8::from(v)),
                JsonView::Integer(v) => v
                    .parse::<u8>()
                    .map_err(|_| "bytes must be in range(0, 256)".into()),
                _ => Err("wire byte cannot be interpreted as an integer".into()),
            })
            .collect(),
    }
}
fn decoded_text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    source: &Path,
    native: &Path,
    schema: &Path,
    output: &Path,
    runner: &OsStr,
    source_runner: &OsStr,
    frames: usize,
    decoder: &mut dyn SchemaDecoder,
    process: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let (source_text, native_text) = inputs(frames)?;
    decoder.load(schema)?;
    let source = lexical(source);
    let native = lexical(native);
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let mut outputs = vec![];
    let mut runs = vec![];
    for (lane, binary, runner, input) in [
        ("source", &source, source_runner, source_text),
        ("native", &native, runner, native_text),
    ] {
        fs::write(output.join(format!("{lane}.input")), &input)?;
        let mut argv = split_runner_os(runner)?;
        argv.push(binary.as_os_str().to_owned());
        let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(180));
        spec.input = Some(input.into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let captured = process.run(&spec)?;
        let stdout = decoded_text(captured.stdout)?;
        let stderr = decoded_text(captured.stderr)?;
        fs::write(output.join(format!("{lane}.stdout")), &stdout)?;
        fs::write(output.join(format!("{lane}.stderr")), &stderr)?;
        let digest = shared::sha(&fs::read(binary)?);
        runs.push(JsonValue::parse(&format!(
            "{{\"command\": {}, \"exit\": {}, \"sha256\": \"{digest}\"}}",
            shared::argv_json(&argv)?,
            captured.code
        ))?);
        fs::write(
            output.join("runs.json"),
            json::pretty(&array(&runs)?, 0)? + "\n",
        )?;
        if captured.code != 0 {
            return Err(format!("camera state {lane} command exited {}", captured.code).into());
        }
        if ["ERROR: AddressSanitizer", "runtime error:"]
            .iter()
            .any(|v| stderr.contains(v))
        {
            return Err("camera state sanitizer failure".into());
        }
        let mut rows = parse_lines(&stdout)?;
        let mut deferred = None;
        for row in &mut rows {
            // Python membership on nonmapping rows is retained for arrays/strings; an
            // accessed wire value must still be a string-keyed mapping entry.
            if let Some(wire) = row.get("wire") {
                let decoded = decoder.decode(&wire_bytes(&wire)?)?;
                if deferred.is_none() {
                    deferred = decoded.serialization_error;
                }
                *row = replace_wire(row, decoded.value)?;
            } else {
                match row.view() {
                    JsonView::Object(_) => (),
                    JsonView::Array(items) => {
                        if items.iter().any(|value| value.text_eq("wire")) {
                            return Err("list indices must be integers or slices, not str".into());
                        }
                    }
                    JsonView::Text(points) => {
                        if points.windows(4).any(|part| part == [119, 105, 114, 101]) {
                            return Err("string indices must be integers, not 'str'".into());
                        }
                    }
                    _ => return Err("wire membership requires iterable row".into()),
                }
            }
        }
        if let Some(error) = deferred {
            return Err(error.into());
        }
        let rows = array(&rows)?;
        let fields = rows.to_json()? + "\n";
        fs::write(output.join(format!("{lane}.fields.json")), fields)?;
        outputs.push(rows);
    }
    if json::sequence(&outputs[0])?.len() != frames * 9
        || json::sequence(&outputs[1])?.len() != frames * 9
    {
        return Err("camera state output cardinality differs".into());
    }
    if let Err(error) = compare_with_limit(&outputs[0], &outputs[1], "camera_state", 998) {
        if error.is_assertion() {
            fs::write(output.join("failure.txt"), format!("{error}\n"))?;
        }
        return Err(error.into());
    }
    let report=JsonValue::parse(&format!("{{\"status\": \"pass\", \"frames\": {}, \"sensors\": 3, \"cameras\": 3, \"runs\": {}, \"scope\": \"original sendState wire fields, image decimation, pre-AE snapshot, post-AE state and writes, transport order, manual exposure failures\"}}",frames*9,array(&runs)?.to_json()?))?;
    shared::write_report(&output, &report, printed)
}
pub fn cli() -> Result<()> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--schema", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--runner", false),
            OptionSpec::value("--source-runner", false),
            OptionSpec::value("--frames", false),
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-state --source PATH --native PATH --output PATH --schema PATH [--runner COMMAND] [--source-runner COMMAND] [--frames N]");
        return Ok(());
    }
    let raw = options
        .values
        .get("--frames")
        .map(|v| {
            v.to_str()
                .ok_or_else(|| ArgumentError("invalid frames integer".into()))
        })
        .transpose()?
        .unwrap_or("120");
    let integer = crate::stats_clock_check::integer(raw)
        .map_err(|_| ArgumentError("invalid frames integer".into()))?;
    let frames = integer
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=400).contains(n))
        .ok_or("camera state frames must be between 1 and 400")?;
    let capnp = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let mut decoder = NativeSchemaDecoder::new(PathBuf::from(capnp));
    check(
        Path::new(&options.values["--source"]),
        Path::new(&options.values["--native"]),
        Path::new(&options.values["--schema"]),
        Path::new(&options.values["--output"]),
        options
            .values
            .get("--runner")
            .map(OsString::as_os_str)
            .unwrap_or_default(),
        options
            .values
            .get("--source-runner")
            .map(OsString::as_os_str)
            .unwrap_or_default(),
        frames,
        &mut decoder,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
