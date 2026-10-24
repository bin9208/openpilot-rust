//! Original complete packet-byte corpus generation and ordered process comparator.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_sensor_check::{lexical, split_runner_os, Random},
    control_tools_policy_check::json,
    python_fs,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
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

fn push(cases: &mut Vec<JsonValue>, value: String) -> Result<()> {
    cases.push(JsonValue::parse(&value)?);
    Ok(())
}
fn registers(rng: &mut Random, count: usize) -> String {
    (0..count)
        .map(|_| format!("[{}, {}]", rng.getrandbits(32), rng.getrandbits(32)))
        .collect::<Vec<_>>()
        .join(", ")
}
pub fn cases() -> Result<Vec<JsonValue>> {
    let mut rng = Random::new(187);
    let mut out = vec![
        JsonValue::parse(r#"{"op":"layout"}"#)?,
        JsonValue::parse(r#"{"op":"csiphy"}"#)?,
    ];
    for sensor in 1..4 {
        for port in 0..3 {
            push(
                &mut out,
                format!("{{\"op\": \"probe\", \"sensor\": {sensor}, \"port\": {port}}}"),
            )?;
        }
    }
    for request in [i32::MIN as i64, -1, 0, 1, i32::MAX as i64] {
        push(
            &mut out,
            format!("{{\"op\": \"poke\", \"request\": {request}}}"),
        )?;
    }
    for _ in 0..300 {
        let request = i32::MIN as i64 + rng.below(1u64 << 32) as i64;
        push(
            &mut out,
            format!("{{\"op\": \"poke\", \"request\": {request}}}"),
        )?;
    }
    for words in [false, true] {
        for count in [0, 1, 3, 9, 31, 64, 221, 512, 65535, 65536, 65537] {
            let values = registers(&mut rng, count);
            push(&mut out,format!("{{\"op\": \"i2c\", \"words\": {words}, \"opcode\": 4, \"registers\": [{values}]}}"))?;
        }
        for _ in 0..300 {
            let opcode = rng.getrandbits(32);
            let count = rng.below(150) as usize;
            let values = registers(&mut rng, count);
            push(&mut out,format!("{{\"op\": \"i2c\", \"words\": {words}, \"opcode\": {opcode}, \"registers\": [{values}]}}"))?;
        }
    }
    for sensor in 1..4 {
        for raw in [false, true] {
            for phy in [0, 0x4001, 0x4002, 0x4003, u32::MAX as u64] {
                for (width, height) in [(1928, 1208), (1344, 760)] {
                    for handle in [-1, 0, i32::MAX] {
                        push(&mut out,format!("{{\"op\": \"acquire\", \"sensor\": {sensor}, \"raw\": {raw}, \"phy\": {phy}, \"width\": {width}, \"height\": {height}, \"handle\": {handle}, \"size\": 768}}"))?;
                    }
                }
            }
        }
        push(
            &mut out,
            format!("{{\"op\": \"bps_tables\", \"sensor\": {sensor}}}"),
        )?;
        for slot in [0, 1, 17, 19] {
            for request in [i32::MIN, -1, 0, 1, i32::MAX] {
                for (width, height) in [(1928, 1208), (1344, 760), (2, 2), (1929, 1209)] {
                    push(&mut out,format!("{{\"op\": \"bps\", \"sensor\": {sensor}, \"slot\": {slot}, \"request\": {request}, \"width\": {width}, \"height\": {height}}}"))?;
                }
            }
        }
        for raw in [false, true] {
            for vignetting in [false, true] {
                for initial in [false, true] {
                    for slot in [0, 1, 17, 19] {
                        for request in [i32::MIN, -1, 0, 1, i32::MAX] {
                            for (width, height) in [(1928, 1208), (1344, 760), (2, 2), (1929, 1209)]
                            {
                                push(&mut out,format!("{{\"op\": \"ife\", \"sensor\": {sensor}, \"raw\": {raw}, \"vignetting\": {vignetting}, \"slot\": {slot}, \"request\": {request}, \"initial\": {initial}, \"width\": {width}, \"height\": {height}}}"))?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}
fn get(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("packet field {key} missing").into())
}
fn integer(value: &JsonValue) -> Result<String> {
    match value.view() {
        JsonView::Integer(v) => Ok(v.into()),
        JsonView::Bool(v) => Ok(u8::from(v).to_string()),
        _ => Err("packet command requires an integer".into()),
    }
}
fn fields(case: &JsonValue, keys: &[&str]) -> Result<String> {
    Ok(keys
        .iter()
        .map(|key| integer(&get(case, key)?))
        .collect::<Result<Vec<_>>>()?
        .join(" "))
}
fn op(case: &JsonValue) -> Result<String> {
    get(case, "op")?
        .to_utf8()
        .ok_or_else(|| "packet op must be text".into())
}
fn source_line(case: &JsonValue) -> Result<String> {
    let op = op(case)?;
    Ok(match op.as_str() {
        "layout" | "csiphy" => op,
        "probe" => format!("probe {}", fields(case, &["sensor", "port"])?),
        "poke" => format!("poke {}", fields(case, &["request"])?),
        "i2c" => {
            let opcode: num_bigint::BigInt = integer(&get(case, "opcode")?)?.parse()?;
            let limit = num_bigint::BigInt::from(1u64 << 31);
            let signed = if opcode < limit {
                opcode
            } else {
                opcode - num_bigint::BigInt::from(1u64 << 32)
            };
            let registers = json::sequence(&get(case, "registers")?)?;
            let values = registers
                .iter()
                .map(|pair| {
                    json::sequence(pair)?
                        .iter()
                        .map(integer)
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "i2c {} {signed} {} {values}",
                integer(&get(case, "words")?)?,
                registers.len()
            )
        }
        "ife" => format!(
            "ife {}",
            fields(
                case,
                &[
                    "sensor",
                    "raw",
                    "vignetting",
                    "slot",
                    "request",
                    "initial",
                    "width",
                    "height"
                ]
            )?
        ),
        "bps" => format!(
            "bps {} 0 0 {} {} 0 {} {}",
            fields(case, &["sensor"])?,
            fields(case, &["slot"])?,
            fields(case, &["request"])?,
            fields(case, &["width"])?,
            fields(case, &["height"])?
        ),
        "bps_tables" => format!("bps_tables {}", fields(case, &["sensor"])?),
        "acquire" => format!(
            "acquire {}",
            fields(
                case,
                &["sensor", "raw", "phy", "width", "height", "handle", "size"]
            )?
        ),
        _ => return Err("unsupported packet command".into()),
    })
}
fn native_input(cases: &[JsonValue]) -> Result<String> {
    Ok(cases
        .iter()
        .map(|value| Ok(value.to_json()? + "\n"))
        .collect::<Result<Vec<_>>>()?
        .concat())
}
pub fn inputs(cases: &[JsonValue]) -> Result<(String, String)> {
    Ok((
        cases
            .iter()
            .map(source_line)
            .collect::<Result<Vec<_>>>()?
            .join("\n")
            + "\n",
        native_input(cases)?,
    ))
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
struct Process {
    code: i32,
    stdout: String,
    stderr: String,
}
fn execute(runner: &mut dyn Runner, argv: &[OsString], input: String) -> Result<Process> {
    let mut spec = CommandSpec::new_os(argv.to_vec(), Duration::from_secs(120));
    spec.input = Some(input.into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let Captured {
        code,
        stdout,
        stderr,
    } = runner.run(&spec)?;
    Ok(Process {
        code,
        stdout: text(stdout)?,
        stderr: text(stderr)?,
    })
}
fn command(path: &Path, prefix: &OsStr) -> Result<Vec<OsString>> {
    let mut argv = split_runner_os(prefix)?;
    argv.push(path.as_os_str().to_owned());
    Ok(argv)
}
fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
fn length(value: &JsonValue) -> Result<usize> {
    match value.view() {
        JsonView::Array(v) => Ok(v.len()),
        JsonView::Object(v) => Ok(v.len()),
        JsonView::Text(v) => Ok(v.len()),
        _ => Err("packet segment has no len()".into()),
    }
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
    fs::write(output.join("inputs.jsonl"), native_input(cases)?)?;
    let source_command = command(&reference, source_runner)?;
    let native_command = command(&binary, native_runner)?;
    let mut byte_count = 0usize;
    for (batch_index, batch) in cases.chunks(50).enumerate() {
        let start = batch_index * 50;
        let source_input = batch
            .iter()
            .map(source_line)
            .collect::<Result<Vec<_>>>()?
            .join("\n")
            + "\n";
        let source = execute(runner, &source_command, source_input)?;
        let native = execute(runner, &native_command, native_input(batch)?)?;
        // The original checks each lane after writing that lane's two files.
        for (label, process) in [("source", &source), ("native", &native)] {
            fs::write(
                output.join(format!("batch-{start:03}.{label}.jsonl")),
                &process.stdout,
            )?;
            fs::write(
                output.join(format!("batch-{start:03}.{label}.stderr")),
                &process.stderr,
            )?;
            if process.code != 0 {
                return Err(format!("camera packet {label} exited {}", process.code).into());
            }
        }
        let left = crate::camera_sensor_check::parse_lines(&source.stdout)?;
        let right = crate::camera_sensor_check::parse_lines(&native.stdout)?;
        if left.len() != right.len() || left.len() != batch.len() {
            return Err("camera packet output cardinality differs".into());
        }
        for (index, ((case, source), native)) in batch.iter().zip(left).zip(right).enumerate() {
            if !json::equal(&source, &native) {
                let failure = JsonValue::parse(&format!(
                    "{{\"index\": {}, \"case\": {}, \"source\": {}, \"native\": {}}}",
                    start + index,
                    case.to_json()?,
                    source.to_json()?,
                    native.to_json()?
                ))?;
                fs::write(output.join("failure.json"), json::pretty(&failure, 0)?)?;
                return Err(format!("packet mismatch at {}, {}", start + index, op(case)?).into());
            }
            if op(case)? != "layout" {
                for segment in json::sequence(&source)? {
                    byte_count += length(&segment)?;
                }
            }
        }
    }
    let report=JsonValue::parse(&format!("{{\"status\": \"PASS\", \"cases\": {}, \"compared_bytes\": {byte_count}, \"source_command\": {}, \"native_command\": {}, \"source_sha256\": \"{}\", \"native_sha256\": \"{}\", \"scope\": \"Complete zero-initialized packet/payload bytes from original methods; controlled memory handles; no live kernel/device claim\"}}",cases.len(),argv_json(&source_command)?,argv_json(&native_command)?,sha(&fs::read(reference)?),sha(&fs::read(binary)?)))?;
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
        println!("usage: check-camerad-packets --reference PATH --binary PATH --output PATH");
        return Ok(());
    }
    let prefix = |name: &str| {
        options
            .values
            .get(name)
            .map(OsString::as_os_str)
            .unwrap_or(OsStr::new(""))
    };
    check_os(
        Path::new(&options.values["--reference"]),
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        prefix("--reference-runner"),
        prefix("--runner"),
        &cases()?,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
