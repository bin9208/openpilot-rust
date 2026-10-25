//! Dynamic Spectra request comparison with the original ordered process boundary.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_sensor_check::{lexical, split_runner_os, Random},
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::Path,
    time::Duration,
};

fn reset(depth: u64, enabled: u64, bps: u64, ar: u64, staggered: u64, readout: u64) -> String {
    format!(
        r#"{{"op":"reset","depth":{depth},"enabled":{enabled},"bps":{bps},"ar":{ar},"staggered":{staggered},"readout":{readout}}}"#
    )
}
#[allow(clippy::too_many_arguments)]
fn step(
    camera: u64,
    request: u64,
    frame: u64,
    timestamp: u64,
    status: u64,
    failures: u64,
    stress: u64,
    later: Option<u64>,
) -> String {
    let first = timestamp + 25_000_000;
    let later = later.unwrap_or(timestamp + 30_000_000);
    format!(
        r#"{{"op":"step","camera":{camera},"request":{request},"frame":{frame},"timestamp":{timestamp},"status":{status},"first_now":{first},"later_now":{later},"failures":{failures},"stress":{stress}}}"#
    )
}
fn case(name: &str, commands: &[String]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        r#"{{"name":{},"commands":[{}]}}"#,
        JsonValue::text(name).to_json()?,
        commands.join(",")
    ))?)
}
pub fn scenarios() -> Result<Vec<JsonValue>> {
    let mut cases = Vec::new();
    for depth in [1, 3, 18] {
        for enabled in [1, 3, 7] {
            for bps in [0, 5, 7] {
                for ar in [0, 4] {
                    for staggered in [0, 4] {
                        for skew in [0, 200_000, 200_001] {
                            let mut commands =
                                vec![reset(depth, enabled, bps, ar, staggered, 22_500_000)];
                            for frame in 1..46 {
                                for camera in 0..3 {
                                    if enabled & (1 << camera) == 0 {
                                        continue;
                                    }
                                    let offset = if staggered & (1 << camera) != 0
                                        && ar & (1 << camera) == 0
                                    {
                                        25_000_000
                                    } else {
                                        0
                                    };
                                    let timestamp = 1_000_000_000
                                        + frame * 50_000_000
                                        + offset
                                        + if camera == 2 { skew } else { 0 };
                                    commands
                                        .push(step(camera, frame, frame, timestamp, 0, 0, 0, None));
                                }
                            }
                            cases.push(case(
                                &format!(
                                    "alignment-{depth}-{enabled}-{bps}-{ar}-{staggered}-{skew}"
                                ),
                                &commands,
                            )?);
                        }
                    }
                }
            }
        }
    }
    let mut rng = Random::new(187);
    for depth in [1, 3, 18] {
        for bps in [0, 7] {
            for trial in 0..30 {
                let mut commands = vec![reset(depth, 7, bps, 0, 0, 22_500_000)];
                let mut requests = [1u64; 3];
                let mut frames = [1u64; 3];
                for index in 0..240 {
                    let camera = index % 3;
                    let mut timestamp = 1_000_000_000 + (index as u64 / 3) * 50_000_000;
                    let draw = rng.below(20);
                    let mut request = requests[camera];
                    let mut frame = frames[camera];
                    let (mut failures, mut stress) = (0, 0);
                    match draw {
                        0 => request = 0,
                        1 => timestamp -= 100_000_000,
                        2 => frame += 2,
                        3 => request += 2,
                        4 | 5 => failures = draw - 3,
                        6..=9 => stress = 1 << (draw - 6),
                        _ => (),
                    }
                    commands.push(step(
                        camera as u64,
                        request,
                        frame,
                        timestamp,
                        rng.below(4),
                        failures,
                        stress,
                        None,
                    ));
                    requests[camera] = requests[camera].max(request) + 1;
                    frames[camera] = frame + 1;
                }
                cases.push(case(&format!("faults-{depth}-{bps}-{trial}"), &commands)?);
            }
        }
        let mut commands = vec![reset(depth, 1, 7, 0, 0, 22_500_000)];
        for index in 0..depth * 3 + 12 {
            commands.push(step(
                0,
                0,
                index,
                1_000_000_000 + index * 50_000_000,
                0,
                0,
                0,
                None,
            ));
        }
        for index in 1..8 {
            commands.push(step(
                0,
                index,
                100 + index,
                10_000_000_000 + index * 50_000_000,
                0,
                0,
                0,
                None,
            ));
        }
        cases.push(case(&format!("invalid-counter-{depth}"), &commands)?);
    }
    let maximum = u64::MAX;
    let mut commands = vec![reset(18, 1, 0, 0, 0, 25_000_000)];
    for (index, (request, frame)) in [
        (maximum - 1, maximum - 2),
        (maximum, maximum - 1),
        (0, maximum),
        (1, 0),
        (2, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let timestamp = 1_000_000_000 + index as u64 * 50_000_000;
        commands.push(step(
            0,
            request,
            frame,
            timestamp,
            0,
            0,
            0,
            Some(timestamp - 1),
        ));
    }
    cases.push(case("u64-wrap-and-negative-processing-delay", &commands)?);
    for request in [maximum - 18, maximum - 2, maximum] {
        cases.push(case(
            &format!("requeue-overflow-{request}"),
            &[
                reset(18, 7, 7, 0, 0, 22_500_000),
                step(0, request, 42, 1_000_000_000, 0, 1, 0, None),
            ],
        )?);
    }
    cases.push(case(
        "late-camera-after-sync-timeout",
        &[
            reset(18, 7, 7, 0, 0, 22_500_000),
            step(0, 1, 41, 1_000_000_000, 0, 0, 0, None),
            step(2, 1, 99, 1_100_000_000, 0, 0, 0, None),
        ],
    )?);
    Ok(cases)
}
fn array(values: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub fn cases_json(cases: &[JsonValue]) -> Result<String> {
    Ok(json::pretty(&array(cases)?, 0)? + "\n")
}
pub fn inputs(cases: &[JsonValue]) -> Result<(String, String, Vec<JsonValue>, usize)> {
    let mut commands = vec![];
    let (mut source, mut native, mut steps) = (String::new(), String::new(), 0);
    for case in cases {
        let values = case.get("commands").ok_or("missing commands")?;
        let JsonView::Array(values) = values.view() else {
            return Err("commands must be array".into());
        };
        for command in values {
            let op = command
                .get("op")
                .and_then(|v| v.to_utf8())
                .ok_or("missing command op")?;
            let keys = if op == "reset" {
                &["depth", "enabled", "bps", "ar", "staggered", "readout"][..]
            } else {
                &[
                    "camera",
                    "request",
                    "frame",
                    "timestamp",
                    "status",
                    "first_now",
                    "later_now",
                    "failures",
                    "stress",
                ][..]
            };
            source.push_str(&op);
            for key in keys {
                source.push(' ');
                source.push_str(&command.get(key).ok_or("missing command field")?.to_json()?);
            }
            source.push('\n');
            native.push_str(&command.to_json()?);
            native.push('\n');
            steps += usize::from(op == "step");
            commands.push(command);
        }
    }
    Ok((source, native, commands, steps))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn command(path: &Path, prefix: &OsStr) -> Result<Vec<OsString>> {
    let mut result = split_runner_os(prefix)?;
    result.push(path.as_os_str().to_owned());
    Ok(result)
}
fn cmd_json(command: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        command
            .iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    reference: &Path,
    binary: &Path,
    output: &Path,
    source_runner: &str,
    native_runner: &str,
    cases: &[JsonValue],
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    check_os(
        reference,
        binary,
        output,
        OsStr::new(source_runner),
        OsStr::new(native_runner),
        cases,
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
    cases: &[JsonValue],
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let reference = lexical(reference);
    let binary = lexical(binary);
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    fs::write(output.join("cases.json"), cases_json(cases)?)?;
    let reference_cmd = command(&reference, source_runner)?;
    let native_cmd = command(&binary, native_runner)?;
    let mut total_steps = 0;
    for (batch, cases) in cases.chunks(12).enumerate() {
        let start = batch * 12;
        let (source_input, native_input, commands, steps) = inputs(cases)?;
        let mut spec = CommandSpec::new_os(reference_cmd.clone(), Duration::from_secs(90));
        spec.input = Some(source_input.into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let expected = runner.run(&spec)?;
        let expected_stdout = text(expected.stdout)?;
        let expected_stderr = text(expected.stderr)?;
        spec.os_argv = Some(native_cmd.clone());
        spec.argv = native_cmd
            .iter()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        spec.input = Some(native_input.into_bytes());
        let actual = runner.run(&spec)?;
        let actual_stdout = text(actual.stdout)?;
        let actual_stderr = text(actual.stderr)?;
        let prefix = output.join(format!("batch-{start:03}"));
        fs::write(prefix.with_extension("source.jsonl"), &expected_stdout)?;
        fs::write(prefix.with_extension("native.jsonl"), &actual_stdout)?;
        fs::write(
            prefix.with_extension("stderr.json"),
            format!(
                "{{\"source\": {}, \"native\": {}}}\n",
                JsonValue::text(&expected_stderr).to_json()?,
                JsonValue::text(&actual_stderr).to_json()?
            ),
        )?;
        if expected.code != 0 || actual.code != 0 {
            return Err(format!(
                "batch {start}: source={}, native={}; see {}",
                expected.code,
                actual.code,
                prefix.display()
            )
            .into());
        }
        let left = crate::camera_sensor_check::parse_lines(&expected_stdout)?;
        let right = crate::camera_sensor_check::parse_lines(&actual_stdout)?;
        if left.len() != commands.len() || right.len() != commands.len() {
            return Err(format!("batch {start}: output count mismatch").into());
        }
        for (index, ((command, source), native)) in commands.iter().zip(left).zip(right).enumerate()
        {
            if !json::equal(&source, &native) {
                let failure = JsonValue::parse(&format!(
                    r#"{{"batch":{start},"index":{index},"command":{},"source":{},"native":{}}}"#,
                    command.to_json()?,
                    source.to_json()?,
                    native.to_json()?
                ))?;
                fs::write(
                    output.join("failure.json"),
                    json::pretty(&failure, 0)? + "\n",
                )?;
                return Err(format!("lifecycle mismatch at batch {start} index {index}").into());
            }
        }
        total_steps += steps;
    }
    let report = JsonValue::parse(&format!(
        r#"{{"status":"PASS","scenarios":{},"steps":{total_steps},"source_command":{},"native_command":{},"source_sha256":"{}","native_sha256":"{}","scope":"Original verbatim frame lifecycle methods; controlled clock, stress and fence/queue I/O boundaries; no device or ioctl ABI claim"}}"#,
        cases.len(),
        cmd_json(&reference_cmd)?,
        cmd_json(&native_cmd)?,
        hash(&fs::read(&reference)?),
        hash(&fs::read(&binary)?)
    ))?;
    fs::write(output.join("report.json"), json::pretty(&report, 0)? + "\n")?;
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
        println!("usage: check-camerad-requests --reference PATH --binary PATH --output PATH [--runner COMMAND] [--reference-runner COMMAND]");
        return Ok(());
    }
    let prefix = |key: &str| {
        options
            .values
            .get(key)
            .map(OsString::as_os_str)
            .unwrap_or(OsStr::new(""))
    };
    check_os(
        Path::new(&options.values["--reference"]),
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        prefix("--reference-runner"),
        prefix("--runner"),
        &scenarios()?,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
