//! Dynamic sensor constants from the original C++ reference protocol.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, os::unix::ffi::OsStrExt, path::Path, time::Duration};

fn text(points: &[u32]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let ch = char::from_u32(point).ok_or("sensor output cannot encode lone surrogate")?;
            let mut buffer = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
        }
    }
    Ok(bytes)
}
fn python_str(value: &JsonValue) -> Result<Vec<u8>> {
    enum Part {
        Value(JsonValue, bool),
        Text(&'static [u8]),
    }
    let mut pending = vec![Part::Value(value.clone(), false)];
    let mut output = Vec::new();
    while let Some(part) = pending.pop() {
        let Part::Value(value, quoted) = part else {
            if let Part::Text(text) = part {
                output.extend_from_slice(text);
            }
            continue;
        };
        match value.view() {
            JsonView::Array(values) => {
                output.push(b'[');
                pending.push(Part::Text(b"]"));
                for (index, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value, true));
                    if index != 0 {
                        pending.push(Part::Text(b", "));
                    }
                }
            }
            JsonView::Object(values) => {
                output.push(b'{');
                pending.push(Part::Text(b"}"));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value, true));
                    pending.push(Part::Text(b": "));
                    pending.push(Part::Value(
                        JsonValue::codepoints(key.to_vec()).ok_or("sensor invalid key")?,
                        true,
                    ));
                    if index != 0 {
                        pending.push(Part::Text(b", "));
                    }
                }
            }
            JsonView::Text(_) if quoted => {
                let wrapped = JsonValue::parse(&format!("[{}]", value.to_json()?))?;
                let rendered = text(&openpilot_runtime_version::python_str(&wrapped)?)?;
                output.extend_from_slice(&rendered[1..rendered.len() - 1]);
            }
            _ => output.extend_from_slice(&text(&openpilot_runtime_version::python_str(&value)?)?),
        }
    }
    Ok(output)
}
fn join(values: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, value) in values.iter().enumerate() {
        if i != 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(value);
    }
    out
}
fn array(values: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = b"&[".to_vec();
    out.extend(join(values));
    out.push(b']');
    out
}

fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v != "0" && v != "-0",
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn integer(value: &JsonValue) -> Result<BigInt> {
    match value.view() {
        JsonView::Integer(v) => v.parse().map_err(Into::into),
        JsonView::Bool(v) => Ok(BigInt::from(u8::from(v))),
        _ => Err("sensor integer operation requires an integer".into()),
    }
}
fn float_literal(value: &JsonValue) -> Result<String> {
    let value = match value.view() {
        JsonView::Integer(v) => {
            let x: f64 = v.parse()?;
            if !x.is_finite() {
                return Err("sensor integer overflows float conversion".into());
            }
            x
        }
        JsonView::Float(v) => v,
        JsonView::Bool(v) => {
            if v {
                1.0
            } else {
                0.0
            }
        }
        _ => return Err("sensor pack requires a real number".into()),
    };
    let packed = value as f32;
    if value.is_finite() && packed.is_infinite() {
        return Err("sensor float exceeds f32 range".into());
    }
    Ok(format!("f32::from_bits(0x{:08x})", packed.to_bits()))
}
fn lines(text: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\x0b' | '\x0c' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            rows.push(&text[start..at]);
            start = at + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                let (at, ch) = chars.next().unwrap();
                start = at + ch.len_utf8();
            }
        }
    }
    if start < text.len() {
        rows.push(&text[start..]);
    }
    rows
}
// Match argparse Path string conversion; preserve parent components and OS bytes.
// Existing stats_numbers_check has the same private lexical algorithm.
fn lexical(path: &Path) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = path.as_os_str().as_bytes();
    let mut output = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        Vec::new()
    };
    for part in bytes
        .split(|&byte| byte == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !output.is_empty() && !output.ends_with(b"/") {
            output.push(b'/');
        }
        output.extend_from_slice(part);
    }
    if output.is_empty() {
        output.push(b'.');
    }
    std::ffi::OsString::from_vec(output).into()
}
fn reference(runner: &mut dyn Runner, path: &Path, input: String) -> Result<Vec<JsonValue>> {
    let mut spec = CommandSpec::new_os(vec![lexical(path).into_os_string()], Duration::MAX);
    spec.input = Some(input.into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let result = runner.run(&spec)?;
    // text=True decodes both streams before subprocess checks its exit code.
    let stdout = std::str::from_utf8(&result.stdout)?;
    let _ = std::str::from_utf8(&result.stderr)?;
    if result.code != 0 {
        return Err(format!("sensor reference exited {}", result.code).into());
    }
    lines(stdout).into_iter().map(json::parse_text).collect()
}
fn index(value: &JsonValue, at: usize) -> Result<JsonValue> {
    match value.view() {
        JsonView::Array(values) => values
            .get(at)
            .cloned()
            .ok_or_else(|| "sensor register pair is too short".into()),
        JsonView::Text(points) => points
            .get(at)
            .and_then(|&p| JsonValue::codepoints(vec![p]))
            .ok_or_else(|| "sensor register string is too short".into()),
        JsonView::Object(_) => Err("sensor object has no numeric register index".into()),
        _ => Err("sensor register pair is not subscriptable".into()),
    }
}
fn literal(key: &str, value: &JsonValue) -> Result<Vec<u8>> {
    Ok(match key {
        "pixel_size_mm"
        | "dc_gain_factor"
        | "dc_gain_on_grey"
        | "dc_gain_off_grey"
        | "ev_scale"
        | "analog_gain_cost_low"
        | "analog_gain_cost_high"
        | "target_grey_factor"
        | "min_ev"
        | "max_ev" => float_literal(value)?.into_bytes(),
        "data_word" => {
            if truth(value) {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }
        }
        "start_reg_array" | "init_reg_array" => array(
            json::sequence(value)?
                .iter()
                .map(|pair| {
                    let mut out = b"Register(".to_vec();
                    out.extend(python_str(&index(pair, 0)?)?);
                    out.extend_from_slice(b", ");
                    out.extend(python_str(&index(pair, 1)?)?);
                    out.push(b')');
                    Ok(out)
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        "sensor_analog_gains" => array(
            json::sequence(value)?
                .iter()
                .map(|v| Ok(float_literal(v)?.into_bytes()))
                .collect::<Result<Vec<_>>>()?,
        ),
        "color_correct_matrix"
        | "gamma_lut_rgb"
        | "linearization_lut"
        | "linearization_pts"
        | "vignetting_lut" => array(
            json::sequence(value)?
                .iter()
                .map(python_str)
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => python_str(value)?,
    })
}

fn hashable(value: &JsonValue) -> bool {
    !matches!(value.view(), JsonView::Array(_) | JsonView::Object(_))
}
fn registers(value: &JsonValue) -> Result<BigInt> {
    let mut dict: Vec<(JsonValue, JsonValue)> = Vec::new();
    let pairs = match value.view() {
        JsonView::Object(values) => values
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    JsonValue::codepoints(key.to_vec()).ok_or("sensor invalid dictionary key")?,
                    value,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        _ => json::sequence(value)?
            .into_iter()
            .map(|pair| {
                let pair = json::sequence(&pair)?;
                if pair.len() != 2 {
                    return Err("sensor dictionary update needs pair length two".into());
                }
                Ok((pair[0].clone(), pair[1].clone()))
            })
            .collect::<Result<Vec<_>>>()?,
    };
    for (key, value) in pairs {
        if !hashable(&key) {
            return Err("sensor unhashable register key".into());
        }
        if let Some((_, old)) = dict.iter_mut().find(|(prior, _)| json::equal(prior, &key)) {
            *old = value;
        } else {
            dict.push((key, value));
        }
    }
    let get = |address: u32| -> Result<BigInt> {
        let key = JsonValue::parse(&address.to_string())?;
        integer(
            &dict
                .iter()
                .find(|(prior, _)| json::equal(prior, &key))
                .ok_or("sensor register address missing")?
                .1,
        )
    };
    Ok((get(0x3508)? << 8) | get(0x3509)?)
}
fn path_points(path: &Path) -> Vec<u32> {
    let mut input = path.as_os_str().as_bytes();
    let mut points = Vec::new();
    while !input.is_empty() {
        match std::str::from_utf8(input) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let n = error.valid_up_to();
                points.extend(
                    std::str::from_utf8(&input[..n])
                        .unwrap()
                        .chars()
                        .map(u32::from),
                );
                let bad = error.error_len().unwrap_or(input.len() - n);
                points.extend(
                    input[n..n + bad]
                        .iter()
                        .map(|&byte| 0xdc00 + u32::from(byte)),
                );
                input = &input[n + bad..];
            }
        }
    }
    points
}
pub fn generate(
    runner: &mut dyn Runner,
    root: &Path,
    path: &Path,
    output: &mut dyn Write,
) -> Result<()> {
    let configurations = reference(runner, path, "config 1\nconfig 2\nconfig 3\n".into())?;
    writeln!(
        output,
        "// Generated by rust/tools/generate_camerad_sensor_data.py. Original MIT license applies."
    )?;
    writeln!(
        output,
        "// Pure constant sensor data; no runtime C++ dependency."
    )?;
    let folder = root.join("openpilot/system/camerad/sensors");
    let mut paths = match fs::read_dir(folder) {
        Ok(entries) => entries
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    paths.sort_by_cached_key(|path| path_points(path));
    for path in paths {
        if path
            .extension()
            .is_some_and(|ext| ext == "cc" || ext == "h")
        {
            let digest = Sha256::digest(fs::read(&path)?);
            output.write_all(b"// ")?;
            output.write_all(
                path.file_name()
                    .ok_or("sensor filename missing")?
                    .as_bytes(),
            )?;
            writeln!(output, " SHA-256 {digest:x}")?;
        }
    }
    writeln!(output, "use super::{{Register, SensorConfig}};\n")?;
    for (kind, name) in ["AR0231", "OX03C10", "OS04C10"].iter().enumerate() {
        let config = configurations
            .get(kind)
            .ok_or("sensor strict zip has too few configurations")?;
        writeln!(output, "pub static {name}: SensorConfig = SensorConfig {{")?;
        let JsonView::Object(fields) = config.view() else {
            return Err("sensor config has no items()".into());
        };
        for (key, value) in fields {
            // Python computes the field literal before formatting its key.
            let key_name = key
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>();
            let value = literal(key_name.as_deref().unwrap_or(""), &value)?;
            let key = text(key)?;
            output.write_all(b"    ")?;
            output.write_all(&key)?;
            output.write_all(b": ")?;
            output.write_all(&value)?;
            output.write_all(b",\n")?;
        }
        writeln!(output, "}};\n")?;
        if kind > 0 {
            let count = integer(
                &config
                    .get("analog_gain_max_idx")
                    .ok_or("sensor gain maximum missing")?,
            )? + BigInt::from(1);
            let mut requests = String::new();
            let mut gain = BigInt::from(0);
            while gain < count {
                use std::fmt::Write;
                writeln!(requests, "exposure {} 5 {gain} 0", kind + 1)?;
                gain += 1;
            }
            let values = reference(runner, path, requests)?
                .iter()
                .map(registers)
                .collect::<Result<Vec<_>>>()?;
            let values = values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                output,
                "pub const {name}_GAIN_REGISTERS: [u32; {count}] = [{values}];\n"
            )?;
        }
    }
    if configurations.len() > 3 {
        return Err("sensor strict zip has too many configurations".into());
    }
    Ok(())
}
pub fn cli() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = match scan_os(
        &args,
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--reference", true),
        ],
    ) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if parsed.help {
        println!("Usage: generate-camerad-sensor-data --source PATH --reference PATH");
        return;
    }
    if let Err(error) = generate(
        &mut NativeRunner,
        Path::new(&parsed.values["--source"]),
        Path::new(&parsed.values["--reference"]),
        &mut std::io::stdout().lock(),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
