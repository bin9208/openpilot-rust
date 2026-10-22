//! Original complete camera lifecycle comparison with live libc/preload boundaries.
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

#[path = "camera_lifecycle_cleanup.rs"]
mod cleanup;
#[path = "camera_lifecycle_images.rs"]
mod image_values;
#[path = "camera_lifecycle_values.rs"]
pub(crate) mod values;
pub use cleanup::{audit, normalize_disabled};
pub use image_values::{images, result_value};
use values::{error, PythonError};
#[derive(Clone)]
pub struct Scenario {
    pub name: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}
fn scenario(name: String, arguments: Vec<String>, environment: Vec<(&str, String)>) -> Scenario {
    Scenario {
        name,
        arguments,
        environment: environment
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
    }
}
pub fn scenarios(smoke: bool, cleanup_only: bool, selected: &[OsString]) -> Result<Vec<Scenario>> {
    let mut cases = vec![];
    for (sensor, id) in [("ar", 0x354), ("ox", 0x5803), ("os", 0x5304)] {
        for mode in 0..3 {
            for depth in [1, 18] {
                cases.push(scenario(
                    format!("{sensor}-mode-{mode}-depth-{depth}"),
                    vec![mode.to_string(), depth.to_string(), "3".into()],
                    vec![("CK_SENSOR", id.to_string())],
                ));
            }
        }
    }
    if smoke {
        cases = vec![cases[2].clone(), cases[cases.len() - 2].clone()];
    } else {
        for mode in [1, 2] {
            for event in ["stale", "frame-gap", "request-gap", "invalid"] {
                let count = if event == "invalid" { 22 } else { 4 };
                cases.push(scenario(
                    format!("events-{mode}-{event}"),
                    vec![mode.to_string(), "18".into(), count.to_string()],
                    vec![
                        ("CK_SENSOR", 0x5304.to_string()),
                        ("CK_EVENT_CASE", event.into()),
                    ],
                ));
            }
            for operation in ["sync:6", "sync:1", "camera:271"] {
                cases.push(scenario(
                    format!("error-{mode}-{operation}"),
                    vec![mode.to_string(), "18".into(), "3".into()],
                    vec![
                        ("CK_SENSOR", 0x5304.to_string()),
                        ("CK_FAIL_OP", operation.into()),
                        ("CK_FAIL_COUNT", "1".into()),
                        ("CK_FAIL_ERRNO", "5".into()),
                    ],
                ));
            }
        }
    }
    if cleanup_only {
        let sensor = 0x5304.to_string();
        let failure = vec![
            ("CK_SENSOR", sensor.clone()),
            ("CK_FAIL_COUNT", "1".into()),
            ("CK_FAIL_ERRNO", "5".into()),
        ];
        cases = vec![
            scenario(
                "cleanup-disabled".into(),
                vec!["1".into(), "1".into(), "0".into()],
                vec![
                    ("CK_SENSOR", sensor.clone()),
                    ("CK_ENABLED", "0".into()),
                    ("CK_CLEANUP_KIND", "disabled".into()),
                ],
            ),
            scenario(
                "cleanup-probe-none".into(),
                vec!["1".into(), "1".into(), "0".into()],
                vec![
                    ("CK_SENSOR", "-1".into()),
                    ("CK_CLEANUP_KIND", "probe-none".into()),
                ],
            ),
        ];
        let mut overrides = failure.clone();
        overrides.extend([
            ("CK_FAIL_OP", "camera:261".into()),
            ("CK_FAIL_TARGET", "502".into()),
            ("CK_FAIL_PACKET", "4".into()),
            ("CK_CLEANUP_KIND", "sensor-init".into()),
        ]);
        cases.push(scenario(
            "cleanup-sensor-init".into(),
            vec!["1".into(), "1".into(), "0".into()],
            overrides,
        ));
        for mode in [1, 2] {
            let mut overrides = failure.clone();
            overrides.extend([
                ("CK_FAIL_OP", "camera:261".into()),
                ("CK_FAIL_TARGET", "502".into()),
                ("CK_FAIL_PACKET", "127".into()),
                ("CK_CLEANUP_KIND", "sensor-poke".into()),
            ]);
            cases.push(scenario(
                format!("cleanup-sensor-poke-{mode}"),
                vec![mode.to_string(), "18".into(), "0".into()],
                overrides,
            ));
        }
        for (name, op, target, args, skip) in [
            (
                "cleanup-isp-acquire",
                "camera:258",
                "503",
                ["1", "1", "0"],
                None,
            ),
            (
                "cleanup-phy-config",
                "camera:261",
                "506",
                ["1", "1", "0"],
                None,
            ),
            (
                "cleanup-raw-map",
                "camera:275",
                "501",
                ["2", "2", "0"],
                Some("2"),
            ),
        ] {
            let mut overrides = failure.clone();
            overrides.extend([("CK_FAIL_OP", op.into()), ("CK_FAIL_TARGET", target.into())]);
            if let Some(skip) = skip {
                overrides.push(("CK_FAIL_SKIP", skip.into()));
            }
            overrides.push(("CK_EXPECT_SOURCE_ABORT", "1".into()));
            cases.push(scenario(
                name.into(),
                args.map(String::from).into(),
                overrides,
            ));
        }
    }
    if !selected.is_empty() {
        cases.retain(|case| {
            selected
                .iter()
                .any(|name| name == &OsString::from(&case.name))
        });
        values::assert(!cases.is_empty(), "")?;
    }
    Ok(cases)
}
// urllib.quote always leaves ASCII alphanumerics and ~ even with safe='-_.'.
fn quote_name(name: &str) -> String {
    let mut out = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            use std::fmt::Write;
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}
fn fresh_directory(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    Ok(fs::create_dir(path)?)
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
            && !["CK_", "ASAN_", "UBSAN_", "LD_", "OPENPILOT_"]
                .iter()
                .any(|prefix| {
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
    fresh_directory(&folder)?;
    let trace = folder.join("trace.jsonl");
    let mut env = vec![];
    for (key, value) in environment {
        set(&mut env, key.clone(), value.clone());
    }
    for (key, value) in &case.environment {
        set(&mut env, key.into(), value.into());
    }
    set(&mut env, "CK_TRACE".into(), trace.as_os_str().to_owned());
    let namespace = tempfile::Builder::new()
        .prefix("msgq_camera_lifecycle_")
        .tempdir_in("/dev/shm")?;
    let prefix = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap()
        .to_owned();
    set(&mut env, "OPENPILOT_PREFIX".into(), prefix.into());
    let denied = ["CEREAL_FAKE", "SPECTRA_ERROR_PROB", "SPECTRA_ERROR_DT"];
    env.retain(|(key, _)| {
        !denied
            .iter()
            .any(|name| key.as_os_str() == std::ffi::OsStr::new(name))
    });
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
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(90));
    spec.set_os_env(env.iter().cloned().collect());
    spec.env_remove = denied.map(OsString::from).into();
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.inherit_stdin = true;
    spec.disable_core_dump = true;
    let process = runner.run(&spec)?;
    let stdout = text(process.stdout)?;
    let stderr = text(process.stderr)?;
    namespace.close()?;
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
    Ok(Observation {
        metadata,
        records,
        exit: process.code,
        sanitizer,
        stdout,
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
    free_bytes: u64,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<usize> {
    let output = lexical(output);
    fresh_directory(&output)?;
    compare(
        source,
        native,
        library,
        &output,
        qemu,
        sysroot,
        cases,
        environment,
        free_bytes,
        runner,
        printed,
    )
}
#[allow(clippy::too_many_arguments)]
fn compare(
    source: &Path,
    native: &Path,
    library: &Path,
    output: &Path,
    qemu: Option<&Path>,
    sysroot: Option<&Path>,
    cases: &[Scenario],
    environment: &[(OsString, OsString)],
    free_bytes: u64,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<usize> {
    values::assert(free_bytes >= 1024u64.pow(3), "")?;
    let source = lexical(source);
    let native = lexical(native);
    let library = lexical(library);
    let output = lexical(output);
    let qemu = qemu.map(lexical);
    let sysroot = sysroot.map(lexical);

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
        let mut cleaned = native.records.clone();
        let mut ownership = values::object(&[])?;
        let mut source_ids = values::object(&[])?;
        let mut native_ids = values::object(&[])?;
        let mut source_rows = source.records.clone();
        let mut resources = values::object(&[])?;
        let mut extra_cleanup = array(&[])?;
        let mut failure = None;
        let expected_abort = case
            .environment
            .iter()
            .any(|(key, value)| key == "CK_EXPECT_SOURCE_ABORT" && value == "1");
        let comparison = (|| -> Result<bool> {
            resources = audit(&native.records)?;
            let (normalized, owned, normalization_error) =
                crate::camera_isp_lifecycle_check::normalize(&source.records, &native.records)
                    .map_err(isp_error)?;
            if let Some(error) = normalization_error {
                return Err(values::error("AssertionError", error));
            }
            cleaned = normalized;
            ownership = owned;
            if let Some((_, kind)) = case
                .environment
                .iter()
                .find(|(key, _)| key == "CK_CLEANUP_KIND")
                .filter(|(_, kind)| !kind.is_empty())
            {
                let (rows, receipt) = normalize_disabled(&source.records, &cleaned, kind)?;
                cleaned = rows;
                extra_cleanup = receipt;
            }
            let (rows, ids) = images(&source.records)?;
            source_rows = rows;
            source_ids = ids;
            let (rows, ids) = images(&cleaned)?;
            cleaned = rows;
            native_ids = ids;
            if expected_abort {
                let stop = |records: &JsonValue| -> Result<JsonValue> {
                    let rows = json::sequence(records)?;
                    for (index, row) in rows.iter().enumerate() {
                        if values::get(row, "op")?.text_eq("ioctl")
                            && !json::equal(&values::get(row, "ret")?, &values::number(0))
                        {
                            return array(&rows[..index + 1]);
                        }
                    }
                    Err(error("StopIteration", ""))
                };
                // Compute both slice bounds before either assignment, as Python.
                let left = stop(&source_rows)?;
                let right = stop(&cleaned)?;
                source_rows = left;
                cleaned = right;
                Ok(source.exit == -6 && native.exit == 1)
            } else {
                Ok(json::equal(
                    &result_value(&source.stdout, false)?,
                    &result_value(&native.stdout, true)?,
                ))
            }
        })();
        let same_result = match comparison {
            Ok(result) => result,
            Err(e) => {
                if let Some(py) = e.downcast_ref::<PythonError>() {
                    if matches!(
                        py.kind,
                        "AssertionError" | "KeyError" | "ValueError" | "JSONDecodeError"
                    ) {
                        failure = Some(py.message.clone());
                        false
                    } else {
                        return Err(e);
                    }
                } else {
                    return Err(e);
                }
            }
        };
        let successful_exit = expected_abort && same_result || source.exit == 0 && native.exit == 0;
        let passed = successful_exit
            && !source.sanitizer
            && !native.sanitizer
            && failure.is_none()
            && same_result
            && json::equal(&source_rows, &cleaned);
        let calls = json::sequence(&source_rows)?.len();
        entries.push(format!("{{\"name\": {}, \"passed\": {passed}, \"calls\": {calls}, \"source_image_fds\": {}, \"native_image_fds\": {}, \"ownership\": {}, \"native_resource_cleanup\": {}, \"native_extra_cleanup\": {}, \"expected_source_abort\": {expected_abort}}}",JsonValue::text(&case.name).to_json()?,source_ids.to_json()?,native_ids.to_json()?,ownership.to_json()?,resources.to_json()?,extra_cleanup.to_json()?));
        if !passed {
            let a = json::sequence(&source_rows)?;
            let b = json::sequence(&cleaned)?;
            let first = a
                .iter()
                .zip(&b)
                .enumerate()
                .find(|(_, (a, b))| !json::equal(a, b));
            let difference = if let Some((index, (a, b))) = first {
                format!("[{index}, {}, {}]", a.to_json()?, b.to_json()?)
            } else {
                "null".into()
            };
            failures.push(format!("{{\"name\": {}, \"source\": {}, \"native\": {}, \"error\": {}, \"lengths\": [{}, {}], \"first_difference\": {difference}, \"equal_results\": {same_result}}}",JsonValue::text(&case.name).to_json()?,source.metadata.to_json()?,native.metadata.to_json()?,failure.as_deref().map(JsonValue::text).unwrap_or_else(values::null).to_json()?,a.len(),b.len()));
        }
        let report = JsonValue::parse(&format!(
            "{{\"binaries\": {binaries}, \"free_bytes\": {free_bytes}, \"estimated_growth_bytes\": 1073741824, \"cases\": [{}], \"failures\": [{}]}}",
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
        if !passed {
            break;
        }
    }
    writeln!(
        printed,
        "{{\"cases\": {}, \"failures\": {}}}",
        entries.len(),
        failures.len()
    )?;
    Ok(failures.len())
}
fn isp_error(cause: Box<dyn std::error::Error>) -> Box<dyn std::error::Error> {
    let text = cause.to_string();
    if let Some(key) = text.strip_prefix("missing or invalid ISP trace field ") {
        return error("KeyError", format!("'{key}'"));
    }
    if text.contains("hexadecimal") {
        return error("ValueError", text);
    }
    if text.contains("unhashable") {
        return error("TypeError", "unhashable type: 'list'");
    }
    cause
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
            OptionSpec::flag("--smoke", None),
            OptionSpec::flag("--cleanup-only", None),
            OptionSpec::append("--case"),
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-camera-lifecycle --source PATH --native PATH --fixture PATH --output PATH [--smoke] [--case NAME] [--cleanup-only] [--qemu PATH --sysroot PATH]");
        return Ok(0);
    }
    let output = lexical(Path::new(&options.values["--output"]));
    fresh_directory(&output)?;
    let stat = rustix::fs::statvfs(&output)?;
    let free = stat.f_bavail.saturating_mul(stat.f_frsize);
    values::assert(free >= 1024u64.pow(3), "")?;
    let selected = options
        .append_values
        .get("--case")
        .cloned()
        .unwrap_or_default();
    let cases = scenarios(
        options.values.contains_key("--smoke"),
        options.values.contains_key("--cleanup-only"),
        &selected,
    )?;
    compare(
        Path::new(&options.values["--source"]),
        Path::new(&options.values["--native"]),
        Path::new(&options.values["--fixture"]),
        &output,
        options.values.get("--qemu").map(Path::new),
        options.values.get("--sysroot").map(Path::new),
        &cases,
        &std::env::vars_os().collect::<Vec<_>>(),
        free,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
