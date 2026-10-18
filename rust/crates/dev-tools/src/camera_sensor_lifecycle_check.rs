//! Original sensor lifecycle comparison with live libc/preload boundaries.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_exposure_check::shared,
    camera_sensor_check::lexical,
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Write, path::Path, time::Duration};

#[derive(Clone)]
pub struct Scenario {
    pub name: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}
fn scenario(name: String, port: u32, enabled: bool, environment: Vec<(&str, String)>) -> Scenario {
    Scenario {
        name,
        arguments: vec![port.to_string(), u8::from(enabled).to_string()],
        environment: environment
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    }
}
pub fn scenarios() -> Vec<Scenario> {
    let mut cases = vec![];
    for port in 0..3 {
        for enabled in [false, true] {
            let flag = if enabled { "True" } else { "False" };
            for failed in 0..4 {
                cases.push(scenario(
                    format!("port-{port}-enabled-{flag}-probe-{failed}"),
                    port,
                    enabled,
                    vec![
                        ("CK_FAIL_OP", "camera:266".into()),
                        ("CK_FAIL_COUNT", failed.to_string()),
                        ("CK_FAIL_ERRNO", "5".into()),
                    ],
                ));
            }
            for (operation, skip) in [
                ("camera:267", 0),
                ("camera:261", 0),
                ("camera:261", 1),
                ("camera:261", 2),
                ("camera:261", 3),
            ] {
                cases.push(scenario(
                    format!("port-{port}-enabled-{flag}-{operation}-{skip}"),
                    port,
                    enabled,
                    vec![
                        ("CK_FAIL_OP", operation.into()),
                        ("CK_FAIL_SKIP", skip.to_string()),
                        ("CK_FAIL_COUNT", "1".into()),
                        ("CK_FAIL_ERRNO", "5".into()),
                    ],
                ));
            }
        }
    }
    for operation in ["camera:266", "camera:267", "camera:258", "camera:261"] {
        for count in [1, 100] {
            cases.push(scenario(
                format!("retry-{operation}-{count}"),
                1,
                true,
                vec![
                    ("CK_FAIL_OP", operation.into()),
                    ("CK_FAIL_COUNT", count.to_string()),
                    ("CK_FAIL_ERRNO", "4".into()),
                ],
            ));
        }
    }
    cases
}
// urllib.parse.quote(name, safe='-_.') always leaves ASCII letters/digits/~.
fn quote_name(name: &str) -> String {
    let mut output = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            output.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(output, "%{byte:02X}").unwrap();
        }
    }
    output
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
    let folder = output.join(quote_name(&case.name)).join(lane);
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
    let mut argv = vec![binary.as_os_str().to_owned()];
    argv.extend(case.arguments.iter().map(OsString::from));
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
    let records = if trace.exists() {
        let bytes = fs::read(&trace)?;
        let text = text(bytes)?;
        array(&crate::camera_sensor_check::parse_lines(&text)?)?
    } else {
        JsonValue::parse("[]")?
    };
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
        if let Some((_, prior)) = binaries.iter_mut().find(|(prior, _)| *prior == key) {
            *prior = value;
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
    let mut entries = vec![];
    let mut failures = vec![];
    for case in cases {
        let source = run(
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
        let native = run(
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
        let passed =
            if source.exit == 0 && native.exit == 0 && !source.sanitizer && !native.sanitizer {
                let a = json::parse_text(if source.stdout.is_empty() {
                    "null"
                } else {
                    &source.stdout
                })?;
                let b = json::parse_text(if native.stdout.is_empty() {
                    "null"
                } else {
                    &native.stdout
                })?;
                json::equal(&a, &b) && json::equal(&source.records, &native.records)
            } else {
                false
            };
        let mut snapshots = 0;
        for record in json::sequence(&source.records)? {
            if record
                .get("op")
                .ok_or("sensor lifecycle trace missing op")?
                .text_eq("snapshot")
            {
                snapshots += 1;
            }
        }
        entries.push(format!(
            "{{\"name\": {}, \"passed\": {passed}, \"calls\": {}, \"snapshots\": {snapshots}}}",
            JsonValue::text(&case.name).to_json()?,
            source.calls
        ));
        if !passed {
            let a = json::sequence(&source.records)?;
            let b = json::sequence(&native.records)?;
            let first = a
                .iter()
                .zip(&b)
                .enumerate()
                .find(|(_, (a, b))| !json::equal(a, b));
            let difference = if let Some((index, (a, b))) = first {
                format!("[[{index}, {}, {}]]", a.to_json()?, b.to_json()?)
            } else {
                "[]".into()
            };
            failures.push(format!("{{\"name\": {}, \"source\": {}, \"native\": {}, \"lengths\": [{}, {}], \"first_difference\": {difference}}}",JsonValue::text(&case.name).to_json()?,source.metadata.to_json()?,native.metadata.to_json()?,source.calls,native.calls));
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
        cases.len(),
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
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-sensor-lifecycle --source PATH --native PATH --fixture PATH --output PATH [--qemu PATH --sysroot PATH]");
        return Ok(0);
    }
    check(
        Path::new(&options.values["--source"]),
        Path::new(&options.values["--native"]),
        Path::new(&options.values["--fixture"]),
        Path::new(&options.values["--output"]),
        options.values.get("--qemu").map(Path::new),
        options.values.get("--sysroot").map(Path::new),
        &scenarios(),
        &std::env::vars_os().collect::<Vec<_>>(),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
