//! Native continuous camera stress comparator preserving original process phases.
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
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
#[path = "camera_stress_cases.rs"]
mod cases;
pub use cases::{scenarios, Scenario};
pub struct Options {
    pub reference: PathBuf,
    pub native: PathBuf,
    pub parser: PathBuf,
    pub fixture: PathBuf,
    pub output: PathBuf,
    pub qemu: Option<PathBuf>,
    pub sysroot: Option<PathBuf>,
}
fn object(fields: &[(&str, String)]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(k, v)| format!("\"{k}\": {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))?)
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
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing {key}").into())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn command(opt: &Options, exe: &Path, extra: &[&str], preload: bool) -> Result<Vec<OsString>> {
    let mut argv = vec![];
    if let Some(qemu) = &opt.qemu {
        argv.extend([
            qemu.as_os_str().to_owned(),
            "-L".into(),
            opt.sysroot
                .as_ref()
                .ok_or("qemu requires sysroot")?
                .as_os_str()
                .to_owned(),
        ]);
        if preload {
            let mut lib = OsString::from("LD_PRELOAD=");
            lib.push(&opt.fixture);
            argv.extend(["-E".into(), lib]);
        }
    }
    argv.push(exe.as_os_str().to_owned());
    argv.extend(extra.iter().map(OsString::from));
    Ok(argv)
}
fn environment() -> Vec<OsString> {
    std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|key| {
            let b = key.as_encoded_bytes();
            [b"SPECTRA_ERROR_".as_slice(), b"STRESS_", b"LD_PRELOAD"]
                .iter()
                .any(|p| b.starts_with(p))
        })
        .collect()
}
fn spec(argv: Vec<OsString>, remove: &[OsString]) -> CommandSpec {
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(10));
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.os_env = Some(BTreeMap::new());
    spec.env_remove = remove.to_vec();
    spec
}
struct Observation {
    result: JsonValue,
    trace: JsonValue,
    stderr: String,
}
#[allow(clippy::too_many_arguments)]
fn run(
    opt: &Options,
    name: &str,
    side: &str,
    exe: &Path,
    env: &[(String, String)],
    actions: &str,
    extra: &[&str],
    remove: &[OsString],
    runner: &mut dyn Runner,
) -> Result<Observation> {
    let prefix = opt.output.join(format!("{name}-{side}"));
    let trace = prefix.with_extension("jsonl");
    let argv = command(opt, exe, extra, true)?;
    let mut spec = spec(argv.clone(), remove);
    let overlay = spec.os_env.as_mut().unwrap();
    for (k, v) in env {
        overlay.insert(k.into(), v.into());
    }
    overlay.insert("STRESS_TRACE".into(), trace.as_os_str().to_owned());
    if opt.qemu.is_none() {
        overlay.insert("LD_PRELOAD".into(), opt.fixture.as_os_str().to_owned());
    }
    spec.env_remove.retain(|k| !overlay.contains_key(k));
    spec.input = Some(actions.as_bytes().to_vec());
    let captured = runner.run(&spec)?;
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    fs::write(prefix.with_extension("stdout"), &stdout)?;
    fs::write(prefix.with_extension("stderr"), &stderr)?;
    let result = if captured.code == 0 {
        json::parse_text(&stdout)?
    } else {
        JsonValue::parse("null")?
    };
    let trace = if trace.exists() {
        let text = fs::read_to_string(trace)?;
        array(
            &shared::splitlines(&text)
                .into_iter()
                .map(json::parse_text)
                .collect::<Result<Vec<_>>>()?,
        )?
    } else {
        JsonValue::parse("[]")?
    };
    let environment = format!(
        "{{{}}}",
        env.iter()
            .map(|(k, v)| Ok(format!("{}: {}", shared::string(k)?, shared::string(v)?)))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    );
    let record = object(&[
        ("command", shared::argv_json(&argv)?),
        ("environment", environment),
        ("actions", shared::string(actions)?),
        ("exit", captured.code.to_string()),
        ("result", result.to_json()?),
        ("trace", trace.to_json()?),
        ("stderr", shared::string(&stderr)?),
    ])?;
    fs::write(
        prefix.with_extension("json"),
        json::pretty(&record, 0)? + "\n",
    )?;
    if captured.code != 0 {
        return Err(format!("{name}/{side} exit {}", captured.code).into());
    }
    Ok(Observation {
        result,
        trace,
        stderr,
    })
}
fn equal(a: &JsonValue, b: &JsonValue) -> Result<()> {
    if !json::equal(a, b) {
        return Err("camera stress comparison differs".into());
    }
    Ok(())
}
fn count(v: &JsonValue) -> Result<usize> {
    if let JsonView::Array(v) = v.view() {
        Ok(v.len())
    } else {
        Err("camera stress trace must be a list".into())
    }
}
// Python's `errno or before` also falls back for null and empty containers.
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value.trim_start_matches('-').bytes().any(|b| b != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
fn errno(record: &Observation) -> Result<JsonValue> {
    let before = field(&record.result, "previous_errno")?;
    if json::equal(&before, &JsonValue::parse("0")?) {
        return Err("initial errno must be nonzero".into());
    }
    let JsonView::Array(trace) = record.trace.view() else {
        return Err("invalid errno trace".into());
    };
    let error = field(trace.first().ok_or("missing errno observation")?, "errno")?;
    equal(
        &field(&record.result, "after_errno")?,
        &if truth(&error) { error } else { before.clone() },
    )?;
    Ok(before)
}
pub fn check(
    opt: &Options,
    runner: &mut dyn Runner,
    clock: &mut dyn FnMut() -> Result<u128>,
    printed: &mut dyn Write,
) -> Result<()> {
    if opt.output.exists() {
        return Err("output already exists".into());
    }
    fs::create_dir_all(&opt.output)?;
    let remove = environment();
    let mut reports = vec![];
    for case in scenarios() {
        let source = run(
            opt,
            &case.name,
            "source",
            &opt.reference,
            &case.environment,
            &case.actions,
            &[],
            &remove,
            runner,
        )?;
        let native = run(
            opt,
            &case.name,
            "native",
            &opt.native,
            &case.environment,
            &case.actions,
            &[],
            &remove,
            runner,
        )?;
        let expected = serde_json::json!({"results":case.expected.iter().map(|v|if v.is_boolean(){serde_json::json!({"triggered":v})}else{v.clone()}).collect::<Vec<_>>(),"error":case.error,"error_step":case.error.as_ref().map(|_|case.expected.len())});
        equal(&source.result, &JsonValue::parse(&expected.to_string())?)?;
        equal(&source.result, &native.result)?;
        equal(&source.trace, &native.trace)?;
        if !native.stderr.is_empty() {
            return Err("native policy stderr must be empty".into());
        }
        reports.push(object(&[
            ("name", shared::string(&case.name)?),
            ("kind", "\"policy\"".into()),
            ("observations", count(&source.trace)?.to_string()),
        ])?);
    }
    for (i, value) in cases::PARSER_VALUES.iter().enumerate() {
        let name = format!("parse-{i}");
        let source = run(
            opt,
            &name,
            "source",
            &opt.reference,
            &[],
            "",
            &["parse", value],
            &remove,
            runner,
        )?;
        let native = run(
            opt,
            &name,
            "native",
            &opt.parser,
            &[],
            "",
            &[value],
            &remove,
            runner,
        )?;
        equal(
            &field(&source.result, "result")?,
            &field(&native.result, "result")?,
        )?;
        equal(&source.trace, &native.trace)?;
        if source.stderr != native.stderr {
            return Err("parser stderr differs".into());
        }
        let s = errno(&source)?;
        let n = errno(&native)?;
        reports.push(object(&[
            ("name", shared::string(&name)?),
            ("input", shared::string(value)?),
            ("kind", "\"parser\"".into()),
            ("observations", count(&source.trace)?.to_string()),
            (
                "initial_errno",
                object(&[("source", s.to_json()?), ("native", n.to_json()?)])?.to_json()?,
            ),
        ])?);
    }
    let before = clock()?;
    let mut spec = spec(command(opt, &opt.native, &["real"], false)?, &remove);
    spec.inherit_stdin = true;
    let real = runner.run(&spec)?;
    let stdout = text(real.stdout)?;
    let _ = text(real.stderr)?;
    if real.code != 0 {
        return Err("real clock process failed".into());
    }
    let after = clock()?;
    let value = field(&json::parse_text(&stdout)?, "now")?;
    let in_bounds = match value.view() {
        JsonView::Integer(n) => n.parse::<u128>().is_ok_and(|n| before <= n && n <= after),
        JsonView::Bool(n) => before <= u128::from(n) && u128::from(n) <= after,
        JsonView::Float(n) if n.is_finite() && n >= 0.0 && n < 2.0_f64.powi(128) => {
            // Compare the exact integer part, rather than rounding the clock's
            // integer endpoints to f64. A fractional value at `after` is late.
            let floor = n as u128;
            before <= floor && (floor < after || (floor == after && n.fract() == 0.0))
        }
        _ => false,
    };
    if !in_bounds {
        return Err("real clock outside BOOTTIME observations".into());
    }
    let mut executables = vec![];
    for path in [&opt.reference, &opt.native, &opt.parser, &opt.fixture] {
        executables.push(format!(
            "{}: {}",
            python_fs::json_string(path.as_os_str())?,
            shared::string(&shared::sha(&fs::read(path)?))?
        ));
    }
    let report = object(&[
        ("status", "\"PASS\"".into()),
        (
            "profile",
            shared::string(if opt.qemu.is_some() { "arm" } else { "host" })?,
        ),
        ("cases", array(&reports)?.to_json()?),
        (
            "native_log_sink",
            shared::string("uninitialized; actual binary logging validated separately")?,
        ),
        (
            "real_boottime",
            object(&[
                ("before", before.to_string()),
                ("value", value.to_json()?),
                ("after", after.to_string()),
            ])?
            .to_json()?,
        ),
        ("executables", format!("{{{}}}", executables.join(", "))),
    ])?;
    fs::write(
        opt.output.join("report.json"),
        json::pretty(&report, 0)? + "\n",
    )?;
    let observations = reports
        .iter()
        .map(|r| {
            field(r, "observations")
                .unwrap()
                .to_json()
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .sum::<usize>();
    writeln!(
        printed,
        "{{\"status\": \"PASS\", \"cases\": {}, \"observations\": {observations}}}",
        reports.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--reference", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--parser", true),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--qemu", false),
            OptionSpec::value("--sysroot", false),
        ],
    )?;
    if options.help {
        println!("Compare camera runtime stress policy and parser boundaries");
        return Ok(());
    }
    let p = |key| lexical(Path::new(options.values.get(key).unwrap()));
    let opt = Options {
        reference: p("--reference"),
        native: p("--native"),
        parser: p("--parser"),
        fixture: p("--fixture"),
        output: p("--output"),
        qemu: options.values.get("--qemu").map(|p| lexical(Path::new(p))),
        sysroot: options
            .values
            .get("--sysroot")
            .map(|p| lexical(Path::new(p))),
    };
    let mut clock = || {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        Ok(t.tv_sec as u128 * 1_000_000_000 + t.tv_nsec as u128)
    };
    check(
        &opt,
        &mut NativeRunner,
        &mut clock,
        &mut std::io::stdout().lock(),
    )
}
