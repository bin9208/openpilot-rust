//! Original camera kernel scenarios through actual libc/preload boundaries.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_exposure_check::shared,
    camera_sensor_check::lexical,
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Write, path::Path, time::Duration};

#[derive(Clone)]
pub struct Scenario {
    pub name: String,
    pub mode: String,
    pub seed: u64,
    pub environment: Vec<(String, String)>,
}
fn scenario(name: String, mode: &str, seed: u64, environment: Vec<(&str, String)>) -> Scenario {
    Scenario {
        name,
        mode: mode.into(),
        seed,
        environment: environment
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
    }
}
fn error_env(opcode: &str, error: i32, count: i32) -> Vec<(&'static str, String)> {
    vec![
        ("CK_FAIL_OP", opcode.into()),
        ("CK_FAIL_ERRNO", error.to_string()),
        ("CK_FAIL_COUNT", count.to_string()),
    ]
}
pub fn scenarios() -> Vec<Scenario> {
    let mut cases = vec![];
    for (mode, opcode) in [
        ("session", "camera:267"),
        ("destroy_session", "camera:268"),
        ("acquire_sensor", "camera:258"),
        ("acquire_phy", "camera:258"),
        ("configure", "camera:261"),
        ("start", "camera:259"),
        ("stop", "camera:260"),
        ("release_device", "camera:262"),
        ("flush_device", "camera:264"),
        ("link", "camera:269"),
        ("activate", "camera:278"),
        ("deactivate", "camera:278"),
        ("unlink", "camera:270"),
        ("schedule", "camera:271"),
        ("flush_requests", "camera:272"),
        ("probe", "camera:266"),
        ("release_buffer", "camera:276"),
        ("fences", "sync:0"),
        ("wait", "sync:6"),
        ("destroy_fence", "sync:1"),
        ("poll", "poll"),
        ("event", "event"),
        ("imports", "camera:275"),
        ("allocation", "camera:274"),
        ("pool", "camera:274"),
    ] {
        let seeds = if mode == "imports" {
            &[2, 3, 4, 5, 6, 7][..]
        } else {
            &[0, 1, 2, 3, 17, 0x80000000, 0xffffffff][..]
        };
        for &seed in seeds {
            cases.push(scenario(format!("{mode}-{seed}"), mode, seed, vec![]));
        }
        if ["link", "pool", "allocation", "imports"].contains(&mode) {
            continue;
        }
        for (error, count) in [(4, 1), (4, 100), (4, 101), (11, 1), (5, 1)] {
            cases.push(scenario(
                format!("{mode}-error-{error}-{count}"),
                mode,
                7,
                error_env(opcode, error, count),
            ));
        }
        let mut env = error_env(opcode, 4, 2);
        env.push(("CK_MUTATE_FAILURE", "1".into()));
        cases.push(scenario(format!("{mode}-retry-mutation"), mode, 7, env));
    }
    for mode in ["fences", "wait", "destroy_fence"] {
        for code in [-5, 3] {
            cases.push(scenario(
                format!("{mode}-embedded-{code}"),
                mode,
                7,
                vec![("CK_SYNC_RESULT", code.to_string())],
            ));
        }
    }
    for (error, count) in [(4, 2), (4, 101), (11, 1), (5, 1)] {
        cases.push(scenario(
            format!("bps-create-error-{error}-{count}"),
            "fences",
            7,
            error_env("camera:0", error, count),
        ));
        cases.push(scenario(
            format!("yuv-import-error-{error}-{count}"),
            "imports",
            7,
            vec![
                ("CK_FAIL_OP", "camera:275".into()),
                ("CK_FAIL_SKIP", "1".into()),
                ("CK_FAIL_ERRNO", error.to_string()),
                ("CK_FAIL_COUNT", count.to_string()),
            ],
        ));
    }
    for mode in ["allocation", "pool"] {
        cases.push(scenario(
            format!("{mode}-allocation-error-mutated"),
            mode,
            3,
            vec![
                ("CK_FAIL_OP", "camera:274".into()),
                ("CK_FAIL_ERRNO", "5".into()),
                ("CK_MUTATE_FAILURE", "1".into()),
            ],
        ));
    }
    for result in [0, -1, 1] {
        for events in [0, 1, 2, 8, 16, 32] {
            cases.push(scenario(
                format!("poll-return-{result}-{events}"),
                "poll",
                1,
                vec![
                    ("CK_POLL_RETURN", result.to_string()),
                    ("CK_POLL_EVENTS", events.to_string()),
                ],
            ));
        }
    }
    cases
}
pub fn baseline(cases: &[Scenario]) -> Vec<Scenario> {
    let mut output: Vec<Scenario> = vec![];
    for case in cases {
        if case.environment.is_empty() && !output.iter().any(|prior| prior.mode == case.mode) {
            output.push(case.clone());
        }
    }
    output
}
fn get(record: &JsonValue, key: &str) -> Result<JsonValue> {
    record
        .get(key)
        .ok_or_else(|| format!("kernel trace missing {key}").into())
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
pub fn normalize(records: &JsonValue) -> Result<JsonValue> {
    let mut output = vec![];
    let fd601 = JsonValue::parse("601")?;
    let fd602 = JsonValue::parse("602")?;
    for record in json::sequence(records)? {
        let operation = get(&record, "op")?;
        if operation.text_eq("open") {
            let fd = get(&record, "fd")?;
            if json::equal(&fd, &fd601) || json::equal(&fd, &fd602) {
                let flags = get(&record, "flags")?;
                let flags: BigInt = match flags.view() {
                    JsonView::Integer(v) => v.parse()?,
                    JsonView::Bool(v) => BigInt::from(u8::from(v)),
                    _ => return Err("kernel flags bit operation needs integer".into()),
                };
                let flags = flags & !BigInt::from(rustix::fs::OFlags::CLOEXEC.bits());
                let JsonView::Object(fields) = record.view() else {
                    return Err("kernel record must be mapping".into());
                };
                let mut encoded = vec![];
                for (key, value) in fields {
                    let field = JsonValue::codepoints(key.to_vec()).ok_or("invalid kernel key")?;
                    encoded.push(format!(
                        "{}: {}",
                        field.to_json()?,
                        if field.text_eq("flags") {
                            flags.to_string()
                        } else {
                            value.to_json()?
                        }
                    ));
                }
                output.push(JsonValue::parse(&format!("{{{}}}", encoded.join(", ")))?);
                continue;
            }
        }
        output.push(record);
    }
    array(&output)
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest(path: &Path) -> Result<String> {
    Ok(sha(&fs::read(path)?))
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn set(environment: &mut Vec<(OsString, OsString)>, key: OsString, value: OsString) {
    if let Some((_, old)) = environment.iter_mut().find(|(prior, _)| *prior == key) {
        *old = value;
    } else {
        environment.push((key, value));
    }
}
fn object(environment: &[(OsString, OsString)], filtered: bool) -> Result<String> {
    let mut fields = vec![];
    for (key, value) in environment {
        let selected = python_fs::codepoints(key);
        if filtered
            && !["CK_", "ASAN_", "UBSAN_", "LD_"].iter().any(|prefix| {
                selected.starts_with(&prefix.chars().map(u32::from).collect::<Vec<_>>())
            })
        {
            continue;
        }
        fields.push(format!(
            "{}: {}",
            python_fs::json_string(key)?,
            python_fs::json_string(value)?
        ));
    }
    Ok(format!("{{{}}}", fields.join(", ")))
}
fn overrides(case: &Scenario) -> Result<String> {
    Ok(format!(
        "{{{}}}",
        case.environment
            .iter()
            .map(|(key, value)| Ok(format!(
                "{}: {}",
                JsonValue::text(key).to_json()?,
                JsonValue::text(value).to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
struct Observation {
    metadata: JsonValue,
    records: JsonValue,
    exit: i32,
    sanitizer: bool,
    stdout: String,
    calls: usize,
}
#[allow(clippy::too_many_arguments)]
fn run(
    binary: &Path,
    library: &Path,
    output: &Path,
    qemu: Option<&Path>,
    sysroot: Option<&Path>,
    case: &Scenario,
    lane: &str,
    environment: &[(OsString, OsString)],
    runner: &mut dyn Runner,
) -> Result<Observation> {
    let folder = output.join(&case.name).join(lane);
    fs::create_dir_all(&folder)?;
    let trace = folder.join("trace.jsonl");
    match fs::remove_file(&trace) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let mut env = vec![];
    for (key, value) in environment {
        set(&mut env, key.clone(), value.clone());
    }
    for (key, value) in &case.environment {
        set(&mut env, key.into(), value.into());
    }
    set(&mut env, "CK_TRACE".into(), trace.as_os_str().to_owned());
    let mut argv = vec![
        binary.as_os_str().to_owned(),
        case.mode.clone().into(),
        case.seed.to_string().into(),
    ];
    if let Some(qemu) = qemu {
        let sysroot = sysroot.ok_or("qemu requires sysroot")?;
        let mut preload = OsString::from("LD_PRELOAD=");
        preload.push(library);
        argv = [
            vec![
                qemu.as_os_str().to_owned(),
                "-L".into(),
                sysroot.as_os_str().to_owned(),
                "-E".into(),
                preload,
            ],
            argv,
        ]
        .concat();
    } else {
        let asan = "/usr/lib/llvm-18/lib/clang/18/lib/linux";
        let mut preload = OsString::from(format!("{asan}/libclang_rt.asan-x86_64.so:"));
        preload.push(library);
        for (key, value) in [
            (OsString::from("LD_PRELOAD"), preload),
            ("LD_LIBRARY_PATH".into(), asan.into()),
            (
                "ASAN_OPTIONS".into(),
                "detect_leaks=0:abort_on_error=1".into(),
            ),
            (
                "UBSAN_OPTIONS".into(),
                "halt_on_error=1:print_stacktrace=1".into(),
            ),
        ] {
            set(&mut env, key, value);
        }
    }
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(30));
    spec.set_os_env(env.iter().cloned().collect());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.inherit_stdin = true;
    spec.disable_core_dump = true;
    let process = runner.run(&spec)?;
    let stdout = text(process.stdout)?;
    let stderr = text(process.stderr)?;
    fs::write(folder.join("stdout"), &stdout)?;
    fs::write(folder.join("stderr"), &stderr)?;
    let sanitizer = [
        "ERROR: AddressSanitizer",
        "runtime error:",
        "Sanitizer CHECK failed",
    ]
    .iter()
    .any(|v| stderr.contains(v));
    let trace_hash = if trace.exists() {
        format!("\"{}\"", digest(&trace)?)
    } else {
        "null".into()
    };
    let metadata=JsonValue::parse(&format!("{{\"command\": {}, \"environment\": {}, \"exit\": {}, \"stdout\": {}, \"stderr_sha256\": \"{}\", \"trace_sha256\": {trace_hash}, \"sanitizer_failure\": {sanitizer}}}",shared::argv_json(&argv)?,object(&env,true)?,process.code,JsonValue::text(&stdout).to_json()?,digest(&folder.join("stderr"))?))?;
    fs::write(folder.join("run.json"), json::pretty(&metadata, 0)? + "\n")?;
    let records = if trace.exists() {
        let bytes = fs::read(&trace)?;
        let text = text(bytes)?;
        array(&crate::camera_sensor_check::parse_lines(&text)?)?
    } else {
        JsonValue::parse("[]")?
    };
    let records = normalize(&records)?;
    let calls = json::sequence(&records)?.len();
    Ok(Observation {
        metadata,
        records,
        exit: process.code,
        sanitizer,
        stdout,
        calls,
    })
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    source: &Path,
    native: &Path,
    library: &Path,
    output: &Path,
    qemu: Option<&Path>,
    sysroot: Option<&Path>,
    baseline_only: bool,
    cases: &[Scenario],
    environment: &[(OsString, OsString)],
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<usize> {
    let source = lexical(source);
    let native = lexical(native);
    let library = lexical(library);
    let output = lexical(output);
    let qemu = qemu.map(lexical);
    let sysroot = sysroot.map(lexical);
    fs::create_dir_all(&output)?;
    let mut binaries: Vec<(String, String)> = vec![];
    for path in [&source, &native, &library] {
        let key = python_fs::json_string(path.as_os_str())?;
        let value = digest(path)?;
        if let Some((_, old)) = binaries.iter_mut().find(|(prior, _)| *prior == key) {
            *old = value;
        } else {
            binaries.push((key, value));
        }
    }
    let binaries = format!(
        "{{{}}}",
        binaries
            .iter()
            .map(|(key, value)| format!("{key}: \"{value}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let selected = if baseline_only {
        baseline(cases)
    } else {
        cases.to_vec()
    };
    let mut entries = vec![];
    let mut failures = vec![];
    for case in &selected {
        let left = run(
            &source,
            &library,
            &output,
            qemu.as_deref(),
            sysroot.as_deref(),
            case,
            "source",
            environment,
            runner,
        )?;
        let right = run(
            &native,
            &library,
            &output,
            qemu.as_deref(),
            sysroot.as_deref(),
            case,
            "native",
            environment,
            runner,
        )?;
        let passed = if left.exit == 0 && right.exit == 0 && !left.sanitizer && !right.sanitizer {
            let a = json::parse_text(if left.stdout.is_empty() {
                "null"
            } else {
                &left.stdout
            })?;
            let b = json::parse_text(if right.stdout.is_empty() {
                "null"
            } else {
                &right.stdout
            })?;
            json::equal(&a, &b) && json::equal(&left.records, &right.records)
        } else {
            false
        };
        let entry=format!("\"name\": {}, \"mode\": {}, \"seed\": {}, \"environment\": {}, \"passed\": {passed}, \"calls\": {}",JsonValue::text(&case.name).to_json()?,JsonValue::text(&case.mode).to_json()?,case.seed,overrides(case)?,left.calls);
        entries.push(format!("{{{entry}}}"));
        if !passed {
            let a = json::sequence(&left.records)?;
            let b = json::sequence(&right.records)?;
            let difference = a
                .iter()
                .zip(&b)
                .enumerate()
                .find(|(_, (a, b))| !json::equal(a, b))
                .map(|(i, (a, b))| {
                    Ok::<_, Box<dyn std::error::Error>>(format!(
                        "[[{i}, {}, {}]]",
                        a.to_json()?,
                        b.to_json()?
                    ))
                })
                .transpose()?
                .unwrap_or_else(|| "[]".into());
            failures.push(format!("{{{entry}, \"source\": {}, \"native\": {}, \"lengths\": [{}, {}], \"first_difference\": {difference}}}",left.metadata.to_json()?,right.metadata.to_json()?,left.calls,right.calls));
        }
        let report = JsonValue::parse(&format!(
            "{{\"binaries\": {binaries}, \"cases\": [{}], \"failures\": [{}]}}",
            entries.join(", "),
            failures.join(", ")
        ))?;
        fs::write(output.join("report.json"), json::pretty(&report, 0)? + "\n")?;
        writeln!(
            printed,
            "{} {}",
            case.name,
            if passed { "PASS" } else { "FAIL" }
        )?;
        printed.flush()?;
    }
    writeln!(
        printed,
        "{{\"cases\": {}, \"failures\": {}}}",
        selected.len(),
        failures.len()
    )?;
    Ok(failures.len())
}
pub fn cli() -> Result<usize> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--qemu", false),
            OptionSpec::value("--sysroot", false),
            OptionSpec::flag("--baseline-only", None),
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-kernel --source PATH --native PATH --fixture PATH --output PATH [--baseline-only] [--qemu PATH --sysroot PATH]");
        return Ok(0);
    }
    check(
        Path::new(&options.values["--source"]),
        Path::new(&options.values["--native"]),
        Path::new(&options.values["--fixture"]),
        Path::new(&options.values["--output"]),
        options.values.get("--qemu").map(Path::new),
        options.values.get("--sysroot").map(Path::new),
        options.values.contains_key("--baseline-only"),
        &scenarios(),
        &std::env::vars_os().collect::<Vec<_>>(),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
