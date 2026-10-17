//! Original continuous exposure input generator and independent C++ comparator.
use crate::{
    argparse_scan::{decimal_value, ArgumentError},
    camera_sensor_check::{command, general17, lexical, Random},
    qa_process::{NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{ffi::OsStr, fmt::Write as _, fs, io::Write, path::Path};
#[path = "camera_pair_check.rs"]
pub(crate) mod shared;
pub struct Inputs {
    pub source: String,
    pub native: String,
    pub scenarios: usize,
    pub steps: usize,
}
pub fn frames(value: &OsStr) -> std::result::Result<usize, ArgumentError> {
    let value = value
        .to_str()
        .ok_or_else(|| ArgumentError("invalid frames integer".into()))?
        .trim_matches(char::is_whitespace);
    let (negative, value) = if let Some(v) = value.strip_prefix('-') {
        (true, v)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut digits = 0;
    let mut previous = false;
    let mut number = 0usize;
    for ch in value.chars() {
        if ch == '_' && previous {
            previous = false;
            continue;
        }
        let d = decimal_value(ch).ok_or_else(|| ArgumentError("invalid frames integer".into()))?;
        digits += 1;
        previous = true;
        number = number.saturating_mul(10).saturating_add(d as usize);
    }
    if !previous || digits > 4300 || negative || !(1..=1200).contains(&number) {
        return Err(ArgumentError("--frames must be between 1 and 1200".into()));
    }
    Ok(number)
}
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn reset(
    source: &mut String,
    native: &mut String,
    sensor: u32,
    camera: u32,
    width: u32,
    height: u32,
    focal: f64,
) -> Result<()> {
    writeln!(
        source,
        "reset {sensor} {camera} {width} {height} {}",
        shared::float(focal)?
    )?;
    writeln!(native,"{{\"op\": \"reset\", \"sensor\": {sensor}, \"camera\": {camera}, \"width\": {width}, \"height\": {height}, \"focal\": {}}}",shared::float(focal)?)?;
    Ok(())
}
fn step(
    source: &mut String,
    native: &mut String,
    frame: u32,
    grey: f64,
    enabled: bool,
    gain: &str,
    time: &str,
) -> Result<()> {
    writeln!(
        source,
        "step {frame} {} {} {} {}",
        general17(grey),
        u8::from(enabled),
        quoted(gain),
        quoted(time)
    )?;
    writeln!(native,"{{\"op\": \"step\", \"frame_id\": {frame}, \"grey\": {}, \"enabled\": {enabled}, \"gain\": {}, \"time\": {}}}",shared::float(grey)?,shared::string(gain)?,shared::string(time)?)?;
    Ok(())
}
pub fn generate_inputs(frames: usize) -> Result<Inputs> {
    if !(1..=1200).contains(&frames) {
        return Err("frames outside original bounds".into());
    }
    let mut input = Inputs {
        source: String::new(),
        native: String::new(),
        scenarios: 0,
        steps: 0,
    };
    let mut rng = Random::new(1871);
    for (sensor, gain_max, max_time) in [(1, 13, 2133), (2, 54, 2016), (3, 40, 2352)] {
        for (camera, focal) in [(0, 1.71), (1, 8.0), (2, 1.71)] {
            let (width, height) = if sensor == 3 {
                (1344, 760)
            } else {
                (1928, 1208)
            };
            for mode in 0..5 {
                input.scenarios += 1;
                reset(
                    &mut input.source,
                    &mut input.native,
                    sensor,
                    camera,
                    width,
                    height,
                    focal,
                )?;
                for frame in 0..frames {
                    let grey = match mode {
                        0 => [0.0, 0.99609375, 0.125, 0.5][frame / 300 % 4],
                        1 => rng.below(256) as f64 / 256.0,
                        2 => {
                            if frame / 100 % 2 == 1 {
                                0.01
                            } else {
                                0.95
                            }
                        }
                        3 => 0.1 + 0.05 * (frame % 19) as f64 / 19.0,
                        _ => 0.125,
                    };
                    let enabled = frame % 7 != 0;
                    let (mut gain, mut time) = (String::new(), String::new());
                    if mode == 4 {
                        gain = rng.below(gain_max + 1).to_string();
                        time = (2 + rng.below(max_time - 1)).to_string();
                        if frame % 5 == 0 {
                            gain = format!("\t +{gain}suffix");
                            time = format!(" {time} trailing");
                        }
                        if frame % 13 == 0 {
                            gain.clear();
                        }
                        if frame % 17 == 0 {
                            time.clear();
                        }
                    }
                    let id = (frame as u32).wrapping_add(if mode == 3 { 4294967100 } else { 1 });
                    step(
                        &mut input.source,
                        &mut input.native,
                        id,
                        grey,
                        enabled,
                        &gain,
                        &time,
                    )?;
                    input.steps += 1;
                }
            }
            for (gain, time) in [
                ("x", "1"),
                ("1", "x"),
                ("99999999999999", "1"),
                ("1", "999999999999"),
                ("1", "+"),
                ("  ", "5"),
            ] {
                input.scenarios += 1;
                reset(
                    &mut input.source,
                    &mut input.native,
                    sensor,
                    camera,
                    width,
                    height,
                    focal,
                )?;
                step(
                    &mut input.source,
                    &mut input.native,
                    1,
                    0.125,
                    true,
                    gain,
                    time,
                )?;
                step(&mut input.source, &mut input.native, 2, 0.125, true, "", "")?;
                input.steps += 2;
            }
        }
    }
    Ok(input)
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &str,
    native_runner: &str,
    frames: usize,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    check_os(
        reference,
        binary,
        output,
        OsStr::new(source_runner),
        OsStr::new(native_runner),
        frames,
        runner,
        printed,
    )
}
#[allow(clippy::too_many_arguments)]
fn check_os(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &OsStr,
    native_runner: &OsStr,
    frames: usize,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let reference = lexical(reference);
    let binary = lexical(binary);
    let output = lexical(output);
    let input = generate_inputs(frames)?;
    // Native runner grammar is evaluated only after the source process succeeds.
    let source_command = command(source_runner, &reference)?;
    let source = shared::execute(runner, &source_command, input.source.as_bytes())?;
    let native_command = command(native_runner, &binary)?;
    let native = shared::execute(runner, &native_command, input.native.as_bytes())?;
    let source_lines = shared::splitlines(&source);
    let native_lines = shared::splitlines(&native);
    let requests = shared::splitlines(&input.native);
    let observations = input.source.lines().count();
    if source_lines.len() != native_lines.len() || source_lines.len() != observations {
        return Err("camera exposure output cardinality differs".into());
    }
    fs::create_dir_all(&output)?;
    for (index, (left, right)) in source_lines.into_iter().zip(native_lines).enumerate() {
        shared::compare_row(
            left,
            right,
            &format!("step[{index}]"),
            |a, b, error| {
                Ok(format!("{{\"index\": {index}, \"request\": {}, \"source\": {}, \"native\": {}, \"error\": {error}}}",shared::string(requests[index])?,a.to_json()?,b.to_json()?))
            },
            &output,
        )?;
    }
    let source_hash = shared::sha(&fs::read(&reference)?);
    let native_hash = shared::sha(&fs::read(&binary)?);
    let report=JsonValue::parse(&format!("{{\"status\": \"PASS\", \"scenarios\": {}, \"steps\": {}, \"observations\": {observations}, \"source_command\": {}, \"native_command\": {}, \"source_sha256\": \"{source_hash}\", \"native_sha256\": \"{native_hash}\", \"source_input_sha256\": \"{}\", \"native_input_sha256\": \"{}\", \"comparison\": \"full exposure state, rectangle and register writes; float values compared as binary32\", \"limits\": \"original methods with controlled sensor/Params/write boundary; no camera hardware\"}}",input.scenarios,input.steps,shared::argv_json(&source_command)?,shared::argv_json(&native_command)?,shared::sha(input.source.as_bytes()),shared::sha(input.native.as_bytes())))?;
    shared::write_report(&output, &report, printed)
}
pub fn cli() -> Result<()> {
    let options = shared::options(true)?;
    if options.help {
        println!("usage: check-camerad-exposure --reference PATH --binary PATH --output PATH [--frames 1..1200]");
        return Ok(());
    }
    let frames = frames(
        options
            .values
            .get("--frames")
            .map(std::ffi::OsString::as_os_str)
            .unwrap_or(OsStr::new("1200")),
    )?;
    check_os(
        Path::new(&options.values["--reference"]),
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        shared::prefix(&options, "--reference-runner"),
        shared::prefix(&options, "--runner"),
        frames,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
