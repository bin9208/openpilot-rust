//! Original ISP lifecycle ownership comparison with live libc/preload boundaries.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_exposure_check::shared,
    camera_sensor_check::lexical,
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Write, path::Path, time::Duration};

#[derive(Clone)]
pub struct Scenario {
    pub name: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}
pub fn scenarios(smoke: bool) -> Vec<Scenario> {
    let mut cases = vec![];
    for kind in 1..4 {
        for mode in 0..3 {
            for depth in [1, 18] {
                cases.push(Scenario {
                    name: format!("sensor-{kind}-mode-{mode}-depth-{depth}"),
                    arguments: vec![kind.to_string(), mode.to_string(), depth.to_string()],
                    environment: vec![],
                });
            }
        }
    }
    if smoke {
        return vec![cases[0].clone(), cases[4].clone()];
    }
    for mode in [1, 2] {
        for operation in [258, 261, 274] {
            for count in [1, 100] {
                cases.push(Scenario {
                    name: format!("retry-{mode}-{operation}-{count}"),
                    arguments: vec!["3".into(), mode.to_string(), "18".into()],
                    environment: vec![
                        ("CK_FAIL_OP".into(), format!("camera:{operation}")),
                        ("CK_FAIL_COUNT".into(), count.to_string()),
                        ("CK_FAIL_ERRNO".into(), "4".into()),
                    ],
                });
            }
        }
    }
    cases
}
fn member(record: &JsonValue, key: &str) -> Result<JsonValue> {
    record
        .get(key)
        .ok_or_else(|| format!("missing or invalid ISP trace field {key}").into())
}
fn hashable(value: &JsonValue) -> Result<()> {
    if matches!(value.view(), JsonView::Object(_) | JsonView::Array(_)) {
        return Err("unhashable ISP descriptor".into());
    }
    Ok(())
}
fn belongs(value: &JsonValue, fds: &[u32]) -> Result<bool> {
    hashable(value)?;
    Ok(fds
        .iter()
        .any(|fd| json::equal(value, &JsonValue::parse(&fd.to_string()).unwrap())))
}
fn from_hex(value: &JsonValue) -> Result<Vec<u8>> {
    let JsonView::Text(points) = value.view() else {
        return Err("fromhex argument must be str".into());
    };
    let mut output = vec![];
    let mut index = 0;
    let whitespace = |v: u32| matches!(v, 9..=13 | 32);
    let digit = |v: u32| {
        char::from_u32(v).and_then(|c| {
            if c.is_ascii_hexdigit() {
                c.to_digit(16)
            } else {
                None
            }
        })
    };
    while index < points.len() {
        if whitespace(points[index]) {
            index += 1;
            continue;
        }
        let high = digit(points[index]).ok_or("invalid hexadecimal trace byte")?;
        let low = points
            .get(index + 1)
            .and_then(|&v| digit(v))
            .ok_or("invalid hexadecimal trace byte")?;
        output.push((high * 16 + low) as u8);
        index += 2;
    }
    Ok(output)
}
fn little(bytes: &[u8], start: usize) -> u32 {
    bytes
        .iter()
        .skip(start)
        .take(4)
        .enumerate()
        .fold(0, |value, (index, byte)| {
            value | (u32::from(*byte) << (index * 8))
        })
}
fn replace_stage(record: &JsonValue) -> Result<JsonValue> {
    let JsonView::Object(fields) = record.view() else {
        return Err("expected memory record mapping".into());
    };
    let mut out = vec![];
    let mut replaced = false;
    for (key, value) in fields {
        let key_json = JsonValue::codepoints(key.to_vec())
            .ok_or("invalid field key")?
            .to_json()?;
        if key == "stage".chars().map(u32::from).collect::<Vec<_>>() {
            replaced = true;
            out.push(format!("{key_json}: \"release\""));
        } else {
            out.push(format!("{key_json}: {}", value.to_json()?));
        }
    }
    if !replaced {
        out.push("\"stage\": \"release\"".into());
    }
    Ok(JsonValue::parse(&format!("{{{}}}", out.join(", ")))?)
}
/// Return an assertion failure as data, matching the original main's narrow
/// catch. Invalid JSON record shapes, fields and hex data propagate as errors.
pub fn normalize(
    source: &JsonValue,
    native: &JsonValue,
) -> Result<(JsonValue, JsonValue, Option<String>)> {
    let source = json::sequence(source)?;
    let native_records = json::sequence(native)?;
    let mut allocations: Vec<(u32, u32)> = vec![];
    for record in &source {
        if !matches!(record.view(), JsonView::Object(_)) {
            return Err("source trace record has no get method".into());
        }
        if record.get("name").is_some_and(|v| v.text_eq("camera:274"))
            && json::equal(&member(record, "ret")?, &JsonValue::parse("0")?)
        {
            let before = from_hex(&member(record, "before")?)?;
            let after = from_hex(&member(record, "after")?)?;
            let fd = little(&after, 92);
            let flags = little(&before, 84);
            if let Some((_, prior)) = allocations.iter_mut().find(|(prior, _)| *prior == fd) {
                *prior = flags;
            } else {
                allocations.push((fd, flags));
            }
        }
    }
    let owned = allocations
        .iter()
        .filter(|(_, flags)| *flags != 0x58)
        .map(|(fd, _)| *fd)
        .collect::<Vec<_>>();
    let blobs = allocations
        .iter()
        .filter(|(_, flags)| *flags == 0x811)
        .map(|(fd, _)| *fd)
        .collect::<Vec<_>>();
    let failed = |error: String| -> Result<(JsonValue, JsonValue, Option<String>)> {
        Ok((native.clone(), JsonValue::parse("{}")?, Some(error)))
    };
    let mut closed: Vec<(JsonValue, usize)> = vec![];
    for record in &native_records {
        if member(record, "op")?.text_eq("close") {
            let fd = member(record, "fd")?;
            if belongs(&fd, &owned)? {
                if let Some((_, count)) =
                    closed.iter_mut().find(|(prior, _)| json::equal(prior, &fd))
                {
                    *count += 1;
                } else {
                    closed.push((fd, 1));
                }
            }
        }
    }
    if closed.len() != owned.len() || closed.iter().any(|(_, count)| *count != 1) {
        let mut counts = closed.clone();
        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let repr = |value: &JsonValue| -> Result<String> {
            Ok(match value.view() {
                JsonView::Bool(v) => {
                    if v {
                        "True".into()
                    } else {
                        "False".into()
                    }
                }
                _ => value.to_json()?,
            })
        };
        let counter = if counts.is_empty() {
            "Counter()".into()
        } else {
            format!(
                "Counter({{{}}})",
                counts
                    .iter()
                    .map(|(fd, count)| Ok(format!("{}: {count}", repr(fd)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            )
        };
        let set = if owned.is_empty() {
            "set()".into()
        } else {
            format!(
                "{{{}}}",
                python_set_order(&owned)
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return failed(format!("({counter}, {set})"));
    }
    let mut unmaps = 0;
    for record in &native_records {
        if member(record, "op")?.text_eq("munmap") && belongs(&member(record, "fd")?, &blobs)? {
            unmaps += 1;
        }
    }
    if unmaps != blobs.len() {
        return failed(String::new());
    }
    for fd in python_set_order(&blobs) {
        let fd_value = JsonValue::parse(&fd.to_string())?;
        let mut observed = vec![];
        let mut expected = vec![];
        for record in &native_records {
            if member(record, "op")?.text_eq("memory")
                && member(record, "stage")?.text_eq("release")
                && json::equal(&member(record, "fd")?, &fd_value)
            {
                observed.push(record.clone());
            }
        }
        for record in &source {
            if member(record, "op")?.text_eq("memory")
                && member(record, "stage")?.text_eq("snapshot")
                && json::equal(&member(record, "fd")?, &fd_value)
            {
                expected.push(replace_stage(record)?);
            }
        }
        if !json::equal(&array(&observed)?, &array(&expected)?) {
            return failed(format!(
                "BPS config allocation {fd} changed between acquire and release"
            ));
        }
    }
    let mut output = vec![];
    for record in native_records {
        let op = member(&record, "op")?;
        if (op.text_eq("close") && belongs(&member(&record, "fd")?, &owned)?)
            || (op.text_eq("munmap") && belongs(&member(&record, "fd")?, &blobs)?)
            || (op.text_eq("memory")
                && member(&record, "stage")?.text_eq("release")
                && belongs(&member(&record, "fd")?, &blobs)?)
        {
            continue;
        }
        output.push(record);
    }
    let mut owned = owned;
    owned.sort_unstable();
    let mut blobs = blobs;
    blobs.sort_unstable();
    let numbers = |fds: &[u32]| {
        fds.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ownership = JsonValue::parse(&format!("{{\"native_closes_source_leaked_fds\": [{}], \"native_unmaps_source_leaked_config\": [{}]}}",numbers(&owned),numbers(&blobs)))?;
    Ok((array(&output)?, ownership, None))
}
// CPython 3.12 set insertion for nonnegative u32 descriptor hashes. Reproduces
// iteration for assertion diagnostics; report ownership itself is sorted.
fn python_set_order(values: &[u32]) -> Vec<u32> {
    fn insert(table: &mut [Option<u32>], value: u32) {
        let mask = table.len() - 1;
        let hash = value as usize;
        let mut perturb = hash;
        let mut i = hash & mask;
        loop {
            let probes = if i + 9 <= mask { 9 } else { 0 };
            for offset in 0..=probes {
                let slot = &mut table[i + offset];
                if slot.is_none() {
                    *slot = Some(value);
                    return;
                }
                if *slot == Some(value) {
                    return;
                }
            }
            perturb >>= 5;
            i = (i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb)) & mask;
        }
    }
    let mut table = vec![None; 8];
    let mut used = 0;
    for &value in values {
        insert(&mut table, value);
        used += 1;
        if used * 5 >= (table.len() - 1) * 3 {
            let minimum = used * if used > 50_000 { 2 } else { 4 };
            let mut size = 8;
            while size <= minimum {
                size *= 2;
            }
            let mut resized = vec![None; size];
            for &value in table.iter().flatten() {
                insert(&mut resized, value);
            }
            table = resized;
        }
    }
    table.into_iter().flatten().collect()
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
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(45));
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
        // Only AssertionError is caught by the original. Other record/type/hex
        // failures escape after both child receipts, before updating report.json.
        let (cleaned, ownership, error) = normalize(&source.records, &native.records)?;
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
                json::equal(&a, &b) && error.is_none() && json::equal(&source.records, &cleaned)
            } else {
                false
            };
        entries.push(format!(
            "{{\"name\": {}, \"passed\": {passed}, \"calls\": {}, \"ownership\": {}}}",
            JsonValue::text(&case.name).to_json()?,
            source.calls,
            ownership.to_json()?
        ));
        if !passed {
            let a = json::sequence(&source.records)?;
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
            failures.push(format!("{{\"name\": {}, \"source\": {}, \"native\": {}, \"lengths\": [{}, {}], \"ownership_error\": {}, \"first_difference\": {difference}}}",JsonValue::text(&case.name).to_json()?,source.metadata.to_json()?,native.metadata.to_json()?,source.calls,b.len(),error.as_deref().map(JsonValue::text).unwrap_or_else(|| JsonValue::parse("null").unwrap()).to_json()?));
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
        ],
    )?;
    if options.help {
        println!("usage: check-camerad-isp-lifecycle --source PATH --native PATH --fixture PATH --output PATH [--qemu PATH --sysroot PATH] [--smoke]");
        return Ok(0);
    }
    check(
        Path::new(&options.values["--source"]),
        Path::new(&options.values["--native"]),
        Path::new(&options.values["--fixture"]),
        Path::new(&options.values["--output"]),
        options.values.get("--qemu").map(Path::new),
        options.values.get("--sysroot").map(Path::new),
        &scenarios(options.values.contains_key("--smoke")),
        &std::env::vars_os().collect::<Vec<_>>(),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
