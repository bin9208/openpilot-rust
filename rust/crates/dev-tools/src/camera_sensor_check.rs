//! Dynamic original C++ sensor comparator: seeded inputs, exact data and f32 bits.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fmt, fs, io::Write, path::Path, time::Duration};
#[path = "camera_random.rs"]
mod random;
pub use random::Random;

/// Python's default POSIX shlex.split, with comments disabled.
pub fn split_runner(input: &str) -> Result<Vec<String>> {
    split_runner_os(std::ffi::OsStr::new(input))?
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "invalid UTF-8 runner word".into())
        })
        .collect()
}
pub(crate) fn split_runner_os(input: &std::ffi::OsStr) -> Result<Vec<OsString>> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut words = vec![];
    let mut word = vec![];
    let mut opened = false;
    let mut quote = None;
    let mut chars = input.as_bytes().iter().copied();
    while let Some(ch) = chars.next() {
        match quote {
            Some(b'\'') => {
                if ch == b'\'' {
                    quote = None;
                } else {
                    word.push(ch);
                }
            }
            Some(b'"') => match ch {
                b'"' => quote = None,
                b'\\' => {
                    let next = chars.next().ok_or("No escaped character")?;
                    if !matches!(next, b'"' | b'\\') {
                        word.push(b'\\');
                    }
                    word.push(next);
                }
                _ => word.push(ch),
            },
            _ => match ch {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    if opened {
                        words.push(OsString::from_vec(std::mem::take(&mut word)));
                        opened = false;
                    }
                }
                b'\'' | b'"' => {
                    opened = true;
                    quote = Some(ch);
                }
                b'\\' => {
                    opened = true;
                    word.push(chars.next().ok_or("No escaped character")?);
                }
                _ => {
                    opened = true;
                    word.push(ch);
                }
            },
        }
    }
    if quote.is_some() {
        return Err("No closing quotation".into());
    }
    if opened {
        words.push(OsString::from_vec(word));
    }
    Ok(words)
}
pub fn parse_lines(text: &str) -> Result<Vec<JsonValue>> {
    let mut lines = vec![];
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\x0b' | '\x0c' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            lines.push(json::parse_text(&text[start..at])?);
            start = at + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                let (at, _) = chars.next().unwrap();
                start = at + 1;
            }
        }
    }
    if start < text.len() {
        lines.push(json::parse_text(&text[start..])?);
    }
    Ok(lines)
}
/// Python format(value, '.17g'), using correctly rounded scientific digits.
pub fn general17(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    let text = format!("{value:.16e}");
    let (mantissa, exponent) = text.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let negative = mantissa.starts_with('-');
    let digits = mantissa.trim_start_matches('-').replace('.', "");
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let mut out = if negative {
        "-".to_owned()
    } else {
        String::new()
    };
    if !(-4..17).contains(&exponent) {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exponent.abs()));
    } else if exponent < 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-exponent - 1) as usize));
        out.push_str(digits);
    } else {
        let position = exponent as usize + 1;
        if digits.len() <= position {
            out.push_str(digits);
            out.extend(std::iter::repeat_n('0', position - digits.len()));
        } else {
            out.push_str(&digits[..position]);
            out.push('.');
            out.push_str(&digits[position..]);
        }
    }
    out
}
fn number(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(v) => {
            let value: f64 = v.parse()?;
            if value.is_finite() {
                Ok(value)
            } else {
                Err("int too large to convert to float".into())
            }
        }
        JsonView::Bool(v) => Ok(f64::from(u8::from(v))),
        _ => Err("a real number is required".into()),
    }
}
fn product(a: &JsonValue, b: &JsonValue) -> Result<f64> {
    let integer = |v: &JsonValue| -> Option<num_bigint::BigInt> {
        match v.view() {
            JsonView::Integer(v) => v.parse().ok(),
            JsonView::Bool(v) => Some(num_bigint::BigInt::from(u8::from(v))),
            _ => None,
        }
    };
    if let (Some(a), Some(b)) = (integer(a), integer(b)) {
        let v: f64 = (a * b).to_string().parse()?;
        if !v.is_finite() {
            return Err("int too large to convert to float".into());
        }
        Ok(v)
    } else {
        Ok(number(a)? * number(b)?)
    }
}
fn index_integer(value: &JsonValue) -> Result<i64> {
    match value.view() {
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Bool(v) => Ok(i64::from(v)),
        _ => Err("integer index required".into()),
    }
}
fn field(config: &JsonValue, name: &str) -> Result<JsonValue> {
    config
        .get(name)
        .ok_or_else(|| format!("missing sensor field {name}").into())
}
pub fn generate_requests(configs: &[JsonValue]) -> Result<Vec<String>> {
    let mut requests = (1..4)
        .map(|kind| format!("config {kind}"))
        .collect::<Vec<_>>();
    let mut rng = Random::new(187);
    let one = JsonValue::parse("1.0")?;
    for (index, config) in configs.iter().enumerate() {
        let kind = index + 1;
        requests.extend((0..3).map(|port| format!("address {kind} {port}")));
        let gains_value = field(config, "sensor_analog_gains")?;
        let gains = json::sequence(&gains_value)?;
        let maximum = index_integer(&field(config, "exposure_time_max")?)?;
        for time in 2..=maximum {
            for gain in 0..gains.len() {
                for dc in 0..2 {
                    requests.push(format!("exposure {kind} {time} {gain} {dc}"));
                }
            }
        }
        for time in [-2147483648i64, -65537, -1, 0, 1, 65535, 65536, 2147483647] {
            for gain in 0..gains.len() {
                requests.push(format!("exposure {kind} {time} {gain} 0"));
            }
        }
        for _ in 0..20000 {
            if gains.is_empty() {
                return Err("empty range for randrange()".into());
            }
            let gain_index = rng.below(gains.len() as u64) as usize;
            // Python eagerly evaluates both choice-list elements before choice.
            let factor = field(config, "dc_gain_factor")?;
            let selected = if rng.below(2) == 0 { &one } else { &factor };
            if matches!(gains_value.view(), JsonView::Object(_)) {
                return Err("sensor gains has no numeric index".into());
            }
            let gain = product(&gains[gain_index], selected)?;
            let maximum_ev = number(&field(config, "max_ev")?)? * 1.2;
            let desired = 0.0 + (maximum_ev - 0.0) * rng.random();
            if maximum < 2 {
                return Err("empty range for randrange()".into());
            }
            let time = 2 + rng.below((maximum - 1) as u64);
            let previous = rng.below(gains.len() as u64);
            requests.push(format!(
                "score {kind} {} {time} {gain_index} {} {previous}",
                general17(desired),
                general17(gain)
            ));
        }
    }
    Ok(requests)
}

#[derive(Debug)]
pub struct CompareError {
    message: Vec<u32>,
    assertion: bool,
}
impl CompareError {
    pub fn is_assertion(&self) -> bool {
        self.assertion
    }
    pub fn message_json(&self) -> String {
        JsonValue::codepoints(self.message.clone())
            .unwrap()
            .to_json()
            .unwrap()
    }
}
impl fmt::Display for CompareError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered: String = self
            .message
            .iter()
            .map(|&p| char::from_u32(p).unwrap_or('\u{fffd}'))
            .collect();
        output.write_str(&rendered)
    }
}
impl std::error::Error for CompareError {}
fn points(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn repr(value: &JsonValue) -> Result<Vec<u32>> {
    enum Part {
        Value(JsonValue),
        Text(&'static str),
    }
    let mut pending = vec![Part::Value(value.clone())];
    let mut output = vec![];
    while let Some(part) = pending.pop() {
        let Part::Value(value) = part else {
            if let Part::Text(text) = part {
                output.extend(points(text));
            }
            continue;
        };
        match value.view() {
            JsonView::Array(values) => {
                output.push(91);
                pending.push(Part::Text("]"));
                for (i, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    if i != 0 {
                        pending.push(Part::Text(", "));
                    }
                }
            }
            JsonView::Object(values) => {
                output.push(123);
                pending.push(Part::Text("}"));
                for (i, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    pending.push(Part::Text(": "));
                    pending.push(Part::Value(JsonValue::codepoints(key.to_vec()).unwrap()));
                    if i != 0 {
                        pending.push(Part::Text(", "));
                    }
                }
            }
            JsonView::Text(_) => {
                let wrapped = JsonValue::parse(&format!("[{}]", value.to_json()?))?;
                let quoted = openpilot_runtime_version::python_str(&wrapped)?;
                output.extend_from_slice(&quoted[1..quoted.len() - 1]);
            }
            _ => output.extend(openpilot_runtime_version::python_str(&value)?),
        }
    }
    Ok(output)
}
fn tuple(parts: Vec<Vec<u32>>, assertion: bool) -> CompareError {
    let mut message = vec![40];
    for (i, part) in parts.into_iter().enumerate() {
        if i != 0 {
            message.extend(points(", "));
        }
        message.extend(part);
    }
    message.push(41);
    CompareError { message, assertion }
}
fn comparison_error(path: &[u32], a: &JsonValue, b: &JsonValue) -> CompareError {
    let result = (|| -> Result<CompareError> {
        Ok(tuple(
            vec![
                repr(&JsonValue::codepoints(path.to_vec()).unwrap())?,
                repr(a)?,
                repr(b)?,
            ],
            true,
        ))
    })();
    result.unwrap_or_else(|e| CompareError {
        message: points(&e.to_string()),
        assertion: false,
    })
}
fn packed(value: &JsonValue) -> std::result::Result<u32, CompareError> {
    let value = number(value).map_err(|e| CompareError {
        message: points(&e.to_string()),
        assertion: false,
    })?;
    let small = value as f32;
    if value.is_finite() && small.is_infinite() {
        return Err(CompareError {
            message: points("float too large to pack with f format"),
            assertion: false,
        });
    }
    Ok(small.to_bits())
}
pub fn compare(
    source: &JsonValue,
    native: &JsonValue,
    path: &str,
) -> std::result::Result<(), CompareError> {
    compare_with_limit(source, native, path, 998)
}
fn compare_with_limit(
    source: &JsonValue,
    native: &JsonValue,
    path: &str,
    limit: usize,
) -> std::result::Result<(), CompareError> {
    let mut pending = vec![(source.clone(), native.clone(), points(path), 0usize)];
    while let Some((source, native, path, depth)) = pending.pop() {
        if depth > limit {
            return Err(CompareError {
                message: points("maximum recursion depth exceeded"),
                assertion: false,
            });
        }
        match source.view() {
            JsonView::Object(left) => {
                let JsonView::Object(right) = native.view() else {
                    return Err(comparison_error(&path, &source, &native));
                };
                if left.len() != right.len()
                    || left
                        .iter()
                        .any(|(key, _)| !right.iter().any(|(other, _)| key == other))
                {
                    let keys = |items: Vec<(&[u32], JsonValue)>| -> Result<Vec<u32>> {
                        let keys = items
                            .into_iter()
                            .map(|(key, _)| JsonValue::codepoints(key.to_vec()).unwrap().to_json())
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        let mut output = points("dict_keys(");
                        output.extend(repr(&JsonValue::parse(&format!("[{}]", keys.join(",")))?)?);
                        output.push(41);
                        Ok(output)
                    };
                    return Err(tuple(
                        vec![
                            repr(&JsonValue::codepoints(path).unwrap()).unwrap(),
                            keys(left).unwrap(),
                            keys(right).unwrap(),
                        ],
                        true,
                    ));
                }
                for (key, value) in left.into_iter().rev() {
                    let other = right
                        .iter()
                        .find(|(other, _)| *other == key)
                        .unwrap()
                        .1
                        .clone();
                    let mut at = path.clone();
                    at.push(46);
                    at.extend_from_slice(key);
                    pending.push((value, other, at, depth + 1));
                }
            }
            JsonView::Array(left) => {
                let JsonView::Array(right) = native.view() else {
                    return Err(comparison_error(&path, &source, &native));
                };
                if left.len() != right.len() {
                    return Err(comparison_error(&path, &source, &native));
                }
                for (index, (a, b)) in left.into_iter().zip(right).enumerate().rev() {
                    let mut at = path.clone();
                    at.extend(points(&format!("[{index}]")));
                    pending.push((a, b, at, depth + 1));
                }
            }
            JsonView::Float(_) => {
                if !matches!(
                    native.view(),
                    JsonView::Float(_) | JsonView::Integer(_) | JsonView::Bool(_)
                ) {
                    return Err(comparison_error(&path, &source, &native));
                }
                let a = packed(&source)?;
                let b = packed(&native)?;
                if a != b {
                    let mut error = comparison_error(&path, &source, &native);
                    error.message.pop();
                    error.message.extend(points(&format!(
                        ", '{}', '{}')",
                        a.to_le_bytes()
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect::<String>(),
                        b.to_le_bytes()
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect::<String>()
                    )));
                    return Err(error);
                }
            }
            _ => {
                if !json::equal(&source, &native) {
                    return Err(comparison_error(&path, &source, &native));
                }
            }
        }
    }
    Ok(())
}
fn execute(runner: &mut dyn Runner, argv: &[OsString], input: &str) -> Result<Vec<JsonValue>> {
    let mut spec = CommandSpec::new_os(argv.to_vec(), Duration::MAX);
    spec.input = Some(input.as_bytes().to_vec());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = runner.run(&spec)?;
    let stdout = std::str::from_utf8(&captured.stdout)?;
    let _ = std::str::from_utf8(&captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("sensor process exited {}", captured.code).into());
    }
    parse_lines(stdout)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
// Match argparse type=Path before filesystem operations. In particular, a
// trailing slash on a file is discarded, but parent components are retained.
pub(crate) fn lexical(path: &Path) -> std::path::PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut output = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
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
    OsString::from_vec(output).into()
}
fn command(prefix: &std::ffi::OsStr, binary: &Path) -> Result<Vec<OsString>> {
    let mut argv = split_runner_os(prefix)?;
    argv.push(resolve_non_strict(binary)?.into_os_string());
    Ok(argv)
}
fn command_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
/// Preserve original initial-config/generation/batch/receipt ordering.
#[allow(clippy::too_many_arguments)]
pub fn check(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &str,
    native_runner: &str,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    check_os(
        reference,
        binary,
        output,
        std::ffi::OsStr::new(source_runner),
        std::ffi::OsStr::new(native_runner),
        runner,
        printed,
    )
}
#[allow(clippy::too_many_arguments)]
fn check_os(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &std::ffi::OsStr,
    native_runner: &std::ffi::OsStr,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let reference = lexical(reference);
    let binary = lexical(binary);
    let output = lexical(output);
    let source_command = command(source_runner, &reference)?;
    let native_command = command(native_runner, &binary)?;
    let configs = execute(runner, &source_command, "config 1\nconfig 2\nconfig 3\n")?;
    let requests = generate_requests(&configs)?;
    fs::create_dir_all(&output)?;
    let mut digest = Sha256::new();
    let mut counts: Vec<(&str, usize)> = vec![];
    for batch in requests.chunks(2000) {
        let input = batch.join("\n") + "\n";
        let source = execute(runner, &source_command, &input)?;
        let native = execute(runner, &native_command, &input)?;
        if source.len() != native.len() || source.len() != batch.len() {
            return Err("sensor output cardinality differs".into());
        }
        for ((request, left), right) in batch.iter().zip(source).zip(native) {
            if let Err(error) = compare_with_limit(&left, &right, request, 997) {
                if error.is_assertion() {
                    let report = JsonValue::parse(&format!(
                        "{{\"request\":{},\"source\":{},\"native\":{},\"error\":{}}}",
                        JsonValue::text(request).to_json()?,
                        left.to_json()?,
                        right.to_json()?,
                        error.message_json()
                    ))?;
                    let text = json::pretty_with_limit(&report, 0, 995)? + "\n";
                    fs::write(output.join("failure.json"), text)?;
                }
                return Err(error.into());
            }
            let verb = request.split_whitespace().next().unwrap();
            if let Some((_, value)) = counts.iter_mut().find(|(name, _)| *name == verb) {
                *value += 1;
            } else {
                counts.push((verb, 1));
            }
        }
        digest.update(input.as_bytes());
    }
    let counts = counts
        .iter()
        .map(|(name, count)| format!("\"{name}\": {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    // Reading original paths (not resolved command paths) is observable if a
    // reference modifies or unlinks files during an earlier invocation.
    let source_hash = hash(&fs::read(&reference)?);
    let native_hash = hash(&fs::read(&binary)?);
    let report=JsonValue::parse(&format!("{{\"status\": \"PASS\", \"cases\": {}, \"counts\": {{{counts}}}, \"comparison\": \"exact registers/integer data and bit-identical binary32 sensor/score values\", \"input_sha256\": \"{:x}\", \"seed\": 187, \"source_sha256\": \"{source_hash}\", \"native_sha256\": \"{native_hash}\", \"source_command\": {}, \"native_command\": {}, \"limits\": \"host or explicit emulator only; no camera hardware or complete runtime startup\"}}",requests.len(),digest.finalize(),command_json(&source_command)?,command_json(&native_command)?))?;
    fs::write(
        output.join("report.json"),
        json::pretty_with_limit(&report, 0, 995)? + "\n",
    )?;
    writeln!(printed, "{}", report.to_json()?)?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--reference", true),
            OptionSpec::value("--binary", true),
            OptionSpec::value("--runner", false),
            OptionSpec::value("--reference-runner", false),
            OptionSpec::value("--output", true),
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-sensors --reference PATH --binary PATH --output PATH [--runner COMMAND] [--reference-runner COMMAND]");
        return Ok(());
    }
    let prefix = |name: &str| {
        options
            .values
            .get(name)
            .map(OsString::as_os_str)
            .unwrap_or(std::ffi::OsStr::new(""))
    };
    check_os(
        Path::new(&options.values["--reference"]),
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        prefix("--reference-runner"),
        prefix("--runner"),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
